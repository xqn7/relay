//! Downloads each song to a file in the background and serves it to the webview through the
//! `relay://` protocol with HTTP Range support. Serving from a local file means an expiring
//! upstream URL can be refreshed mid-download without the player noticing, HLS can be joined
//! into one file, and replays never touch the network. Finished downloads move into the song
//! cache or the downloads folder (see storage.rs), and later plays are served from there.

use crate::error::{AppError, AppResult};
use crate::resolver::{self, Resolution};
use crate::sources::{Quality, StreamKind, Track, LOCAL};
use crate::AppState;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::http::{header, Request, Response, StatusCode};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;
use tokio::task::AbortHandle;

/// Ranged requests in 4 MB pieces, like yt-dlp: YouTube throttles or cuts single huge requests.
const CHUNK: u64 = 4 * 1024 * 1024;
/// Largest slice handed to the webview per request; it asks for the rest as it plays.
const SERVE_MAX: u64 = 1024 * 1024;
const KEEP_BUFFERS: usize = 12;
const MAX_URL_REFRESHES: u32 = 3;
const MAX_NETWORK_RETRIES: u32 = 5;
const SERVE_WAIT: Duration = Duration::from_secs(30);
const HLS_PARALLEL: usize = 6;
const HLS_HEAD_PARALLEL: usize = 16;

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub written: u64,
    pub total: Option<u64>,
    pub done: bool,
    pub failed: Option<AppError>,
}

pub struct Buffer {
    pub played: Track,
    pub stand_in: bool,
    /// True while the file is a download in progress that belongs to this buffer. Local music
    /// files and finished songs (kept by the cache or downloads) are never deleted here.
    owned: AtomicBool,
    pub mime: String,
    pub loudness_db: Option<f32>,
    /// Changes once, when the finished file moves into the cache or the downloads folder.
    path: Mutex<PathBuf>,
    progress: watch::Sender<Progress>,
    task: Mutex<Option<AbortHandle>>,
    last_used: Mutex<Instant>,
}

impl Buffer {
    fn usable(&self) -> bool {
        self.progress.borrow().failed.is_none()
    }
    fn touch(&self) {
        *self.last_used.lock().unwrap_or_else(|p| p.into_inner()) = Instant::now();
    }
    fn path(&self) -> PathBuf {
        self.path.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Waits until the whole song is on disk.
    pub async fn wait_done(&self) -> AppResult<()> {
        let mut rx = self.progress.subscribe();
        loop {
            {
                let p = rx.borrow_and_update();
                if let Some(e) = &p.failed {
                    return Err(e.clone());
                }
                if p.done {
                    return Ok(());
                }
            }
            if rx.changed().await.is_err() {
                return Err(AppError::new("internal", "download stopped"));
            }
        }
    }
}

pub struct Streams {
    dir: PathBuf,
    buffers: Mutex<HashMap<String, Arc<Buffer>>>,
}

impl Streams {
    /// The buffer folder only holds downloads in progress, so leftovers from the last run go.
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let streams = Self { dir, buffers: Mutex::new(HashMap::new()) };
        streams.sweep(&[]);
        Ok(streams)
    }

