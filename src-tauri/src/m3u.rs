//! M3U/M3U8 playlists: export any playlist (links for streamed songs, file paths for Local
//! music) and import files made by other players. Imported local files are copied into the
//! Local music folder like any other added file.

use crate::error::{AppError, AppResult};
use crate::sources::links::{self, Kind, Link};
use crate::sources::{deezer, spotify, RemoteList, Skipped, Track, APPLE, DEEZER, LOCAL, SOUNDCLOUD, SPOTIFY, YOUTUBE};
use crate::{local, AppState};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_BYTES: u64 = 4 * 1024 * 1024;

pub fn is_playlist_path(s: &str) -> bool {
    let lower = s.trim().to_lowercase();
    (lower.ends_with(".m3u") || lower.ends_with(".m3u8")) && !lower.starts_with("http")
}

/// One entry: the "#EXTINF" info (seconds, "Artist - Title") and the location line.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub secs: Option<u64>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub location: String,
}

pub fn parse(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut info: Option<(Option<u64>, Option<String>, Option<String>)> = None;
    for line in text.lines().map(|l| l.trim().trim_start_matches('\u{feff}')) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (dur, name) = rest.split_once(',').unwrap_or((rest, ""));
            let secs = dur.split_whitespace().next().and_then(|d| d.parse::<i64>().ok()).filter(|d| *d > 0).map(|d| d as u64);
            let (artist, title) = match local::names_from_file(name.trim()) {
                (a, t) if !t.is_empty() => (a, Some(t)),
                _ => (None, None),
            };
            info = Some((secs, artist, title));
        } else if !line.is_empty() && !line.starts_with('#') {
            let (secs, artist, title) = info.take().unwrap_or((None, None, None));
            out.push(Entry { secs, artist, title, location: line.to_string() });
        }
    }
    out
}

pub fn write(state: &AppState, tracks: &[Track]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for t in tracks {
        let location = if t.source == LOCAL { local::path_for(state, &t.source_id).ok().map(|p| p.to_string_lossy().into_owned()) } else { t.link.clone() };
        let Some(location) = location else { continue };
        let secs = t.duration_ms.map(|d| (d / 1000) as i64).unwrap_or(-1);
        let artist = t.artist_line();
        let name = if artist.is_empty() { t.title.clone() } else { format!("{artist} - {}", t.title) };
        out.push_str(&format!("#EXTINF:{secs},{}\n{location}\n", name.replace(['\n', '\r'], " ")));
    }
    out
}

pub async fn read(state: &Arc<AppState>, path: &Path) -> AppResult<RemoteList> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| AppError::new("not_found", "playlist file not found"))?;
    if meta.len() > MAX_BYTES {
        return Err(AppError::new("bad_file", "playlist file is too large"));
    }
    let bytes = tokio::fs::read(path).await?;
    let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let folder = state.local.folder();
    let mut list = RemoteList {
        origin: format!("m3u:{}", path.to_string_lossy().to_lowercase()),
        service: "m3u".into(),
        kind: "playlist".into(),
        title: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Playlist".into()),
        owner: String::new(),
        cover: None,
        tracks: Vec::new(),
        skipped: Vec::new(),
        truncated: false,
    };
    let mut local_ids = Vec::new();
    for e in parse(&text) {
        let label = || Skipped { title: e.title.clone().unwrap_or_else(|| e.location.clone()), artist: e.artist.clone().unwrap_or_default(), reason: "unavailable".into() };
        if e.location.starts_with("http") {
            match link_track(state, &e).await {
                Ok(t) => list.tracks.push(t),
                Err(_) => list.skipped.push(label()),
            }
            continue;
        }
        let file = if Path::new(&e.location).is_absolute() { PathBuf::from(&e.location) } else { base.join(&e.location) };
        if !file.is_file() || !local::is_audio(&file) {
            list.skipped.push(label());
            continue;
        }
        let in_folder = if file.starts_with(&folder) { Some(file.clone()) } else { copy_in(&file, &folder).await.ok() };
        match in_folder.and_then(|p| p.strip_prefix(&folder).ok().map(|r| local::id_for(&r.to_string_lossy()))) {
            Some(id) => {
                local_ids.push((list.tracks.len(), id));
                // Placeholder until the scan below has read the file's tags.
                list.tracks.push(Track::imported(LOCAL, "pending", e.title.as_deref().unwrap_or("?"), vec![], None, None, None));
            }
            None => list.skipped.push(label()),
        }
    }
    if !local_ids.is_empty() {
        local::scan(state).await?;
        for (i, id) in local_ids.into_iter().rev() {
            match state.db.track(&Track::make_id(LOCAL, &id))? {
                Some(t) => list.tracks[i] = t,
                None => {
                    list.tracks.remove(i);
                }
            }
        }
    }
    if list.tracks.is_empty() && list.skipped.is_empty() {
        return Err(AppError::new("empty_list", "no songs in the file"));
    }
    Ok(list)
}

