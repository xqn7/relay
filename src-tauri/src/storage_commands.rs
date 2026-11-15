//! Commands for offline playback and storage: saves, the Cache & Storage page, the connection
//! check, backups and covers. Inputs are validated here.

use crate::commands::{current, St};
use crate::error::{AppError, AppResult};
use crate::network::Check;
use crate::sources::{Track, LOCAL};
use crate::storage::{self, Available, DownloadStatus, OfflineLists, Usage};
use crate::{backup, covers, network};
use serde::Serialize;
use std::path::PathBuf;

const MAX_BATCH: usize = 10_000;

fn check_tracks(tracks: &[Track]) -> AppResult<()> {
    if tracks.len() > MAX_BATCH || tracks.iter().any(|t| !t.is_sane()) {
        return Err(AppError::new("invalid_input", "bad tracks"));
    }
    Ok(())
}

fn check_name(name: &str) -> AppResult<()> {
    if name.chars().count() > 300 {
        return Err(AppError::new("invalid_input", "name too long"));
    }
    Ok(())
}

fn check_cover_url(url: &str) -> AppResult<()> {
    let ok = (url.starts_with("https://") || url.starts_with("http://relay.localhost/art/")) && url.len() <= 2048;
    if !ok {
        return Err(AppError::new("invalid_input", "bad cover url"));
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Marks {
    songs: Vec<String>,
    playlists: Vec<String>,
}

#[tauri::command]
pub fn offline_marks(state: St) -> AppResult<Marks> {
    let (songs, playlists) = storage::marks(&state, &current(&state)?.id)?;
    Ok(Marks { songs, playlists })
}

#[tauri::command]
pub fn offline_set(state: St, tracks: Vec<Track>, on: bool) -> AppResult<()> {
    check_tracks(&tracks)?;
    let tracks: Vec<Track> = tracks.into_iter().filter(|t| t.source != LOCAL).collect();
    storage::set_offline(&state, &current(&state)?.id, &tracks, on)?;
    storage::sync(state.inner());
    Ok(())
}

#[tauri::command]
pub fn playlist_offline_set(state: St, id: String, on: bool) -> AppResult<()> {
    storage::set_playlist_offline(&state, &current(&state)?.id, &id, on)?;
    storage::sync(state.inner());
    Ok(())
}

/// Re-checks downloads after library changes (likes, playlist edits). Cheap when nothing changed.
#[tauri::command]
pub fn offline_sync(state: St) -> AppResult<()> {
    current(&state)?;
    storage::sync(state.inner());
    Ok(())
}

#[tauri::command]
pub fn storage_available(state: St) -> AppResult<Available> {
    current(&state)?;
    storage::available(&state)
}

#[tauri::command]
pub fn offline_lists(state: St) -> AppResult<OfflineLists> {
    storage::offline_lists(&state, &current(&state)?.id)
}

#[tauri::command]
pub fn download_status(state: St) -> DownloadStatus {
    state.storage.status()
}

#[tauri::command]
pub async fn storage_usage(state: St<'_>) -> AppResult<Usage> {
    let account = current(&state)?.id;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || storage::usage(&state, &account)).await.map_err(|e| AppError::new("internal", e.to_string()))?
}

#[tauri::command]
pub fn storage_clear(state: St, what: String) -> AppResult<()> {
    let account = current(&state)?.id;
    storage::clear(state.inner(), &account, &what)
}

#[tauri::command]
pub fn storage_open(state: St, what: String) -> AppResult<()> {
    let folder = match what.as_str() {
        "downloads" => state.storage.folder(storage::DOWNLOAD),
        "cache" => state.storage.folder(storage::CACHE),
        _ => return Err(AppError::new("invalid_input", "unknown folder")),
    };
    std::fs::create_dir_all(&folder)?;
    std::process::Command::new("explorer.exe").arg(folder).spawn()?;
    Ok(())
}

#[tauri::command]
pub fn net_status(state: St) -> bool {
    state.net.online()
}

#[tauri::command]
pub async fn net_check(state: St<'_>) -> AppResult<bool> {
    Ok(network::probe(state.inner(), true).await)
}

#[tauri::command]
pub async fn connection_check(state: St<'_>) -> AppResult<Vec<Check>> {
    Ok(network::check_all(state.inner()).await)
}

// ---- backup -------------------------------------------------------------------------------

#[tauri::command]
pub async fn backup_create(app: tauri::AppHandle, state: St<'_>) -> AppResult<bool> {
    current(&state)?;
    let name = format!("Relay backup {}.{}", backup::date_string(crate::util::now_secs()), backup::EXTENSION);
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::util::file_dialog(&app).set_file_name(name).add_filter("Relay backup", &[backup::EXTENSION]).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(path)) = rx.await.ok() else { return Ok(false) };
    let path = path.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    backup::write_file(&state.db, &path).await?;
    Ok(true)
}

