//! External helper programs (yt-dlp now, ffmpeg later). They are not bundled: each is
//! downloaded the first time it's needed, verified against its published SHA-256, and kept
//! in the app's data folder.

use crate::error::{AppError, AppResult};
use crate::http::{check_status, Http};
use crate::sources::{Quality, ResolvedStream, StreamKind};
use crate::util::{now_secs, query_param};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const YTDLP_EXE: &str = "yt-dlp.exe";
/// Windows: run console programs without flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Tools {
    dir: PathBuf,
    http: Arc<Http>,
    app: AppHandle,
    install_lock: Mutex<()>,
    installing: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolProgress {
    tool: &'static str,
    received: u64,
    total: Option<u64>,
    done: bool,
}

/// Parses a SHA2-256SUMS file ("<hash>  <file>" per line) for one file name.
pub fn expected_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

/// yt-dlp's last error line says whether the song itself is unavailable (age-restricted,
/// private, removed) or YouTube is blocking us; only the latter means the service is broken.
pub fn classify_ytdlp_error(stderr: &str) -> &'static str {
    let s = stderr.to_lowercase();
    if s.contains("not a bot") || s.contains("http error 429") || s.contains("rate-limit") {
        "source_failed"
    } else if ["unavailable", "private video", "confirm your age", "age-restricted", "removed", "copyright", "members-only", "not available"]
        .iter()
        .any(|k| s.contains(k))
    {
        "unavailable"
    } else if s.contains("unable to download") || s.contains("getaddrinfo") || s.contains("timed out") {
        "network"
    } else {
        "source_failed"
    }
}

impl Tools {
    pub fn new(dir: PathBuf, http: Arc<Http>, app: AppHandle) -> Self {
        Self { dir, http, app, install_lock: Mutex::new(()), installing: AtomicBool::new(false) }
    }

    fn ytdlp_path(&self) -> PathBuf {
        self.dir.join(YTDLP_EXE)
    }

    /// Starts downloading yt-dlp in the background if it's missing. YouTube is the main
    /// catalog, so it's needed for almost the first song; fetching it early keeps that song fast.
    pub fn warm_up(self: &Arc<Self>) {
        if self.ytdlp_path().exists() || self.installing.swap(true, Ordering::SeqCst) {
            return;
        }
        let tools = self.clone();
        tauri::async_runtime::spawn(async move {
            let _guard = tools.install_lock.lock().await;
            if let Err(e) = tools.install_ytdlp().await {
                eprintln!("yt-dlp download failed: {e}");
            }
            tools.installing.store(false, Ordering::SeqCst);
        });
    }

    /// Fetches the latest yt-dlp.exe and checks it against the published checksum.
    async fn install_ytdlp(&self) -> AppResult<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let sums = self.http.get_text(&format!("{YTDLP_BASE}/SHA2-256SUMS")).await?;
        let expected = expected_hash(&sums, YTDLP_EXE)
            .ok_or_else(|| AppError::new("bad_response", "yt-dlp checksum list has no entry for yt-dlp.exe"))?;

        let mut resp = self.http.client.get(format!("{YTDLP_BASE}/{YTDLP_EXE}")).send().await?;
        check_status(&resp)?;
        let total = resp.content_length();
        let tmp = self.dir.join("yt-dlp.download");
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut hasher = Sha256::new();
        let (mut received, mut last_emit) = (0u64, 0u64);
        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            if received - last_emit > 512 * 1024 {
                last_emit = received;
                let _ = self.app.emit("tool-progress", ToolProgress { tool: "yt-dlp", received, total, done: false });
            }
        }
        file.flush().await?;
        drop(file);
        let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if actual != expected {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(AppError::new("verify_failed", "yt-dlp download did not match its checksum"));
        }
        tokio::fs::rename(&tmp, self.ytdlp_path()).await?;
        let _ = self.app.emit("tool-progress", ToolProgress { tool: "yt-dlp", received, total, done: true });
        Ok(())
    }

    /// Asks yt-dlp for the best audio-only stream of a YouTube video.
    pub async fn ytdlp_stream(self: &Arc<Self>, video_id: &str, quality: Quality) -> AppResult<ResolvedStream> {
        if video_id.len() != 11 || !video_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(AppError::new("invalid_input", "bad YouTube video id"));
        }
        let exe = self.ytdlp_path();
        if !exe.exists() {
            // Don't make the song wait for a first-time download; the resolver plays a
            // stand-in meanwhile and later songs use yt-dlp once it's ready.
            self.warm_up();
            return Err(AppError::new("helper_installing", "yt-dlp is still downloading"));
        }
        let format = match quality {
            Quality::Best => "bestaudio",
            Quality::DataSaver => "bestaudio[abr<=80]/worstaudio",
        };
        let out = tokio::process::Command::new(exe)
            .args(["-f", format, "--no-playlist", "--no-warnings", "--print", "url", "--print", "ext", "--print", "filesize"])
            .arg(format!("https://www.youtube.com/watch?v={video_id}"))
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .output()
            .await?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed");
            return Err(AppError::new(classify_ytdlp_error(&err), format!("yt-dlp: {last}")));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        let url = lines
            .next()
            .filter(|u| u.starts_with("https://"))
            .ok_or_else(|| AppError::new("bad_response", "yt-dlp gave no URL"))?;
        let mime = match lines.next() {
            Some("m4a") | Some("mp4") => "audio/mp4",
            _ => "audio/webm",
        };
        let size = lines.next().and_then(|s| s.trim().parse().ok());
        let expires_at = query_param(url, "expire").and_then(|e| e.parse::<i64>().ok()).map(|e| e - 120).unwrap_or(now_secs() + 3600);
        Ok(ResolvedStream {
            url: url.to_string(),
            kind: StreamKind::Direct,
            mime: mime.into(),
            expires_at,
            size,
            loudness_db: None,
            user_agent: None,
        })
    }
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

    #[test]
    fn tells_song_problems_from_service_problems() {
        assert_eq!(classify_ytdlp_error("ERROR: [youtube] x: Sign in to confirm your age"), "unavailable");
        assert_eq!(classify_ytdlp_error("ERROR: [youtube] x: Video unavailable"), "unavailable");
        assert_eq!(classify_ytdlp_error("ERROR: Sign in to confirm you're not a bot"), "source_failed");
        assert_eq!(classify_ytdlp_error("ERROR: something new"), "source_failed");
    }
}
