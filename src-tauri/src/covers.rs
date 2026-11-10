//! Cover images. Every cover the UI shows goes through `relay://img/<url>`: it adds the CORS
//! headers the backdrop needs to read pixels, and keeps a copy on disk (the "Artwork" cache),
//! so covers load instantly and still show in offline mode. Also finds high-resolution covers
//! for small ones, saves covers to a file, and lets covers be dragged out to Explorer.

use crate::error::{AppError, AppResult};
use crate::sources::{SearchKind, Source, Track, LOCAL};
use crate::util::now_secs;
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::http::{header, Request, Response, StatusCode};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const KEEP: usize = 48;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Covers smaller than this (on their short side) get a sharper replacement looked up.
pub const HD_MIN: u32 = 500;
const HD_SIZE: u32 = 1200;
/// A failed HD lookup is tried again after this long.
const HD_RETRY_SECS: i64 = 7 * 24 * 3600;

/// (url, image bytes, mime type)
type Entry = (String, Arc<Vec<u8>>, String);

#[derive(Default)]
pub struct Covers {
    memory: Mutex<VecDeque<Entry>>,
    /// The file prepared for the next drag out of the window.
    drag_file: Mutex<Option<PathBuf>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Covers {
    fn get(&self, url: &str) -> Option<(Arc<Vec<u8>>, String)> {
        lock(&self.memory).iter().find(|(u, _, _)| u == url).map(|(_, b, m)| (b.clone(), m.clone()))
    }

    fn put(&self, url: String, bytes: Arc<Vec<u8>>, mime: String) {
        let mut list = lock(&self.memory);
        list.push_back((url, bytes, mime));
        while list.len() > KEEP {
            list.pop_front();
        }
    }

    pub fn clear_memory(&self) {
        lock(&self.memory).clear();
    }
}

/// Image type from the file's first bytes; None for anything that isn't a common image.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        _ => None,
    }
}

fn ext_for(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}

/// Width and height of a JPEG or PNG, read from its header.
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) && b.len() >= 24 {
        let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
        return Some((w, h));
    }
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // Walk the JPEG segments to the frame header (SOF0..SOF15, except DHT/JPG/DAC markers).
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// The same cover at high resolution, for image services whose URLs name the size.
pub fn hd_variant(url: &str) -> Option<String> {
    if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
        let base = url.split('=').next().unwrap_or(url);
        return Some(format!("{base}=w{HD_SIZE}-h{HD_SIZE}-l90-rj"));
    }
    if url.contains("mzstatic.com") {
        let (head, tail) = url.rsplit_once('/')?;
        let ext = tail.rsplit_once('.').map(|(_, e)| e).unwrap_or("jpg");
        if tail.contains("bb.") {
            return Some(format!("{head}/{HD_SIZE}x{HD_SIZE}bb.{ext}"));
        }
    }
    None
}

fn art_file(state: &AppState, url: &str) -> PathBuf {
    let key: String = Sha256::digest(url.as_bytes()).iter().take(16).map(|b| format!("{b:02x}")).collect();
    state.storage.art_dir.join(key)
}

/// Bytes and type of a cover: from memory, the disk cache, or the internet (then cached).
async fn fetch(state: &AppState, url: &str) -> AppResult<(Arc<Vec<u8>>, String)> {
    if let Some(hit) = state.covers.get(url) {
        return Ok(hit);
    }
    let file = art_file(state, url);
    if let Ok(bytes) = tokio::fs::read(&file).await {
        if let Some(mime) = sniff(&bytes) {
            let bytes = Arc::new(bytes);
            state.covers.put(url.to_string(), bytes.clone(), mime.to_string());
            return Ok((bytes, mime.to_string()));
        }
    }
    // Image CDNs need no pacing; covers of a long list load in parallel.
    let resp = state.http.client.get(url).timeout(FETCH_TIMEOUT).send().await?;
    crate::http::check_status(&resp)?;
    if resp.content_length().is_some_and(|l| l as usize > MAX_BYTES) {
        return Err(AppError::new("bad_response", "cover too large"));
    }
    let bytes = resp.bytes().await?;
    let mime = sniff(&bytes).ok_or_else(|| AppError::new("bad_response", "not an image"))?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::new("bad_response", "cover too large"));
    }
    let bytes = Arc::new(bytes.to_vec());
    // The disk copy is a cache; failing to write it only costs a download next time.
    let _ = tokio::fs::write(&file, bytes.as_slice()).await;
    state.covers.put(url.to_string(), bytes.clone(), mime.to_string());
    Ok((bytes, mime.to_string()))
}

