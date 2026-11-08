//! The only entry points the webview can call. Every input is validated here.

use crate::accounts::{self, Account, Created};
use crate::error::{AppError, AppResult};
use crate::media::NowPlaying;
use crate::sources::{Quality, SearchKind, SearchResults, Source, Track, SOUNDCLOUD, YOUTUBE};
use crate::streams;
use crate::util::{now_secs, uuid_v4};
use crate::AppState;
use rusqlite::params;
use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::Arc;
use tauri::State;

type St<'a> = State<'a, Arc<AppState>>;

const MAX_SETTING_BYTES: usize = 64 * 1024;
const MAX_QUEUE_BYTES: usize = 4 * 1024 * 1024;
const HISTORY_LIMIT: i64 = 12;

fn current(state: &AppState) -> AppResult<Account> {
    state.account.lock().unwrap_or_else(|p| p.into_inner()).clone().ok_or_else(|| AppError::new("signed_out", ""))
}

fn set_current(state: &AppState, account: Option<Account>) {
    *state.account.lock().unwrap_or_else(|p| p.into_inner()) = account;
}

/// Password hashing is deliberately slow, so it runs off the async workers.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| AppError::new("internal", e.to_string()))?
}

#[tauri::command]
pub fn auth_state(state: St) -> AppResult<Option<Account>> {
    let account = accounts::remembered(&state.db)?;
    set_current(&state, account.clone());
    Ok(account)
}

#[tauri::command]
pub async fn sign_up(state: St<'_>, email: String, password: String, keep: bool) -> AppResult<Created> {
    let db = state.db.clone();
    let created = blocking(move || accounts::create(&db, &email, &password, keep)).await?;
    set_current(&state, Some(created.account.clone()));
    Ok(created)
}

#[tauri::command]
pub async fn sign_in(state: St<'_>, email: String, password: String, keep: bool) -> AppResult<Account> {
    let db = state.db.clone();
    let account = blocking(move || accounts::sign_in(&db, &email, &password, keep)).await?;
    set_current(&state, Some(account.clone()));
    Ok(account)
}

#[tauri::command]
pub async fn reset_password(state: St<'_>, email: String, recovery_key: String, new_password: String) -> AppResult<Account> {
    let db = state.db.clone();
    let account = blocking(move || accounts::reset_password(&db, &email, &recovery_key, &new_password)).await?;
    set_current(&state, Some(account.clone()));
    Ok(account)
}

#[tauri::command]
pub fn sign_out(state: St) -> AppResult<()> {
    accounts::sign_out(&state.db)?;
    state.media.clear();
    set_current(&state, None);
    Ok(())
}

#[derive(Serialize)]
pub struct Settings {
    account: Map<String, Value>,
    device: Map<String, Value>,
}

fn rows_to_map(rows: Vec<(String, String)>) -> Map<String, Value> {
    rows.into_iter().filter_map(|(k, v)| serde_json::from_str(&v).ok().map(|v| (k, v))).collect()
}

