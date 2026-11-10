//! Lyrics: line-synced lyrics from LRCLIB, YouTube Music's plain lyrics when LRCLIB has none,
//! and the user's own from the lyrics editor, which always win. Every result (also "nothing
//! found") is kept in the database, so a song is looked up once and its lyrics show offline.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::http::check_status;
use crate::matcher::{best_match, clean_title, fold};
use crate::sources::{is_import_only, Track, YOUTUBE};
use crate::util::{encode_query, now_secs};
use crate::AppState;
use reqwest::header::USER_AGENT;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const LRCLIB: &str = "https://lrclib.net/api";
/// LRCLIB asks apps to name themselves in the User-Agent.
const LRCLIB_AGENT: &str = concat!("Relay/", env!("CARGO_PKG_VERSION"), " (https://github.com/xqn7/relay)");
/// Songs without lyrics are looked up again after this long; lyrics get added all the time.
const RETRY_MISSING_SECS: i64 = 3 * 24 * 3600;
/// Synced lyrics only fit a recording of the same length (slowed or live versions don't).
const DURATION_SLACK_MS: u64 = 3000;
pub const MAX_LINES: usize = 1000;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
const SEARCH_RESULTS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub time_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub synced: Option<Vec<Line>>,
    pub plain: Option<String>,
    pub instrumental: bool,
    /// "lrclib", "youtube" or "user".
    pub source: String,
    /// The credit line YouTube shows under its lyrics.
    pub credit: Option<String>,
}

impl Lyrics {
    /// The lines in order, as the translation and the lyrics view see them.
    pub fn text_lines(&self) -> Vec<String> {
        match &self.synced {
            Some(lines) => lines.iter().map(|l| l.text.clone()).collect(),
            None => self.plain.as_deref().unwrap_or("").lines().map(|l| l.trim().to_string()).collect(),
        }
    }
}

/// One lookup per song at a time: the current song and the preloads may ask for the same one.
#[derive(Default)]
pub struct Locks(Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>);

impl Locks {
    fn for_track(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.0.lock().unwrap_or_else(|p| p.into_inner());
        map.retain(|_, lock| Arc::strong_count(lock) > 1);
        map.entry(id.to_string()).or_default().clone()
    }
}

// ---- LRC ------------------------------------------------------------------------------------

/// "mm:ss", "mm:ss.x", "mm:ss.xx" or "mm:ss.xxx" → milliseconds. Metadata tags ("ar:…") fail.
fn parse_stamp(s: &str) -> Option<u64> {
    let (m, rest) = s.split_once(':')?;
    let (sec, frac) = rest.split_once(['.', ':']).unwrap_or((rest, ""));
    let digits = |x: &str| !x.is_empty() && x.len() <= 3 && x.chars().all(|c| c.is_ascii_digit());
    if !digits(m.trim()) || !digits(sec) || (!frac.is_empty() && !digits(frac)) {
        return None;
    }
    let (m, sec): (u64, u64) = (m.trim().parse().ok()?, sec.parse().ok()?);
    if sec >= 60 {
        return None;
    }
    let frac_ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        _ => frac.parse::<u64>().ok()?,
    };
    Some(m * 60_000 + sec * 1000 + frac_ms)
}

/// Enhanced LRC puts "<mm:ss.xx>" before words; the lyrics view times words itself.
fn strip_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        match rest[i..].find('>') {
            Some(j) if parse_stamp(&rest[i + 1..i + j]).is_some() => {
                out.push_str(&rest[..i]);
                rest = &rest[i + j + 1..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            let Some(ms) = parse_stamp(&inner[..end]) else { break };
            times.push(ms);
            rest = inner[end + 1..].trim_start();
        }
        let text = strip_word_stamps(rest);
        out.extend(times.into_iter().map(|time_ms| Line { time_ms, text: text.clone() }));
    }
    out.sort_by_key(|l| l.time_ms);
    out
}

pub fn format_lrc(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|l| format!("[{:02}:{:02}.{:02}]{}", l.time_ms / 60_000, l.time_ms / 1000 % 60, l.time_ms % 1000 / 10, l.text))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- stored lyrics --------------------------------------------------------------------------