/// A cover by the URL the UI uses: an https image or a Local music cover served by the app.
async fn load(state: &AppState, url: &str) -> AppResult<(Arc<Vec<u8>>, String)> {
    if let Some(rest) = url.strip_prefix("http://relay.localhost/art/") {
        let file = rest.split('?').next().unwrap_or(rest);
        let path = state.local.art_path(file).ok_or_else(|| AppError::new("invalid_input", "bad cover"))?;
        let bytes = tokio::fs::read(path).await?;
        let mime = sniff(&bytes).ok_or_else(|| AppError::new("bad_response", "not an image"))?;
        return Ok((Arc::new(bytes), mime.to_string()));
    }
    if !url.starts_with("https://") || url.len() > 2048 {
        return Err(AppError::new("invalid_input", "bad cover url"));
    }
    fetch(state, url).await
}

pub(crate) fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "max-age=86400")
        .body(body)
        .expect("valid response")
}

pub async fn serve(state: Arc<AppState>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let raw = req.uri().path().trim_start_matches("/img/");
    let url = crate::streams::percent_decode(raw);
    if !url.starts_with("https://") || url.len() > 2048 {
        return respond(StatusCode::BAD_REQUEST, "text/plain", b"bad url".to_vec());
    }
    match fetch(&state, &url).await {
        Ok((bytes, mime)) => respond(StatusCode::OK, &mime, bytes.to_vec()),
        Err(_) => respond(StatusCode::BAD_GATEWAY, "text/plain", b"cover unavailable".to_vec()),
    }
}

/// Covers read from Local music files, served from the app's art folder.
pub fn serve_local(state: &AppState, path: &str) -> Response<Vec<u8>> {
    let file = path.trim_start_matches("/art/");
    match state.local.art_path(file).and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => {
            let mime = sniff(&bytes).unwrap_or("image/jpeg");
            respond(StatusCode::OK, mime, bytes)
        }
        None => respond(StatusCode::NOT_FOUND, "text/plain", b"no cover".to_vec()),
    }
}

// ---- HD covers ----------------------------------------------------------------------------

/// A sharper cover for the track, or None if its cover is already sharp or nothing better
/// exists. Results are remembered, so each song is looked up once.
pub async fn hd(state: &Arc<AppState>, track: &Track) -> AppResult<Option<String>> {
    if let Some(url) = track.cover.as_deref().and_then(hd_variant) {
        return Ok(Some(url));
    }
    let cached: Option<(Option<String>, i64)> = state.db.with(|c| {
        c.query_row("SELECT url,checked_at FROM hd_covers WHERE track_id=?1", [&track.id], |r| Ok((r.get(0)?, r.get(1)?))).optional()
    })?;
    if let Some((url, at)) = cached {
        if url.is_some() || now_secs() - at < HD_RETRY_SECS {
            return Ok(url);
        }
    }
    let found = look_up_hd(state, track).await?;
    state.db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO hd_covers(track_id,url,checked_at) VALUES(?1,?2,?3)",
            params![track.id, found, now_secs()],
        )
    })?;
    Ok(found)
}

