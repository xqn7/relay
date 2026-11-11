//! External helper programs (yt-dlp and ffmpeg). They are not bundled: each is
//! downloaded the first time it's needed, verified against its published SHA-256, and kept
//! in the app's data folder.

use crate::error::{AppError, AppResult};
use crate::http::{check_status, Http};
use crate::sources::{Quality, ResolvedStream, StreamKind};
use crate::util::{now_secs, query_param};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const YTDLP_EXE: &str = "yt-dlp.exe";
const FFMPEG_EXE: &str = "ffmpeg.exe";
const FFMPEG_ZIP: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
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

/// A download or unpack cut off by closing the app leaves files behind that are never
/// finished; nothing can be using them at startup, so they go.
fn remove_unfinished(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".download") {
            let _ = std::fs::remove_file(entry.path());
        } else if name == "ffmpeg-unpack" {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
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

/// "Stream #0:1(und): Audio: aac (LC) (mp4a / …)" → "aac"
pub fn parse_audio_codec(ffmpeg_info: &str) -> Option<String> {
    ffmpeg_info.lines().find_map(|l| {
        let rest = l.split("Audio: ").nth(1)?;
        let codec: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        (!codec.is_empty()).then_some(codec)
    })
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

impl Tools {
    pub fn new(dir: PathBuf, http: Arc<Http>, app: AppHandle) -> Self {
        remove_unfinished(&dir);
        Self { dir, http, app, install_lock: Mutex::new(()), installing: AtomicBool::new(false) }
    }

    fn ytdlp_path(&self) -> PathBuf {
        self.dir.join(YTDLP_EXE)
    }

    pub fn ytdlp_ready(&self) -> bool {
        self.ytdlp_path().exists()
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
        let tmp = self.dir.join("yt-dlp.download");
        self.download_verified("yt-dlp", &format!("{YTDLP_BASE}/{YTDLP_EXE}"), &expected, &tmp).await?;
        tokio::fs::rename(&tmp, self.ytdlp_path()).await?;
        Ok(())
    }

    /// Streams a download to `dest` while hashing it, with progress events. The file is
    /// deleted again if it doesn't match `expected` (lowercase hex SHA-256).
    async fn download_verified(&self, tool: &'static str, url: &str, expected: &str, dest: &Path) -> AppResult<()> {
        let mut resp = self.http.client.get(url).send().await?;
        check_status(&resp)?;
        let total = resp.content_length();
        let mut file = tokio::fs::File::create(dest).await?;
        let mut hasher = Sha256::new();
        let (mut received, mut last_emit) = (0u64, 0u64);
        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            if received - last_emit > 512 * 1024 {
                last_emit = received;
                let _ = self.app.emit("tool-progress", ToolProgress { tool, received, total, done: false });
            }
        }
        file.flush().await?;
        drop(file);
        let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if actual != expected {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(AppError::new("verify_failed", format!("{tool} download did not match its checksum")));
        }
        let _ = self.app.emit("tool-progress", ToolProgress { tool, received, total, done: true });
        Ok(())
    }

    /// The installed yt-dlp's version (a date like "2026.10.31"), if it's there.
    pub async fn ytdlp_version(&self) -> Option<String> {
        let out = tokio::process::Command::new(self.ytdlp_path()).arg("--version").creation_flags(CREATE_NO_WINDOW).output().await.ok()?;
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !v.is_empty() && v.len() < 40).then_some(v)
    }

    /// Replaces yt-dlp with the latest release when the published file differs from ours.
    /// Returns whether anything changed.
    pub async fn update_ytdlp(&self) -> AppResult<bool> {
        let _guard = self.install_lock.lock().await;
        let sums = self.http.get_text(&format!("{YTDLP_BASE}/SHA2-256SUMS")).await?;
        let expected = expected_hash(&sums, YTDLP_EXE)
            .ok_or_else(|| AppError::new("bad_response", "yt-dlp checksum list has no entry for yt-dlp.exe"))?;
        let exe = self.ytdlp_path();
        if let Ok(current) = tokio::fs::read(&exe).await {
            let have: String = Sha256::digest(&current).iter().map(|b| format!("{b:02x}")).collect();
            if have == expected {
                return Ok(false);
            }
        }
        tokio::fs::create_dir_all(&self.dir).await?;
        let tmp = self.dir.join("yt-dlp.download");
        self.download_verified("yt-dlp", &format!("{YTDLP_BASE}/{YTDLP_EXE}"), &expected, &tmp).await?;
        // A running yt-dlp keeps its file locked against deletion, but it can be renamed away.
        let old = self.dir.join("yt-dlp.old.download");
        let _ = tokio::fs::remove_file(&old).await;
        if exe.exists() {
            tokio::fs::rename(&exe, &old).await?;
        }
        tokio::fs::rename(&tmp, &exe).await?;
        let _ = tokio::fs::remove_file(&old).await;
        Ok(true)
    }

    fn ffmpeg_path(&self) -> PathBuf {
        self.dir.join(FFMPEG_EXE)
    }

    /// ffmpeg, downloading it the first time (imports of video files need it). Several
    /// imports at once share one download.
    pub async fn ffmpeg(&self) -> AppResult<PathBuf> {
        let exe = self.ffmpeg_path();
        if exe.exists() {
            return Ok(exe);
        }
        let _guard = self.install_lock.lock().await;
        if !exe.exists() {
            self.install_ffmpeg().await?;
        }
        Ok(exe)
    }

    /// Downloads the "essentials" build (the one ffmpeg.org links for Windows), checks its
    /// published SHA-256 and keeps only ffmpeg.exe. Windows' own tar.exe unpacks the zip.
    async fn install_ffmpeg(&self) -> AppResult<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let sum = self.http.get_text(&format!("{FFMPEG_ZIP}.sha256")).await?;
        let expected = sum.split_whitespace().next().filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()));
        let expected = expected.ok_or_else(|| AppError::new("bad_response", "ffmpeg checksum is missing"))?.to_lowercase();
        let zip = self.dir.join("ffmpeg.download.zip");
        self.download_verified("ffmpeg", FFMPEG_ZIP, &expected, &zip).await?;
        let unpack = self.dir.join("ffmpeg-unpack");
        let _ = tokio::fs::remove_dir_all(&unpack).await;
        tokio::fs::create_dir_all(&unpack).await?;
        let tar = std::env::var("SystemRoot").map(|r| PathBuf::from(r).join("System32").join("tar.exe")).unwrap_or_else(|_| PathBuf::from("tar.exe"));
        let out = tokio::process::Command::new(tar)
            .arg("-xf")
            .arg(&zip)
            .arg("-C")
            .arg(&unpack)
            .arg("*/bin/ffmpeg.exe")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .await?;
        let _ = tokio::fs::remove_file(&zip).await;
        let found = find_file(&unpack, FFMPEG_EXE);
        let result = match (out.status.success(), found) {
            (true, Some(path)) => tokio::fs::rename(&path, self.ffmpeg_path()).await.map_err(AppError::from),
            _ => Err(AppError::new("install_failed", format!("could not unpack ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()))),
        };
        let _ = tokio::fs::remove_dir_all(&unpack).await;
        result
    }

    /// Runs ffmpeg quietly; returns its stderr (where it prints stream info) on success.
    pub async fn run_ffmpeg(&self, args: &[&std::ffi::OsStr]) -> AppResult<String> {
        let exe = self.ffmpeg().await?;
        let out = tokio::process::Command::new(exe)
            .args(["-hide_banner", "-nostdin", "-y"])
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .output()
            .await?;
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        if !out.status.success() {
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").to_string();
            return Err(AppError::new("convert_failed", format!("ffmpeg: {last}")));
        }
        Ok(err)
    }

    /// Decodes a file's audio to mono 16-bit PCM at `rate` Hz (for waveforms).
    pub async fn decode_mono(&self, input: &Path, rate: u32) -> AppResult<Vec<u8>> {
        let exe = self.ffmpeg().await?;
        let out = tokio::process::Command::new(exe)
            .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
            .arg(input)
            .args(["-vn", "-ac", "1", "-ar", &rate.to_string(), "-f", "s16le", "-"])
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .output()
            .await?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed");
            return Err(AppError::new("convert_failed", format!("ffmpeg: {last}")));
        }
        Ok(out.stdout)
    }

    /// The codec of the first audio stream ("aac", "opus", "mp3", …), read from `ffmpeg -i`.
    pub async fn audio_codec(&self, input: &Path) -> AppResult<Option<String>> {
        let exe = self.ffmpeg().await?;
        // Without an output file ffmpeg exits with an error after printing the stream info.
        let out = tokio::process::Command::new(exe).arg("-hide_banner").arg("-i").arg(input).creation_flags(CREATE_NO_WINDOW).output().await?;
        Ok(parse_audio_codec(&String::from_utf8_lossy(&out.stderr)))
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
    fn reads_audio_codec_from_ffmpeg_info() {
        let info = "Input #0, mov,mp4\n  Stream #0:0(und): Video: h264 (High)\n  Stream #0:1(und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz";
        assert_eq!(parse_audio_codec(info).as_deref(), Some("aac"));
        assert_eq!(parse_audio_codec("Stream #0:0: Audio: opus, 48000 Hz").as_deref(), Some("opus"));
        assert_eq!(parse_audio_codec("no audio here"), None);
    }

    #[test]
    fn tells_song_problems_from_service_problems() {
        assert_eq!(classify_ytdlp_error("ERROR: [youtube] x: Sign in to confirm your age"), "unavailable");
        assert_eq!(classify_ytdlp_error("ERROR: [youtube] x: Video unavailable"), "unavailable");
        assert_eq!(classify_ytdlp_error("ERROR: Sign in to confirm you're not a bot"), "source_failed");
        assert_eq!(classify_ytdlp_error("ERROR: something new"), "source_failed");
    }
}
