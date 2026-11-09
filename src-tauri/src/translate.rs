//! Lyrics translation: Google's free web endpoint (no key), or DeepL Free when the user saved a
//! DeepL key. Each song is translated once per language; the result is kept in the database.

use crate::error::{AppError, AppResult};
use crate::http::check_status;
use crate::lyrics;
use crate::util::{encode_query, now_secs};
use crate::AppState;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Response;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

pub const LANGS: &[&str] = &["en", "de"];
/// Name of the DeepL key in the Windows Credential Manager.
pub const DEEPL_SECRET: &str = "deepl";
/// Google's endpoint handles a few thousand characters per request comfortably.
const GOOGLE_CHUNK_CHARS: usize = 1500;
/// DeepL takes at most 50 texts per request.
const DEEPL_CHUNK: usize = 50;
const FORM: &str = "application/x-www-form-urlencoded;charset=utf-8";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    /// One entry per lyrics line (empty for empty lines).
    pub lines: Vec<String>,
    /// The language the lyrics are in, as the service detected it.
    pub source_lang: Option<String>,
}

fn cached(state: &AppState, track_id: &str, lang: &str) -> AppResult<Option<Translation>> {
    let row: Option<(String, Option<String>)> = state.db.with(|c| {
        c.query_row("SELECT lines,source_lang FROM translations WHERE track_id=?1 AND lang=?2", params![track_id, lang], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
    })?;
    Ok(row.and_then(|(lines, source_lang)| Some(Translation { lines: serde_json::from_str(&lines).ok()?, source_lang })))
}

pub async fn translate(state: &Arc<AppState>, track_id: &str, lang: &str) -> AppResult<Translation> {
    if !LANGS.contains(&lang) {
        return Err(AppError::new("invalid_input", "unsupported language"));
    }
    let lines = lyrics::stored(&state.db, track_id)?.ok_or_else(|| AppError::new("not_found", "no lyrics to translate"))?.text_lines();
    if let Some(found) = cached(state, track_id, lang)?.filter(|t| t.lines.len() == lines.len()) {
        return Ok(found);
    }
    if !state.net.online() {
        return Err(AppError::new("offline", "translations need the internet"));
    }
    let result = match crate::secrets::get(DEEPL_SECRET) {
        Some(key) => deepl(state, &key, &lines, lang).await,
        None => google(state, &lines, lang).await,
    };
    let found = result.inspect_err(|e| crate::network::suspect(state, e.code))?;
    state.db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO translations(track_id,lang,lines,source_lang,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![track_id, lang, serde_json::to_string(&found.lines).unwrap_or_default(), found.source_lang, now_secs()],
        )
    })?;
    Ok(found)
}

/// Indices of non-empty lines, grouped so each group stays under `max_chars` (or `max_count`).
fn chunks(lines: &[String], max_chars: usize, max_count: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut size = 0;
    for (i, line) in lines.iter().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let fits = out.last().is_some_and(|c| c.len() < max_count && size + line.len() < max_chars);
        if !fits {
            out.push(Vec::new());
            size = 0;
        }
        size += line.len() + 1;
        out.last_mut().expect("pushed above").push(i);
    }
    out
}

/// Google's answer: [[["translated","original",…],…],null,"detected language",…].
pub fn parse_google(v: &Value) -> AppResult<(String, Option<String>)> {
    let segments = v.get(0).and_then(Value::as_array).ok_or_else(|| AppError::new("bad_response", "Google: no translation"))?;
    let text: String = segments.iter().filter_map(|s| s.get(0).and_then(Value::as_str)).collect();
    Ok((text, v.get(2).and_then(Value::as_str).map(str::to_string)))
}

async fn google_request(state: &AppState, text: &str, lang: &str) -> AppResult<(String, Option<String>)> {
    let url = format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={lang}&dt=t");
    // Sent as a form body: a long song doesn't fit into a URL.
    let body = format!("q={}", encode_query(text));
    let resp = state.http.send(&url, |c| c.post(&url).header(CONTENT_TYPE, FORM).body(body.clone())).await?;
    check_status(&resp)?;
    parse_google(&resp.json().await?)
}

