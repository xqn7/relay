//! The Local music folder: files the user adds (copied in, never moved), files they put there
//! themselves (picked up by a folder watcher, no polling), tags and covers, video files
//! (audio extracted with ffmpeg), and the trash for removed songs.

use crate::error::{AppError, AppResult};
use crate::sources::{AlbumRef, ArtistRef, Track, LOCAL};
use crate::util::now_secs;
use crate::AppState;
use lofty::config::WriteOptions;
use lofty::picture::{Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::Tag;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};
use tauri::{Emitter, Manager};

pub const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "ogg", "opus"];
pub const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "mov"];
const FOLDER_KEY: &str = "local_folder";
const MAX_DEPTH: usize = 8;
/// Changes usually come in bursts (a copy writes many times); scan once they settle.
const SETTLE: Duration = Duration::from_millis(800);
const COVER_MAX_BYTES: usize = 8 * 1024 * 1024;

pub struct LocalMusic {
    folder: Mutex<PathBuf>,
    art_dir: PathBuf,
    trash_dir: PathBuf,
    watcher: Mutex<Option<RecommendedWatcher>>,
    rescan_pending: AtomicBool,
    scan_lock: tokio::sync::Mutex<()>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AddProgress {
    done: usize,
    total: usize,
    /// Set when the whole batch is finished.
    finished: bool,
    failed: Vec<String>,
    /// The ffmpeg helper is being downloaded first.
    converting: bool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn ext_of(p: &Path) -> String {
    p.extension().and_then(OsStr::to_str).unwrap_or("").to_lowercase()
}

pub fn is_audio(p: &Path) -> bool {
    AUDIO_EXTS.contains(&ext_of(p).as_str())
}

pub fn is_video(p: &Path) -> bool {
    VIDEO_EXTS.contains(&ext_of(p).as_str())
}

pub fn mime_for(p: &Path) -> &'static str {
    match ext_of(p).as_str() {
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

/// Stable id from the path inside the folder, so the same file keeps its likes and playlist
/// spots across restarts and PCs.
pub fn id_for(relative: &str) -> String {
    let digest = Sha256::digest(relative.replace('\\', "/").to_lowercase().as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// "Artist - Title.mp3" → (Some("Artist"), "Title"); otherwise the whole name is the title.
pub fn names_from_file(stem: &str) -> (Option<String>, String) {
    match stem.split_once(" - ") {
        Some((a, t)) if !a.trim().is_empty() && !t.trim().is_empty() => (Some(a.trim().to_string()), t.trim().to_string()),
        _ => (None, stem.trim().to_string()),
    }
}

fn mtime_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Picks a free name in `dir` ("song.mp3", then "song (2).mp3", …).
fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && depth < MAX_DEPTH {
            walk(&path, depth + 1, out);
        } else if kind.is_file() && (is_audio(&path) || is_video(&path)) {
            out.push(path);
        }
    }
}

impl LocalMusic {
    pub fn new(app: &tauri::AppHandle, db: &crate::db::Db) -> AppResult<Self> {
        let data = app.path().app_local_data_dir().map_err(|e| AppError::new("io", e.to_string()))?;
        let folder = match db.device_get(FOLDER_KEY)? {
            Some(saved) => PathBuf::from(saved),
            None => app
                .path()
                .audio_dir()
                .or_else(|_| app.path().home_dir().map(|h| h.join("Music")))
                .map_err(|e| AppError::new("io", e.to_string()))?
                .join("Relay")
                .join("Local music"),
        };
        let art_dir = data.join("local-art");
        let trash_dir = data.join("local-trash");
        std::fs::create_dir_all(&art_dir)?;
        std::fs::create_dir_all(&trash_dir)?;
        Ok(Self {
            folder: Mutex::new(folder),
            art_dir,
            trash_dir,
            watcher: Mutex::new(None),
            rescan_pending: AtomicBool::new(false),
            scan_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn folder(&self) -> PathBuf {
        lock(&self.folder).clone()
    }

    pub fn art_path(&self, file: &str) -> Option<PathBuf> {
        let ok = file.len() < 40 && file.ends_with(".jpg") && file.trim_end_matches(".jpg").chars().all(|c| c.is_ascii_hexdigit());
        ok.then(|| self.art_dir.join(file))
    }
}

/// Creates the folder, scans it and starts watching. Runs at startup in the background.
pub async fn start(state: Arc<AppState>) {
    let folder = state.local.folder();
    if let Err(e) = std::fs::create_dir_all(&folder) {
        eprintln!("local music folder unavailable: {e}");
    }
    if let Err(e) = scan(&state).await {
        eprintln!("local music scan failed: {e}");
    }
    watch(&state);
    purge_trash(&state);
}

fn watch(state: &Arc<AppState>) {
    let folder = state.local.folder();
    let weak = Arc::downgrade(state);
    let handler = move |res: notify::Result<notify::Event>| {
        if res.is_err() {
            return;
        }
        let Some(state) = weak.upgrade() else { return };
        if state.local.rescan_pending.swap(true, Ordering::SeqCst) {
            return;
        }
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(SETTLE).await;
            state.local.rescan_pending.store(false, Ordering::SeqCst);
            if let Err(e) = scan(&state).await {
                eprintln!("local music rescan failed: {e}");
            }
        });
    };
    let watcher = notify::recommended_watcher(handler).and_then(|mut w| w.watch(&folder, RecursiveMode::Recursive).map(|_| w));
    match watcher {
        Ok(w) => *lock(&state.local.watcher) = Some(w),
        Err(e) => eprintln!("can't watch the local music folder: {e}"),
    }
}

struct Known {
    track_id: String,
    size: i64,
    mtime: i64,
    trashed: bool,
}

/// Brings the database in line with the folder: new and changed files are read, files that
/// are gone disappear. Video files without extracted audio next to them are converted.
pub async fn scan(state: &Arc<AppState>) -> AppResult<()> {
    let _guard = state.local.scan_lock.lock().await;
    let folder = state.local.folder();
    let known: HashMap<String, Known> = state.db.with(|c| {
        let mut s = c.prepare("SELECT path, track_id, size, mtime, trashed_at FROM local_files")?;
        let rows = s
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, Known { track_id: r.get(1)?, size: r.get(2)?, mtime: r.get(3)?, trashed: r.get::<_, Option<i64>>(4)?.is_some() }))
            })?
            .collect();
        rows
    })?;
    let (state2, folder2) = (state.clone(), folder.clone());
    let (changed, videos) = tokio::task::spawn_blocking(move || scan_files(&state2, &folder2, known))
        .await
        .map_err(|e| AppError::new("internal", e.to_string()))??;
    if changed {
        let _ = state.app.emit("library-changed", ());
    }
    if !videos.is_empty() {
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            for v in videos {
                if let Err(e) = extract_audio(&state, &v, v.parent().unwrap_or(&folder)).await {
                    eprintln!("could not extract audio from {}: {e}", v.display());
                }
            }
        });
    }
    Ok(())
}

