//! Playlist and Liked Songs imports. A link is read once for the preview (kept briefly so the
//! import doesn't fetch it again), then an import job matches the songs in the background with
//! progress events and can be cancelled at any point before it saves.

use crate::error::{AppError, AppResult};
use crate::library;
use crate::matching;
use crate::sources::links::{self, Link};
use crate::sources::{apple, deezer, is_import_only, spotify, RemoteList, Skipped, Track, APPLE, DEEZER, SOUNDCLOUD, SPOTIFY, YOUTUBE};
use crate::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::Semaphore;

const PREVIEW_TTL: Duration = Duration::from_secs(15 * 60);
/// Songs matched at the same time: quick enough for big playlists, polite to YouTube.
const MATCH_PARALLEL: usize = 4;
/// Pasted Liked Songs links read at the same time.
const READ_PARALLEL: usize = 4;
const MAX_LIKED_LINKS: usize = 10_000;

#[derive(Default)]
pub struct Imports {
    previews: Mutex<HashMap<String, (Instant, RemoteList)>>,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

pub enum Target {
    /// Create or update the playlist for this list.
    Playlist(RemoteList, String),
    /// Add the songs to Liked Songs.
    Likes(LikedSource),
}

pub enum LikedSource {
    SpotifyIds(Vec<String>),
    Tracks(Vec<Track>),
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    job_id: String,
    /// "reading" or "matching"
    phase: &'static str,
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    pub job_id: String,
    pub ok: bool,
    pub cancelled: bool,
    pub error: Option<AppError>,
    pub playlist_id: Option<String>,
    pub title: String,
    pub total: usize,
    pub added: usize,
    pub removed: usize,
    pub updated: bool,
    pub unmatched: Vec<Track>,
    pub skipped: Vec<Skipped>,
    pub truncated: bool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Follows a share link to the page it points to and reads the real link from there.
async fn expand_short(state: &Arc<AppState>, service: &'static str, url: &str) -> AppResult<Link> {
    let resp = state.http.send(url, |c| c.get(url)).await?;
    let final_url = resp.url().to_string();
    if let Some(link @ Link::Target { .. }) = links::parse(&final_url) {
        return Ok(link);
    }
    let html = resp.text().await.unwrap_or_default();
    links::find_in_html(&html, service).ok_or_else(|| AppError::new("unsupported_link", "short link leads nowhere readable"))
}

/// Reads any supported link. Errors carry codes the UI explains ("unsupported_link",
/// "not_found" for private/deleted lists, network codes).
pub async fn read(state: &Arc<AppState>, url: &str) -> AppResult<RemoteList> {
    let url = url.trim();
    if url.is_empty() || url.len() > 2048 {
        return Err(AppError::new("unsupported_link", "empty or too long"));
    }
    if let Some((at, list)) = lock(&state.imports.previews).get(url) {
        if at.elapsed() < PREVIEW_TTL {
            return Ok(list.clone());
        }
    }
    if crate::sharelink::is_share_link(url) {
        let list = crate::sharelink::parse(url)?;
        lock(&state.imports.previews).insert(url.to_string(), (Instant::now(), list.clone()));
        return Ok(list);
    }
    if crate::m3u::is_playlist_path(url) {
        let list = crate::m3u::read(state, std::path::Path::new(url)).await?;
        lock(&state.imports.previews).insert(url.to_string(), (Instant::now(), list.clone()));
        return Ok(list);
    }
    let mut link = links::parse(url).ok_or_else(|| AppError::new("unsupported_link", "not a supported link"))?;
    if let Link::Short { service, url } = &link {
        link = expand_short(state, service, url).await?;
    }
    let Link::Target { service, kind, id, url: canonical } = link else {
        return Err(AppError::new("unsupported_link", "short link"));
    };
    let list = match service {
        SPOTIFY => spotify::fetch(&state.http, kind.as_str(), &id).await?,
        DEEZER => deezer::fetch(&state.http, kind.as_str(), &id).await?,
        APPLE => apple::fetch(&state.http, kind, &id, &canonical).await?,
        YOUTUBE => state.yt.import(kind.as_str(), &id).await?,
        SOUNDCLOUD => state.sc.import(&canonical).await?,
        _ => return Err(AppError::new("unsupported_link", "unknown service")),
    };
    if list.tracks.is_empty() && list.skipped.is_empty() {
        return Err(AppError::new("empty_list", "the list has no songs"));
    }
    let mut previews = lock(&state.imports.previews);
    previews.retain(|_, (at, _)| at.elapsed() < PREVIEW_TTL);
    previews.insert(url.to_string(), (Instant::now(), list.clone()));
    Ok(list)
}

pub fn cancel(state: &AppState, job_id: &str) {
    if let Some(flag) = lock(&state.imports.jobs).get(job_id) {
        flag.store(true, Ordering::SeqCst);
    }
}

pub fn start(state: Arc<AppState>, account: String, target: Target) -> String {
    let job_id = crate::util::uuid_v4();
    let cancelled = Arc::new(AtomicBool::new(false));
    lock(&state.imports.jobs).insert(job_id.clone(), cancelled.clone());
    let id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let mut done = match run(&state, &account, &id, target, &cancelled).await {
            Ok(f) => f,
            Err(e) => Finished { ok: false, error: Some(e), ..Default::default() },
        };
        done.job_id = id.clone();
        done.cancelled = cancelled.load(Ordering::SeqCst);
        lock(&state.imports.jobs).remove(&id);
        let _ = state.app.emit("import-done", done);
        let _ = state.app.emit("library-changed", ());
    });
    job_id
}