struct Row {
    synced: Option<String>,
    plain: Option<String>,
    instrumental: bool,
    source: Option<String>,
    credit: Option<String>,
    checked_at: i64,
}

fn read(db: &Db, track_id: &str) -> AppResult<Option<Row>> {
    db.with(|c| {
        c.query_row("SELECT synced,plain,instrumental,source,credit,checked_at FROM lyrics WHERE track_id=?1", [track_id], |r| {
            Ok(Row { synced: r.get(0)?, plain: r.get(1)?, instrumental: r.get(2)?, source: r.get(3)?, credit: r.get(4)?, checked_at: r.get(5)? })
        })
        .optional()
    })
}

fn to_lyrics(row: Row) -> Option<Lyrics> {
    let source = row.source?;
    let synced = row.synced.map(|s| parse_lrc(&s)).filter(|l| !l.is_empty());
    Some(Lyrics { synced, plain: row.plain, instrumental: row.instrumental, source, credit: row.credit })
}

/// Lyrics already on this PC, without going online.
pub fn stored(db: &Db, track_id: &str) -> AppResult<Option<Lyrics>> {
    Ok(read(db, track_id)?.and_then(to_lyrics))
}

fn store(db: &Db, track_id: &str, l: Option<&Lyrics>, user: bool) -> AppResult<()> {
    let now = now_secs();
    db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO lyrics(track_id,synced,plain,instrumental,source,credit,user_edited,checked_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",
            params![
                track_id,
                l.and_then(|l| l.synced.as_deref()).map(format_lrc),
                l.and_then(|l| l.plain.clone()),
                l.is_some_and(|l| l.instrumental),
                l.map(|l| l.source.clone()),
                l.and_then(|l| l.credit.clone()),
                user,
                now
            ],
        )
    })?;
    Ok(())
}

fn drop_translations(db: &Db, track_id: &str) -> AppResult<()> {
    db.with(|c| c.execute("DELETE FROM translations WHERE track_id=?1", [track_id]))?;
    Ok(())
}

/// The lyrics editor's result. Timed lines make synced lyrics; text alone makes plain ones.
pub fn save_user(db: &Db, track_id: &str, mut lines: Vec<Line>, plain: &str) -> AppResult<Lyrics> {
    let text_bytes: usize = lines.iter().map(|l| l.text.len()).sum::<usize>() + plain.len();
    if lines.len() > MAX_LINES || text_bytes > MAX_TEXT_BYTES {
        return Err(AppError::new("invalid_input", "lyrics too long"));
    }
    for l in &mut lines {
        l.text = l.text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    lines.sort_by_key(|l| l.time_ms);
    let plain = plain.trim();
    let lyrics = if !lines.is_empty() {
        Lyrics { synced: Some(lines), plain: None, instrumental: false, source: "user".into(), credit: None }
    } else if !plain.is_empty() {
        Lyrics { synced: None, plain: Some(plain.to_string()), instrumental: false, source: "user".into(), credit: None }
    } else {
        return Err(AppError::new("invalid_input", "no lyrics"));
    };
    store(db, track_id, Some(&lyrics), true)?;
    drop_translations(db, track_id)?;
    Ok(lyrics)
}

/// Removes the user's own lyrics, so the song's lyrics are looked up online again.
pub fn reset_user(db: &Db, track_id: &str) -> AppResult<()> {
    db.with(|c| c.execute("DELETE FROM lyrics WHERE track_id=?1 AND user_edited=1", [track_id]))?;
    drop_translations(db, track_id)
}

/// Space used by looked-up lyrics and translations (the user's own lyrics aren't cache).
pub fn cache_bytes(db: &Db) -> AppResult<u64> {
    db.with(|c| {
        let lyrics: i64 = c.query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(synced,''))+LENGTH(COALESCE(plain,''))+64),0) FROM lyrics WHERE user_edited=0",
            [],
            |r| r.get(0),
        )?;
        let translations: i64 = c.query_row("SELECT COALESCE(SUM(LENGTH(lines)+32),0) FROM translations", [], |r| r.get(0))?;
        Ok((lyrics + translations).max(0) as u64)
    })
}

