//! Apple Music, import only. Reads the song list from the public web page's embedded JSON
//! ("serialized-server-data"), which needs no key or account. See FIXING.md.

use super::links::Kind;
use super::{RemoteList, Track, APPLE};
use crate::error::{AppError, AppResult};
use crate::http::Http;
use serde_json::Value;

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

pub fn page_data(html: &str) -> AppResult<Value> {
    let marker = "id=\"serialized-server-data\">";
    let start = html.find(marker).ok_or_else(|| AppError::new("not_found", "Apple Music page has no song data (private or removed?)"))? + marker.len();
    let end = html[start..].find("</script>").ok_or_else(|| AppError::new("bad_response", "Apple Music data is cut off"))?;
    Ok(serde_json::from_str(&html[start..start + end])?)
}

/// Artwork URLs are templates like ".../{w}x{h}bb.{f}".
fn artwork(v: &Value) -> Option<String> {
    let url = v.pointer("/artwork/dictionary/url").and_then(Value::as_str)?;
    url.starts_with("https://").then(|| url.replace("{w}x{h}bb.{f}", "600x600bb.jpg").replace("{w}", "600").replace("{h}", "600").replace("{f}", "jpg"))
}

fn song_id(v: &Value) -> Option<&str> {
    v.pointer("/contentDescriptor/identifiers/storeAdamID").and_then(Value::as_str).filter(|id| id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty())
}

fn artists(v: &Value) -> Vec<String> {
    let linked: Vec<String> =
        v.get("subtitleLinks").and_then(Value::as_array).into_iter().flatten().filter_map(|l| text(l, "title").map(str::to_string)).collect();
    if !linked.is_empty() {
        return linked;
    }
    text(v, "artistName").or_else(|| text(v, "artists")).map(|a| vec![a.to_string()]).unwrap_or_default()
}

fn map_lockup(item: &Value, fallback_cover: Option<&String>, album: Option<&str>) -> Option<Track> {
    let id = song_id(item)?;
    let mut t = Track::imported(
        APPLE,
        id,
        text(item, "title")?,
        artists(item),
        item.get("duration").and_then(Value::as_u64),
        artwork(item).or_else(|| fallback_cover.cloned()),
        album.map(str::to_string),
    );
    t.link = item.pointer("/contentDescriptor/url").and_then(Value::as_str).filter(|u| u.starts_with("https://")).map(str::to_string);
    t.is_sane().then_some(t)
}

/// Leading number of "50 songs, 2 hours 48 minutes" (in any language).
fn declared_count(sections: &[Value]) -> Option<usize> {
    sections
        .iter()
        .filter(|s| s.get("itemKind").and_then(Value::as_str) == Some("containerDetailTracklistFooterLockup"))
        .flat_map(|s| s.get("items").and_then(Value::as_array).into_iter().flatten())
        .find_map(|i| text(i, "description"))
        .and_then(|d| d.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.').filter(char::is_ascii_digit).collect::<String>().parse().ok())
}

pub fn parse_page(kind: Kind, id: &str, data: &Value) -> AppResult<RemoteList> {
    let sections = data
        .pointer("/data/0/data/sections")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::new("bad_response", "Apple Music page layout changed"))?;
    let of_kind = |k: &'static str| sections.iter().filter(move |s| s.get("itemKind").and_then(Value::as_str) == Some(k)).flat_map(|s| s.get("items").and_then(Value::as_array).into_iter().flatten());
    let header = of_kind("containerDetailHeaderLockup").next().or_else(|| of_kind("songDetailHeader").next());
    let header = header.ok_or_else(|| AppError::new("not_found", "Apple Music page has no songs (private or removed?)"))?;
    let cover = artwork(header);
    let title = text(header, "title").unwrap_or("Apple Music").to_string();
    let mut list = RemoteList {
        origin: format!("{APPLE}:{}:{id}", kind.as_str()),
        service: APPLE.into(),
        kind: kind.as_str().into(),
        owner: artists(header).join(", "),
        title: title.clone(),
        cover: cover.clone(),
        tracks: Vec::new(),
        skipped: Vec::new(),
        truncated: false,
    };
    let album = (kind == Kind::Album).then_some(title.as_str());
    let lockups: Vec<&Value> = of_kind("trackLockup").collect();
    for item in &lockups {
        if kind == Kind::Track && song_id(item) != Some(id) {
            continue;
        }
        match map_lockup(item, cover.as_ref(), album) {
            Some(t) => list.tracks.push(t),
            None => list.skipped.push(super::Skipped { title: text(item, "title").unwrap_or("?").into(), artist: artists(item).join(", "), reason: "unavailable".into() }),
        }
    }
    if kind == Kind::Track && list.tracks.is_empty() {
        // Song pages describe the song in their header instead of a track list.
        let mut t = Track::imported(APPLE, id, &title, artists(header), None, cover, text(header, "album").map(str::to_string));
        t.link = Some(format!("https://music.apple.com/song/{id}"));
        if t.is_sane() {
            list.tracks.push(t);
        }
    }
    if list.tracks.is_empty() && list.skipped.is_empty() {
        return Err(AppError::new("not_found", "Apple Music list is empty or private"));
    }
    list.truncated = declared_count(sections).is_some_and(|n| n > lockups.len());
    Ok(list)
}

