//! SoundCloud through its public web API (api-v2), the same way soundcloud.com itself talks to it.
//! The web client_id is scraped from soundcloud.com's scripts and refreshed when it stops working.

use super::*;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::http::{check_status, Http};
use crate::util::{encode_query, now_secs, query_param};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

const API: &str = "https://api-v2.soundcloud.com";
const CLIENT_ID_KEY: &str = "soundcloud_client_id";
/// Signed SoundCloud CDN URLs usually live for a while; assume a short life when we can't tell.
const DEFAULT_URL_LIFETIME: i64 = 10 * 60;

pub struct SoundCloud {
    http: Arc<Http>,
    db: Arc<Db>,
    client_id: Mutex<Option<String>>,
}

impl SoundCloud {
    pub fn new(http: Arc<Http>, db: Arc<Db>) -> Self {
        let cached = db.device_get(CLIENT_ID_KEY).ok().flatten();
        Self { http, db, client_id: Mutex::new(cached) }
    }

    async fn client_id(&self, refresh: bool) -> AppResult<String> {
        let mut guard = self.client_id.lock().await;
        if let (Some(id), false) = (guard.as_ref(), refresh) {
            return Ok(id.clone());
        }
        let id = self.scrape_client_id().await?;
        // Losing the cached id only costs one extra scrape next start.
        let _ = self.db.device_set(CLIENT_ID_KEY, &id);
        *guard = Some(id.clone());
        Ok(id)
    }

    async fn scrape_client_id(&self) -> AppResult<String> {
        let html = self.http.get_text("https://soundcloud.com/").await?;
        let scripts: Vec<&str> = html
            .split("<script crossorigin src=\"")
            .skip(1)
            .filter_map(|s| s.split('"').next())
            .filter(|s| s.starts_with("https://a-v2.sndcdn.com/assets/") && s.ends_with(".js"))
            .collect();
        // The id lives in one of the last bundles; check those first.
        for src in scripts.iter().rev() {
            let Ok(js) = self.http.get_text(src).await else { continue };
            if let Some(id) = find_client_id(&js) {
                return Ok(id);
            }
        }
        Err(AppError::new("source_failed", "SoundCloud: could not find the web client id"))
    }

    /// GET an api-v2 path; on 401/403 the client_id has expired, so refresh it once and retry.
    async fn api(&self, path_and_query: &str) -> AppResult<Value> {
        for refresh in [false, true] {
            let id = self.client_id(refresh).await?;
            let sep = if path_and_query.contains('?') { '&' } else { '?' };
            let url = format!("{API}{path_and_query}{sep}client_id={id}");
            let resp = self.http.send(&url, |c| c.get(&url)).await?;
            if matches!(resp.status().as_u16(), 401 | 403) && !refresh {
                continue;
            }
            check_status(&resp)?;
            return Ok(resp.json().await?);
        }
        Err(AppError::new("forbidden", "SoundCloud rejected the client id"))
    }

    async fn fetch_track(&self, id: &str) -> AppResult<Value> {
        if !id.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::new("invalid_input", "bad SoundCloud track id"));
        }
        self.api(&format!("/tracks/{id}")).await
    }
}

pub fn find_client_id(js: &str) -> Option<String> {
    for marker in ["client_id:\"", "client_id=\"", "client_id=", "?client_id="] {
        for part in js.split(marker).skip(1) {
            let id: String = part.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if id.len() == 32 {
                return Some(id);
            }
        }
    }
    None
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// "-large" artwork is 100px; the "-t500x500" variant of the same file is the sharp one.
fn artwork(t: &Value) -> Option<String> {
    let url = str_of(t, "artwork_url").or_else(|| t.get("user").and_then(|u| str_of(u, "avatar_url")))?;
    url.starts_with("https://").then(|| url.replace("-large.", "-t500x500."))
}

pub fn map_track(t: &Value) -> Option<Track> {
    let id = t.get("id")?.as_u64()?.to_string();
    let title = str_of(t, "title")?.trim().to_string();
    let uploader = t.get("user").and_then(|u| str_of(u, "username")).unwrap_or("").to_string();
    let artist = t.get("publisher_metadata").and_then(|m| str_of(m, "artist")).map(str::to_string).unwrap_or(uploader);
    let user_id = t.get("user").and_then(|u| u.get("id")).and_then(Value::as_u64).map(|i| i.to_string());
    let track = Track {
        id: Track::make_id(SOUNDCLOUD, &id),
        source: SOUNDCLOUD.into(),
        source_id: id,
        title,
        artists: vec![ArtistRef { id: user_id, name: artist }],
        album: None,
        duration_ms: t.get("full_duration").or_else(|| t.get("duration")).and_then(Value::as_u64),
        cover: artwork(t),
        is_video: false,
        link: str_of(t, "permalink_url").map(str::to_string),
    };
    track.is_sane().then_some(track)
}

/// Go+ tracks only offer a 30-second preview to everyone else; treat them as unplayable here.
pub fn is_preview_only(t: &Value) -> bool {
    str_of(t, "policy") == Some("SNIP")
}

struct Pick<'a> {
    url: &'a str,
    kind: StreamKind,
    mime: String,
}