pub fn clear_cache(db: &Db) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM lyrics WHERE user_edited=0", [])?;
        c.execute("DELETE FROM translations", [])
    })?;
    Ok(())
}

// ---- lookup ---------------------------------------------------------------------------------

/// The song's lyrics: stored ones, or looked up online (and stored). Ok(None) = there are none.
pub async fn get(state: &Arc<AppState>, track: &Track) -> AppResult<Option<Lyrics>> {
    let lock = state.lyrics_locks.for_track(&track.id);
    let _one_lookup = lock.lock().await;
    if let Some(row) = read(&state.db, &track.id)? {
        if row.source.is_some() || now_secs() - row.checked_at < RETRY_MISSING_SECS {
            return Ok(to_lyrics(row));
        }
    }
    if !state.net.online() {
        return Err(AppError::new("offline", "lyrics are looked up online"));
    }
    match look_up(state, track).await {
        Ok(found) => {
            store(&state.db, &track.id, found.as_ref(), false)?;
            Ok(found)
        }
        Err(e) => {
            crate::network::suspect(state, e.code);
            Err(e)
        }
    }
}

async fn look_up(state: &AppState, track: &Track) -> AppResult<Option<Lyrics>> {
    let lrclib = lrclib(state, track).await;
    if let Ok(Some(found)) = lrclib {
        return Ok(Some(found));
    }
    match (lrclib, youtube(state, track).await) {
        (_, Ok(Some(found))) => Ok(Some(found)),
        (Ok(_), Ok(None)) => Ok(None),
        // YouTube answered, so the internet works and it's LRCLIB that failed: say so instead
        // of passing on its connection error, which would read as "you're offline".
        (Err(e), Ok(None)) => Err(AppError::new("source_failed", format!("LRCLIB: {e}"))),
        // "Nothing found" is only remembered when both services really answered.
        (_, Err(e)) => Err(e),
    }
}

/// Lowercase, accent-free, punctuation-free text for comparing titles and artists.
fn norm(s: &str) -> String {
    let folded = fold(s);
    let kept: String = folded.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn main_artist(track: &Track) -> String {
    let name = track.artists.first().map(|a| a.name.as_str()).unwrap_or("");
    name.strip_suffix(" - Topic").unwrap_or(name).trim().to_string()
}

/// (artist, title) pairs to try: as given, with junk tags removed, and "Artist - Title" uploads split.
pub fn queries(track: &Track) -> Vec<(String, String)> {
    let artist = main_artist(track);
    let mut out = vec![(artist.clone(), track.title.trim().to_string()), (artist, clean_title(&track.title))];
    if let Some((left, right)) = track.title.split_once(" - ") {
        out.push((left.trim().to_string(), clean_title(right)));
    }
    let mut seen = Vec::new();
    out.retain(|(a, t)| {
        let key = (norm(a), norm(t));
        let keep = !key.0.is_empty() && !key.1.is_empty() && !seen.contains(&key);
        seen.push(key);
        keep
    });
    out
}

/// One LRCLIB record → lyrics, if it has any that fit this recording.
pub fn from_record(v: &Value, duration_ms: Option<u64>) -> Option<Lyrics> {
    let record_ms = v.get("duration").and_then(Value::as_f64).filter(|d| *d > 0.0).map(|d| (d * 1000.0) as u64);
    let fits = match (duration_ms, record_ms) {
        (Some(a), Some(b)) => a.abs_diff(b) <= DURATION_SLACK_MS,
        _ => true,
    };
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty() && s.len() <= MAX_TEXT_BYTES);
    let synced = text("syncedLyrics").map(parse_lrc).filter(|l| fits && l.len() >= 2 && l.len() <= MAX_LINES);
    let plain = if synced.is_some() { None } else { text("plainLyrics").map(String::from) };
    let instrumental = fits && synced.is_none() && plain.is_none() && v.get("instrumental").and_then(Value::as_bool) == Some(true);
    if synced.is_none() && plain.is_none() && !instrumental {
        return None;
    }
    Some(Lyrics { synced, plain, instrumental, source: "lrclib".into(), credit: None })
}