pub async fn fetch(http: &Http, kind: Kind, id: &str, url: &str) -> AppResult<RemoteList> {
    if !url.starts_with("https://music.apple.com/") && !url.starts_with("https://itunes.apple.com/") {
        return Err(AppError::new("invalid_input", "not an Apple Music link"));
    }
    let html = http.get_text(url).await?;
    parse_page(kind, id, &page_data(&html)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(sections: Value) -> Value {
        json!({"data": [{"intent": {}, "data": {"sections": sections}}]})
    }

    fn lockup(id: &str, title: &str) -> Value {
        json!({
            "title": title, "duration": 232226, "artistName": "Ella Langley",
            "contentDescriptor": {"kind": "song", "identifiers": {"storeAdamID": id}, "url": format!("https://music.apple.com/us/album/x/1?i={id}")},
            "subtitleLinks": [{"title": "Ella Langley"}]
        })
    }

    #[test]
    fn parses_playlist_and_notices_cut_off() {
        let data = page(json!([
            {"itemKind": "containerDetailHeaderLockup", "items": [{"title": "Today’s Hits", "subtitleLinks": [{"title": "Apple Music Hits"}],
                "artwork": {"dictionary": {"url": "https://is1-ssl.mzstatic.com/a/{w}x{h}bb.{f}"}}}]},
            {"itemKind": "trackLockup", "items": [lockup("1844932150", "Choosin' Texas"), lockup("2", "Other")]},
            {"itemKind": "containerDetailTracklistFooterLockup", "items": [{"description": "150 songs, 9 hours"}]}
        ]));
        let list = parse_page(Kind::Playlist, "pl.abc", &data).unwrap();
        assert_eq!(list.title, "Today’s Hits");
        assert_eq!(list.tracks.len(), 2);
        assert_eq!(list.tracks[0].duration_ms, Some(232226));
        assert_eq!(list.cover.as_deref(), Some("https://is1-ssl.mzstatic.com/a/600x600bb.jpg"));
        assert!(list.truncated);
    }

    #[test]
    fn picks_one_song_from_album_page() {
        let data = page(json!([
            {"itemKind": "containerDetailHeaderLockup", "items": [{"title": "Album"}]},
            {"itemKind": "trackLockup", "items": [lockup("1", "One"), lockup("2", "Two")]}
        ]));
        let list = parse_page(Kind::Track, "2", &data).unwrap();
        assert_eq!(list.tracks.len(), 1);
        assert_eq!(list.tracks[0].title, "Two");
    }

    #[test]
    fn reads_song_page_header() {
        let data = page(json!([{"itemKind": "songDetailHeader", "items": [{"title": "Choosin' Texas", "artists": "Ella Langley", "album": "Single"}]}]));
        let list = parse_page(Kind::Track, "1844932150", &data).unwrap();
        assert_eq!(list.tracks[0].artist_line(), "Ella Langley");
    }

    #[test]
    fn private_page_is_an_error() {
        assert_eq!(page_data("<html></html>").unwrap_err().code, "not_found");
        assert!(parse_page(Kind::Playlist, "pl.x", &page(json!([]))).is_err());
    }
}