async fn google(state: &AppState, lines: &[String], lang: &str) -> AppResult<Translation> {
    let mut out = vec![String::new(); lines.len()];
    let mut source_lang = None;
    for chunk in chunks(lines, GOOGLE_CHUNK_CHARS, usize::MAX) {
        let text = chunk.iter().map(|&i| lines[i].trim()).collect::<Vec<_>>().join("\n");
        let (translated, detected) = google_request(state, &text, lang).await?;
        source_lang = source_lang.or(detected);
        let parts: Vec<&str> = translated.split('\n').collect();
        if parts.len() == chunk.len() {
            for (&i, part) in chunk.iter().zip(parts) {
                out[i] = part.trim().to_string();
            }
        } else {
            // Google merged or split lines: translate this part line by line so each
            // translation stays under its own line.
            for &i in &chunk {
                out[i] = google_request(state, lines[i].trim(), lang).await?.0.trim().to_string();
            }
        }
    }
    Ok(Translation { lines: out, source_lang })
}

fn deepl_base(key: &str) -> &'static str {
    // DeepL Free keys end in ":fx" and use their own host.
    if key.ends_with(":fx") {
        "https://api-free.deepl.com/v2"
    } else {
        "https://api.deepl.com/v2"
    }
}

fn deepl_status(resp: &Response) -> AppResult<()> {
    match resp.status().as_u16() {
        401 | 403 => Err(AppError::new("deepl_key_invalid", "DeepL rejected the key")),
        456 => Err(AppError::new("deepl_quota", "DeepL monthly limit reached")),
        _ => check_status(resp),
    }
}

async fn deepl(state: &AppState, key: &str, lines: &[String], lang: &str) -> AppResult<Translation> {
    let url = format!("{}/translate", deepl_base(key));
    let target = if lang == "de" { "DE" } else { "EN-US" };
    let mut out = vec![String::new(); lines.len()];
    let mut source_lang = None;
    for chunk in chunks(lines, usize::MAX, DEEPL_CHUNK) {
        let mut body = format!("target_lang={target}");
        for &i in &chunk {
            body.push_str("&text=");
            body.push_str(&encode_query(lines[i].trim()));
        }
        let resp = state
            .http
            .send(&url, |c| c.post(&url).header(AUTHORIZATION, format!("DeepL-Auth-Key {key}")).header(CONTENT_TYPE, FORM).body(body.clone()))
            .await?;
        deepl_status(&resp)?;
        let v: Value = resp.json().await?;
        let got = v.get("translations").and_then(Value::as_array).filter(|t| t.len() == chunk.len());
        let got = got.ok_or_else(|| AppError::new("bad_response", "DeepL: unexpected answer"))?;
        for (&i, item) in chunk.iter().zip(got) {
            out[i] = item.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if source_lang.is_none() {
                source_lang = item.get("detected_source_language").and_then(Value::as_str).map(str::to_lowercase);
            }
        }
    }
    Ok(Translation { lines: out, source_lang })
}

/// Checks a DeepL key before it's saved, so a typo shows right away.
pub async fn check_deepl_key(state: &AppState, key: &str) -> AppResult<()> {
    let ok = !key.is_empty() && key.len() <= 128 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':');
    if !ok {
        return Err(AppError::new("deepl_key_invalid", "not a DeepL key"));
    }
    let url = format!("{}/usage", deepl_base(key));
    let resp = state.http.send(&url, |c| c.get(&url).header(AUTHORIZATION, format!("DeepL-Auth-Key {key}"))).await?;
    deepl_status(&resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn groups_lines_and_skips_empty_ones() {
        let lines: Vec<String> = ["aaaa", "", "bbbb", "cccc", " "].iter().map(|s| s.to_string()).collect();
        assert_eq!(chunks(&lines, 11, usize::MAX), vec![vec![0, 2], vec![3]]);
        assert_eq!(chunks(&lines, usize::MAX, 2), vec![vec![0, 2], vec![3]]);
    }

    #[test]
    fn reads_googles_answer() {
        let v = json!([[["Hallo\n", "Hello\n", null], ["Welt", "World", null]], null, "en"]);
        assert_eq!(parse_google(&v).unwrap(), ("Hallo\nWelt".to_string(), Some("en".to_string())));
        assert!(parse_google(&json!({"error": 1})).is_err());
    }
}