fn scan_files(state: &AppState, folder: &Path, mut known: HashMap<String, Known>) -> AppResult<(bool, Vec<PathBuf>)> {
    let mut files = Vec::new();
    walk(folder, 0, &mut files);
    let mut changed = false;
    let mut videos = Vec::new();
    let stems: std::collections::HashSet<PathBuf> = files.iter().filter(|p| is_audio(p)).map(|p| p.with_extension("")).collect();
    for path in files {
        if is_video(&path) {
            if !stems.contains(&path.with_extension("")) {
                videos.push(path);
            }
            continue;
        }
        let key = path.to_string_lossy().to_string();
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let (size, mtime) = (meta.len() as i64, mtime_of(&meta));
        if let Some(k) = known.remove(&key) {
            if k.trashed || (k.size == size && k.mtime == mtime) {
                continue;
            }
        }
        let relative = path.strip_prefix(folder).unwrap_or(&path).to_string_lossy().to_string();
        let id = id_for(&relative);
        let track = read_track(state, &path, &id, mtime);
        state.db.with(|c| {
            crate::db::upsert_track_conn(c, &track)?;
            c.execute(
                "INSERT INTO local_files(track_id,path,size,mtime) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(track_id) DO UPDATE SET path=excluded.path, size=excluded.size, mtime=excluded.mtime",
                params![id, key, size, mtime],
            )
        })?;
        changed = true;
    }
    // Whatever is left was deleted or moved out of the folder by the user.
    for (_, k) in known.into_iter().filter(|(_, k)| !k.trashed) {
        state.db.with(|c| c.execute("DELETE FROM local_files WHERE track_id=?1", [&k.track_id]))?;
        changed = true;
    }
    Ok((changed, videos))
}