/// Prefers one progressive file (seekable, instant) over HLS; skips encrypted and snipped variants.
fn pick_transcoding(t: &Value) -> Option<Pick<'_>> {
    let list = t.get("media")?.get("transcodings")?.as_array()?;
    let mut best: Option<(u8, Pick)> = None;
    for tc in list {
        if tc.get("snipped").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(url) = str_of(tc, "url") else { continue };
        let format = tc.get("format");
        let protocol = format.and_then(|f| str_of(f, "protocol")).unwrap_or("");
        let mime = format.and_then(|f| str_of(f, "mime_type")).unwrap_or("").split(';').next().unwrap_or("").trim().to_string();
        let (rank, kind) = match (protocol, mime.as_str()) {
            ("progressive", _) => (4, StreamKind::Direct),
            ("hls", "audio/mp4") => (3, StreamKind::Hls),
            ("hls", "audio/mpeg") => (2, StreamKind::Hls),
            ("hls", m) if m.starts_with("audio/ogg") => (1, StreamKind::Hls),
            _ => continue,
        };
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            let mime = if mime.is_empty() { "audio/mpeg".into() } else { mime };
            best = Some((rank, Pick { url, kind, mime }));
        }
    }
    best.map(|(_, p)| p)
}

fn url_expiry(url: &str) -> i64 {
    // CloudFront-signed URLs carry "Expires" as unix seconds; anything else gets a safe default.
    ["Expires", "expires"]
        .iter()
        .find_map(|k| query_param(url, k).and_then(|v| v.parse::<i64>().ok()))
        .map(|t| t - 60)
        .unwrap_or_else(|| now_secs() + DEFAULT_URL_LIFETIME)
}

fn map_playlist(p: &Value) -> Option<Collection> {
    let id = p.get("id")?.as_u64()?.to_string();
    let title = str_of(p, "title")?.to_string();
    let owner = p.get("user").and_then(|u| str_of(u, "username")).unwrap_or("");
    let count = p.get("track_count").and_then(Value::as_u64).unwrap_or(0);
    let kind = if p.get("is_album").and_then(Value::as_bool) == Some(true) { "album" } else { "playlist" };
    Some(Collection {
        source: SOUNDCLOUD.into(),
        kind: kind.into(),
        id,
        title,
        subtitle: format!("{owner} · {count}"),
        cover: artwork(p),
    })
}