    /// Deletes buffer files that no live buffer owns. Best effort: a file the webview still has
    /// open can't be deleted on Windows, so this runs again whenever a buffer is added.
    fn sweep(&self, keep: &[PathBuf]) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if !keep.contains(&path) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Buffer>>> {
        self.buffers.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn get(&self, track_id: &str) -> Option<Arc<Buffer>> {
        self.lock().get(track_id).cloned()
    }

    /// Drops a song's buffer so the next play resolves it again (after "Fix match").
    pub fn forget(&self, track_id: &str) {
        if let Some(old) = self.lock().remove(track_id) {
            drop_buffer(&old);
        }
    }

    /// Drops every buffer (after a restore, when matches may have changed).
    pub fn forget_all(&self) {
        let old: Vec<Arc<Buffer>> = self.lock().drain().map(|(_, b)| b).collect();
        for b in old {
            drop_buffer(&b);
        }
    }

    /// Songs with a buffer in memory; the cache never evicts these while they may be playing.
    pub fn live_ids(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    /// Points a live buffer at its file's new place after the file moved between folders.
    pub fn relocate(&self, track_id: &str, path: &Path) {
        if let Some(buf) = self.get(track_id) {
            *buf.path.lock().unwrap_or_else(|p| p.into_inner()) = path.to_path_buf();
        }
    }

    fn file_for(&self, track_id: &str) -> PathBuf {
        self.dir.join(format!("{}.part", crate::storage::file_stem(track_id)))
    }

    fn insert(&self, track_id: String, buf: Arc<Buffer>) {
        let mut map = self.lock();
        if let Some(old) = map.insert(track_id, buf) {
            drop_buffer(&old);
        }
        let keep: Vec<PathBuf> = map.values().map(|b| b.path()).collect();
        self.sweep(&keep);
        if map.len() > KEEP_BUFFERS {
            let mut by_age: Vec<(String, Instant)> =
                map.iter().map(|(k, b)| (k.clone(), *b.last_used.lock().unwrap_or_else(|p| p.into_inner()))).collect();
            by_age.sort_by_key(|(_, t)| *t);
            for (key, _) in by_age.into_iter().take(map.len() - KEEP_BUFFERS) {
                if let Some(old) = map.remove(&key) {
                    drop_buffer(&old);
                }
            }
        }
    }
}

fn drop_buffer(buf: &Buffer) {
    if let Some(task) = buf.task.lock().unwrap_or_else(|p| p.into_inner()).take() {
        task.abort();
    }
    // The webview may still hold the file for a moment; startup cleanup catches what's left.
    if buf.owned.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(buf.path());
    }
}

pub fn stream_url(track_id: &str) -> String {
    format!("http://relay.localhost/stream/{}", crate::util::encode_query(track_id))
}

/// Makes sure the track has a buffer that is (or will be) filling, resolving it if needed.
pub async fn prepare(state: &Arc<AppState>, track: &Track, quality: Quality, retry: bool) -> AppResult<Arc<Buffer>> {
    if let Some(buf) = state.streams.get(&track.id).filter(|b| b.usable() && !retry) {
        buf.touch();
        return Ok(buf);
    }
    if track.source == LOCAL {
        return local_buffer(state, track);
    }
    if retry {
        // Playing the stored file failed, so it is damaged: fetch the song again.
        crate::storage::discard(state, &track.id);
    } else if let Some(disk) = crate::storage::on_disk(state, &track.id)? {
        let buf = complete_buffer(disk.played, disk.stand_in, disk.mime, disk.loudness_db, disk.path);
        state.streams.insert(track.id.clone(), buf.clone());
        return Ok(buf);
    }
    if !crate::network::ensure_online(state).await {
        return Err(AppError::new("offline", "not saved on this PC"));
    }
    let resolution = match resolver::resolve(state, track, quality, retry).await {
        Ok(r) => r,
        Err(e) => {
            crate::network::suspect(state, e.code);
            return Err(e);
        }
    };
    Ok(start_buffer(state, &track.id, resolution, quality))
}

/// A song that is already complete on disk: nothing to download.
fn complete_buffer(played: Track, stand_in: bool, mime: String, loudness_db: Option<f32>, path: PathBuf) -> Arc<Buffer> {
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let (tx, _) = watch::channel(Progress { written: size, total: Some(size), done: true, failed: None });
    Arc::new(Buffer {
        played,
        stand_in,
        owned: AtomicBool::new(false),
        mime,
        loudness_db,
        path: Mutex::new(path),
        progress: tx,
        task: Mutex::new(None),
        last_used: Mutex::new(Instant::now()),
    })
}

/// A Local music file is already complete on disk: serve it where it is.
fn local_buffer(state: &Arc<AppState>, track: &Track) -> AppResult<Arc<Buffer>> {
    let path = crate::local::path_for(state, &track.source_id)?;
    std::fs::metadata(&path)?;
    let buf = complete_buffer(track.clone(), false, crate::local::mime_for(&path).into(), None, path);
    state.streams.insert(track.id.clone(), buf.clone());
    Ok(buf)
}

fn start_buffer(state: &Arc<AppState>, requested_id: &str, r: Resolution, quality: Quality) -> Arc<Buffer> {
    let (tx, _) = watch::channel(Progress { total: r.stream.size, ..Default::default() });
    let buf = Arc::new(Buffer {
        stand_in: r.stand_in,
        owned: AtomicBool::new(true),
        mime: r.stream.mime.clone(),
        loudness_db: r.stream.loudness_db,
        path: Mutex::new(state.streams.file_for(requested_id)),
        played: r.played.clone(),
        progress: tx,
        task: Mutex::new(None),
        last_used: Mutex::new(Instant::now()),
    });
    let handle = tokio::spawn(download(state.clone(), requested_id.to_string(), buf.clone(), r, quality));
    *buf.task.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle.abort_handle());
    state.streams.insert(requested_id.to_string(), buf.clone());
    buf
}

