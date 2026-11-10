//! Spotify, import only. Lists are read from the public embed page
//! (open.spotify.com/embed/<kind>/<id>), which needs no key or login and lists up to 100 songs.
//! Longer playlists are read in full with the anonymous token that same page carries, through
//! the query API Spotify's web player uses; if that fails, the first 100 songs are kept and the
//! list is marked as cut off. See FIXING.md when either changes shape.

use super::{RemoteList, Track, SPOTIFY};
use crate::error::{AppError, AppResult};
use crate::http::Http;
use serde_json::{json, Value};
use std::sync::Mutex;

/// The embed page stops after this many songs.
pub const EMBED_LIMIT: usize = 100;
const QUERY_URL: &str = "https://api-partner.spotify.com/pathfinder/v2/query";
const WEB_PLAYER: &str = "https://open.spotify.com/";
/// Songs per page of the full read (the most the web player asks for).
const PAGE: usize = 100;
/// A playlist this long is almost certainly bad data; stop instead of paging forever.
const MAX_SONGS: usize = 10_000;

/// The web player's query id for "fetchPlaylist", read from its script; kept until it stops working.
static QUERY_HASH: Mutex<Option<String>> = Mutex::new(None);

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
    let data = next_data(&html)?;
    let mut list = parse_embed(kind, id, &data)?;
    if list.truncated {
        let token = data.pointer("/props/pageProps/state/settings/session/accessToken").and_then(Value::as_str);
        match token {
            Some(token) => match full_playlist(http, id, token).await {
                Ok((tracks, skipped)) if tracks.len() >= list.tracks.len() => {
                    list.tracks = tracks;
                    list.skipped = skipped;
                    list.truncated = false;
                }
                Ok(_) => eprintln!("Spotify full read of {id} returned fewer songs than the embed page; keeping 100"),
                Err(e) => eprintln!("Spotify full read of {id} failed, keeping the first 100: {e}"),
            },
            None => eprintln!("Spotify embed page carries no token; keeping the first 100"),
        }
    }
    Ok(list)
}

/// The web player's script names its queries next to their ids:
/// `"fetchPlaylist","query","<64 hex>"`.
pub fn find_query_hash(script: &str) -> Option<String> {
    let marker = "\"fetchPlaylist\",\"query\",\"";
    let start = script.find(marker)? + marker.len();
    let hash = script.get(start..start + 64)?;
    hash.chars().all(|c| c.is_ascii_hexdigit()).then(|| hash.to_lowercase())
}

/// The web player's main script in its page: the quoted `.../web-player/web-player.<hash>.js`
/// (the page also links a stylesheet of the same name).
pub fn find_player_script(page: &str) -> Option<&str> {
    page.split('"').find(|s| s.starts_with("https://") && s.contains("/web-player/web-player.") && s.ends_with(".js"))
}

/// Reads the query id from the current web player script.
async fn load_query_hash(http: &Http) -> AppResult<String> {
    let page = http.get_text(WEB_PLAYER).await?;
    let url = find_player_script(&page).ok_or_else(|| AppError::new("bad_response", "Spotify web player script not found"))?;
    let script = http.get_text(url).await?;
    find_query_hash(&script).ok_or_else(|| AppError::new("bad_response", "Spotify playlist query id not found"))
}

async fn query_hash(http: &Http, fresh: bool) -> AppResult<String> {
    if !fresh {
        if let Some(h) = QUERY_HASH.lock().unwrap_or_else(|p| p.into_inner()).clone() {
            return Ok(h);
        }
    }
    let h = load_query_hash(http).await?;
    *QUERY_HASH.lock().unwrap_or_else(|p| p.into_inner()) = Some(h.clone());
    Ok(h)
}

/// One page of a playlist from the web player's query API.
async fn playlist_page(http: &Http, id: &str, token: &str, hash: &str, offset: usize) -> AppResult<Value> {
    let body = json!({
        "operationName": "fetchPlaylist",
        "variables": { "uri": format!("spotify:playlist:{id}"), "offset": offset, "limit": PAGE, "enableWatchFeedEntrypoint": false },
        "extensions": { "persistedQuery": { "version": 1, "sha256Hash": hash } },
    });
    let resp = http.send(QUERY_URL, |c| c.post(QUERY_URL).bearer_auth(token).json(&body)).await?;
    // 412: the query id changed with a new web player.
    if resp.status().as_u16() == 412 {
        return Err(AppError::new("stale_query", "Spotify changed its playlist query"));
    }
    crate::http::check_status(&resp)?;
    Ok(resp.json().await?)
}