/// Tags, duration and cover of one file; the file name stands in for missing tags.
fn read_track(state: &AppState, path: &Path, id: &str, mtime: i64) -> Track {
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("Untitled");
    let (file_artist, file_title) = names_from_file(stem);
    let tagged = lofty::read_from_path(path).ok();
    let tag = tagged.as_ref().and_then(|t| t.primary_tag().or_else(|| t.first_tag()));
    let title = tag.and_then(|t| t.title().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).unwrap_or(file_title);
    let artist = tag.and_then(|t| t.artist().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).or(file_artist);
    let album = tag.and_then(|t| t.album().map(|s| s.trim().to_string())).filter(|s| !s.is_empty());
    let duration = tagged.as_ref().map(|t| t.properties().duration().as_millis() as u64).filter(|d| *d > 0);
    let art_file = format!("{id}.jpg");
    let art_path = state.local.art_dir.join(&art_file);
    let picture = tag.and_then(|t| t.pictures().iter().find(|p| p.pic_type() == PictureType::CoverFront).or_else(|| t.pictures().first()));
    let cover = match picture {
        Some(p) if p.data().len() <= COVER_MAX_BYTES && std::fs::write(&art_path, p.data()).is_ok() => {
            Some(format!("http://relay.localhost/art/{art_file}?v={mtime}"))
        }
        _ => {
            let _ = std::fs::remove_file(&art_path);
            None
        }
    };
    Track {
        id: Track::make_id(LOCAL, id),
        source: LOCAL.into(),
        source_id: id.into(),
        title,
        artists: artist.map(|name| vec![ArtistRef { id: None, name }]).unwrap_or_default(),
        album: album.map(|name| AlbumRef { id: String::new(), name }),
        duration_ms: duration,
        cover,
        is_video: false,
        link: None,
        via: None,
    }
}

pub fn list(state: &AppState) -> AppResult<Vec<Track>> {
    state.db.with(|c| {
        let mut s = c.prepare("SELECT t.data FROM local_files l JOIN tracks t ON t.id='local:'||l.track_id WHERE l.trashed_at IS NULL ORDER BY l.mtime DESC")?;
        let rows: Vec<String> = s.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows.iter().filter_map(|d| serde_json::from_str(d).ok()).collect())
    })
}

pub fn path_for(state: &AppState, source_id: &str) -> AppResult<PathBuf> {
    let path: Option<String> = state.db.with(|c| {
        c.query_row("SELECT path FROM local_files WHERE track_id=?1 AND trashed_at IS NULL", [source_id], |r| r.get(0)).optional()
    })?;
    path.map(PathBuf::from).filter(|p| p.exists()).ok_or_else(|| AppError::new("not_found", "the file is no longer in the Local music folder"))
}

/// Copies files and folders into the Local music folder (originals are never touched) and
/// extracts the audio of video files. Progress goes out as "local-progress" events.
pub fn add(state: Arc<AppState>, paths: Vec<PathBuf>) {
    tauri::async_runtime::spawn(async move {
        let mut files = Vec::new();
        for p in paths {
            if p.is_dir() {
                walk(&p, 0, &mut files);
            } else if is_audio(&p) || is_video(&p) {
                files.push(p);
            }
        }
        let total = files.len();
        let folder = state.local.folder();
        let _ = std::fs::create_dir_all(&folder);
        let mut failed = Vec::new();
        let emit = |done: usize, finished: bool, failed: &Vec<String>, converting: bool| {
            let _ = state.app.emit("local-progress", AddProgress { done, total, finished, failed: failed.clone(), converting });
        };
        emit(0, total == 0, &failed, false);
        for (i, file) in files.iter().enumerate() {
            let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            // Files already inside the folder (dragged from it) need no copy.
            let inside = file.starts_with(&folder);
            let result = if inside {
                Ok(())
            } else if is_video(file) {
                emit(i, false, &failed, true);
                extract_audio(&state, file, &folder).await
            } else {
                let stem = file.file_stem().and_then(OsStr::to_str).unwrap_or("song");
                let dest = free_path(&folder, stem, &ext_of(file));
                tokio::fs::copy(file, &dest).await.map(|_| ()).map_err(AppError::from)
            };
            if let Err(e) = result {
                eprintln!("could not add {name}: {e}");
                failed.push(name);
            }
            emit(i + 1, false, &failed, false);
        }
        let _ = scan(&state).await;
        emit(total, true, &failed, false);
    });
}