async fn download(state: Arc<AppState>, requested_id: String, buf: Arc<Buffer>, r: Resolution, quality: Quality) {
    let result = match r.stream.kind {
        StreamKind::Direct => download_direct(&state, &buf, r, quality).await,
        StreamKind::Hls => match download_hls(&state, &buf, &r.stream.url).await {
            // Segment links are signed and can expire before the playlist does: resolve again once.
            Err(e) if e.code == "forbidden" => {
                resolver::forget_stream(&state, &r.played.id);
                match resolver::resolve(&state, &r.played, quality, true).await {
                    Ok(fresh) if fresh.stream.kind == StreamKind::Hls => download_hls(&state, &buf, &fresh.stream.url).await,
                    Ok(fresh) => download_direct(&state, &buf, fresh, quality).await,
                    Err(e) => Err(e),
                }
            }
            other => other,
        },
    };
    match &result {
        // Hand the finished file to the cache (or downloads) before "done" is announced, so a
        // download waiting for this song finds it already stored.
        Ok(()) if buf.progress.borrow().written > 0 => {
            match crate::storage::keep(&state, &requested_id, &buf.path(), &buf.played, buf.stand_in, &buf.mime, buf.loudness_db) {
                Ok(path) => {
                    *buf.path.lock().unwrap_or_else(|p| p.into_inner()) = path;
                    buf.owned.store(false, Ordering::SeqCst);
                }
                Err(e) => eprintln!("keeping {} failed: {e}", buf.played.id),
            }
        }
        Err(e) => crate::network::suspect(&state, e.code),
        _ => {}
    }
    buf.progress.send_modify(|p| match result {
        Ok(()) => {
            p.done = true;
            p.total = Some(p.written);
        }
        Err(e) => {
            eprintln!("stream download failed for {}: {e}", buf.played.id);
            p.failed = Some(e);
        }
    });
}

fn parse_total(content_range: Option<&str>) -> Option<u64> {
    content_range?.rsplit('/').next()?.parse().ok()
}