#[tauri::command]
pub fn settings_load(state: St) -> AppResult<Settings> {
    let account = current(&state)?;
    let acc_rows = state.db.with(|c| {
        let mut s = c.prepare("SELECT key,value FROM account_settings WHERE account_id=?1")?;
        let rows = s.query_map([&account.id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    })?;
    let dev_rows = state.db.with(|c| {
        // Preferences are stored as "pref.<key>" so internal device values never reach the UI.
        let mut s = c.prepare("SELECT substr(key,6),value FROM device_settings WHERE key LIKE 'pref.%'")?;
        let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    })?;
    Ok(Settings { account: rows_to_map(acc_rows), device: rows_to_map(dev_rows) })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

#[tauri::command]
pub fn setting_set(state: St, scope: String, key: String, value: Value) -> AppResult<()> {
    if !valid_key(&key) {
        return Err(AppError::new("invalid_input", "bad setting key"));
    }
    let json = serde_json::to_string(&value)?;
    if json.len() > MAX_SETTING_BYTES {
        return Err(AppError::new("invalid_input", "setting too large"));
    }
    match scope.as_str() {
        "device" => state.db.device_set(&format!("pref.{key}"), &json),
        "account" => {
            let account = current(&state)?;
            state.db.with(|c| {
                c.execute(
                    "INSERT INTO account_settings(account_id,key,value,updated_at) VALUES(?1,?2,?3,?4)
                     ON CONFLICT(account_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                    params![account.id, key, json, now_secs()],
                )
            })?;
            Ok(())
        }
        _ => Err(AppError::new("invalid_input", "bad scope")),
    }
}

#[tauri::command]
pub async fn search(state: St<'_>, source: String, query: String, kind: SearchKind) -> AppResult<SearchResults> {
    let query = query.trim().to_string();
    if query.is_empty() || query.chars().count() > 200 {
        return Err(AppError::new("invalid_input", "bad query"));
    }
    match source.as_str() {
        YOUTUBE => state.yt.search(&query, kind).await,
        SOUNDCLOUD => state.sc.search(&query, kind).await,
        _ => Err(AppError::new("invalid_input", "unknown source")),
    }
}

#[tauri::command]
pub fn search_history(state: St) -> AppResult<Vec<String>> {
    let account = current(&state)?;
    state.db.with(|c| {
        let mut s = c.prepare("SELECT query FROM search_history WHERE account_id=?1 ORDER BY updated_at DESC LIMIT ?2")?;
        let rows = s.query_map(params![account.id, HISTORY_LIMIT], |r| r.get(0))?.collect();
        rows
    })
}

#[tauri::command]
pub fn search_history_add(state: St, query: String) -> AppResult<()> {
    let account = current(&state)?;
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return Ok(());
    }
    let now = crate::util::now_millis();
    state.db.with(|c| {
        c.execute(
            "INSERT INTO search_history(id,account_id,query,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)
             ON CONFLICT(account_id,query) DO UPDATE SET updated_at=excluded.updated_at",
            params![uuid_v4(), account.id, query, now],
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn search_history_clear(state: St) -> AppResult<()> {
    let account = current(&state)?;
    state.db.with(|c| c.execute("DELETE FROM search_history WHERE account_id=?1", [&account.id]))?;
    Ok(())
}

#[tauri::command]
pub async fn collection_tracks(state: St<'_>, source: String, kind: String, id: String) -> AppResult<Vec<Track>> {
    if id.is_empty() || id.len() > 128 {
        return Err(AppError::new("invalid_input", "bad id"));
    }
    match source.as_str() {
        YOUTUBE => state.yt.collection_tracks(&kind, &id).await,
        SOUNDCLOUD => state.sc.collection_tracks(&kind, &id).await,
        _ => Err(AppError::new("invalid_input", "unknown source")),
    }
}

/// Similar songs to continue with when the queue runs out. SoundCloud songs borrow the radio
/// of their closest YouTube Music match, since SoundCloud has no comparable feature.
#[tauri::command]
pub async fn autoplay_tracks(state: St<'_>, track: Track) -> AppResult<Vec<Track>> {
    if !track.is_sane() {
        return Err(AppError::new("invalid_input", "bad track"));
    }
    let seed = if track.source == YOUTUBE {
        track.source_id.clone()
    } else {
        let query = format!("{} {}", track.artist_line(), crate::matcher::clean_title(&track.title));
        let found = state.yt.search(&query, SearchKind::Songs).await?;
        found.tracks.first().map(|t| t.source_id.clone()).ok_or_else(|| AppError::new("no_match", "no seed song"))?
    };
    state.yt.radio(&seed).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    url: String,
    loudness_db: Option<f32>,
    /// Set when another source's version plays instead of the requested one.
    stand_in: Option<Track>,
}

#[tauri::command]
pub async fn prepare_track(state: St<'_>, track: Track, quality: Quality, retry: bool) -> AppResult<Prepared> {
    if !track.is_sane() {
        return Err(AppError::new("invalid_input", "bad track"));
    }
    state.db.upsert_track(&track)?;
    let buf = streams::prepare(state.inner(), &track, quality, retry).await?;
    Ok(Prepared {
        url: streams::stream_url(&track.id),
        loudness_db: buf.loudness_db,
        stand_in: (buf.played.id != track.id).then(|| buf.played.clone()),
    })
}

/// Starts resolving and downloading the next song so it can start without a gap.
#[tauri::command]
pub fn prefetch_track(state: St, track: Track, quality: Quality) -> AppResult<()> {
    if !track.is_sane() {
        return Err(AppError::new("invalid_input", "bad track"));
    }
    state.db.upsert_track(&track)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        // A failed prefetch is retried (and reported) when the song actually starts.
        let _ = streams::prepare(&state, &track, quality, false).await;
    });
    Ok(())
}

#[tauri::command]
pub fn queue_save(state: St, queue: Value) -> AppResult<()> {
    let account = current(&state)?;
    let json = serde_json::to_string(&queue)?;
    if json.len() > MAX_QUEUE_BYTES {
        return Err(AppError::new("invalid_input", "queue too large"));
    }
    state.db.with(|c| {
        c.execute(
            "INSERT INTO queue_state(account_id,state,updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(account_id) DO UPDATE SET state=excluded.state, updated_at=excluded.updated_at",
            params![account.id, json, now_secs()],
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn queue_load(state: St) -> AppResult<Option<Value>> {
    let account = current(&state)?;
    let raw: Option<String> = state.db.with(|c| {
        use rusqlite::OptionalExtension;
        c.query_row("SELECT state FROM queue_state WHERE account_id=?1", [&account.id], |r| r.get(0)).optional()
    })?;
    Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
}

#[tauri::command]
pub fn media_track(state: St, now_playing: NowPlaying) {
    state.media.set_track(&now_playing);
}

#[tauri::command]
pub fn media_state(state: St, playing: bool, position_ms: u64, duration_ms: u64) {
    state.media.set_playing(playing);
    state.media.set_position(position_ms, duration_ms);
}

/// Opens a web link in the user's browser; the app window itself never leaves the app.
#[tauri::command]
pub fn open_external(url: String) -> AppResult<()> {
    let ok = url.starts_with("https://") && url.len() < 2048 && !url.contains(['"', ' ', '\n', '\r']);
    if !ok {
        return Err(AppError::new("invalid_input", "only plain https links can be opened"));
    }
    std::process::Command::new("explorer.exe").arg(&url).spawn()?;
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle, state: St) {
    state.media.clear();
    app.exit(0);
}
