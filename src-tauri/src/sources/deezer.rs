//! Deezer, import only, through its free public API (api.deezer.com, no key).

use super::{RemoteList, Track, DEEZER};
use crate::error::{AppError, AppResult};
use crate::http::Http;
use serde_json::Value;

const API: &str = "https://api.deezer.com";
const PAGE: usize = 100;
/// Deezer caps playlists at 10,000 songs; stop there even if "next" links keep coming.
const MAX_SONGS: usize = 10_000;

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Deezer reports errors as HTTP 200 with an "error" object.
fn check(v: Value) -> AppResult<Value> {
    match v.get("error") {
        None => Ok(v),
        Some(e) => {
            let code = e.get("code").and_then(Value::as_u64).unwrap_or(0);
            // 800 = "no data": the list is private, deleted or never existed.
            let kind = if code == 800 { "not_found" } else if code == 4 { "rate_limited" } else { "service_error" };
            Err(AppError::new(kind, format!("Deezer: {}", text(e, "message").unwrap_or("error"))))
        }
    }
}

pub fn map_track(t: &Value, album_cover: Option<&str>) -> Option<Track> {
    let id = t.get("id")?.as_u64()?.to_string();
    let album = t.get("album");
    let cover = album.and_then(|a| text(a, "cover_xl").or_else(|| text(a, "cover_big"))).or(album_cover).map(str::to_string);
    let mut track = Track::imported(
        DEEZER,
        &id,
        text(t, "title")?,
        vec![t.get("artist").and_then(|a| text(a, "name")).unwrap_or("").to_string()],
        t.get("duration").and_then(Value::as_u64).map(|s| s * 1000),
        cover,
        album.and_then(|a| text(a, "title")).map(str::to_string),
    );
    track.link = text(t, "link").filter(|l| l.starts_with("https://")).map(str::to_string);
    track.is_sane().then_some(track)
}

async fn get(http: &Http, path: &str) -> AppResult<Value> {
    check(http.get_json(&format!("{API}{path}")).await?)
}

pub async fn fetch(http: &Http, kind: &str, id: &str) -> AppResult<RemoteList> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::new("invalid_input", "bad Deezer id"));
    }
    let head = get(http, &format!("/{kind}/{id}")).await?;
    let mut list = RemoteList {
        origin: format!("{DEEZER}:{kind}:{id}"),
        service: DEEZER.into(),
        kind: kind.into(),
        title: text(&head, "title").unwrap_or("Deezer").into(),
        owner: String::new(),
        cover: None,
        tracks: Vec::new(),
        skipped: Vec::new(),
        truncated: false,
    };
    match kind {
        "track" => {
            let track = map_track(&head, None).ok_or_else(|| AppError::new("bad_response", "Deezer track data is incomplete"))?;
            list.owner = track.artist_line();
            list.cover = track.cover.clone();
            list.tracks.push(track);
        }
        "playlist" | "album" => {
            list.cover = text(&head, "picture_xl").or_else(|| text(&head, "cover_xl")).map(str::to_string);
            list.owner = head
                .get("creator")
                .or_else(|| head.get("artist"))
                .and_then(|o| text(o, "name"))
                .unwrap_or("")
                .to_string();
            let cover = list.cover.clone();
            let mut index = 0;
            loop {
                let page = get(http, &format!("/{kind}/{id}/tracks?index={index}&limit={PAGE}")).await?;
                let items = page.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
                for item in &items {
                    match map_track(item, cover.as_deref()) {
                        Some(t) => list.tracks.push(t),
                        None => list.skipped.push(super::Skipped {
                            title: text(item, "title").unwrap_or("?").into(),
                            artist: String::new(),
                            reason: "unavailable".into(),
                        }),
                    }
                }
                index += items.len();
                if items.is_empty() || page.get("next").is_none() || index >= MAX_SONGS {
                    break;
                }
            }
        }
        _ => return Err(AppError::new("invalid_input", "unknown Deezer link")),
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_track() {
        let v = json!({
            "id": 116348632, "title": "Hey Jude (Remastered 2015)", "duration": 429, "link": "https://www.deezer.com/track/116348632",
            "artist": {"name": "The Beatles"}, "album": {"title": "1", "cover_xl": "https://cdn-images.dzcdn.net/x/1000x1000.jpg"}
        });
        let t = map_track(&v, None).unwrap();
        assert_eq!(t.id, "deezer:116348632");
        assert_eq!(t.duration_ms, Some(429_000));
        assert_eq!(t.artist_line(), "The Beatles");
        assert_eq!(t.album.as_ref().unwrap().name, "1");
        assert!(t.cover.unwrap().ends_with("1000x1000.jpg"));
    }

    #[test]
    fn reads_error_objects() {
        let err = check(json!({"error": {"type": "DataException", "message": "no data", "code": 800}})).unwrap_err();
        assert_eq!(err.code, "not_found");
        assert!(check(json!({"id": 1})).is_ok());
    }
}
