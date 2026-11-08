//! External helper programs (yt-dlp now, ffmpeg later). They are not bundled: each is
//! downloaded the first time it's needed, verified against its published SHA-256, and kept
//! in the app's data folder.

use crate::error::{AppError, AppResult};
use crate::http::check_status;
use crate::sources::{ResolvedStream, StreamKind};
use crate::util::{now_secs, query_param};
use crate::AppState;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const YTDLP_EXE: &str = "yt-dlp.exe";
/// Windows: run console programs without flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Tools {
    dir: PathBuf,
    install_lock: Mutex<()>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolProgress {
    tool: &'static str,
    received: u64,
    total: Option<u64>,
    done: bool,
}

impl Tools {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, install_lock: Mutex::new(()) }
    }

    pub fn ytdlp_path(&self) -> PathBuf {
        self.dir.join(YTDLP_EXE)
    }
}

/// Parses a SHA2-256SUMS file ("<hash>  <file>" per line) for one file name.
pub fn expected_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

/// Downloads yt-dlp once (concurrent callers wait for the same download) and verifies it.
pub async fn ensure_ytdlp(state: &AppState) -> AppResult<PathBuf> {
    let path = state.tools.ytdlp_path();
    let _guard = state.tools.install_lock.lock().await;
    if path.exists() {
        return Ok(path);
    }
    install_ytdlp(state).await?;
    Ok(path)
}

/// Fetches the latest yt-dlp.exe; also used by the "Update yt-dlp" button later.
pub async fn install_ytdlp(state: &AppState) -> AppResult<()> {
    tokio::fs::create_dir_all(&state.tools.dir).await?;
    let sums = state.http.get_text(&format!("{YTDLP_BASE}/SHA2-256SUMS")).await?;
    let expected = expected_hash(&sums, YTDLP_EXE)
        .ok_or_else(|| AppError::new("bad_response", "yt-dlp checksum list has no entry for yt-dlp.exe"))?;

    let url = format!("{YTDLP_BASE}/{YTDLP_EXE}");
    let mut resp = state.http.client.get(&url).send().await?;
    check_status(&resp)?;
    let total = resp.content_length();
    let tmp = state.tools.dir.join("yt-dlp.download");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last_emit = 0u64;
    while let Some(chunk) = resp.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        if received - last_emit > 512 * 1024 {
            last_emit = received;
            let _ = state.app.emit("tool-progress", ToolProgress { tool: "yt-dlp", received, total, done: false });
        }
    }
    file.flush().await?;
    drop(file);
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(AppError::new("verify_failed", "yt-dlp download did not match its checksum"));
    }
    tokio::fs::rename(&tmp, state.tools.ytdlp_path()).await?;
    let _ = state.app.emit("tool-progress", ToolProgress { tool: "yt-dlp", received, total, done: true });
    Ok(())
}

/// Asks yt-dlp for the best audio-only stream of a YouTube video.
pub async fn ytdlp_stream(state: &AppState, video_id: &str) -> AppResult<ResolvedStream> {
    if video_id.len() != 11 || !video_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::new("invalid_input", "bad YouTube video id"));
    }
    let exe = ensure_ytdlp(state).await?;
    let out = tokio::process::Command::new(exe)
        .args(["-f", "bestaudio", "--no-playlist", "--no-warnings", "--print", "url", "--print", "ext", "--print", "filesize"])
        .arg(format!("https://www.youtube.com/watch?v={video_id}"))
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .output()
        .await?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::new("source_failed", format!("yt-dlp: {}", err.lines().last().unwrap_or("failed"))));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let url = lines.next().filter(|u| u.starts_with("https://")).ok_or_else(|| AppError::new("bad_response", "yt-dlp gave no URL"))?;
    let mime = match lines.next() {
        Some("m4a") | Some("mp4") => "audio/mp4",
        _ => "audio/webm",
    };
    let size = lines.next().and_then(|s| s.trim().parse().ok());
    let expires_at = query_param(url, "expire").and_then(|e| e.parse::<i64>().ok()).map(|e| e - 120).unwrap_or(now_secs() + 3600);
    Ok(ResolvedStream { url: url.to_string(), kind: StreamKind::Direct, mime: mime.into(), expires_at, size, loudness_db: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_checksum_file() {
        let h = "a".repeat(64);
        let sums = format!("{h}  yt-dlp.exe\n{}  yt-dlp\n", "b".repeat(64));
        assert_eq!(expected_hash(&sums, "yt-dlp.exe"), Some(h));
        assert_eq!(expected_hash(&sums, "nope"), None);
    }
}