/// The best search result for the song: same title and artist, synced lyrics preferred.
pub fn pick(items: &[Value], artist: &str, title: &str, duration_ms: Option<u64>) -> Option<Lyrics> {
    let (want_artist, want_title) = (norm(artist), norm(&clean_title(title)));
    let mut best: Option<(u8, Lyrics)> = None;
    for v in items.iter().take(SEARCH_RESULTS) {
        let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
        let (a, t) = (norm(field("artistName")), norm(&clean_title(field("trackName"))));
        if t != want_title || a.is_empty() || !(a.contains(&want_artist) || want_artist.contains(&a)) {
            continue;
        }
        let Some(found) = from_record(v, duration_ms) else { continue };
        let rank = if found.synced.is_some() { 3 } else if found.plain.is_some() { 2 } else { 1 };
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, found));
        }
    }
    best.map(|(_, l)| l)
}

/// GET from LRCLIB; Ok(None) when it has no such record.
async fn lrclib_request(state: &AppState, url: &str) -> AppResult<Option<Value>> {
    let resp = state.http.send(url, |c| c.get(url).header(USER_AGENT, LRCLIB_AGENT)).await?;
    if matches!(resp.status().as_u16(), 400 | 404) {
        return Ok(None);
    }
    check_status(&resp)?;
    Ok(Some(resp.json().await?))
}

async fn lrclib(state: &AppState, track: &Track) -> AppResult<Option<Lyrics>> {
    let queries = queries(track);
    let Some((artist, title)) = queries.first() else { return Ok(None) };
    if let Some(ms) = track.duration_ms {
        let mut url = format!(
            "{LRCLIB}/get?artist_name={}&track_name={}&duration={}",
            encode_query(artist),
            encode_query(title),
            (ms + 500) / 1000
        );
        if let Some(album) = track.album.as_ref().filter(|a| !a.name.is_empty()) {
            url.push_str(&format!("&album_name={}", encode_query(&album.name)));
        }
        if let Some(found) = lrclib_request(state, &url).await?.and_then(|v| from_record(&v, track.duration_ms)) {
            return Ok(Some(found));
        }
    }
    let mut fallback: Option<Lyrics> = None;
    for (artist, title) in &queries {
        let url = format!("{LRCLIB}/search?track_name={}&artist_name={}", encode_query(title), encode_query(artist));
        let Some(Value::Array(items)) = lrclib_request(state, &url).await? else { continue };
        match pick(&items, artist, title, track.duration_ms) {
            Some(found) if found.synced.is_some() => return Ok(Some(found)),
            Some(found) if fallback.is_none() => fallback = Some(found),
            _ => {}
        }
    }
    Ok(fallback)
}

async fn youtube_id(state: &AppState, track: &Track) -> AppResult<Option<String>> {
    if track.source == YOUTUBE {
        return Ok(Some(track.source_id.clone()));
    }
    if is_import_only(&track.source) {
        if let Some(Some(m)) = crate::library::get_match(&state.db, &track.source, &track.source_id)? {
            if m.source == YOUTUBE {
                return Ok(Some(m.source_id));
            }
        }
    }
    let query = format!("{} {}", main_artist(track), clean_title(&track.title));
    let songs = state.yt.search_songs(query.trim(), false).await?;
    Ok(best_match(track, &songs).map(|t| t.source_id.clone()))
}

