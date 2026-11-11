//! Commands for desktop integration: overlays, tray texts, Game mode and hardware info,
//! Discord preview, lock screen art, and updates.

use crate::commands::St;
use crate::error::{AppError, AppResult};
use crate::hardware::{self, Hardware, Recommendation};
use crate::lockscreen::Applied;
use crate::overlays;
use crate::tray::Labels;
use crate::updater::{self, UpdateInfo};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInfo {
    started_in_tray: bool,
    game: bool,
    shortcut_ok: bool,
    mini_custom: bool,
    lyrics_custom: bool,
    lyrics_unlocked: bool,
    /// "unset" (no Discord application yet), "off", "waiting" (Discord not running), "connected".
    discord: &'static str,
    version: String,
}

#[tauri::command]
pub fn desktop_info(state: St) -> DesktopInfo {
    let discord = if crate::discord::APP_ID.is_empty() {
        "unset"
    } else {
        match state.desktop.discord.status() {
            crate::discord::STATUS_CONNECTED => "connected",
            crate::discord::STATUS_NOT_RUNNING => "waiting",
            _ => "off",
        }
    };
    DesktopInfo {
        started_in_tray: state.desktop.started_in_tray,
        game: crate::desktop::is_game(&state),
        shortcut_ok: crate::os_events::shortcut_ok(),
        mini_custom: overlays::has_custom_position(&state, overlays::MINI),
        lyrics_custom: overlays::has_custom_position(&state, overlays::LYRICS),
        lyrics_unlocked: overlays::lyrics_unlocked(&state),
        discord,
        version: state.app.package_info().version.to_string(),
    }
}

/// From the mini player (cover or title) and anywhere else that brings Relay forward.
#[tauri::command]
pub fn main_window_show(app: tauri::AppHandle) {
    crate::show_main(&app);
}

#[tauri::command]
pub fn mini_toggle(app: tauri::AppHandle) {
    overlays::toggle_mini(&app);
}

#[tauri::command]
pub fn lyrics_unlock(app: tauri::AppHandle, unlocked: bool) {
    overlays::set_lyrics_unlocked(&app, unlocked);
}

/// Tray menu, taskbar tooltips and Discord texts in the UI's language.
#[tauri::command]
pub fn ui_labels(state: St, labels: Labels) -> AppResult<()> {
    let too_long = [&labels.play, &labels.pause, &labels.next, &labels.previous, &labels.quit, &labels.show].iter().any(|s| s.len() > 200);
    if too_long {
        return Err(AppError::new("invalid_input", "label too long"));
    }
    let tips = [labels.previous.clone(), labels.play.clone(), labels.pause.clone(), labels.next.clone()];
    *state.desktop.tray.labels.lock().unwrap_or_else(|p| p.into_inner()) = labels;
    crate::taskbar::set_tips(&state.app, tips);
    crate::tray::refresh(&state.app);
    crate::desktop::refresh_discord(state.inner());
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    hardware: Hardware,
    recommended: Recommendation,
}

/// Read once per PC (it takes a moment) and remembered.
#[tauri::command]
pub async fn hardware_info(state: St<'_>) -> AppResult<HardwareInfo> {
    let saved: Option<Hardware> = state.db.device_get("hardware")?.and_then(|v| serde_json::from_str(&v).ok());
    let hardware = match saved {
        Some(h) => h,
        None => {
            let h = tokio::task::spawn_blocking(hardware::detect).await.map_err(|e| AppError::new("internal", e.to_string()))?;
            state.db.device_set("hardware", &serde_json::to_string(&h)?)?;
            h
        }
    };
    let recommended = hardware::recommend(&hardware);
    Ok(HardwareInfo { hardware, recommended })
}

/// What Discord shows for the current song with the current settings (the settings preview;
/// shown even while Rich Presence is off, so the card can say what turning it on would do).
#[tauri::command]
pub fn discord_preview(state: St) -> Option<serde_json::Value> {
    crate::desktop::discord_activity(state.inner(), true)
}

#[tauri::command]
pub async fn lockscreen_set(state: St<'_>, request: Request<'_>) -> AppResult<Applied> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::new("invalid_input", "expected raw bytes"));
    };
    if !state.db.pref::<bool>("lockScreen").unwrap_or(false) {
        return Err(AppError::new("invalid_input", "lock screen art is off"));
    }
    let png = bytes.clone();
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.desktop.lockscreen.set(&png)).await.map_err(|e| AppError::new("internal", e.to_string()))?
}

#[tauri::command]
pub async fn lockscreen_restore(state: St<'_>) -> AppResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.desktop.lockscreen.restore()).await.map_err(|e| AppError::new("internal", e.to_string()))?
}

#[tauri::command]
pub async fn update_check(state: St<'_>) -> AppResult<Option<UpdateInfo>> {
    updater::check(state.inner()).await
}

#[tauri::command]
pub async fn update_install(state: St<'_>) -> AppResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || updater::install(&state)).await.map_err(|e| AppError::new("internal", e.to_string()))?
}

#[tauri::command]
pub async fn ytdlp_version(state: St<'_>) -> AppResult<Option<String>> {
    Ok(state.tools.ytdlp_version().await)
}

/// Returns the version now installed, and whether it changed.
#[tauri::command]
pub async fn ytdlp_update(state: St<'_>) -> AppResult<(Option<String>, bool)> {
    let changed = state.tools.update_ytdlp().await?;
    Ok((state.tools.ytdlp_version().await, changed))
}

#[tauri::command]
pub async fn youtube_status(state: St<'_>) -> AppResult<crate::youtube_account::AccountStatus> {
    Ok(crate::youtube_account::status(&state).await)
}

/// Async: the login window must not be created on the UI thread.
#[tauri::command]
pub async fn youtube_sign_in(app: tauri::AppHandle) -> AppResult<()> {
    crate::youtube_account::open_login(&app)
}

#[tauri::command]
pub async fn youtube_sign_out(state: St<'_>) -> AppResult<()> {
    crate::youtube_account::sign_out(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn youtube_playlists(state: St<'_>) -> AppResult<Vec<crate::sources::Collection>> {
    crate::youtube_account::playlists(&state).await
}

#[tauri::command]
pub async fn youtube_playlist_tracks(state: St<'_>, id: String) -> AppResult<Vec<crate::sources::Track>> {
    if id.is_empty() || id.len() > 128 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::new("invalid_input", "bad playlist id"));
    }
    crate::youtube_account::playlist_tracks(&state, &id).await
}