async fn copy_in(file: &Path, folder: &Path) -> AppResult<PathBuf> {
    tokio::fs::create_dir_all(folder).await?;
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "song".into());
    let ext = file.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut dest = folder.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while dest.exists() {
        dest = folder.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    tokio::fs::copy(file, &dest).await?;
    Ok(dest)
}

/// A song from a link line. Songs are built from the #EXTINF info when present, so a long
/// list doesn't need one request per song (and Relay's own exports always have it).
async fn link_track(state: &Arc<AppState>, e: &Entry) -> AppResult<Track> {
    let Some(Link::Target { service, kind: Kind::Track, id, url }) = links::parse(&e.location) else {
        return Err(AppError::new("unsupported_link", "not a song link"));
    };
    if let Some(t) = from_info(service, &id, &url, e) {
        return Ok(t);
    }
    match service {
        YOUTUBE => first(state.yt.import("track", &id).await?),
        SOUNDCLOUD => first(state.sc.import(&url).await?),
        SPOTIFY => spotify::track(&state.http, &id).await,
        DEEZER => first(deezer::fetch(&state.http, "track", &id).await?),
        _ => Err(AppError::new("unsupported_link", "unsupported link")),
    }
}

/// The song as the file describes it. Imported services' songs are matched to something
/// playable later, like any other import.
fn from_info(service: &str, id: &str, url: &str, e: &Entry) -> Option<Track> {
    let title = e.title.as_deref()?;
    // Exports join several artists with ", " (see `write`).
    let artists: Vec<String> = e.artist.iter().flat_map(|a| a.split(", ")).map(str::to_string).collect();
    let ms = e.secs.map(|s| s * 1000);
    let mut t = match service {
        YOUTUBE => Track::imported(YOUTUBE, id, title, artists, ms, Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")), None),
        SPOTIFY | APPLE | DEEZER => Track::imported(service, id, title, artists, ms, None, None),
        _ => return None,
    };
    t.link = Some(if service == YOUTUBE { format!("https://music.youtube.com/watch?v={id}") } else { url.to_string() });
    Some(t)
}

fn first(list: RemoteList) -> AppResult<Track> {
    list.tracks.into_iter().next().ok_or_else(|| AppError::new("not_found", "song not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extended_m3u() {
        let text = "\u{feff}#EXTM3U\n#EXTINF:209,LONOWN - addiction\nhttps://music.youtube.com/watch?v=abcdefghijk\n\nmusic\\song.mp3\n#EXTINF:-1,Just A Title\nC:\\x.flac\n";
        let e = parse(text);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0], Entry { secs: Some(209), artist: Some("LONOWN".into()), title: Some("addiction".into()), location: "https://music.youtube.com/watch?v=abcdefghijk".into() });
        assert_eq!(e[1].title, None);
        assert_eq!(e[2].secs, None);
        assert_eq!(e[2].title.as_deref(), Some("Just A Title"));
    }

    #[test]
    fn songs_from_other_services_come_back_from_an_export() {
        let text = "#EXTM3U
#EXTINF:144,Drake, Don Toliver - Solar Eclipse
https://music.apple.com/us/album/x/1852536368?i=1852536738
";
        let e = &parse(text)[0];
        let Some(Link::Target { service, id, url, .. }) = links::parse(&e.location) else { panic!("link not read") };
        let t = from_info(service, &id, &url, e).expect("built from the #EXTINF line");
        assert_eq!((t.source.as_str(), t.source_id.as_str(), t.title.as_str()), (APPLE, "1852536738", "Solar Eclipse"));
        assert_eq!(t.artist_line(), "Drake, Don Toliver");
        assert_eq!(t.duration_ms, Some(144_000));
        assert!(from_info(SOUNDCLOUD, "a/b", "https://soundcloud.com/a/b", e).is_none());
    }

    #[test]
    fn recognizes_playlist_paths() {
        assert!(is_playlist_path("C:\\Music\\list.M3U8"));
        assert!(!is_playlist_path("https://x.test/list.m3u8"));
        assert!(!is_playlist_path("C:\\song.mp3"));
    }
}