/// Restores a backup the user picks. Returns the number of accounts restored (0 if cancelled);
/// the UI reloads afterwards, since the signed-in account's library may have changed. Works
/// signed out too: on a reinstalled PC there is no account yet, the backup brings them back.
#[tauri::command]
pub async fn backup_restore(app: tauri::AppHandle, state: St<'_>) -> AppResult<usize> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::util::file_dialog(&app).add_filter("Relay backup", &[backup::EXTENSION]).pick_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(path)) = rx.await.ok() else { return Ok(0) };
    let path = path.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    let n = backup::read_file(&state.db, &path).await?;
    state.streams.forget_all();
    storage::sync(state.inner());
    backup::schedule(state.inner());
    Ok(n)
}

/// Where automatic backups go unless the user picks another place: OneDrive when it's set
/// up (it survives a reinstall and syncs online), else Documents.
#[tauri::command]
pub fn backup_suggested_folder(app: tauri::AppHandle) -> AppResult<String> {
    use tauri::Manager;
    let base = std::env::var_os("OneDrive").map(PathBuf::from).filter(|p| p.is_dir());
    let base = match base {
        Some(p) => p,
        None => app.path().document_dir().map_err(|e| AppError::new("io", e.to_string()))?,
    };
    Ok(base.join(crate::app_name()).to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn backup_folder_pick(app: tauri::AppHandle, state: St<'_>) -> AppResult<Option<String>> {
    current(&state)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::util::file_dialog(&app).pick_folder(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(dir)) = rx.await.ok() else { return Ok(None) };
    let dir: PathBuf = dir.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    Ok(Some(dir.to_string_lossy().into_owned()))
}

// ---- covers -------------------------------------------------------------------------------

#[tauri::command]
pub async fn cover_hd(state: St<'_>, track: Track) -> AppResult<Option<String>> {
    check_tracks(std::slice::from_ref(&track))?;
    covers::hd(state.inner(), &track).await
}

#[tauri::command]
pub async fn cover_save(app: tauri::AppHandle, state: St<'_>, url: String, name: String) -> AppResult<bool> {
    check_cover_url(&url)?;
    check_name(&name)?;
    covers::save(&app, &state, &url, &name).await
}

#[tauri::command]
pub async fn cover_drag_prepare(state: St<'_>, url: String, name: String) -> AppResult<()> {
    check_cover_url(&url)?;
    check_name(&name)?;
    covers::prepare_drag(&state, &url, &name).await
}

#[tauri::command]
pub fn cover_drag_start(app: tauri::AppHandle, state: St) -> AppResult<()> {
    covers::start_drag(&app, &state)
}

#[tauri::command]
pub async fn local_use_hd_cover(state: St<'_>, track: Track) -> AppResult<()> {
    check_tracks(std::slice::from_ref(&track))?;
    if track.source != LOCAL {
        return Err(AppError::new("invalid_input", "only Local music files have tags"));
    }
    covers::use_hd_for_local(state.inner(), &track).await
}
