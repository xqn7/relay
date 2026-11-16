//! Shareable playlist links: `https://xqn7.github.io/relay/open/#<data>` (clickable in chat apps;
//! the page forwards to `relay://playlist/<data>`) carries the playlist itself (name, and
//! each song's source, id, title, artist and length), compressed, so no server is involved.
//! Opening one imports it like any other playlist link: songs from Spotify, Deezer or Apple
//! are matched again on the friend's PC. Windows opens the links with Relay because Relay
//! registers the scheme for the signed-in user at start.

use crate::error::{AppError, AppResult};
use crate::sources::{is_import_only, ArtistRef, RemoteList, Track, LOCAL, SOUNDCLOUD, YOUTUBE};
use serde_json::{json, Value};

/// Songs a link holds at most: Windows passes links of up to about 32,000 characters.
pub const MAX_SONGS: usize = 1500;
/// The unpacked data is never allowed to grow past this, whatever the link says.
const MAX_DATA_BYTES: usize = 2 * 1024 * 1024;
const FORMAT: u64 = 1;

/// "relay", or "relay-dev" for Relay Dev, so the two don't open each other's links.
pub fn scheme() -> &'static str {
    if crate::is_dev() {
        "relay-dev"
    } else {
        "relay"
    }
}

/// The page that turns a clickable https link into a relay:// one (docs/open in the repo, on
/// GitHub Pages). Chat apps like Discord only make https links clickable.
const PAGE: &str = "https://xqn7.github.io/relay/open/#";

pub fn is_share_link(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("relay://playlist/") || t.starts_with("relay-dev://playlist/") || t.starts_with(PAGE)
}

/// The packed playlist inside any form of the link.
fn data_of(link: &str) -> Option<&str> {
    let t = link.trim();
    match t.strip_prefix(PAGE) {
        Some(rest) => Some(rest.strip_prefix("dev:").unwrap_or(rest)),
        None => t.split_once("://playlist/").map(|(_, d)| d.trim_end_matches('/')),
    }
}

/// The link for a playlist. Songs only on this PC (local files) can't travel and are left out.
pub fn build(name: &str, tracks: &[Track]) -> AppResult<(String, usize)> {
    let songs: Vec<Value> = tracks
        .iter()
        .filter(|t| t.source != LOCAL)
        .take(MAX_SONGS)
        .map(|t| json!([t.source, t.source_id, t.title, t.artist_line(), t.duration_ms.map(|ms| ms / 1000)]))
        .collect();
    let count = songs.len();
    let data = serde_json::to_vec(&json!({ "v": FORMAT, "n": name, "t": songs }))?;
    let packed = miniz_oxide::deflate::compress_to_vec(&data, 9);
    let prefix = if crate::is_dev() { "dev:" } else { "" };
    Ok((format!("{PAGE}{prefix}{}", base64url(&packed)), count))
}

/// A shared playlist as a list to import.
pub fn parse(link: &str) -> AppResult<RemoteList> {
    let bad = || AppError::new("bad_file", "not a Relay playlist link");
    let data = data_of(link).ok_or_else(bad)?;
    let packed = unbase64url(data).ok_or_else(bad)?;
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(&packed, MAX_DATA_BYTES).map_err(|_| bad())?;
    let v: Value = serde_json::from_slice(&raw).map_err(|_| bad())?;
    if v["v"].as_u64() != Some(FORMAT) {
        return Err(AppError::new("bad_file", "link from a newer Relay"));
    }
    let name: String = v["n"].as_str().unwrap_or("Shared playlist").trim().chars().take(200).collect();
    let tracks: Vec<Track> = v["t"].as_array().ok_or_else(bad)?.iter().take(MAX_SONGS).filter_map(song).collect();
    if tracks.is_empty() {
        return Err(AppError::new("empty_list", "the link has no songs"));
    }
    // The same link always imports into the same playlist, like re-importing any other link.
    let digest = <sha2::Sha256 as sha2::Digest>::digest(data.as_bytes());
    let origin = format!("relay:playlist:{}", digest.iter().take(8).map(|b| format!("{b:02x}")).collect::<String>());
    Ok(RemoteList {
        origin,
        service: "relay".into(),
        kind: "playlist".into(),
        title: if name.is_empty() { "Shared playlist".into() } else { name },
        owner: String::new(),
        cover: tracks.first().and_then(|t| t.cover.clone()),
        tracks,
        skipped: Vec::new(),
        truncated: false,
    })
}

/// One song from the link; anything malformed is left out.
fn song(v: &Value) -> Option<Track> {
    let field = |i: usize| v.get(i).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty() && s.len() <= 300);
    let (source, id, title) = (field(0)?, field(1)?, field(2)?);
    let artist = field(3).unwrap_or("");
    let duration_ms = v.get(4).and_then(Value::as_u64).filter(|s| *s < 24 * 3600).map(|s| s * 1000);
    let track = if is_import_only(source) {
        Track::imported(source, id, title, vec![artist.to_string()], duration_ms, None, None)
    } else if source == YOUTUBE || source == SOUNDCLOUD {
        Track {
            id: Track::make_id(source, id),
            source: source.into(),
            source_id: id.into(),
            title: title.into(),
            artists: crate::sources::split_credit(artist).into_iter().filter(|a| !a.is_empty()).map(|name| ArtistRef { id: None, name }).collect(),
            album: None,
            duration_ms,
            cover: (source == YOUTUBE).then(|| format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
            is_video: false,
            link: Some(if source == YOUTUBE { format!("https://music.youtube.com/watch?v={id}") } else { format!("https://api.soundcloud.com/tracks/{id}") }),
            via: None,
            listens: None,
        }
    } else {
        return None;
    };
    track.is_sane().then_some(track)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// URL-safe base64 without padding (a link must not need escaping).
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 3);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn unbase64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = ALPHABET.iter().position(|a| *a == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yt(id: &str, title: &str) -> Track {
        let mut t = Track::imported("x", "1", title, vec!["Kolja Goldstein".into()], Some(200_000), None, None);
        t.id = Track::make_id(YOUTUBE, id);
        t.source = YOUTUBE.into();
        t.source_id = id.into();
        t
    }

    #[test]
    fn a_link_carries_the_playlist_there_and_back() {
        let spotify = Track::imported("spotify", "4cOdK2wGLETKBW3PvgPWqT", "XXX", vec!["Lacazette".into()], Some(207_000), None, None);
        let tracks = vec![yt("dQw4w9WgXcQ", "Song A"), spotify];
        let (link, count) = build("Rap", &tracks).unwrap();
        assert_eq!(count, 2);
        assert!(link.starts_with(PAGE));
        assert!(link.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_:/.#".contains(&b)), "nothing to escape");
        let list = parse(&link).unwrap();
        let data = link.strip_prefix(PAGE).unwrap();
        assert_eq!(parse(&format!("relay://playlist/{data}")).unwrap().tracks.len(), 2, "the relay:// form reads the same");
        assert_eq!(list.title, "Rap");
        assert_eq!(list.tracks.len(), 2);
        assert_eq!(list.tracks[0].id, "youtube:dQw4w9WgXcQ");
        assert_eq!(list.tracks[1].source, "spotify");
        assert_eq!(list.tracks[1].duration_ms, Some(207_000));
    }

    #[test]
    fn bad_links_are_refused() {
        assert!(parse("relay://playlist/!!!").is_err());
        assert!(parse("relay://playlist/AAAA").is_err());
        assert_eq!(unbase64url(&base64url(b"any bytes \x00\xff")).unwrap(), b"any bytes \x00\xff");
    }
}
