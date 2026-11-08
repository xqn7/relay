//! Downloads each song to a file in the background and serves it to the webview through the
//! `relay://` protocol with HTTP Range support. Serving from a local file means an expiring
//! upstream URL can be refreshed mid-download without the player noticing, HLS can be joined
//! into one file, and replays never touch the network.

use crate::error::{AppError, AppResult};
use crate::resolver::{self, Resolution};
use crate::sources::{Quality, StreamKind, Track};
use crate::AppState;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
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

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub written: u64,
    pub total: Option<u64>,
    pub done: bool,
    pub failed: Option<AppError>,
}

pub struct Buffer {
    pub played: Track,
    pub mime: String,
    pub loudness_db: Option<f32>,
    path: PathBuf,
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
}

pub struct Streams {
    dir: PathBuf,
    buffers: Mutex<HashMap<String, Arc<Buffer>>>,
}

impl Streams {
    /// Phase 1 keeps buffers for this run only, so leftovers from the last run are removed.
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir, buffers: Mutex::new(HashMap::new()) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Buffer>>> {
        self.buffers.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn get(&self, track_id: &str) -> Option<Arc<Buffer>> {
        self.lock().get(track_id).cloned()
    }

    fn file_for(&self, track_id: &str) -> PathBuf {
        let safe: String = track_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        self.dir.join(format!("{safe}.part"))
    }

    fn insert(&self, track_id: String, buf: Arc<Buffer>) {
        let mut map = self.lock();
        if let Some(old) = map.insert(track_id, buf) {
            drop_buffer(&old);
        }
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
    let _ = std::fs::remove_file(&buf.path);
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
    let resolution = resolver::resolve(state, track, quality, retry).await?;
    Ok(start_buffer(state, &track.id, resolution, quality))
}

fn start_buffer(state: &Arc<AppState>, requested_id: &str, r: Resolution, quality: Quality) -> Arc<Buffer> {
    let (tx, _) = watch::channel(Progress { total: r.stream.size, ..Default::default() });
    let buf = Arc::new(Buffer {
        mime: r.stream.mime.clone(),
        loudness_db: r.stream.loudness_db,
        path: state.streams.file_for(requested_id),
        played: r.played.clone(),
        progress: tx,
        task: Mutex::new(None),
        last_used: Mutex::new(Instant::now()),
    });
    let handle = tokio::spawn(download(state.clone(), buf.clone(), r, quality));
    *buf.task.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle.abort_handle());
    state.streams.insert(requested_id.to_string(), buf.clone());
    buf
}

async fn download(state: Arc<AppState>, buf: Arc<Buffer>, r: Resolution, quality: Quality) {
    let result = match r.stream.kind {
        StreamKind::Direct => download_direct(&state, &buf, r, quality).await,
        StreamKind::Hls => download_hls(&state, &buf, &r.stream.url).await,
    };
    buf.progress.send_modify(|p| match result {
        Ok(()) => {
            p.done = true;
            p.total = Some(p.written);
        }
        Err(e) => p.failed = Some(e),
    });
}

fn parse_total(content_range: Option<&str>) -> Option<u64> {
    content_range?.rsplit('/').next()?.parse().ok()
}

async fn download_direct(state: &Arc<AppState>, buf: &Buffer, mut r: Resolution, quality: Quality) -> AppResult<()> {
    let mut file = tokio::fs::File::create(&buf.path).await?;
    let (mut written, mut refreshes, mut retries) = (0u64, 0u32, 0u32);
    loop {
        let total = buf.progress.borrow().total;
        if total.is_some_and(|t| written >= t) {
            break;
        }
        let end = written + CHUNK - 1;
        let url = r.stream.url.clone();
        let resp = state.http.client.get(&url).header(header::RANGE, format!("bytes={written}-{end}")).send().await;
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
    let mut file = tokio::fs::File::create(&buf.path).await?;
    let mut written = 0u64;
    // Four segments in flight at a time, written strictly in order.
    for group in segments.chunks(4) {
        let handles: Vec<_> = group
            .iter()
            .map(|url| {
                let (state, url) = (state.clone(), url.clone());
                tokio::spawn(async move {
                    let resp = state.http.send(&url, |c| c.get(&url)).await?;
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
    Ok(())
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
    match read_range(&buf.path, start, end) {
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
