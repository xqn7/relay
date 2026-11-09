//! Songs kept on disk. Two kinds share one table (`audio_files`):
//! - the song cache: every song that finished downloading while playing, size-limited, the
//!   least recently played removed first;
//! - downloads: songs saved for offline (single songs, whole playlists, and Liked Songs when
//!   that is switched on). Never removed automatically; they stop when the space limit is full.
//!
//! A song moves between the two folders when it is saved or unsaved. Files are shared by all
//! accounts on the PC; which songs an account sees is decided by its own library.

use crate::error::{AppError, AppResult};
use crate::sources::{Quality, Track};
use crate::util::now_secs;
use crate::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub const CACHE: &str = "cache";
pub const DOWNLOAD: &str = "download";
const GB: u64 = 1024 * 1024 * 1024;
const DEFAULT_MAX_CACHE_GB: u64 = 3;
const DEFAULT_DOWNLOAD_LIMIT_GB: u64 = 5;
/// Used for "about N songs" until enough real files exist to take an average.
const TYPICAL_SONG_BYTES: u64 = 4 * 1024 * 1024;
const AVERAGE_FROM: i64 = 5;

pub struct Storage {
    songs_dir: PathBuf,
    pub art_dir: PathBuf,
    downloads_dir: PathBuf,
    running: AtomicBool,
    again: AtomicBool,
    status: Mutex<DownloadStatus>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub active: bool,
    /// Wanted songs that are on disk / all wanted songs.
    pub done: usize,
    pub total: usize,
    /// The space for downloads is used up; the rest waits.
    pub full: bool,
    /// Songs that couldn't be downloaded in the last run (no match, removed, …).
    pub failed: usize,
}

pub struct OnDisk {
    pub path: PathBuf,
    pub mime: String,
    pub played: Track,
    pub stand_in: bool,
    pub loudness_db: Option<f32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub downloads_bytes: u64,
    pub downloads_count: u64,
    pub liked_bytes: u64,
    pub imported_bytes: u64,
    pub cache_songs_bytes: u64,
    pub cache_songs_count: u64,
    pub artwork_bytes: u64,
    pub free_bytes: Option<u64>,
    pub average_song_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub downloaded: Vec<String>,
    pub cached: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineLists {
    pub downloaded: Vec<Track>,
    pub cached: Vec<Track>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// File name (without extension) for a track id. Hashed, because ids are case-sensitive
/// (YouTube) while Windows file names are not.
pub fn file_stem(track_id: &str) -> String {
    Sha256::digest(track_id.as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect()
}

pub fn ext_for_mime(mime: &str) -> &'static str {
    let base = mime.split(';').next().unwrap_or("").trim();
    match base {
        "audio/webm" => "webm",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "audio/aac" => "m4a",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/flac" => "flac",
        "audio/wav" | "audio/x-wav" => "wav",
        _ => "audio",
    }
}

/// Oldest-first cache rows to delete so that everything fits in `limit`. `rows` must be
/// ordered least recently used first; songs in `live` (possibly playing) are kept.
pub fn pick_evictions<'a>(rows: &'a [(String, u64)], other_bytes: u64, limit: u64, live: &HashSet<String>) -> Vec<&'a str> {
    let mut total: u64 = rows.iter().map(|(_, b)| *b).sum::<u64>() + other_bytes;
    let mut out = Vec::new();
    for (id, bytes) in rows {
        if total <= limit {
            break;
        }
        if live.contains(id) {
            continue;
        }
        total = total.saturating_sub(*bytes);
        out.push(id.as_str());
    }
    out
}

/// Songs some account wants on disk: saved songs, songs of saved playlists, and liked songs
/// when "Download Liked Songs" is on. Local music is already on disk.
const WANTED_SQL: &str = "SELECT track_id FROM (
    SELECT track_id FROM offline
    UNION SELECT pi.track_id FROM playlist_items pi JOIN playlists p ON p.id = pi.playlist_id
          WHERE p.offline = 1 AND p.deleted_at IS NULL
    UNION SELECT track_id FROM likes WHERE ?1
) WHERE track_id NOT LIKE 'local:%'";

pub fn wanted(c: &Connection, liked_too: bool) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare(WANTED_SQL)?;
    let rows = stmt.query_map([liked_too], |r| r.get(0))?.collect();
    rows
}

