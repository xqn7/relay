//! Commands for the library, imports, Local music and catalog pages. Inputs are validated
//! here; the library module scopes everything to the signed-in account.

use crate::commands::{current, St};
use crate::error::{AppError, AppResult};
use crate::importer::{self, LikedSource, Target};
use crate::library::{self, Followed, Hidden, Item, Overview, PlaylistDetail, Removed, TrashEntry};
use crate::sources::{is_import_only, AlbumPage, ArtistCard, ArtistPage, Collection, RemoteList, Track, LOCAL, SOUNDCLOUD, YOUTUBE};
use crate::{local, m3u, matching, AppState};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri_plugin_dialog::DialogExt;

const MAX_BATCH: usize = 10_000;
const ARTIST_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const FOLLOW_PARALLEL: usize = 3;

pub(crate) fn check_tracks(tracks: &[Track]) -> AppResult<()> {
    if tracks.len() > MAX_BATCH || tracks.iter().any(|t| !t.is_sane()) {
        return Err(AppError::new("invalid_input", "bad tracks"));
    }
    Ok(())
}

pub(crate) fn check_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 128 || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')) {
        return Err(AppError::new("invalid_input", "bad id"));
    }
    Ok(())
}

// ---- playlists and folders ------------------------------------------------------------

#[tauri::command]
pub fn library_overview(state: St) -> AppResult<Overview> {
    library::overview(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub fn playlist_get(state: St, id: String) -> AppResult<PlaylistDetail> {
    library::playlist_detail(&state.db, &current(&state)?.id, &id)
}

#[tauri::command]
pub fn playlist_create(state: St, name: String, folder_id: Option<String>, tracks: Vec<Track>) -> AppResult<String> {
    check_tracks(&tracks)?;
    let account = current(&state)?.id;
    let id = library::create_playlist(&state.db, &account, &name, folder_id.as_deref())?;
    library::add_tracks(&state.db, &account, &id, &tracks)?;
    Ok(id)
}

#[tauri::command]
pub fn playlist_rename(state: St, id: String, name: String) -> AppResult<()> {
    library::rename_playlist(&state.db, &current(&state)?.id, &id, &name)
}

#[tauri::command]
pub fn playlist_set_folder(state: St, id: String, folder_id: Option<String>) -> AppResult<()> {
    library::set_playlist_folder(&state.db, &current(&state)?.id, &id, folder_id.as_deref())
}

#[tauri::command]
pub fn playlist_delete(state: St, id: String) -> AppResult<()> {
    library::trash_playlist(&state.db, &current(&state)?.id, &id)
}

#[tauri::command]
pub fn playlist_add(state: St, id: String, tracks: Vec<Track>) -> AppResult<usize> {
    check_tracks(&tracks)?;
    library::add_tracks(&state.db, &current(&state)?.id, &id, &tracks)
}

#[tauri::command]
pub fn playlist_remove(state: St, id: String, item_ids: Vec<String>) -> AppResult<Vec<Removed>> {
    if item_ids.len() > MAX_BATCH {
        return Err(AppError::new("invalid_input", "too many items"));
    }
    library::remove_items(&state.db, &current(&state)?.id, &id, &item_ids)
}

#[tauri::command]
pub fn playlist_restore_items(state: St, id: String, rows: Vec<Removed>) -> AppResult<()> {
    if rows.len() > MAX_BATCH {
        return Err(AppError::new("invalid_input", "too many items"));
    }
    library::restore_items(&state.db, &current(&state)?.id, &id, &rows)
}

#[tauri::command]
pub fn playlist_move(state: St, id: String, item_id: String, to: usize) -> AppResult<()> {
    library::move_item(&state.db, &current(&state)?.id, &id, &item_id, to)
}

#[tauri::command]
pub fn playlist_replace(state: St, id: String, item_id: String, track: Track) -> AppResult<()> {
    check_tracks(std::slice::from_ref(&track))?;
    library::replace_item(&state.db, &current(&state)?.id, &id, &item_id, &track)
}

#[tauri::command]
pub fn playlist_played(state: St, id: String) -> AppResult<()> {
    if state.private.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    library::count_play(&state.db, &current(&state)?.id, &id)
}

#[tauri::command]
pub fn playlist_merge(state: St, a: String, b: String, into: Option<String>, new_name: Option<String>) -> AppResult<String> {
    library::merge(&state.db, &current(&state)?.id, &a, &b, into.as_deref(), new_name.as_deref())
}

#[tauri::command]
pub fn folder_create(state: St, name: String, parent_id: Option<String>) -> AppResult<String> {
    library::create_folder(&state.db, &current(&state)?.id, &name, parent_id.as_deref())
}

#[tauri::command]
pub fn folder_rename(state: St, id: String, name: String) -> AppResult<()> {
    library::rename_folder(&state.db, &current(&state)?.id, &id, &name)
}

#[tauri::command]
pub fn folder_set_parent(state: St, id: String, parent_id: Option<String>) -> AppResult<()> {
    library::set_folder_parent(&state.db, &current(&state)?.id, &id, parent_id.as_deref())
}

#[tauri::command]
pub fn folder_delete(state: St, id: String) -> AppResult<()> {
    library::trash_folder(&state.db, &current(&state)?.id, &id)
}

// ---- likes, hidden, follows -----------------------------------------------------------

#[tauri::command]
pub fn like_set(state: St, track: Track, liked: bool) -> AppResult<()> {
    check_tracks(std::slice::from_ref(&track))?;
    library::set_like(&state.db, &current(&state)?.id, &track, liked)
}

#[tauri::command]
pub fn liked_list(state: St) -> AppResult<Vec<Item>> {
    library::liked(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub fn liked_ids(state: St) -> AppResult<Vec<String>> {
    library::liked_ids(&state.db, &current(&state)?.id)
}

/// Artists are stored by folded name so they're hidden on every service at once.
#[tauri::command]
pub fn hidden_set(state: St, kind: String, value: String, label: String, on: bool) -> AppResult<()> {
    let value = if kind == "artist" { crate::matcher::fold(&value) } else { value };
    library::set_hidden(&state.db, &current(&state)?.id, &kind, &value, &label, on)
}

#[tauri::command]
pub fn hidden_list(state: St) -> AppResult<Vec<Hidden>> {
    library::hidden(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub fn follow_set(state: St, artist: Followed, on: bool) -> AppResult<()> {
    if !matches!(artist.source.as_str(), YOUTUBE | SOUNDCLOUD) || check_id(&artist.artist_id).is_err() || artist.name.trim().is_empty() {
        return Err(AppError::new("invalid_input", "bad artist"));
    }
    library::set_follow(&state.db, &current(&state)?.id, &artist, on)
}

#[tauri::command]
pub fn follows_list(state: St) -> AppResult<Vec<Followed>> {
    library::follows(&state.db, &current(&state)?.id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Releases {
    artist: Followed,
    releases: Vec<Collection>,
    failed: bool,
}

/// Newest albums and singles of every followed artist, newest first.
#[tauri::command]
pub async fn followed_releases(state: St<'_>) -> AppResult<Vec<Releases>> {
    let follows = library::follows(&state.db, &current(&state)?.id)?;
    let sem = Arc::new(tokio::sync::Semaphore::new(FOLLOW_PARALLEL));
    let mut set = tokio::task::JoinSet::new();
    for (i, f) in follows.into_iter().enumerate() {
        let (state, sem) = (state.inner().clone(), sem.clone());
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.ok();
            let page = cached_artist(&state, &f.source, &f.artist_id).await;
            let releases = page.as_ref().map(latest).unwrap_or_default();
            (i, Releases { failed: page.is_err(), artist: f, releases })
        });
    }
    let mut out = Vec::new();
    while let Some(Ok(r)) = set.join_next().await {
        out.push(r);
    }
    out.sort_by_key(|(i, _)| *i);
    Ok(out.into_iter().map(|(_, r)| r).collect())
}

fn latest(p: &ArtistPage) -> Vec<Collection> {
    let mut all: Vec<Collection> = p.albums.iter().chain(&p.singles).cloned().collect();
    all.sort_by_key(|c| std::cmp::Reverse(c.year));
    all.truncate(6);
    all
}

pub(crate) async fn cached_artist(state: &Arc<AppState>, source: &str, id: &str) -> AppResult<ArtistPage> {
    let key = format!("{source}:{id}");
    if let Some((at, page)) = state.artist_cache.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        if at.elapsed() < ARTIST_CACHE_TTL {
            return Ok(page.clone());
        }
    }
    let page = match source {
        YOUTUBE => state.yt.artist_page(id).await?,
        SOUNDCLOUD => state.sc.artist_page(id).await?,
        _ => return Err(AppError::new("invalid_input", "unknown source")),
    };
    let mut cache = state.artist_cache.lock().unwrap_or_else(|p| p.into_inner());
    cache.retain(|_, (at, _)| at.elapsed() < ARTIST_CACHE_TTL);
    cache.insert(key, (Instant::now(), page.clone()));
    Ok(page)
}

// ---- catalog pages --------------------------------------------------------------------

#[tauri::command]
pub async fn artist_page(state: St<'_>, source: String, id: String) -> AppResult<ArtistPage> {
    check_id(&id)?;
    cached_artist(state.inner(), &source, &id).await
}

/// The YouTube Music artist with this name, for artist names that came without an id.
#[tauri::command]
pub async fn artist_find(state: St<'_>, name: String) -> AppResult<Option<ArtistCard>> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(AppError::new("invalid_input", "bad name"));
    }
    state.yt.find_artist(name).await
}

#[tauri::command]
pub async fn album_page(state: St<'_>, source: String, id: String) -> AppResult<AlbumPage> {
    check_id(&id)?;
    match source.as_str() {
        YOUTUBE => state.yt.album_page(&id).await,
        SOUNDCLOUD => state.sc.album_page(&id).await,
        _ => Err(AppError::new("invalid_input", "unknown source")),
    }
}

/// One SoundCloud song by id (artist pages list SoundCloud singles as single songs).
#[tauri::command]
pub async fn soundcloud_track(state: St<'_>, id: String) -> AppResult<Track> {
    state.sc.track(&id).await
}

// ---- trash ----------------------------------------------------------------------------

#[tauri::command]
pub fn trash_list(state: St) -> AppResult<Vec<TrashEntry>> {
    library::trash(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub async fn trash_restore(state: St<'_>, kind: String, id: String) -> AppResult<()> {
    let account = current(&state)?.id;
    match kind.as_str() {
        "local" => local::restore(state.inner(), &id).await,
        _ => library::restore(&state.db, &account, &kind, &id),
    }
}

#[tauri::command]
pub async fn trash_delete(state: St<'_>, kind: String, id: String) -> AppResult<()> {
    let account = current(&state)?.id;
    match kind.as_str() {
        "local" => local::delete_forever(state.inner(), &id).await,
        _ => library::delete_forever(&state.db, &account, &kind, &id),
    }
}

#[tauri::command]
pub async fn trash_empty(state: St<'_>) -> AppResult<()> {
    let account = current(&state)?.id;
    for e in library::trash(&state.db, &account)? {
        match e.kind.as_str() {
            "local" => local::delete_forever(state.inner(), &e.id).await?,
            _ => library::delete_forever(&state.db, &account, &e.kind, &e.id)?,
        }
    }
    Ok(())
}

/// Empties trash entries older than 30 days. Called once after sign-in.
#[tauri::command]
pub fn trash_purge_expired(state: St) -> AppResult<()> {
    let account = current(&state)?.id;
    for (kind, id) in library::expired_trash(&state.db, &account)? {
        library::delete_forever(&state.db, &account, &kind, &id)?;
    }
    Ok(())
}

// ---- imports --------------------------------------------------------------------------

#[tauri::command]
pub async fn import_preview(state: St<'_>, url: String) -> AppResult<RemoteList> {
    current(&state)?;
    importer::read(&state, &url).await
}

#[tauri::command]
pub async fn import_start(state: St<'_>, url: String) -> AppResult<String> {
    let account = current(&state)?.id;
    let list = importer::read(&state, &url).await?;
    Ok(importer::start(state.inner().clone(), account, Target::Playlist(list, url)))
}

#[tauri::command]
pub fn import_cancel(state: St, job_id: String) {
    importer::cancel(&state, &job_id);
}

#[tauri::command]
pub fn import_liked_links(state: St, text: String) -> AppResult<String> {
    let account = current(&state)?.id;
    if text.len() > 4 * 1024 * 1024 {
        return Err(AppError::new("too_many", "too much text"));
    }
    let source = importer::liked_from_links(&text)?;
    Ok(importer::start(state.inner().clone(), account, Target::Likes(source)))
}

/// Opens a file picker for YourLibrary.json and starts the import. None when cancelled.
#[tauri::command]
pub async fn import_liked_file(app: tauri::AppHandle, state: St<'_>) -> AppResult<Option<String>> {
    let account = current(&state)?.id;
    let Some(path) = pick_file(&app, "YourLibrary.json", &["json"]).await? else { return Ok(None) };
    let meta = tokio::fs::metadata(&path).await?;
    if meta.len() > 64 * 1024 * 1024 {
        return Err(AppError::new("bad_file", "file is too large"));
    }
    let json = tokio::fs::read_to_string(&path).await.map_err(|_| AppError::new("bad_file", "file is not text"))?;
    let source: LikedSource = importer::liked_from_export(&json)?;
    Ok(Some(importer::start(state.inner().clone(), account, Target::Likes(source))))
}

/// Picks an .m3u/.m3u8 file; its path then goes through the normal preview and import,
/// like a pasted link.
#[tauri::command]
pub async fn import_m3u_pick(app: tauri::AppHandle, state: St<'_>) -> AppResult<Option<String>> {
    current(&state)?;
    Ok(pick_file(&app, "M3U", &["m3u", "m3u8"]).await?.map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn export_m3u(app: tauri::AppHandle, state: St<'_>, id: String) -> AppResult<bool> {
    let detail = library::playlist_detail(&state.db, &current(&state)?.id, &id)?;
    let safe: String = detail.playlist.name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_file_name(format!("{safe}.m3u8")).add_filter("M3U8", &["m3u8"]).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(path)) = rx.await.ok() else { return Ok(false) };
    let path = path.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    let tracks: Vec<Track> = detail.items.into_iter().map(|i| i.track).collect();
    let text = m3u::write(state.inner(), &tracks);
    tokio::fs::write(path, text).await?;
    Ok(true)
}

// ---- matching -------------------------------------------------------------------------

#[tauri::command]
pub async fn match_candidates(state: St<'_>, track: Track, query: Option<String>) -> AppResult<Vec<Track>> {
    check_tracks(std::slice::from_ref(&track))?;
    let query = query.map(|q| q.trim().to_string()).filter(|q| !q.is_empty() && q.chars().count() <= 200);
    matching::candidates(&state, &track, query.as_deref()).await
}

#[tauri::command]
pub fn match_get(state: St, track: Track) -> AppResult<Option<Track>> {
    Ok(library::get_match(&state.db, &track.source, &track.source_id)?.flatten())
}

/// "Fix match": the chosen song plays from now on, and the old stream is dropped.
#[tauri::command]
pub fn match_set(state: St, track: Track, matched: Track) -> AppResult<()> {
    check_tracks(&[track.clone(), matched.clone()])?;
    if !is_import_only(&track.source) || is_import_only(&matched.source) || matched.source == LOCAL {
        return Err(AppError::new("invalid_input", "only imported songs have matches"));
    }
    library::set_match(&state.db, &track.source, &track.source_id, Some(&matched), true, 1.0)?;
    state.streams.forget(&track.id);
    // The stored audio is the old match; a saved song downloads again with the new one.
    crate::storage::discard(&state, &track.id);
    crate::storage::sync(state.inner());
    Ok(())
}

/// Matches the next songs in the queue ahead of time, so they start without a search.
#[tauri::command]
pub fn prematch(state: St, tracks: Vec<Track>) -> AppResult<()> {
    check_tracks(&tracks)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        for t in tracks.into_iter().filter(|t| is_import_only(&t.source)).take(5) {
            let _ = matching::matched(&state, &t, false).await;
        }
    });
    Ok(())
}

// ---- Local music ----------------------------------------------------------------------

#[tauri::command]
pub fn local_list(state: St) -> AppResult<Vec<Track>> {
    current(&state)?;
    local::list(&state)
}

#[tauri::command]
pub async fn local_add_dialog(app: tauri::AppHandle, state: St<'_>) -> AppResult<bool> {
    current(&state)?;
    let exts: Vec<&str> = local::AUDIO_EXTS.iter().chain(local::VIDEO_EXTS).copied().collect();
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter("Music and video", &exts).pick_files(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(files)) = rx.await.ok() else { return Ok(false) };
    let paths: Vec<PathBuf> = files.into_iter().filter_map(|f| f.into_path().ok()).collect();
    local::add(state.inner().clone(), paths);
    Ok(true)
}

#[tauri::command]
pub async fn local_edit(app: tauri::AppHandle, state: St<'_>, source_id: String, title: String, artist: String, pick_cover: bool) -> AppResult<bool> {
    check_id(&source_id)?;
    let cover = if pick_cover {
        match pick_file(&app, "Image", &["jpg", "jpeg", "png"]).await? {
            Some(p) => Some(p),
            None => return Ok(false),
        }
    } else {
        None
    };
    local::edit(state.inner(), &source_id, &title, &artist, cover).await?;
    Ok(true)
}

#[tauri::command]
pub async fn local_remove(state: St<'_>, source_id: String) -> AppResult<()> {
    check_id(&source_id)?;
    local::remove(state.inner(), &source_id).await
}

#[tauri::command]
pub fn local_folder(state: St) -> String {
    state.local.folder().to_string_lossy().into_owned()
}

#[tauri::command]
pub async fn local_folder_pick(app: tauri::AppHandle, state: St<'_>) -> AppResult<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(dir)) = rx.await.ok() else { return Ok(None) };
    let dir = dir.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    local::set_folder(state.inner(), dir.clone()).await?;
    Ok(Some(dir.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn local_open_folder(state: St) -> AppResult<()> {
    let folder = state.local.folder();
    std::fs::create_dir_all(&folder)?;
    std::process::Command::new("explorer.exe").arg(folder).spawn()?;
    Ok(())
}

async fn pick_file(app: &tauri::AppHandle, label: &str, exts: &[&str]) -> AppResult<Option<PathBuf>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter(label, exts).pick_file(move |p| {
        let _ = tx.send(p);
    });
    match rx.await.ok().flatten() {
        Some(p) => Ok(Some(p.into_path().map_err(|e| AppError::new("io", e.to_string()))?)),
        None => Ok(None),
    }
}