/// Pulls the audio track out of a video without re-encoding when the codec fits a normal
/// audio file, and uses a frame of the video as the cover.
async fn extract_audio(state: &Arc<AppState>, video: &Path, dest_dir: &Path) -> AppResult<()> {
    let codec = state.tools.audio_codec(video).await?.ok_or_else(|| AppError::new("no_audio", "the video has no sound"))?;
    let copy_ext = match codec.as_str() {
        "aac" | "alac" => Some("m4a"),
        "mp3" => Some("mp3"),
        "opus" => Some("opus"),
        "vorbis" => Some("ogg"),
        "flac" => Some("flac"),
        _ => None,
    };
    let stem = video.file_stem().and_then(OsStr::to_str).unwrap_or("video");
    let tmp_dir = std::env::temp_dir().join(format!("relay-{}", crate::util::uuid_v4()));
    tokio::fs::create_dir_all(&tmp_dir).await?;
    let result = async {
        let ext = copy_ext.unwrap_or("m4a");
        // Work in a temp folder so the watcher never sees a half-written file.
        let tmp_audio = tmp_dir.join(format!("audio.{ext}"));
        let mut args: Vec<&OsStr> = vec![OsStr::new("-i"), video.as_os_str(), OsStr::new("-vn"), OsStr::new("-map"), OsStr::new("0:a:0")];
        if copy_ext.is_some() {
            args.extend([OsStr::new("-c:a"), OsStr::new("copy")]);
        } else {
            args.extend([OsStr::new("-c:a"), OsStr::new("aac"), OsStr::new("-b:a"), OsStr::new("256k")]);
        }
        args.push(tmp_audio.as_os_str());
        if state.tools.run_ffmpeg(&args).await.is_err() && copy_ext.is_some() {
            // Some containers refuse a straight copy; re-encode as a last resort.
            let tmp = tmp_dir.join("audio.m4a");
            state
                .tools
                .run_ffmpeg(&[OsStr::new("-i"), video.as_os_str(), OsStr::new("-vn"), OsStr::new("-map"), OsStr::new("0:a:0"), OsStr::new("-c:a"), OsStr::new("aac"), OsStr::new("-b:a"), OsStr::new("256k"), tmp.as_os_str()])
                .await?;
            return finish_extract(state, video, &tmp, &tmp_dir, dest_dir, stem, "m4a").await;
        }
        finish_extract(state, video, &tmp_audio, &tmp_dir, dest_dir, stem, ext).await
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
    result
}

async fn finish_extract(state: &Arc<AppState>, video: &Path, audio: &Path, tmp_dir: &Path, dest_dir: &Path, stem: &str, ext: &str) -> AppResult<()> {
    let frame = tmp_dir.join("cover.jpg");
    // A few seconds in skips black intro frames; very short clips fall back to the first frame.
    for at in ["5", "0"] {
        let args = [OsStr::new("-ss"), OsStr::new(at), OsStr::new("-i"), video.as_os_str(), OsStr::new("-frames:v"), OsStr::new("1"), OsStr::new("-vf"), OsStr::new("scale=600:-2"), frame.as_os_str()];
        if state.tools.run_ffmpeg(&args).await.is_ok() && frame.exists() {
            break;
        }
    }
    let (audio_path, frame_path) = (audio.to_path_buf(), frame.clone());
    tokio::task::spawn_blocking(move || {
        // A missing cover is cosmetic; the song is still added.
        if frame_path.exists() {
            let _ = embed_cover_if_missing(&audio_path, &frame_path);
        }
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?;
    let dest = free_path(dest_dir, stem, ext);
    tokio::fs::copy(audio, &dest).await?;
    Ok(())
}

fn embed_cover_if_missing(audio: &Path, image: &Path) -> AppResult<()> {
    let mut file = lofty::read_from_path(audio).map_err(|e| AppError::new("tags", e.to_string()))?;
    let has_picture = file.primary_tag().is_some_and(|t| !t.pictures().is_empty());
    if has_picture {
        return Ok(());
    }
    let tag_type = file.primary_tag_type();
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(tag_type));
    }
    let tag = file.primary_tag_mut().ok_or_else(|| AppError::new("tags", "no tag"))?;
    let mut reader = std::fs::File::open(image)?;
    let picture = Picture::from_reader(&mut reader).map_err(|e| AppError::new("tags", e.to_string()))?;
    tag.push_picture(picture_as_front(picture));
    tag.save_to_path(audio, WriteOptions::default()).map_err(|e| AppError::new("tags", e.to_string()))?;
    Ok(())
}

fn picture_as_front(p: Picture) -> Picture {
    let mut builder = Picture::unchecked(p.data().to_vec()).pic_type(PictureType::CoverFront);
    if let Some(m) = p.mime_type() {
        builder = builder.mime_type(m.clone());
    }
    builder.build()
}

/// Writes new title/artist (and optionally a cover image) into the file's own tags, so the
/// change survives outside the app too.
pub async fn edit(state: &Arc<AppState>, source_id: &str, title: &str, artist: &str, cover: Option<PathBuf>) -> AppResult<()> {
    let path = path_for(state, source_id)?;
    let (title, artist) = (title.trim().to_string(), artist.trim().to_string());
    if title.is_empty() || title.len() > 500 || artist.len() > 500 {
        return Err(AppError::new("invalid_input", "bad title or artist"));
    }
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let mut file = lofty::read_from_path(&path).map_err(|e| AppError::new("tags", e.to_string()))?;
        let tag_type = file.primary_tag_type();
        if file.primary_tag().is_none() {
            file.insert_tag(Tag::new(tag_type));
        }
        let tag = file.primary_tag_mut().ok_or_else(|| AppError::new("tags", "this file type can't hold tags"))?;
        tag.set_title(title);
        if artist.is_empty() {
            tag.remove_artist();
        } else {
            tag.set_artist(artist);
        }
        if let Some(img) = cover {
            let meta = std::fs::metadata(&img)?;
            if meta.len() as usize > COVER_MAX_BYTES {
                return Err(AppError::new("too_large", "cover image is too large"));
            }
            let picture = Picture::from_reader(&mut std::fs::File::open(&img)?).map_err(|_| AppError::new("bad_image", "not a JPEG or PNG image"))?;
            tag.remove_picture_type(PictureType::CoverFront);
            tag.push_picture(picture_as_front(picture));
        }
        tag.save_to_path(&path, WriteOptions::default()).map_err(|e| AppError::new("tags", e.to_string()))?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))??;
    scan(state).await
}