fn emit_progress(state: &AppState, job_id: &str, phase: &'static str, done: usize, total: usize) {
    let _ = state.app.emit("import-progress", Progress { job_id: job_id.into(), phase, done, total });
}

async fn run(state: &Arc<AppState>, account: &str, job_id: &str, target: Target, cancelled: &AtomicBool) -> AppResult<Finished> {
    match target {
        Target::Playlist(list, url) => import_playlist(state, account, job_id, list, &url, cancelled).await,
        Target::Likes(source) => {
            let (tracks, skipped) = match source {
                LikedSource::Tracks(t) => (t, Vec::new()),
                LikedSource::SpotifyIds(ids) => read_spotify_tracks(state, job_id, ids, cancelled).await?,
            };
            if cancelled.load(Ordering::SeqCst) {
                return Ok(Finished::default());
            }
            let unmatched = match_all(state, job_id, &tracks, cancelled).await?;
            if cancelled.load(Ordering::SeqCst) {
                return Ok(Finished::default());
            }
            let added = library::like_many(&state.db, account, &tracks)?;
            Ok(Finished { ok: true, total: tracks.len(), added, unmatched, skipped, ..Default::default() })
        }
    }
}

async fn import_playlist(state: &Arc<AppState>, account: &str, job_id: &str, list: RemoteList, url: &str, cancelled: &AtomicBool) -> AppResult<Finished> {
    let unmatched = match_all(state, job_id, &list.tracks, cancelled).await?;
    if cancelled.load(Ordering::SeqCst) {
        return Ok(Finished::default());
    }
    let existing = library::find_by_origin(&state.db, account, &list.origin)?;
    let playlist = match &existing {
        Some(id) => id.clone(),
        None => library::create_playlist(&state.db, account, &list.title, None)?,
    };
    let (added, removed) = library::sync_items(&state.db, account, &playlist, &list.tracks)?;
    library::set_import_info(&state.db, &playlist, &list.origin, url, list.cover.as_deref(), &list.skipped)?;
    Ok(Finished {
        ok: true,
        playlist_id: Some(playlist),
        title: list.title,
        total: list.tracks.len(),
        added,
        removed,
        updated: existing.is_some(),
        unmatched,
        skipped: list.skipped,
        truncated: list.truncated,
        ..Default::default()
    })
}

/// Matches every imported song (YouTube and SoundCloud songs need none) and returns those
/// that found nothing. Fails only if matching broke for every song, e.g. when offline.
async fn match_all(state: &Arc<AppState>, job_id: &str, tracks: &[Track], cancelled: &AtomicBool) -> AppResult<Vec<Track>> {
    let todo: Vec<Track> = tracks.iter().filter(|t| is_import_only(&t.source)).cloned().collect();
    let total = todo.len();
    if total == 0 {
        return Ok(Vec::new());
    }
    emit_progress(state, job_id, "matching", 0, total);
    let sem = Arc::new(Semaphore::new(MATCH_PARALLEL));
    let mut set = tokio::task::JoinSet::new();
    for t in todo {
        let (state, sem) = (state.clone(), sem.clone());
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            Some((matching::matched(&state, &t, true).await, t))
        });
    }
    let (mut done, mut unmatched, mut last_err, mut failures) = (0, Vec::new(), None, 0);
    while let Some(res) = set.join_next().await {
        if cancelled.load(Ordering::SeqCst) {
            set.abort_all();
            return Ok(Vec::new());
        }
        let Ok(Some((result, track))) = res else { continue };
        match result {
            Ok(Some(_)) => {}
            Ok(None) => unmatched.push(track),
            Err(e) => {
                failures += 1;
                last_err = Some(e);
                unmatched.push(track);
            }
        }
        done += 1;
        emit_progress(state, job_id, "matching", done, total);
    }
    if failures == total {
        return Err(last_err.unwrap_or_else(|| AppError::new("network", "matching failed")));
    }
    // Keep the playlist's order in the report.
    let order: HashMap<&str, usize> = tracks.iter().enumerate().map(|(i, t)| (t.id.as_str(), i)).collect();
    unmatched.sort_by_key(|t| order.get(t.id.as_str()).copied().unwrap_or(usize::MAX));
    Ok(unmatched)
}

