//! Commands for the sound tools: trims, waveforms, SponsorBlock, other versions and MP3 export.

use crate::commands::{current, St};
use crate::error::{AppError, AppResult};
use crate::export::{self, ExportDone};
use crate::library_commands::check_tracks;
use crate::sound::{self, Trim};
use crate::sources::Track;
use crate::versions::{self, Version};
use std::collections::HashMap;

/// Songs exported in one go (a whole selection of a playlist).
const MAX_EXPORT: usize = 500;

#[tauri::command]
pub fn trims_list(state: St) -> AppResult<HashMap<String, Trim>> {
    sound::trims(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub fn trim_set(state: St, track: Track, trim: Option<Trim>) -> AppResult<()> {
    check_tracks(std::slice::from_ref(&track))?;
    sound::set_trim(&state.db, &current(&state)?.id, &track, trim)
}

#[tauri::command]
pub async fn waveform(state: St<'_>, track: Track) -> AppResult<Vec<u8>> {
    check_tracks(std::slice::from_ref(&track))?;
    sound::waveform(state.inner(), &track).await
}

#[tauri::command]
pub async fn sponsor_segments(state: St<'_>, video_id: String) -> AppResult<Vec<(f64, f64)>> {
    sound::segments(&state, &video_id).await
}

#[tauri::command]
pub async fn other_versions(state: St<'_>, track: Track) -> AppResult<Vec<Version>> {
    check_tracks(std::slice::from_ref(&track))?;
    versions::find(&state, &track).await
}

/// Asks for a folder, then saves the songs there as MP3s. With a range (clip export) only
/// that part of the one song is saved. None when the folder picker was cancelled.
#[tauri::command]
pub async fn export_mp3(app: tauri::AppHandle, state: St<'_>, tracks: Vec<Track>, start_ms: Option<u64>, end_ms: Option<u64>) -> AppResult<Option<ExportDone>> {
    check_tracks(&tracks)?;
    if tracks.is_empty() || tracks.len() > MAX_EXPORT {
        return Err(AppError::new("invalid_input", "bad song count"));
    }
    let range = match (start_ms, end_ms) {
        (Some(a), Some(b)) if tracks.len() == 1 && b > a + 500 => Some((a, b)),
        (None, None) => None,
        _ => return Err(AppError::new("invalid_input", "bad clip range")),
    };
    let Some(folder) = export::pick_folder(&app, &state).await? else { return Ok(None) };
    let done = export::export_all(state.inner(), &tracks, range, &folder).await;
    *state.last_export.lock().unwrap_or_else(|p| p.into_inner()) = Some(done.file.clone().map(Into::into).unwrap_or_else(|| folder.clone()));
    Ok(Some(done))
}

/// Shows the last exported file (or folder) in Explorer.
#[tauri::command]
pub fn export_reveal(state: St) -> AppResult<()> {
    let last = state.last_export.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let path = last.ok_or_else(|| AppError::new("not_found", "nothing exported yet"))?;
    let mut cmd = std::process::Command::new("explorer.exe");
    if path.is_file() {
        // explorer wants "/select," and the path as one argument.
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(&path);
        cmd.arg(arg);
    } else {
        cmd.arg(&path);
    }
    cmd.spawn()?;
    Ok(())
}