fn is_wanted(c: &Connection, track_id: &str, liked_too: bool) -> rusqlite::Result<bool> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM offline WHERE track_id = ?1)
             OR EXISTS(SELECT 1 FROM playlist_items pi JOIN playlists p ON p.id = pi.playlist_id
                       WHERE pi.track_id = ?1 AND p.offline = 1 AND p.deleted_at IS NULL)
             OR (?2 AND EXISTS(SELECT 1 FROM likes WHERE track_id = ?1))",
        params![track_id, liked_too],
        |r| r.get(0),
    )
}

/// Songs an account has in its own library (likes, playlists, saves). Only these show up in
/// its offline lists, even though the files themselves are shared.
const ACCOUNT_SONGS_SQL: &str = "SELECT track_id FROM likes WHERE account_id = ?1
    UNION SELECT pi.track_id FROM playlist_items pi JOIN playlists p ON p.id = pi.playlist_id
          WHERE p.account_id = ?1 AND p.deleted_at IS NULL
    UNION SELECT track_id FROM offline WHERE account_id = ?1";

fn liked_too(state: &AppState) -> bool {
    state.db.pref::<bool>("autoDownloadLiked").unwrap_or(false)
}

fn limit_bytes(state: &AppState, key: &str, default_gb: u64) -> Option<u64> {
    // 0 means "No limit".
    let gb = state.db.pref::<u64>(key).unwrap_or(default_gb);
    (gb > 0).then_some(gb * GB)
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map(|it| it.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()).unwrap_or(0)
}

fn free_space(dir: &Path) -> Option<u64> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(dir.as_os_str()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}

impl Storage {
    pub fn new(local_dir: &Path) -> std::io::Result<Self> {
        let s = Self {
            songs_dir: local_dir.join("cache").join("songs"),
            art_dir: local_dir.join("cache").join("art"),
            downloads_dir: local_dir.join("downloads"),
            running: AtomicBool::new(false),
            again: AtomicBool::new(false),
            status: Mutex::new(DownloadStatus::default()),
        };
        for d in [&s.songs_dir, &s.art_dir, &s.downloads_dir] {
            std::fs::create_dir_all(d)?;
        }
        Ok(s)
    }

    fn dir(&self, kind: &str) -> &Path {
        if kind == DOWNLOAD { &self.downloads_dir } else { &self.songs_dir }
    }

    pub fn folder(&self, kind: &str) -> PathBuf {
        self.dir(kind).to_path_buf()
    }

    pub fn status(&self) -> DownloadStatus {
        lock(&self.status).clone()
    }
}

fn changed(state: &AppState) {
    let _ = state.app.emit("storage-changed", ());
}

fn set_status(state: &AppState, f: impl FnOnce(&mut DownloadStatus)) {
    let snapshot = {
        let mut st = lock(&state.storage.status);
        f(&mut st);
        st.clone()
    };
    let _ = state.app.emit("download-status", snapshot);
}

/// The stored file for a song, if it is still there. A row whose file vanished is dropped.
pub fn on_disk(state: &AppState, track_id: &str) -> AppResult<Option<OnDisk>> {
    type Row = (String, String, String, String, bool, Option<f64>);
    let row: Option<Row> = state.db.with(|c| {
        c.query_row(
            "SELECT kind,file,mime,played,stand_in,loudness_db FROM audio_files WHERE track_id=?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()
    })?;
    let Some((kind, file, mime, played, stand_in, loudness)) = row else { return Ok(None) };
    let path = state.storage.dir(&kind).join(&file);
    match (path.is_file(), serde_json::from_str::<Track>(&played)) {
        (true, Ok(played)) => {
            state.db.with(|c| c.execute("UPDATE audio_files SET last_used=?2 WHERE track_id=?1", params![track_id, now_secs()]))?;
            Ok(Some(OnDisk { path, mime, played, stand_in, loudness_db: loudness.map(|l| l as f32) }))
        }
        _ => {
            discard(state, track_id);
            Ok(None)
        }
    }
}

/// Takes a finished download from the buffer folder into the cache, or into downloads when
/// the song is saved for offline. Returns the file's new place.
pub fn keep(state: &AppState, track_id: &str, from: &Path, played: &Track, stand_in: bool, mime: &str, loudness_db: Option<f32>) -> AppResult<PathBuf> {
    let kind = if state.db.with(|c| is_wanted(c, track_id, liked_too(state)))? { DOWNLOAD } else { CACHE };
    let file = format!("{}.{}", file_stem(track_id), ext_for_mime(mime));
    let to = state.storage.dir(kind).join(&file);
    move_file(from, &to)?;
    let bytes = std::fs::metadata(&to)?.len();
    let played = serde_json::to_string(played)?;
    state.db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO audio_files(track_id,kind,file,bytes,mime,played,stand_in,loudness_db,last_used)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![track_id, kind, file, bytes as i64, mime, played, stand_in, loudness_db.map(f64::from), now_secs()],
        )
    })?;
    if kind == CACHE {
        enforce_cache_limit(state);
    }
    changed(state);
    Ok(to)
}