/// Moves the file out of the folder into the app's trash, restorable for 30 days.
pub async fn remove(state: &Arc<AppState>, source_id: &str) -> AppResult<()> {
    let path = path_for(state, source_id)?;
    let dest = state.local.trash_dir.join(format!("{source_id}.{}", ext_of(&path)));
    move_file(&path, &dest).await?;
    state.db.with(|c| {
        c.execute("UPDATE local_files SET trashed_at=?2, trash_path=?3 WHERE track_id=?1", params![source_id, now_secs(), dest.to_string_lossy()])
    })?;
    let _ = state.app.emit("library-changed", ());
    Ok(())
}

pub async fn restore(state: &Arc<AppState>, source_id: &str) -> AppResult<()> {
    let row: Option<(String, Option<String>)> =
        state.db.with(|c| c.query_row("SELECT path, trash_path FROM local_files WHERE track_id=?1", [source_id], |r| Ok((r.get(0)?, r.get(1)?))).optional())?;
    let (path, Some(trash)) = row.ok_or_else(|| AppError::new("not_found", "not in trash"))? else {
        return Err(AppError::new("not_found", "not in trash"));
    };
    let mut target = PathBuf::from(&path);
    if target.exists() {
        let dir = target.parent().map(Path::to_path_buf).unwrap_or_else(|| state.local.folder());
        target = free_path(&dir, target.file_stem().and_then(OsStr::to_str).unwrap_or("song"), &ext_of(&target));
    }
    if let Some(dir) = target.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    move_file(Path::new(&trash), &target).await?;
    state.db.with(|c| c.execute("DELETE FROM local_files WHERE track_id=?1", [source_id]))?;
    scan(state).await
}

