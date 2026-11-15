//! Commands for the personal side: listening history and play counts, Home (lists, mixes,
//! speed dial), Discover, stats for Wrapped and the profile, private sessions, the profile
//! picture and banner, and background images. Images arrive as raw bytes, not JSON.

use crate::accounts::Account;
use crate::commands::{current, St};
use crate::error::{AppError, AppResult};
use crate::history::{self, ArtistStat, Counted, Stats};
use crate::library_commands::check_tracks;
use crate::mixes::{self, Mix, Suggestion};
use crate::profile::{self, Background, Pin};
use crate::sources::Track;
use crate::util::{now_secs, safe_file_stem};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::ipc::{InvokeBody, Request};
use tauri::Emitter;

const RECENT_LIMIT: usize = 20;
const MOST_LIMIT: usize = 20;
const MOST_SINCE_SECS: i64 = 180 * 86_400;
const MAX_CARD_BYTES: usize = 20 * 1024 * 1024;

fn raw_body<'a>(request: &'a Request<'_>) -> AppResult<&'a [u8]> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(bytes),
        _ => Err(AppError::new("invalid_input", "expected raw bytes")),
    }
}

fn header<'a>(request: &'a Request<'_>, name: &str) -> &'a str {
    request.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

// ---- history ------------------------------------------------------------------------------

/// Counts a play. Nothing is stored during a private session (returns None).
#[tauri::command]
pub fn play_record(state: St, track: Track, ms: i64) -> AppResult<Option<String>> {
    check_tracks(std::slice::from_ref(&track))?;
    let account = current(&state)?.id;
    if state.private.load(Ordering::Relaxed) {
        return Ok(None);
    }
    history::record(&state.db, &account, &track, ms).map(Some)
}

#[tauri::command]
pub fn play_listened(state: St, id: String, ms: i64) -> AppResult<()> {
    history::set_listened(&state.db, &current(&state)?.id, &id, ms)
}

#[tauri::command]
pub fn play_counts(state: St) -> AppResult<HashMap<String, i64>> {
    history::counts(&state.db, &current(&state)?.id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeLists {
    recent: Vec<Track>,
    most: Vec<Counted>,
    top_artists: Vec<ArtistStat>,
}

/// The queue panel's "Recently played": the last songs, newest first, each once.
#[tauri::command]
pub fn recent_plays(state: St) -> AppResult<Vec<Track>> {
    history::recent(&state.db, &current(&state)?.id, RECENT_LIMIT)
}

#[tauri::command]
pub fn home_lists(state: St) -> AppResult<HomeLists> {
    let account = current(&state)?.id;
    let since = now_secs() - MOST_SINCE_SECS;
    Ok(HomeLists {
        recent: history::recent(&state.db, &account, RECENT_LIMIT)?,
        most: history::most_played(&state.db, &account, since, MOST_LIMIT)?,
        top_artists: history::top_artists(&state.db, &account, since, 8)?,
    })
}

#[tauri::command]
pub async fn home_mixes(state: St<'_>, today: String) -> AppResult<Vec<Mix>> {
    let account = current(&state)?.id;
    mixes::daily(state.inner(), &account, &today).await
}

#[tauri::command]
pub async fn discover(state: St<'_>) -> AppResult<Vec<Suggestion>> {
    let account = current(&state)?.id;
    mixes::discover(state.inner(), &account).await
}

/// Stats for plays in [from, to) (Unix seconds); `tz_minutes` is the local offset from UTC.
#[tauri::command]
pub fn listening_stats(state: St, from: i64, to: i64, tz_minutes: i64) -> AppResult<Stats> {
    history::stats(&state.db, &current(&state)?.id, from, to, tz_minutes)
}

// ---- private session ----------------------------------------------------------------------

#[tauri::command]
pub fn private_get(state: St) -> bool {
    state.private.load(Ordering::Relaxed)
}

/// Lives in memory only, so it is off again after a restart. The event lets every place that
/// shows it (player bar, later the tray) follow along.
#[tauri::command]
pub fn private_set(state: St, on: bool) -> AppResult<()> {
    state.private.store(on, Ordering::Relaxed);
    state.app.emit("private-changed", on).map_err(|e| AppError::new("internal", e.to_string()))
}

// ---- profile ------------------------------------------------------------------------------

/// A cropped profile picture or banner; the kind comes in the "kind" header.
#[tauri::command]
pub fn profile_image_set(state: St, request: Request<'_>) -> AppResult<Account> {
    let account = current(&state)?.id;
    let updated = profile::set_image(&state.user_dir, &state.db, &account, header(&request, "kind"), Some(raw_body(&request)?))?;
    *state.account.lock().unwrap_or_else(|p| p.into_inner()) = Some(updated.clone());
    Ok(updated)
}

#[tauri::command]
pub fn profile_image_clear(state: St, kind: String) -> AppResult<Account> {
    let account = current(&state)?.id;
    let updated = profile::set_image(&state.user_dir, &state.db, &account, &kind, None)?;
    *state.account.lock().unwrap_or_else(|p| p.into_inner()) = Some(updated.clone());
    Ok(updated)
}

#[tauri::command]
pub fn profile_name_set(state: St, name: String) -> AppResult<Account> {
    let account = current(&state)?.id;
    let updated = profile::set_name(&state.db, &account, &name)?;
    *state.account.lock().unwrap_or_else(|p| p.into_inner()) = Some(updated.clone());
    Ok(updated)
}

/// Saves a rendered image (a Wrapped card) where the user chooses. The suggested file name
/// comes in the "name" header. False when the dialog was cancelled.
#[tauri::command]
pub async fn image_save(app: tauri::AppHandle, request: Request<'_>) -> AppResult<bool> {
    let bytes = raw_body(&request)?;
    if bytes.len() > MAX_CARD_BYTES || crate::covers::sniff(bytes) != Some("image/png") {
        return Err(AppError::new("invalid_input", "expected a PNG image"));
    }
    // Headers are ASCII, so the name comes percent-encoded.
    let name = format!("{}.png", safe_file_stem(&crate::streams::percent_decode(header(&request, "name")), "Relay"));
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::util::file_dialog(&app).set_file_name(name).add_filter("PNG", &["png"]).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(path)) = rx.await.ok() else { return Ok(false) };
    let path = path.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    tokio::fs::write(path, bytes).await?;
    Ok(true)
}

// ---- backgrounds --------------------------------------------------------------------------

#[tauri::command]
pub fn backgrounds_list(state: St) -> AppResult<Vec<Background>> {
    profile::backgrounds(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub fn background_add(state: St, request: Request<'_>) -> AppResult<Background> {
    let account = current(&state)?.id;
    profile::add_background(&state.user_dir, &state.db, &account, raw_body(&request)?)
}

#[tauri::command]
pub fn background_look_set(state: St, id: String, look: Value) -> AppResult<()> {
    profile::set_look(&state.db, &current(&state)?.id, &id, &look)
}

#[tauri::command]
pub fn background_delete(state: St, id: String) -> AppResult<()> {
    profile::delete_background(&state.user_dir, &state.db, &current(&state)?.id, &id)
}

// ---- speed dial ---------------------------------------------------------------------------

#[tauri::command]
pub fn speed_dial_list(state: St) -> AppResult<Vec<Pin>> {
    profile::pins(&state.db, &current(&state)?.id)
}

#[tauri::command]
pub fn speed_dial_set(state: St, pin: Pin, on: bool) -> AppResult<()> {
    profile::set_pin(&state.db, &current(&state)?.id, &pin, on)
}

#[tauri::command]
pub fn speed_dial_order(state: St, keys: Vec<String>) -> AppResult<()> {
    profile::order_pins(&state.db, &current(&state)?.id, &keys)
}