/// Every song of a playlist, page by page. A changed query id is looked up again once.
async fn full_playlist(http: &Http, id: &str, token: &str) -> AppResult<(Vec<Track>, Vec<super::Skipped>)> {
    let mut hash = query_hash(http, false).await?;
    let (mut tracks, mut skipped) = (Vec::new(), Vec::new());
    let mut offset = 0;
    let mut refreshed = false;
    loop {
        let page = match playlist_page(http, id, token, &hash, offset).await {
            Err(e) if e.code == "stale_query" && !refreshed => {
                refreshed = true;
                hash = query_hash(http, true).await?;
                continue;
            }
            other => other?,
        };
        let (total, count) = parse_page(&page, &mut tracks, &mut skipped)?;
        offset += count;
        if count == 0 || offset >= total || offset >= MAX_SONGS {
            return Ok((tracks, skipped));
        }
    }
}

/// Adds one page's songs; returns the playlist's total and how many items the page had.
/// Pure, so it's tested against the real response shape.
pub fn parse_page(page: &Value, tracks: &mut Vec<Track>, skipped: &mut Vec<super::Skipped>) -> AppResult<(usize, usize)> {
    let content = page
        .pointer("/data/playlistV2/content")
        .ok_or_else(|| AppError::new("bad_response", "Spotify playlist page has no content"))?;
    let total = content.get("totalCount").and_then(Value::as_u64).unwrap_or(0) as usize;
    let items = content.get("items").and_then(Value::as_array).ok_or_else(|| AppError::new("bad_response", "Spotify playlist page has no items"))?;
    for item in items {
        let data = item.pointer("/itemV2/data");
        match data.and_then(map_query_track) {
            Some(t) => tracks.push(t),
            None => skipped.push(super::Skipped {
                title: data.and_then(|d| text(d, "name")).unwrap_or("?").into(),
                artist: String::new(),
                reason: "unavailable".into(),
            }),
        }
    }
    Ok((total, items.len()))
}

fn map_query_track(d: &Value) -> Option<Track> {
    if d.get("__typename").and_then(Value::as_str) != Some("Track") {
        return None;
    }
    let id = id_from_uri(text(d, "uri")?)?;
    let artists = d.pointer("/artists/items")?.as_array()?.iter().filter_map(|a| a.pointer("/profile/name").and_then(Value::as_str)).map(str::to_string).collect();
    let album = d.get("albumOfTrack");
    let mut t = Track::imported(
        SPOTIFY,
        id,
        text(d, "name")?,
        artists,
        d.pointer("/trackDuration/totalMilliseconds").and_then(Value::as_u64),
        album.and_then(best_image),
        album.and_then(|a| text(a, "name")).map(str::to_string),
    );
    t.link = Some(format!("https://open.spotify.com/track/{id}"));
    t.is_sane().then_some(t)
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
    fn finds_the_playlist_query_id() {
        let h = "21ea11a6ac75a5f3e01fd365d29c18e0a4a8b7dc09f89bbb5d73910190c6acaf";
        let script = format!(r#"x("fetchPlaylistMetadata","query","{}"),y("fetchPlaylist","query","{h}")"#, "a".repeat(64));
        assert_eq!(find_query_hash(&script).as_deref(), Some(h));
        assert_eq!(find_query_hash(r#"("fetchPlaylist","query","nothex")"#), None);
        let page = r#"<link href="https://cdn/web-player/web-player.ab.css"/><script src="https://cdn/web-player/vendor~web-player.cd.js"></script><script src="https://cdn/web-player/web-player.ab.js"></script>"#;
        assert_eq!(find_player_script(page), Some("https://cdn/web-player/web-player.ab.js"));
    }

    #[test]
    fn reads_a_full_playlist_page() {
        let page = json!({"data": {"playlistV2": {"content": {"totalCount": 150, "items": [
            {"itemV2": {"__typename": "TrackResponseWrapper", "data": {
                "__typename": "Track", "uri": "spotify:track:3zOYmiNOuyDL0ClxJcEJ0T", "name": "Stop The Wedding!",
                "artists": {"items": [{"profile": {"name": "Ashe"}}]},
                "trackDuration": {"totalMilliseconds": 198840},
                "albumOfTrack": {"name": "Stop The Wedding!", "coverArt": {"sources": [
                    {"url": "https://i.scdn.co/image/small", "width": 64}, {"url": "https://i.scdn.co/image/big", "width": 640}]}}
            }}},
            {"itemV2": {"__typename": "EpisodeResponseWrapper", "data": {"__typename": "Episode", "name": "A podcast"}}}
        ]}}}});
        let (mut tracks, mut skipped) = (Vec::new(), Vec::new());
        assert_eq!(parse_page(&page, &mut tracks, &mut skipped).unwrap(), (150, 2));
        assert_eq!(tracks.len(), 1);
        let t = &tracks[0];
        assert_eq!(t.artist_line(), "Ashe");
        assert_eq!(t.duration_ms, Some(198840));
        assert_eq!(t.cover.as_deref(), Some("https://i.scdn.co/image/big"));
        assert_eq!(t.album.as_ref().map(|a| a.name.as_str()), Some("Stop The Wedding!"));
        assert_eq!(skipped[0].title, "A podcast");
        assert!(parse_page(&json!({"data": {}}), &mut tracks, &mut skipped).is_err());
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