async fn look_up_hd(state: &Arc<AppState>, track: &Track) -> AppResult<Option<String>> {
    let current = match &track.cover {
        Some(url) => load(state, url).await.ok(),
        None => None,
    };
    let size = current.as_ref().and_then(|(b, _)| image_size(b));
    if size.is_some_and(|(w, h)| w.min(h) >= HD_MIN) {
        return Ok(None);
    }
    // YouTube video thumbnails have a full-HD variant for most uploads.
    if let Some(id) = track.cover.as_deref().and_then(|u| u.strip_prefix("https://i.ytimg.com/vi/")).and_then(|r| r.split('/').next()) {
        let url = format!("https://i.ytimg.com/vi/{id}/maxresdefault.jpg");
        if fetch(state, &url).await.is_ok() {
            return Ok(Some(url));
        }
    }
    if track.source != LOCAL && track.cover.is_some() {
        return Ok(None);
    }
    // Local files with a small or missing cover: the official artwork from YouTube Music.
    if track.artists.is_empty() {
        return Ok(None);
    }
    let query = format!("{} {}", track.artist_line(), crate::matcher::clean_title(&track.title));
    let results = state.yt.search(&query, SearchKind::Songs).await?;
    Ok(crate::matcher::best_match(track, &results.tracks).and_then(|m| m.cover.as_deref()).and_then(hd_variant))
}

/// Writes the HD cover into a Local music file's tags ("Use HD cover").
pub async fn use_hd_for_local(state: &Arc<AppState>, track: &Track) -> AppResult<()> {
    let url = hd(state, track).await?.ok_or_else(|| AppError::new("not_found", "no bigger cover found"))?;
    let (bytes, mime) = load(state, &url).await?;
    let tmp = std::env::temp_dir().join(format!("relay-cover-{}.{}", crate::util::uuid_v4(), ext_for(&mime)));
    tokio::fs::write(&tmp, bytes.as_slice()).await?;
    let artist = track.artist_line();
    let result = crate::local::edit(state, &track.source_id, &track.title, &artist, Some(tmp.clone())).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

// ---- save and drag out --------------------------------------------------------------------

/// A file name for a cover, from the song or list name it belongs to.
pub fn file_name(name: &str, mime: &str) -> String {
    format!("{}.{}", crate::util::safe_file_stem(name, "Cover"), ext_for(mime))
}

fn best_url(url: &str) -> String {
    hd_variant(url).unwrap_or_else(|| url.to_string())
}

/// A cover at the biggest size its server offers, else as given.
pub async fn best(state: &AppState, url: &str) -> AppResult<(Arc<Vec<u8>>, String)> {
    match load(state, &best_url(url)).await {
        Ok(found) => Ok(found),
        Err(_) => load(state, url).await,
    }
}

/// "Save image": the cover at full resolution, to a file the user picks.
pub async fn save(app: &tauri::AppHandle, state: &AppState, url: &str, name: &str) -> AppResult<bool> {
    use tauri_plugin_dialog::DialogExt;
    let (bytes, mime) = best(state, url).await?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let ext = ext_for(&mime);
    app.dialog().file().set_file_name(file_name(name, &mime)).add_filter("Image", &[ext]).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(path)) = rx.await.ok() else { return Ok(false) };
    let path = path.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    tokio::fs::write(path, bytes.as_slice()).await?;
    Ok(true)
}

/// Gets the cover file ready before the drag starts, so dragging begins without delay.
pub async fn prepare_drag(state: &AppState, url: &str, name: &str) -> AppResult<()> {
    let (bytes, mime) = best(state, url).await?;
    let dir = std::env::temp_dir().join("Relay covers");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(file_name(name, &mime));
    tokio::fs::write(&path, bytes.as_slice()).await?;
    *lock(&state.covers.drag_file) = Some(path);
    Ok(())
}