pub async fn delete_forever(state: &Arc<AppState>, source_id: &str) -> AppResult<()> {
    let trash: Option<Option<String>> = state.db.with(|c| c.query_row("SELECT trash_path FROM local_files WHERE track_id=?1", [source_id], |r| r.get(0)).optional())?;
    if let Some(Some(p)) = trash {
        let _ = tokio::fs::remove_file(&p).await;
    }
    let _ = std::fs::remove_file(state.local.art_dir.join(format!("{source_id}.jpg")));
    state.db.with(|c| c.execute("DELETE FROM local_files WHERE track_id=?1 AND trashed_at IS NOT NULL", [source_id]))?;
    Ok(())
}

/// Removed songs older than the trash period are deleted for good.
fn purge_trash(state: &Arc<AppState>) {
    let cutoff = now_secs() - crate::library::TRASH_DAYS * 86_400;
    let old: Vec<String> = state
        .db
        .with(|c| {
            let mut s = c.prepare("SELECT track_id FROM local_files WHERE trashed_at IS NOT NULL AND trashed_at < ?1")?;
            let rows = s.query_map([cutoff], |r| r.get(0))?.collect();
            rows
        })
        .unwrap_or_default();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        for id in old {
            let _ = delete_forever(&state, &id).await;
        }
    });
}

/// Rename when possible; copy + delete across drives.
async fn move_file(from: &Path, to: &Path) -> AppResult<()> {
    if tokio::fs::rename(from, to).await.is_ok() {
        return Ok(());
    }
    tokio::fs::copy(from, to).await?;
    tokio::fs::remove_file(from).await?;
    Ok(())
}

/// Switches to another folder: saves it for this PC, rescans and watches it instead.
pub async fn set_folder(state: &Arc<AppState>, folder: PathBuf) -> AppResult<()> {
    std::fs::create_dir_all(&folder)?;
    state.db.device_set(FOLDER_KEY, &folder.to_string_lossy())?;
    *lock(&state.local.watcher) = None;
    *lock(&state.local.folder) = folder;
    scan(state).await?;
    watch(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_ignore_case_and_slashes() {
        assert_eq!(id_for("Sub\\Song.mp3"), id_for("sub/song.MP3"));
        assert_ne!(id_for("a.mp3"), id_for("b.mp3"));
        assert_eq!(id_for("a.mp3").len(), 16);
    }

    #[test]
    fn names_come_from_file_names() {
        assert_eq!(names_from_file("Daft Punk - One More Time"), (Some("Daft Punk".into()), "One More Time".into()));
        assert_eq!(names_from_file("voice memo 3"), (None, "voice memo 3".into()));
        assert_eq!(names_from_file(" - x"), (None, "- x".into()));
    }

    #[test]
    fn knows_supported_files() {
        assert!(is_audio(Path::new("a.FLAC")));
        assert!(is_video(Path::new("clip.webm")));
        assert!(!is_audio(Path::new("cover.jpg")));
        assert_eq!(mime_for(Path::new("x.opus")), "audio/ogg");
    }

    #[test]
    fn free_names_dont_overwrite() {
        let dir = std::env::temp_dir().join(format!("relay-test-{}", crate::util::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("song.mp3"), b"x").unwrap();
        assert_eq!(free_path(&dir, "song", "mp3"), dir.join("song (2).mp3"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