/// Forgets a song's stored file (damaged, or its match was fixed).
pub fn discard(state: &AppState, track_id: &str) {
    let row: Option<(String, String)> = state
        .db
        .with(|c| c.query_row("SELECT kind,file FROM audio_files WHERE track_id=?1", [track_id], |r| Ok((r.get(0)?, r.get(1)?))).optional())
        .ok()
        .flatten();
    if let Some((kind, file)) = row {
        let _ = std::fs::remove_file(state.storage.dir(&kind).join(file));
        let _ = state.db.with(|c| c.execute("DELETE FROM audio_files WHERE track_id=?1", [track_id]));
        changed(state);
    }
}

/// Moves a stored song between the cache and downloads.
fn relocate(state: &AppState, track_id: &str, file: &str, from: &str, to: &str) -> AppResult<()> {
    let target = state.storage.dir(to).join(file);
    move_file(&state.storage.dir(from).join(file), &target)?;
    state.db.with(|c| c.execute("UPDATE audio_files SET kind=?2 WHERE track_id=?1", params![track_id, to]))?;
    state.streams.relocate(track_id, &target);
    Ok(())
}

/// Removes the least recently played cached songs until the cache fits its limit again.
pub fn enforce_cache_limit(state: &AppState) {
    let Some(limit) = limit_bytes(state, "maxCacheGb", DEFAULT_MAX_CACHE_GB) else { return };
    let rows: Vec<(String, u64)> = state
        .db
        .with(|c| {
            let mut s = c.prepare("SELECT track_id,bytes FROM audio_files WHERE kind='cache' ORDER BY last_used ASC")?;
            let rows = s.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)?.max(0) as u64)))?.collect();
            rows
        })
        .unwrap_or_default();
    let art = dir_size(&state.storage.art_dir);
    let live: HashSet<String> = state.streams.live_ids().into_iter().collect();
    let evict: Vec<String> = pick_evictions(&rows, art, limit, &live).into_iter().map(String::from).collect();
    for id in &evict {
        discard(state, id);
    }
    let songs: u64 = rows.iter().filter(|(id, _)| !evict.contains(id)).map(|(_, b)| b).sum();
    if songs + art > limit {
        trim_artwork(state, limit.saturating_sub(songs));
    }
}