async fn download_direct(state: &Arc<AppState>, buf: &Buffer, mut r: Resolution, quality: Quality) -> AppResult<()> {
    let mut file = tokio::fs::File::create(buf.path()).await?;
    let (mut written, mut refreshes, mut retries) = (0u64, 0u32, 0u32);
    loop {
        let total = buf.progress.borrow().total;
        if total.is_some_and(|t| written >= t) {
            break;
        }
        let end = written + CHUNK - 1;
        let mut req = state.http.client.get(&r.stream.url).header(header::RANGE, format!("bytes={written}-{end}"));
        if let Some(ua) = &r.stream.user_agent {
            req = req.header(header::USER_AGENT, ua);
        }
        let resp = req.send().await;
        let resp = match resp {
            Ok(resp) => resp,
            Err(_) if retries < MAX_NETWORK_RETRIES => {
                retries += 1;
                tokio::time::sleep(Duration::from_millis(500 * retries as u64)).await;
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        match resp.status().as_u16() {
            206 => {
                let total = parse_total(resp.headers().get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()));
                buf.progress.send_modify(|p| p.total = p.total.or(total));
            }
            200 => {
                // Server ignored Range and sends the whole file: start over from byte 0.
                file.set_len(0).await?;
                file.seek(SeekFrom::Start(0)).await?;
                written = 0;
                let len = resp.content_length();
                buf.progress.send_modify(|p| {
                    p.written = 0;
                    p.total = len;
                });
            }
            416 => break,
            401 | 403 | 410 if refreshes < MAX_URL_REFRESHES => {
                // The signed URL died early; resolve the same song again and resume where we were.
                refreshes += 1;
                resolver::forget_stream(state, &r.played.id);
                r = resolver::resolve(state, &r.played, quality, true).await?;
                continue;
            }
            s => return Err(AppError::new("service_error", format!("stream HTTP {s}"))),
        }
        let mut resp = resp;
        loop {
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    file.write_all(&bytes).await?;
                    written += bytes.len() as u64;
                    buf.progress.send_modify(|p| p.written = written);
                }
                Ok(None) => break,
                Err(_) if retries < MAX_NETWORK_RETRIES => {
                    retries += 1;
                    tokio::time::sleep(Duration::from_millis(500 * retries as u64)).await;
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        }
        if buf.progress.borrow().total.is_none() {
            // No length known and the body ended: that was the whole file.
            break;
        }
    }
    file.flush().await?;
    Ok(())
}

/// Lists media segment URLs (with the fMP4 init segment first, if any) from an HLS media playlist.
pub fn hls_segments(playlist: &str, base_url: &str) -> Vec<String> {
    let base = base_url.split('?').next().unwrap_or(base_url);
    let base_dir = &base[..base.rfind('/').map(|i| i + 1).unwrap_or(base.len())];
    let absolute = |u: &str| if u.starts_with("https://") { u.to_string() } else { format!("{base_dir}{u}") };
    let mut out = Vec::new();
    for line in playlist.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = rest.split("URI=\"").nth(1).and_then(|s| s.split('"').next()) {
                out.insert(0, absolute(uri));
            }
        } else if !line.is_empty() && !line.starts_with('#') {
            out.push(absolute(line));
        }
    }
    out
}

async fn download_hls(state: &Arc<AppState>, buf: &Buffer, playlist_url: &str) -> AppResult<()> {
    let playlist = state.http.get_text(playlist_url).await?;
    let segments = hls_segments(&playlist, playlist_url);
    if segments.is_empty() || segments.iter().any(|s| !s.starts_with("https://")) {
        return Err(AppError::new("bad_response", "HLS playlist has no usable segments"));
    }
    // The player needs the total size before it can start; asking each segment's size is far
    // quicker than downloading them all first.
    if let Some(total) = hls_total_size(state, &segments).await {
        buf.progress.send_modify(|p| p.total = Some(total));
    }
    let mut file = tokio::fs::File::create(buf.path()).await?;
    let mut written = 0u64;
    // A few segments in flight at a time, written strictly in order.
    for group in segments.chunks(HLS_PARALLEL) {
        let handles: Vec<_> = group
            .iter()
            .map(|url| {
                let (state, url) = (state.clone(), url.clone());
                tokio::spawn(async move {
                    let resp = state.http.client.get(&url).send().await?;
                    crate::http::check_status(&resp)?;
                    Ok::<_, AppError>(resp.bytes().await?)
                })
            })
            .collect();
        for h in handles {
            let bytes = h.await.map_err(|e| AppError::new("internal", e.to_string()))??;
            file.write_all(&bytes).await?;
            written += bytes.len() as u64;
            buf.progress.send_modify(|p| p.written = written);
        }
    }
    file.flush().await?;
    if buf.progress.borrow().total.is_some_and(|t| t != written) {
        // A segment's real size differed from what HEAD said; trust what we actually got.
        buf.progress.send_modify(|p| p.total = Some(written));
    }
    Ok(())
}

/// Sum of all segment sizes, or None if any size is unknown. Asks for one byte of each segment
/// and reads the total from Content-Range, because SoundCloud's CDN answers HEAD with length 0.
async fn hls_total_size(state: &Arc<AppState>, segments: &[String]) -> Option<u64> {
    let mut total = 0u64;
    for group in segments.chunks(HLS_HEAD_PARALLEL) {
        let handles: Vec<_> = group
            .iter()
            .map(|url| {
                let (state, url) = (state.clone(), url.clone());
                tokio::spawn(async move {
                    let resp = state.http.client.get(&url).header(header::RANGE, "bytes=0-0").send().await.ok()?;
                    let size = match resp.status().as_u16() {
                        206 => parse_total(resp.headers().get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok())),
                        200 => resp.content_length(),
                        _ => None,
                    };
                    size.filter(|s| *s > 0)
                })
            })
            .collect();
        for h in handles {
            total += h.await.ok().flatten()?;
        }
    }
    Some(total)
}