async fn youtube(state: &AppState, track: &Track) -> AppResult<Option<Lyrics>> {
    let Some(id) = youtube_id(state, track).await? else { return Ok(None) };
    let Some((body, footer)) = state.yt.lyrics(&id).await? else { return Ok(None) };
    let body = body.trim();
    if body.len() > MAX_TEXT_BYTES {
        return Ok(None);
    }
    let credit = Some(footer.trim().to_string()).filter(|f| !f.is_empty() && f.len() < 200);
    Ok(Some(Lyrics { synced: None, plain: Some(body.to_string()), instrumental: false, source: "youtube".into(), credit }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_lrc() {
        let text = "[ar:Someone]\n[00:12.34]First line\n[00:05.0][01:02.500]Twice <00:06.10>with <00:06.40>words\n[00:20.00]\nnot a line";
        let lines = parse_lrc(text);
        assert_eq!(
            lines,
            vec![
                Line { time_ms: 5000, text: "Twice with words".into() },
                Line { time_ms: 12340, text: "First line".into() },
                Line { time_ms: 20000, text: "".into() },
                Line { time_ms: 62500, text: "Twice with words".into() },
            ]
        );
    }

    #[test]
    fn rejects_bad_stamps() {
        assert_eq!(parse_stamp("01:75.00"), None);
        assert_eq!(parse_stamp("length: 3:20"), None);
        assert_eq!(parse_stamp("3:07"), Some(187_000));
    }

    #[test]
    fn lrc_round_trips() {
        let lines = vec![Line { time_ms: 1230, text: "a".into() }, Line { time_ms: 61_050, text: "b c".into() }];
        assert_eq!(format_lrc(&lines), "[00:01.23]a\n[01:01.05]b c");
        assert_eq!(parse_lrc(&format_lrc(&lines)), lines);
    }

    #[test]
    fn synced_lyrics_need_the_same_length() {
        let rec = json!({"duration": 200.0, "syncedLyrics": "[00:01.00]a\n[00:02.00]b", "plainLyrics": "a\nb"});
        assert!(from_record(&rec, Some(201_500)).unwrap().synced.is_some());
        // A slowed version is much longer: only the plain text fits.
        let slowed = from_record(&rec, Some(260_000)).unwrap();
        assert!(slowed.synced.is_none());
        assert_eq!(slowed.plain.as_deref(), Some("a\nb"));
        assert!(from_record(&json!({"duration": 200.0, "instrumental": true}), Some(200_000)).unwrap().instrumental);
        assert!(from_record(&json!({"duration": 200.0}), Some(200_000)).is_none());
    }

    #[test]
    fn picks_the_right_search_result() {
        let items = vec![
            json!({"artistName": "Other", "trackName": "Song", "duration": 180.0, "syncedLyrics": "[00:01.00]x\n[00:02.00]y"}),
            json!({"artistName": "Artist", "trackName": "Song (Remastered 2011)", "duration": 180.0, "plainLyrics": "plain"}),
            json!({"artistName": "Artist feat. B", "trackName": "Song", "duration": 181.0, "syncedLyrics": "[00:01.00]a\n[00:02.00]b"}),
        ];
        let found = pick(&items, "Artist", "Song", Some(180_000)).unwrap();
        assert_eq!(found.synced.unwrap()[0].text, "a");
        assert!(pick(&items, "Nobody", "Song", Some(180_000)).is_none());
    }

    fn track(title: &str, artist: &str) -> Track {
        Track::imported("spotify", "x", title, vec![artist.into()], Some(1000), None, None)
    }

    #[test]
    fn builds_queries() {
        let q = queries(&track("Daft Punk - One More Time (Official Video)", "Daft Punk - Topic"));
        assert_eq!(q[0].0, "Daft Punk");
        assert!(q.iter().any(|(a, t)| a == "Daft Punk" && t == "one more time"));
        assert_eq!(queries(&track("Song", "Artist")).len(), 1);
    }

    #[test]
    fn user_lyrics_win_and_reset() {
        let db = Db::in_memory();
        let lines = vec![Line { time_ms: 2000, text: " two  words ".into() }, Line { time_ms: 1000, text: "one".into() }];
        let saved = save_user(&db, "t:1", lines, "").unwrap();
        assert_eq!(saved.synced.as_ref().unwrap()[0].text, "one");
        assert_eq!(stored(&db, "t:1").unwrap().unwrap().synced.unwrap()[1].text, "two words");
        assert_eq!(cache_bytes(&db).unwrap(), 0);
        clear_cache(&db).unwrap();
        assert!(stored(&db, "t:1").unwrap().is_some());
        reset_user(&db, "t:1").unwrap();
        assert!(stored(&db, "t:1").unwrap().is_none());
        assert!(save_user(&db, "t:1", vec![], "  ").is_err());
    }
}
