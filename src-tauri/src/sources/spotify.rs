//! Spotify, import only. Lists are read from the public embed page
//! (open.spotify.com/embed/<kind>/<id>), which needs no key or login and lists up to 100 songs.
//! See FIXING.md when the embed page changes shape.

use super::{RemoteList, Track, SPOTIFY};
use crate::error::{AppError, AppResult};
use crate::http::Http;
use serde_json::Value;

/// The embed page stops after this many songs.
pub const EMBED_LIMIT: usize = 100;

fn embed_url(kind: &str, id: &str) -> String {
    format!("https://open.spotify.com/embed/{kind}/{id}")
}

/// The JSON state Next.js embeds into the page.
pub fn next_data(html: &str) -> AppResult<Value> {
    let start_tag = "<script id=\"__NEXT_DATA__\" type=\"application/json\">";
    let start = html.find(start_tag).ok_or_else(|| AppError::new("bad_response", "Spotify embed page has no data block"))? + start_tag.len();
    let end = html[start..].find("</script>").ok_or_else(|| AppError::new("bad_response", "Spotify data block is cut off"))?;
    Ok(serde_json::from_str(&html[start..start + end])?)
}

fn entity(data: &Value) -> AppResult<&Value> {
    data.pointer("/props/pageProps/state/data/entity").ok_or_else(|| AppError::new("not_found", "Spotify page has no playlist data (private or removed?)"))
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Largest image in either of Spotify's two image list shapes.
fn best_image(e: &Value) -> Option<String> {
    let lists = [e.pointer("/coverArt/sources"), e.pointer("/visualIdentity/image")];
    lists
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .max_by_key(|img| img.get("maxWidth").or_else(|| img.get("width")).and_then(Value::as_u64).unwrap_or(0))
        .and_then(|img| text(img, "url"))
        .map(str::to_string)
}

fn id_from_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("spotify:track:").filter(|id| id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// "Pitbull,\u{a0}Sensato" → ["Pitbull", "Sensato"]
fn split_artists(subtitle: &str) -> Vec<String> {
    subtitle.split(',').map(|a| a.trim_matches(|c: char| c.is_whitespace() || c == '\u{a0}').to_string()).filter(|a| !a.is_empty()).collect()
}

fn map_list_item(item: &Value, cover: Option<&String>, album: Option<&str>) -> Option<Track> {
    let id = id_from_uri(text(item, "uri")?)?;
    let mut t = Track::imported(
        SPOTIFY,
        id,
        text(item, "title")?,
        split_artists(text(item, "subtitle").unwrap_or("")),
        item.get("duration").and_then(Value::as_u64),
        cover.cloned(),
        album.map(str::to_string),
    );
    t.link = Some(format!("https://open.spotify.com/track/{id}"));
    t.is_sane().then_some(t)
}

fn map_single(e: &Value) -> Option<Track> {
    let id = id_from_uri(text(e, "uri")?)?;
    let artists = e.get("artists")?.as_array()?.iter().filter_map(|a| text(a, "name").map(str::to_string)).collect();
    let mut t = Track::imported(SPOTIFY, id, text(e, "name").or_else(|| text(e, "title"))?, artists, e.get("duration").and_then(Value::as_u64), best_image(e), None);
    t.link = Some(format!("https://open.spotify.com/track/{id}"));
    t.is_sane().then_some(t)
}

/// Turns the embed page's state into a list. Pure, so it's tested against real page shapes.
pub fn parse_embed(kind: &str, id: &str, data: &Value) -> AppResult<RemoteList> {
    let e = entity(data)?;
    let cover = best_image(e);
    let title = text(e, "name").or_else(|| text(e, "title")).unwrap_or("Spotify").to_string();
    let mut list = RemoteList {
        origin: format!("{SPOTIFY}:{kind}:{id}"),
        service: SPOTIFY.into(),
        kind: kind.into(),
        owner: text(e, "subtitle").unwrap_or("").replace('\u{a0}', " "),
        title,
        cover: cover.clone(),
        tracks: Vec::new(),
        skipped: Vec::new(),
        truncated: false,
    };
    if kind == "track" {
        let track = map_single(e).ok_or_else(|| AppError::new("bad_response", "Spotify track data is incomplete"))?;
        list.owner = track.artist_line();
        list.tracks.push(track);
        return Ok(list);
    }
    let items = e.get("trackList").and_then(Value::as_array).ok_or_else(|| AppError::new("bad_response", "Spotify list has no songs"))?;
    // Playlist songs carry no artwork of their own; album songs share the album's.
    let (track_cover, album) = if kind == "album" { (cover.as_ref(), Some(list.title.as_str())) } else { (None, None) };
    for item in items {
        match map_list_item(item, track_cover, album) {
            Some(t) => list.tracks.push(t),
            None => list.skipped.push(super::Skipped {
                title: text(item, "title").unwrap_or("?").into(),
                artist: text(item, "subtitle").unwrap_or("").replace('\u{a0}', " "),
                reason: "unavailable".into(),
            }),
        }
    }
    list.truncated = kind == "playlist" && items.len() >= EMBED_LIMIT;
    Ok(list)
}

pub async fn fetch(http: &Http, kind: &str, id: &str) -> AppResult<RemoteList> {
    let html = http.get_text(&embed_url(kind, id)).await?;
    parse_embed(kind, id, &next_data(&html)?)
}

/// One song by id, for Liked Songs pasted as links.
pub async fn track(http: &Http, id: &str) -> AppResult<Track> {
    let list = fetch(http, "track", id).await?;
    list.tracks.into_iter().next().ok_or_else(|| AppError::new("not_found", "Spotify track not found"))
}

/// Songs from the "Download your data" export (YourLibrary.json). It has no durations,
/// so matching relies on artist and title alone.
pub fn parse_your_library(json: &str) -> AppResult<Vec<Track>> {
    let v: Value = serde_json::from_str(json).map_err(|_| AppError::new("bad_file", "This isn't a valid YourLibrary.json file."))?;
    let items = v.get("tracks").and_then(Value::as_array).ok_or_else(|| AppError::new("bad_file", "YourLibrary.json has no \"tracks\" list."))?;
    Ok(items
        .iter()
        .filter_map(|t| {
            let id = id_from_uri(text(t, "uri")?)?;
            let mut track = Track::imported(SPOTIFY, id, text(t, "track")?, vec![text(t, "artist")?.to_string()], None, None, text(t, "album").map(str::to_string));
            track.link = Some(format!("https://open.spotify.com/track/{id}"));
            track.is_sane().then_some(track)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(entity: Value) -> Value {
        json!({"props": {"pageProps": {"state": {"data": {"entity": entity}}}}})
    }

    #[test]
    fn reads_data_block() {
        let html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"a":1}</script></html>"#;
        assert_eq!(next_data(html).unwrap()["a"], 1);
        assert!(next_data("<html></html>").is_err());
    }

    #[test]
    fn parses_album_with_shared_cover() {
        let data = page(json!({
            "type": "album", "name": "Global Warming", "subtitle": "Pitbull",
            "coverArt": {"sources": [{"url": "https://i.scdn.co/image/x", "width": 640}]},
            "trackList": [
                {"uri": "spotify:track:6OmhkSOpvYBokMKQxpIGx2", "title": "Global Warming (feat. Sensato)", "subtitle": "Pitbull,\u{a0}Sensato", "duration": 85400},
                {"uri": "spotify:local:x", "title": "Broken", "subtitle": "Nobody"}
            ]
        }));
        let list = parse_embed("album", "4aawyAB9vmqN3uQ7FjRGTy", &data).unwrap();
        assert_eq!(list.origin, "spotify:album:4aawyAB9vmqN3uQ7FjRGTy");
        assert_eq!(list.tracks.len(), 1);
        let t = &list.tracks[0];
        assert_eq!(t.artist_line(), "Pitbull, Sensato");
        assert_eq!(t.duration_ms, Some(85400));
        assert_eq!(t.cover.as_deref(), Some("https://i.scdn.co/image/x"));
        assert_eq!(list.skipped.len(), 1);
        assert!(!list.truncated);
    }

    #[test]
    fn marks_full_embed_playlist_as_cut_off() {
        let items: Vec<Value> = (0..100).map(|i| json!({"uri": format!("spotify:track:{:0>22}", i), "title": format!("Song {i}"), "subtitle": "A"})).collect();
        let list = parse_embed("playlist", "37i9dQZF1DXcBWIGoYBM5M", &page(json!({"name": "Big", "trackList": items}))).unwrap();
        assert_eq!(list.tracks.len(), 100);
        assert!(list.truncated);
        assert!(list.tracks[0].cover.is_none());
    }

    #[test]
    fn parses_single_track() {
        let data = page(json!({
            "type": "track", "name": "Stop The Wedding!", "uri": "spotify:track:3zOYmiNOuyDL0ClxJcEJ0T",
            "artists": [{"name": "Ashe"}], "duration": 198840,
            "visualIdentity": {"image": [{"url": "https://i.scdn.co/a", "maxWidth": 64}, {"url": "https://i.scdn.co/b", "maxWidth": 640}]}
        }));
        let list = parse_embed("track", "3zOYmiNOuyDL0ClxJcEJ0T", &data).unwrap();
        assert_eq!(list.tracks[0].title, "Stop The Wedding!");
        assert_eq!(list.tracks[0].cover.as_deref(), Some("https://i.scdn.co/b"));
    }

    #[test]
    fn parses_your_library_export() {
        let json = r#"{"tracks":[{"artist":"Ashe","album":"A","track":"Stop The Wedding!","uri":"spotify:track:3zOYmiNOuyDL0ClxJcEJ0T"},{"artist":"x","track":"y","uri":"bad"}],"albums":[]}"#;
        let tracks = parse_your_library(json).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "spotify:3zOYmiNOuyDL0ClxJcEJ0T");
        assert!(parse_your_library("{}").is_err());
        assert!(parse_your_library("nope").is_err());
    }
}