fn parse_range(value: Option<&str>) -> (u64, Option<u64>) {
    let Some(spec) = value.and_then(|v| v.strip_prefix("bytes=")) else { return (0, None) };
    let (start, end) = spec.split_once('-').unwrap_or((spec, ""));
    (start.trim().parse().unwrap_or(0), end.trim().parse().ok())
}

fn plain(status: StatusCode, msg: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(msg.as_bytes().to_vec())
        .expect("static response is valid")
}

/// Handles `relay://stream/<track id>` with byte ranges, waiting for the download when needed.
pub async fn serve(state: Arc<AppState>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = req.uri().path();
    let Some(raw_id) = path.strip_prefix("/stream/") else { return plain(StatusCode::NOT_FOUND, "not found") };
    let id = percent_decode(raw_id);
    let Some(buf) = state.streams.get(&id) else { return plain(StatusCode::NOT_FOUND, "no stream") };
    buf.touch();
    let (start, req_end) = parse_range(req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()));

    let mut rx = buf.progress.subscribe();
    let wait = async {
        loop {
            let p = rx.borrow_and_update().clone();
            if p.failed.is_some() {
                return p;
            }
            let ready = match p.total {
                Some(total) => p.done || p.written >= total.min(start + 64 * 1024) || start >= total,
                None => p.done,
            };
            if ready {
                return p;
            }
            if rx.changed().await.is_err() {
                return rx.borrow().clone();
            }
        }
    };
    let Ok(p) = tokio::time::timeout(SERVE_WAIT, wait).await else {
        return plain(StatusCode::GATEWAY_TIMEOUT, "stream too slow");
    };
    if p.failed.is_some() && p.written <= start {
        return plain(StatusCode::BAD_GATEWAY, "stream failed");
    }
    let total = p.total.unwrap_or(p.written);
    if start >= total {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Vec::new())
            .expect("valid response");
    }
    let available_end = p.written.saturating_sub(1);
    let end = req_end.unwrap_or(u64::MAX).min(start + SERVE_MAX - 1).min(available_end).min(total - 1);
    // The file may have moved into the cache a moment ago; a second try finds it there.
    match read_range(&buf.path(), start, end).or_else(|_| read_range(&buf.path(), start, end)) {
        Ok(body) => Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, buf.mime.as_str())
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"))
            .header(header::CONTENT_LENGTH, body.len())
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CACHE_CONTROL, "no-store")
            .body(body)
            .expect("valid response"),
        Err(e) => plain(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn read_range(path: &Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut body = vec![0u8; (end - start + 1) as usize];
    f.read_exact(&mut body)?;
    Ok(body)
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ranges() {
        assert_eq!(parse_range(Some("bytes=0-")), (0, None));
        assert_eq!(parse_range(Some("bytes=100-199")), (100, Some(199)));
        assert_eq!(parse_range(None), (0, None));
    }

    #[test]
    fn parses_content_range_total() {
        assert_eq!(parse_total(Some("bytes 0-99/5000")), Some(5000));
        assert_eq!(parse_total(Some("bytes 0-99/*")), None);
    }

    #[test]
    fn lists_hls_segments_with_init_first() {
        let pl = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:10,\nseg1.m4s\n#EXTINF:10,\nhttps://cdn.test/seg2.m4s\n#EXT-X-ENDLIST\n";
        let segs = hls_segments(pl, "https://cdn.test/a/b/playlist.m3u8?x=1");
        assert_eq!(segs, vec!["https://cdn.test/a/b/init.mp4", "https://cdn.test/a/b/seg1.m4s", "https://cdn.test/seg2.m4s"]);
    }

    #[test]
    fn decodes_ids() {
        assert_eq!(percent_decode("youtube%3AabcD_-1"), "youtube:abcD_-1");
        assert_eq!(percent_decode("x%2"), "x%2");
    }
}