fn collection(v: &Value) -> &[Value] {
    v.get("collection").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

impl Source for SoundCloud {
    async fn search(&self, query: &str, kind: SearchKind) -> AppResult<SearchResults> {
        let q = encode_query(query);
        let mut out = SearchResults { source: SOUNDCLOUD.into(), ..Default::default() };
        let want_tracks = matches!(kind, SearchKind::All | SearchKind::Songs);
        let want_lists = matches!(kind, SearchKind::All | SearchKind::Playlists);
        if want_tracks {
            let v = self.api(&format!("/search/tracks?q={q}&limit=20")).await?;
            out.tracks = collection(&v).iter().filter(|t| !is_preview_only(t)).filter_map(map_track).collect();
        }
        if want_lists {
            let limit = if kind == SearchKind::All { 6 } else { 20 };
            if let Ok(v) = self.api(&format!("/search/playlists_without_albums?q={q}&limit={limit}")).await {
                out.playlists = collection(&v).iter().filter_map(map_playlist).collect();
            }
        }
        Ok(out)
    }

    async fn resolve(&self, track: &Track, _quality: Quality) -> AppResult<ResolvedStream> {
        let t = self.fetch_track(&track.source_id).await?;
        if is_preview_only(&t) {
            return Err(AppError::new("preview_only", "SoundCloud Go+ track (preview only)"));
        }
        let pick = pick_transcoding(&t).ok_or_else(|| AppError::new("unavailable", "SoundCloud has no playable stream"))?;
        let auth = str_of(&t, "track_authorization").unwrap_or("");
        let id = self.client_id(false).await?;
        let sep = if pick.url.contains('?') { '&' } else { '?' };
        let lookup = format!("{}{sep}client_id={id}&track_authorization={auth}", pick.url);
        if !lookup.starts_with("https://") {
            return Err(AppError::new("bad_response", "SoundCloud transcoding URL is not https"));
        }
        let v = self.http.get_json(&lookup).await?;
        let url = str_of(&v, "url")
            .filter(|u| u.starts_with("https://"))
            .ok_or_else(|| AppError::new("bad_response", "SoundCloud returned no stream URL"))?
            .to_string();
        Ok(ResolvedStream { expires_at: url_expiry(&url), url, kind: pick.kind, mime: pick.mime, size: None, loudness_db: None, user_agent: None })
    }

    async fn collection_tracks(&self, _kind: &str, id: &str) -> AppResult<Vec<Track>> {
        if !id.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::new("invalid_input", "bad SoundCloud playlist id"));
        }
        let p = self.api(&format!("/playlists/{id}")).await?;
        let items = p.get("tracks").and_then(Value::as_array).cloned().unwrap_or_default();
        // Sets only embed full data for the first few tracks; the rest are bare ids.
        let (full, bare): (Vec<_>, Vec<_>) = items.into_iter().partition(|t| t.get("title").is_some());
        let mut tracks: Vec<Value> = full;
        let ids: Vec<String> = bare.iter().filter_map(|t| t.get("id").and_then(Value::as_u64)).map(|i| i.to_string()).collect();
        for chunk in ids.chunks(50) {
            let v = self.api(&format!("/tracks?ids={}", chunk.join("%2C"))).await?;
            if let Some(arr) = v.as_array() {
                tracks.extend(arr.iter().cloned());
            }
        }
        Ok(tracks.iter().filter(|t| !is_preview_only(t)).filter_map(map_track).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_client_id_in_bundle() {
        let js = r#"x={client_id:"abcdefghijklmnopqrstuvwxyz012345",y:1}"#;
        assert_eq!(find_client_id(js).as_deref(), Some("abcdefghijklmnopqrstuvwxyz012345"));
        assert_eq!(find_client_id("client_id:\"short\""), None);
    }

    fn sample() -> Value {
        json!({
            "id": 123, "title": "Song", "duration": 30000, "full_duration": 181000, "policy": "ALLOW",
            "artwork_url": "https://i1.sndcdn.com/artworks-x-large.jpg",
            "permalink_url": "https://soundcloud.com/a/song",
            "user": {"id": 9, "username": "uploader"},
            "publisher_metadata": {"artist": "Real Artist"},
            "media": {"transcodings": [
                {"url": "https://api-v2.soundcloud.com/media/1/hls", "snipped": false, "format": {"protocol": "hls", "mime_type": "audio/mpeg"}},
                {"url": "https://api-v2.soundcloud.com/media/1/prog", "snipped": false, "format": {"protocol": "progressive", "mime_type": "audio/mpeg"}},
                {"url": "https://api-v2.soundcloud.com/media/1/enc", "snipped": false, "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}
            ]}
        })
    }

    #[test]
    fn maps_track_fields() {
        let t = map_track(&sample()).unwrap();
        assert_eq!(t.id, "soundcloud:123");
        assert_eq!(t.artist_line(), "Real Artist");
        assert_eq!(t.duration_ms, Some(181000));
        assert_eq!(t.cover.as_deref(), Some("https://i1.sndcdn.com/artworks-x-t500x500.jpg"));
    }

    #[test]
    fn prefers_progressive_and_skips_encrypted() {
        let s = sample();
        let p = pick_transcoding(&s).unwrap();
        assert_eq!(p.kind, StreamKind::Direct);
        assert!(p.url.ends_with("prog"));
    }

    #[test]
    fn detects_go_plus_previews() {
        let mut s = sample();
        assert!(!is_preview_only(&s));
        s["policy"] = json!("SNIP");
        assert!(is_preview_only(&s));
    }
}
