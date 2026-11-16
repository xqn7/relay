//! Genius as the last lyrics source: German rap and other songs LRCLIB and YouTube Music don't
//! have are usually there. Genius has no lyrics API, so the song is found with its public search
//! and the text is read from the song page's lyrics blocks. Plain text only (Genius has no
//! timings), so synced lyrics from LRCLIB always win.

use crate::error::{AppError, AppResult};
use crate::http::check_status;
use crate::matcher::fold;
use crate::util::encode_query;
use crate::AppState;
use serde_json::Value;

const SEARCH: &str = "https://genius.com/api/search/song";
const PAGES: &str = "https://genius.com/";
/// Lyrics pages are a few hundred kB; anything far bigger isn't one.
const MAX_PAGE_BYTES: usize = 3 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;

/// Lyrics for `artist` – `title` from Genius, if it has the song.
pub async fn lyrics(state: &AppState, artist: &str, title: &str) -> AppResult<Option<String>> {
    let url = format!("{SEARCH}?per_page=5&q={}", encode_query(&format!("{artist} {title}")));
    let found = state.http.get_json(&url).await?;
    let Some(page) = best_hit(&found, artist, title) else { return Ok(None) };
    let resp = state.http.send(&page, |c| c.get(&page)).await?;
    check_status(&resp)?;
    let html = resp.text().await?;
    if html.len() > MAX_PAGE_BYTES {
        return Err(AppError::new("bad_response", "Genius page is unexpectedly large"));
    }
    Ok(lyrics_from_page(&html).filter(|t| t.len() <= MAX_TEXT_BYTES))
}

/// Lowercase letters and digits only, for comparing names.
fn simple(s: &str) -> String {
    fold(s).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The page of the search hit that is this song: same title, and the artist among its credits.
fn best_hit(found: &Value, artist: &str, title: &str) -> Option<String> {
    let (want_artist, want_title) = (simple(artist), simple(title));
    if want_artist.is_empty() || want_title.is_empty() {
        return None;
    }
    let hits = found["response"]["sections"].as_array()?.iter().flat_map(|s| s["hits"].as_array().cloned().unwrap_or_default());
    hits.map(|h| h["result"].clone()).find_map(|r| {
        let url = r["url"].as_str().filter(|u| u.starts_with(PAGES) && u.len() < 300)?;
        let complete = r["lyrics_state"].as_str() == Some("complete") && r["instrumental"].as_bool() != Some(true);
        let same_title = simple(r["title"].as_str()?) == want_title;
        // Features and collaborations credit several artists; the song's own artist is one of them.
        let credits = simple(r["artist_names"].as_str().or(r["primary_artist"]["name"].as_str())?);
        (complete && same_title && credits.contains(&want_artist)).then(|| url.to_string())
    })
}

/// The lyrics text of a Genius song page: its lyrics blocks, without the header block Genius
/// marks to leave out, with line breaks kept.
pub fn lyrics_from_page(html: &str) -> Option<String> {
    const MARK: &str = "data-lyrics-container=\"true\"";
    let mut out = String::new();
    let mut rest = html;
    while let Some(at) = rest.find(MARK) {
        let after = &rest[at..];
        let body_start = after.find('>')? + 1;
        let (text, used) = block_text(&after[body_start..]);
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&text);
        rest = &after[body_start + used..];
    }
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let text = lines.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Text of one lyrics block up to its closing </div>, and how many bytes it spanned.
fn block_text(html: &str) -> (String, usize) {
    let mut text = String::new();
    let mut depth = 1usize;
    // Depth at which a left-out part (the header) started, while inside it.
    let mut skipping: Option<usize> = None;
    let mut i = 0;
    while i < html.len() {
        let rest = &html[i..];
        if rest.starts_with('<') {
            let end = rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            let tag = &rest[..end];
            if tag.starts_with("</div") {
                depth -= 1;
                if skipping == Some(depth) {
                    skipping = None;
                }
                if depth == 0 {
                    return (decode(&text), i + end);
                }
            } else if tag.starts_with("<div") {
                if skipping.is_none() && tag.contains("data-exclude-from-selection=\"true\"") {
                    skipping = Some(depth);
                }
                if !tag.ends_with("/>") {
                    depth += 1;
                }
            } else if tag.starts_with("<br") && skipping.is_none() {
                text.push('\n');
            }
            i += end;
        } else {
            let next = rest.find('<').unwrap_or(rest.len());
            if skipping.is_none() {
                text.push_str(&rest[..next]);
            }
            i += next;
        }
    }
    (decode(&text), html.len())
}

/// The HTML entities Genius uses in lyrics text.
fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(semi) = tail.find(';').filter(|&n| n <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..semi];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if name.starts_with("#x") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
            _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_lyrics_blocks_without_the_header() {
        let page = r#"<html><div data-lyrics-container="true" class="x"><div data-exclude-from-selection="true"><span>10 Contributors</span><div>XXX Lyrics</div></div>[Intro]<br/>Erste Zeile &amp; mehr<br><a href="/x"><span>Zweite</span> Zeile</a></div>
<p>ads</p><div data-lyrics-container="true">Dritte Zeile &#x27;ok&#x27;</div></html>"#;
        assert_eq!(lyrics_from_page(page).as_deref(), Some("[Intro]\nErste Zeile & mehr\nZweite Zeile\nDritte Zeile 'ok'"));
        assert_eq!(lyrics_from_page("<html>no lyrics here</html>"), None);
    }

    #[test]
    fn picks_the_hit_that_is_this_song() {
        let found = serde_json::json!({ "response": { "sections": [{ "hits": [
            { "result": { "title": "LAFAYETTE", "artist_names": "Kolja Goldstein", "url": "https://genius.com/a", "lyrics_state": "complete" } },
            { "result": { "title": "XXX", "artist_names": "LACAZETTE, AVIE & Kolja Goldstein", "url": "https://genius.com/b", "lyrics_state": "complete" } },
            { "result": { "title": "XXX", "artist_names": "Someone Else", "url": "https://genius.com/c", "lyrics_state": "complete" } }
        ]}]}});
        assert_eq!(best_hit(&found, "Kolja Goldstein", "XXX").as_deref(), Some("https://genius.com/b"));
        assert_eq!(best_hit(&found, "Kolja Goldstein", "Unknown"), None);
        let elsewhere = serde_json::json!({ "response": { "sections": [{ "hits": [
            { "result": { "title": "XXX", "artist_names": "Kolja Goldstein", "url": "https://evil.example/b", "lyrics_state": "complete" } }
        ]}]}});
        assert_eq!(best_hit(&elsewhere, "Kolja Goldstein", "XXX"), None);
    }
}