/// Reads pasted Spotify track links (one embed page each) with progress.
async fn read_spotify_tracks(state: &Arc<AppState>, job_id: &str, ids: Vec<String>, cancelled: &AtomicBool) -> AppResult<(Vec<Track>, Vec<Skipped>)> {
    let total = ids.len();
    emit_progress(state, job_id, "reading", 0, total);
    let sem = Arc::new(Semaphore::new(READ_PARALLEL));
    let mut set = tokio::task::JoinSet::new();
    for (i, id) in ids.into_iter().enumerate() {
        let (state, sem) = (state.clone(), sem.clone());
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            Some((i, spotify::track(&state.http, &id).await, id))
        });
    }
    let (mut found, mut skipped, mut done, mut last_err) = (Vec::new(), Vec::new(), 0, None);
    while let Some(res) = set.join_next().await {
        if cancelled.load(Ordering::SeqCst) {
            set.abort_all();
            return Ok((Vec::new(), Vec::new()));
        }
        let Ok(Some((i, result, id))) = res else { continue };
        match result {
            Ok(t) => found.push((i, t)),
            Err(e) => {
                skipped.push(Skipped { title: format!("https://open.spotify.com/track/{id}"), artist: String::new(), reason: "unavailable".into() });
                last_err = Some(e);
            }
        }
        done += 1;
        emit_progress(state, job_id, "reading", done, total);
    }
    if found.is_empty() {
        return Err(last_err.unwrap_or_else(|| AppError::new("empty_list", "no songs")));
    }
    found.sort_by_key(|(i, _)| *i);
    Ok((found.into_iter().map(|(_, t)| t).collect(), skipped))
}

pub fn liked_from_links(text: &str) -> AppResult<LikedSource> {
    let ids = links::spotify_track_ids(text);
    if ids.is_empty() {
        return Err(AppError::new("no_links", "no Spotify track links found"));
    }
    if ids.len() > MAX_LIKED_LINKS {
        return Err(AppError::new("too_many", "too many links"));
    }
    Ok(LikedSource::SpotifyIds(ids))
}

pub fn liked_from_export(json: &str) -> AppResult<LikedSource> {
    let tracks = spotify::parse_your_library(json)?;
    if tracks.is_empty() {
        return Err(AppError::new("empty_list", "no songs in the file"));
    }
    Ok(LikedSource::Tracks(tracks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(r: AppResult<LikedSource>) -> String {
        match r {
            Err(e) => e.code.to_string(),
            Ok(_) => "ok".into(),
        }
    }

    #[test]
    fn liked_links_keep_order_and_skip_junk() {
        let pasted = "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC\nnot a link\nspotify:track:7GhIk7Il098yCjg4BQjzvb\nhttps://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC?si=x";
        let Ok(LikedSource::SpotifyIds(ids)) = liked_from_links(pasted) else { panic!("links not read") };
        assert_eq!(ids, ["4uLU6hMCjMI75M1A2tKUQC", "7GhIk7Il098yCjg4BQjzvb"]);
    }

    #[test]
    fn liked_links_without_songs_or_too_many_are_refused() {
        assert_eq!(code(liked_from_links("hello\nhttps://youtube.com/watch?v=abc")), "no_links");
        let many: String = (0..=MAX_LIKED_LINKS).map(|i| format!("spotify:track:{i:0>22}\n")).collect();
        assert_eq!(code(liked_from_links(&many)), "too_many");
    }

    #[test]
    fn liked_export_needs_songs() {
        assert_eq!(code(liked_from_export(r#"{"tracks": []}"#)), "empty_list");
        assert_eq!(code(liked_from_export("not json")), "bad_file");
        let file = r#"{"tracks": [{"artist": "a-ha", "album": "Hunting High and Low", "track": "Take on Me", "uri": "spotify:track:2WfaOiMkCvy7F5fcp2zZ8L"}]}"#;
        let Ok(LikedSource::Tracks(tracks)) = liked_from_export(file) else { panic!("export not read") };
        assert_eq!(tracks[0].title, "Take on Me");
    }
}