/// Starts a system drag of the prepared cover file. Runs on the window's (UI) thread, which
/// OLE drag and drop requires; it returns when the cover is dropped or the drag is cancelled.
pub fn start_drag(app: &tauri::AppHandle, state: &AppState) -> AppResult<()> {
    use tauri::Manager;
    let path = lock(&state.covers.drag_file).take().ok_or_else(|| AppError::new("invalid_input", "no cover prepared"))?;
    let window = app.get_webview_window("main").ok_or_else(|| AppError::new("internal", "no window"))?;
    let hwnd = window.hwnd().map_err(|e| AppError::new("internal", e.to_string()))?.0 as isize;
    app.run_on_main_thread(move || {
        if let Err(e) = drag::drag_file(hwnd, &path) {
            eprintln!("cover drag failed: {e}");
        }
    })
    .map_err(|e| AppError::new("internal", e.to_string()))
}

mod drag {
    use std::path::Path;
    use windows::core::{implement, HSTRING, BOOL, HRESULT};
    use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HWND, S_OK};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, DROPEFFECT, DROPEFFECT_COPY};
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{ILFree, IShellFolder, SHBindToParent, SHParseDisplayName};

    #[implement(IDropSource)]
    struct Source;

    impl IDropSource_Impl for Source_Impl {
        fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
            if escape.as_bool() {
                DRAGDROP_S_CANCEL
            } else if (keys & MK_LBUTTON).0 == 0 {
                DRAGDROP_S_DROP
            } else {
                S_OK
            }
        }
        fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }

    /// Drags one file with the same data Explorer itself uses for files, so any drop target
    /// (desktop, Explorer, other apps) accepts it as a copy.
    pub fn drag_file(hwnd: isize, path: &Path) -> windows::core::Result<()> {
        unsafe {
            // Already initialized by the window on this thread; this only makes sure.
            let _ = OleInitialize(None);
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            SHParseDisplayName(&HSTRING::from(path.as_os_str()), None, &mut pidl, 0, None)?;
            let result = (|| {
                let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
                let folder: IShellFolder = SHBindToParent(pidl, Some(&mut child))?;
                let data: IDataObject = folder.GetUIObjectOf(HWND(hwnd as _), &[child as *const _], None)?;
                let source: IDropSource = Source.into();
                let mut effect = DROPEFFECT(0);
                DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect).ok()
            })();
            ILFree(Some(pidl));
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_image_types_and_sizes() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&300u32.to_be_bytes());
        png.extend_from_slice(&200u32.to_be_bytes());
        assert_eq!(sniff(&png), Some("image/png"));
        assert_eq!(image_size(&png), Some((300, 200)));

        // SOI, an APP0 segment, then a baseline frame header 640x480.
        let jpg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xE0, 0x02, 0x80, 0x03, 0x01, 0x22, 0x00,
        ];
        assert_eq!(sniff(&jpg), Some("image/jpeg"));
        assert_eq!(image_size(&jpg), Some((640, 480)));
        assert_eq!(sniff(b"<html>"), None);
    }

    #[test]
    fn builds_hd_urls() {
        assert_eq!(
            hd_variant("https://lh3.googleusercontent.com/abc=w120-h120-l90-rj").as_deref(),
            Some("https://lh3.googleusercontent.com/abc=w1200-h1200-l90-rj")
        );
        assert_eq!(
            hd_variant("https://is1-ssl.mzstatic.com/image/thumb/x/600x600bb.jpg").as_deref(),
            Some("https://is1-ssl.mzstatic.com/image/thumb/x/1200x1200bb.jpg")
        );
        assert_eq!(hd_variant("https://i1.sndcdn.com/artworks-x-t500x500.jpg"), None);
    }

    #[test]
    fn cover_file_names_are_safe() {
        assert_eq!(file_name("AC/DC: Back in Black?", "image/jpeg"), "AC_DC_ Back in Black_.jpg");
        assert_eq!(file_name("  ", "image/png"), "Cover.png");
    }
}