/// Deletes the oldest cached covers until the artwork cache fits in `keep` bytes.
fn trim_artwork(state: &AppState, keep: u64) {
    let Ok(entries) = std::fs::read_dir(&state.storage.art_dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    files.sort();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    for (_, bytes, path) in files {
        if total <= keep {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= bytes;
        }
    }
}

/// Drops rows whose files are gone and files no row knows about (e.g. after a crash).
pub fn tidy(state: &AppState) {
    let rows: Vec<(String, String, String)> = state
        .db
        .with(|c| {
            let mut s = c.prepare("SELECT track_id,kind,file FROM audio_files")?;
            let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect();
            rows
        })
        .unwrap_or_default();
    let mut known = HashSet::new();
    for (id, kind, file) in rows {
        let path = state.storage.dir(&kind).join(&file);
        if path.is_file() {
            known.insert(path);
        } else {
            let _ = state.db.with(|c| c.execute("DELETE FROM audio_files WHERE track_id=?1", [&id]));
        }
    }
    for dir in [&state.storage.songs_dir, &state.storage.downloads_dir] {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if !known.contains(&entry.path()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Brings the downloads in line with what's wanted: moves songs between cache and downloads,
/// then downloads what's missing, one song at a time. Only one run at a time; a call during a
/// run makes it start over with the new list when it is done.
pub fn sync(state: &Arc<AppState>) {
    let storage = &state.storage;
    if storage.running.swap(true, Ordering::SeqCst) {
        storage.again.store(true, Ordering::SeqCst);
        return;
    }
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            state.storage.again.store(false, Ordering::SeqCst);
            if let Err(e) = run(&state).await {
                eprintln!("download sync failed: {e}");
            }
            if !state.storage.again.load(Ordering::SeqCst) {
                break;
            }
        }
        state.storage.running.store(false, Ordering::SeqCst);
        // A request that arrived right after the last check must not be lost.
        if state.storage.again.swap(false, Ordering::SeqCst) {
            sync(&state);
        }
    });
}

async fn run(state: &Arc<AppState>) -> AppResult<()> {
    let liked = liked_too(state);
    let wanted = state.db.with(|c| wanted(c, liked))?;
    let wanted_set: HashSet<&str> = wanted.iter().map(String::as_str).collect();
    let rows: Vec<(String, String, String)> = state.db.with(|c| {
        let mut s = c.prepare("SELECT track_id,kind,file FROM audio_files")?;
        let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect();
        rows
    })?;
    let mut have = HashSet::new();
    for (id, kind, file) in &rows {
        let want = wanted_set.contains(id.as_str());
        if kind == DOWNLOAD && !want {
            // Unsaved songs become ordinary cached songs, removed when space is needed.
            relocate(state, id, file, DOWNLOAD, CACHE)?;
        } else if kind == CACHE && want {
            relocate(state, id, file, CACHE, DOWNLOAD)?;
        }
        if want {
            have.insert(id.clone());
        }
    }
    enforce_cache_limit(state);
    changed(state);

    let missing: Vec<&String> = wanted.iter().filter(|id| !have.contains(*id)).collect();
    let limit = limit_bytes(state, "downloadLimitGb", DEFAULT_DOWNLOAD_LIMIT_GB);
    set_status(state, |s| *s = DownloadStatus { active: !missing.is_empty(), done: have.len(), total: wanted.len(), full: false, failed: 0 });
    for id in missing {
        if state.storage.again.load(Ordering::SeqCst) || !state.net.online() {
            // The list changed (a fresh run follows), or the internet is gone (going back
            // online starts a new run).
            break;
        }
        if limit.is_some_and(|l| download_bytes(state) >= l) {
            set_status(state, |s| s.full = true);
            break;
        }
        match download_one(state, id).await {
            Ok(()) => set_status(state, |s| s.done += 1),
            Err(e) => {
                eprintln!("download of {id} failed: {e}");
                set_status(state, |s| s.failed += 1);
            }
        }
    }
    set_status(state, |s| s.active = false);
    Ok(())
}

fn download_bytes(state: &AppState) -> u64 {
    state
        .db
        .with(|c| c.query_row("SELECT COALESCE(SUM(bytes),0) FROM audio_files WHERE kind='download'", [], |r| r.get::<_, i64>(0)))
        .map(|b| b.max(0) as u64)
        .unwrap_or(0)
}

async fn download_one(state: &Arc<AppState>, track_id: &str) -> AppResult<()> {
    let track = state.db.track(track_id)?.ok_or_else(|| AppError::new("not_found", "song details missing"))?;
    let buf = crate::streams::prepare(state, &track, Quality::Best, false).await?;
    buf.wait_done().await?;
    // A song that finished as cache before it was saved moves over now.
    let kind: Option<(String, String)> =
        state.db.with(|c| c.query_row("SELECT kind,file FROM audio_files WHERE track_id=?1", [track_id], |r| Ok((r.get(0)?, r.get(1)?))).optional())?;
    match kind {
        Some((k, _)) if k == DOWNLOAD => Ok(()),
        Some((_, file)) => relocate(state, track_id, &file, CACHE, DOWNLOAD),
        None => Err(AppError::new("io", "download was not stored")),
    }
}

// ---- marks --------------------------------------------------------------------------------

pub fn set_offline(state: &AppState, account: &str, tracks: &[Track], on: bool) -> AppResult<()> {
    state.db.with(|c| {
        let tx = c.unchecked_transaction()?;
        for t in tracks {
            if on {
                crate::db::upsert_track_conn(&tx, t)?;
                tx.execute(
                    "INSERT OR IGNORE INTO offline(account_id,track_id,added_at) VALUES(?1,?2,?3)",
                    params![account, t.id, crate::util::now_millis()],
                )?;
            } else {
                tx.execute("DELETE FROM offline WHERE account_id=?1 AND track_id=?2", params![account, t.id])?;
            }
        }
        tx.commit()
    })
}

pub fn set_playlist_offline(state: &AppState, account: &str, playlist: &str, on: bool) -> AppResult<()> {
    let n = state.db.with(|c| c.execute("UPDATE playlists SET offline=?3 WHERE id=?1 AND account_id=?2", params![playlist, account, on]))?;
    if n == 0 {
        return Err(AppError::new("not_found", "playlist not found"));
    }
    Ok(())
}

/// Saved songs and saved playlists of this account.
pub fn marks(state: &AppState, account: &str) -> AppResult<(Vec<String>, Vec<String>)> {
    state.db.with(|c| {
        let mut s = c.prepare("SELECT track_id FROM offline WHERE account_id=?1")?;
        let songs = s.query_map([account], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut s = c.prepare("SELECT id FROM playlists WHERE account_id=?1 AND offline=1 AND deleted_at IS NULL")?;
        let lists = s.query_map([account], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok((songs, lists))
    })
}

// ---- what the UI shows --------------------------------------------------------------------

pub fn available(state: &AppState) -> AppResult<Available> {
    state.db.with(|c| {
        let mut s = c.prepare("SELECT track_id,kind FROM audio_files")?;
        let mut out = Available { downloaded: Vec::new(), cached: Vec::new() };
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (id, kind) = row?;
            if kind == DOWNLOAD { out.downloaded.push(id) } else { out.cached.push(id) }
        }
        Ok(out)
    })
}

/// The account's songs that play without internet, split into downloads and cache.
pub fn offline_lists(state: &AppState, account: &str) -> AppResult<OfflineLists> {
    let sql = format!(
        "SELECT a.kind, t.data FROM audio_files a JOIN tracks t ON t.id = a.track_id
         WHERE a.track_id IN ({ACCOUNT_SONGS_SQL}) ORDER BY a.last_used DESC"
    );
    state.db.with(|c| {
        let mut s = c.prepare(&sql)?;
        let mut out = OfflineLists { downloaded: Vec::new(), cached: Vec::new() };
        for row in s.query_map([account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (kind, data) = row?;
            let Ok(track) = serde_json::from_str::<Track>(&data) else { continue };
            if kind == DOWNLOAD { out.downloaded.push(track) } else { out.cached.push(track) }
        }
        crate::library::decorate(c, &mut out.downloaded)?;
        crate::library::decorate(c, &mut out.cached)?;
        Ok(out)
    })
}

pub fn usage(state: &AppState, account: &str) -> AppResult<Usage> {
    let (downloads_bytes, downloads_count, cache_songs_bytes, cache_songs_count, liked_bytes, imported_bytes) = state.db.with(|c| {
        let sum = |kind: &str| -> rusqlite::Result<(u64, u64)> {
            c.query_row("SELECT COALESCE(SUM(bytes),0), COUNT(*) FROM audio_files WHERE kind=?1", [kind], |r| {
                Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, i64>(1)?.max(0) as u64))
            })
        };
        let (db, dc) = sum(DOWNLOAD)?;
        let (cb, cc) = sum(CACHE)?;
        let liked: i64 = c.query_row(
            "SELECT COALESCE(SUM(a.bytes),0) FROM audio_files a JOIN likes l ON l.track_id = a.track_id AND l.account_id = ?1 WHERE a.kind='download'",
            [account],
            |r| r.get(0),
        )?;
        let imported: i64 = c.query_row("SELECT COALESCE(SUM(size),0) FROM local_files WHERE trashed_at IS NULL", [], |r| r.get(0))?;
        Ok((db, dc, cb, cc, liked.max(0) as u64, imported.max(0) as u64))
    })?;
    let files = downloads_count + cache_songs_count;
    let average_song_bytes =
        if files as i64 >= AVERAGE_FROM { (downloads_bytes + cache_songs_bytes) / files } else { TYPICAL_SONG_BYTES };
    Ok(Usage {
        downloads_bytes,
        downloads_count,
        liked_bytes,
        imported_bytes,
        cache_songs_bytes,
        cache_songs_count,
        artwork_bytes: dir_size(&state.storage.art_dir),
        free_bytes: free_space(&state.storage.downloads_dir),
        average_song_bytes,
    })
}

/// "Clear" buttons on the Cache & Storage page.
pub fn clear(state: &Arc<AppState>, account: &str, what: &str) -> AppResult<()> {
    match what {
        "songs" => {
            let live: HashSet<String> = state.streams.live_ids().into_iter().collect();
            let ids: Vec<String> = state.db.with(|c| {
                let mut s = c.prepare("SELECT track_id FROM audio_files WHERE kind='cache'")?;
                let rows = s.query_map([], |r| r.get(0))?.collect();
                rows
            })?;
            for id in ids.iter().filter(|id| !live.contains(*id)) {
                discard(state, id);
            }
        }
        "artwork" => {
            for entry in std::fs::read_dir(&state.storage.art_dir)?.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
            state.covers.clear_memory();
        }
        "downloads" => {
            // Removes this account's saves; songs other accounts (or Liked Songs downloads)
            // still want stay.
            state.db.with(|c| {
                c.execute("DELETE FROM offline WHERE account_id=?1", [account])?;
                c.execute("UPDATE playlists SET offline=0 WHERE account_id=?1", [account])
            })?;
            let still: HashSet<String> = state.db.with(|c| wanted(c, liked_too(state)))?.into_iter().collect();
            let ids: Vec<String> = state.db.with(|c| {
                let mut s = c.prepare("SELECT track_id FROM audio_files WHERE kind='download'")?;
                let rows = s.query_map([], |r| r.get(0))?.collect();
                rows
            })?;
            for id in ids.iter().filter(|id| !still.contains(*id)) {
                state.streams.forget(id);
                discard(state, id);
            }
        }
        _ => return Err(AppError::new("invalid_input", "unknown storage kind")),
    }
    changed(state);
    sync(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn evicts_oldest_until_it_fits() {
        let rows: Vec<(String, u64)> = vec![("a".into(), 40), ("b".into(), 30), ("c".into(), 20), ("d".into(), 10)];
        let none = HashSet::new();
        assert_eq!(pick_evictions(&rows, 0, 100, &none), Vec::<&str>::new());
        assert_eq!(pick_evictions(&rows, 0, 60, &none), vec!["a"]);
        assert_eq!(pick_evictions(&rows, 25, 60, &none), vec!["a", "b"]);
        // A song that may be playing is skipped, the next oldest goes instead.
        let live: HashSet<String> = ["a".to_string()].into();
        assert_eq!(pick_evictions(&rows, 0, 60, &live), vec!["b", "c"]);
    }

    #[test]
    fn file_names_are_case_safe() {
        assert_ne!(file_stem("youtube:aBc"), file_stem("youtube:abc"));
        assert_eq!(file_stem("x").len(), 24);
        assert_eq!(ext_for_mime("audio/webm; codecs=\"opus\""), "webm");
        assert_eq!(ext_for_mime("audio/mp4"), "m4a");
        assert_eq!(ext_for_mime("application/x-weird"), "audio");
    }

    fn track(id: &str) -> Track {
        let (source, sid) = id.split_once(':').unwrap();
        Track::imported(source, sid, "Song", vec!["Artist".into()], None, None, None)
    }

    #[test]
    fn wanted_covers_saves_playlists_and_liked() {
        let db = Db::in_memory();
        let acc = crate::accounts::create(&db, "a@b.test", "password1", false).unwrap().account.id;
        let pl = crate::library::create_playlist(&db, &acc, "Mix", None).unwrap();
        crate::library::add_tracks(&db, &acc, &pl, &[track("youtube:p1"), track("local:l1")]).unwrap();
        crate::library::set_like(&db, &acc, &track("youtube:l2"), true).unwrap();
        db.with(|c| c.execute("INSERT INTO offline(account_id,track_id,added_at) VALUES(?1,'soundcloud:s1',0)", [&acc])).unwrap();

        let sorted = |mut v: Vec<String>| {
            v.sort();
            v
        };
        assert_eq!(db.with(|c| wanted(c, false)).map(sorted).unwrap(), vec!["soundcloud:s1"]);
        db.with(|c| c.execute("UPDATE playlists SET offline=1", [])).unwrap();
        // Local music is on disk already, so it is never downloaded.
        assert_eq!(db.with(|c| wanted(c, false)).map(sorted).unwrap(), vec!["soundcloud:s1", "youtube:p1"]);
        assert_eq!(db.with(|c| wanted(c, true)).map(sorted).unwrap(), vec!["soundcloud:s1", "youtube:l2", "youtube:p1"]);
        assert!(db.with(|c| is_wanted(c, "youtube:l2", true)).unwrap());
        assert!(!db.with(|c| is_wanted(c, "youtube:l2", false)).unwrap());
        // A trashed playlist no longer keeps its songs.
        crate::library::trash_playlist(&db, &acc, &pl).unwrap();
        assert!(!db.with(|c| is_wanted(c, "youtube:p1", false)).unwrap());
    }
}
