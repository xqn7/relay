//! External helper programs (yt-dlp and ffmpeg). They are not bundled: each is downloaded
//! quietly in the background (yt-dlp at start, ffmpeg a minute later or when first needed),
//! verified against its published SHA-256, and kept in the app's data folder.

use crate::error::{AppError, AppResult};
use crate::http::{check_status, Http};
use crate::sources::{Quality, ResolvedStream, StreamKind};
use crate::util::{now_secs, query_param};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const YTDLP_EXE: &str = "yt-dlp.exe";
const FFMPEG_EXE: &str = "ffmpeg.exe";
/// Gyan's "essentials" build (the one ffmpeg.org links for Windows). Its GitHub mirror is
/// several times faster than gyan.dev and publishes each file's SHA-256; gyan.dev is the backup.
const FFMPEG_RELEASE_API: &str = "https://api.github.com/repos/GyanD/codexffmpeg/releases/latest";
const FFMPEG_ZIP: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
/// Large downloads are fetched in this many parallel ranges: hosts limit speed per connection.
const PARTS: u64 = 6;
const SPLIT_ABOVE: u64 = 8 * 1024 * 1024;
const PART_RETRIES: u32 = 2;
/// Windows: run console programs without flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Tools {
    dir: PathBuf,
    http: Arc<Http>,
    install_lock: Mutex<()>,
    ffmpeg_lock: Mutex<()>,
    installing: AtomicBool,
    /// The YouTube sign-in as a Netscape cookie file's text, while "Play through your account" is on.
    cookies: std::sync::Mutex<Option<String>>,
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
    if s.contains("not a bot") {
        // YouTube wants a signed-in user for this request.
        "signin_required"
    } else if s.contains("http error 429") || s.contains("rate-limit") {
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

fn is_sha256(h: &str) -> bool {
    h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())
}

/// The total size from a "Content-Range: bytes 0-0/12345" header.
pub fn range_total(header: &str) -> Option<u64> {
    header.strip_prefix("bytes ")?.split('/').nth(1)?.trim().parse().ok().filter(|&n| n > 0)
}

/// A release asset whose name ends with `suffix`: its download link and SHA-256, as GitHub's
/// release API lists them.
pub fn github_asset(release: &serde_json::Value, suffix: &str) -> Option<(String, String)> {
    release["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        let url = a["browser_download_url"].as_str().filter(|u| u.starts_with("https://github.com/"))?;
        let digest = a["digest"].as_str()?.strip_prefix("sha256:").filter(|h| is_sha256(h))?;
        name.ends_with(suffix).then(|| (url.to_string(), digest.to_lowercase()))
    })
}

/// The byte ranges (inclusive) a download of `size` bytes is split into.
pub fn split_ranges(size: u64, parts: u64) -> Vec<(u64, u64)> {
    let step = size.div_ceil(parts.max(1));
    (0..size).step_by(step.max(1) as usize).map(|start| (start, (start + step).min(size) - 1)).collect()
}

fn sha256_file(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

async fn download_whole(client: &reqwest::Client, url: &str, dest: &Path) -> AppResult<()> {
    let mut resp = client.get(url).send().await?;
    check_status(&resp)?;
    let mut file = tokio::fs::File::create(dest).await?;
    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

/// Fetches the file in parallel ranges straight into their places in `dest`. A part that
/// breaks off resumes where it stopped.
async fn download_parts(client: &reqwest::Client, url: &str, size: u64, dest: &Path) -> AppResult<()> {
    tokio::fs::File::create(dest).await?.set_len(size).await?;
    let mut tasks = tokio::task::JoinSet::new();
    for (start, end) in split_ranges(size, PARTS) {
        let (client, url, dest) = (client.clone(), url.to_string(), dest.to_path_buf());
        tasks.spawn(async move { download_range(&client, &url, &dest, start, end).await });
    }
    while let Some(done) = tasks.join_next().await {
        done.map_err(|e| AppError::new("internal", e.to_string()))??;
    }
    Ok(())
}

async fn download_range(client: &reqwest::Client, url: &str, dest: &Path, start: u64, end: u64) -> AppResult<()> {
    let mut file = tokio::fs::OpenOptions::new().write(true).open(dest).await?;
    let mut at = start;
    let mut tries = 0;
    while at <= end {
        let attempt = async {
            let mut resp = client.get(url).header(reqwest::header::RANGE, format!("bytes={at}-{end}")).send().await?;
            if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return Err(AppError::new("bad_response", format!("range request answered {}", resp.status())));
            }
            file.seek(std::io::SeekFrom::Start(at)).await?;
            while let Some(chunk) = resp.chunk().await? {
                let room = (end + 1 - at) as usize;
                let chunk = &chunk[..chunk.len().min(room)];
                file.write_all(chunk).await?;
                at += chunk.len() as u64;
            }
            Ok(())
        }
        .await;
        if let Err(e) = attempt {
            tries += 1;
            if tries > PART_RETRIES {
                return Err(e);
            }
        } else if at <= end {
            // The host closed the part early; ask for the rest.
            tries += 1;
            if tries > PART_RETRIES {
                return Err(AppError::new("network", "download kept stopping early"));
            }
        }
    }
    file.flush().await?;
    Ok(())
}

impl Tools {
    pub fn new(dir: PathBuf, http: Arc<Http>) -> Self {
        remove_unfinished(&dir);
        Self {
            dir,
            http,
            install_lock: Mutex::new(()),
            ffmpeg_lock: Mutex::new(()),
            installing: AtomicBool::new(false),
            cookies: std::sync::Mutex::new(None),
        }
    }

    pub fn set_cookies(&self, file: Option<String>) {
        *self.cookies.lock().unwrap_or_else(|p| p.into_inner()) = file;
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

    /// Downloads `url` to `dest` and checks it against `expected` (lowercase hex SHA-256); a
    /// file that doesn't match is deleted again.
    async fn download_verified(&self, tool: &'static str, url: &str, expected: &str, dest: &Path) -> AppResult<()> {
        // A one-byte request tells whether the host serves ranges, and the full size.
        let probe = self.http.client.get(url).header(reqwest::header::RANGE, "bytes=0-0").send().await?;
        check_status(&probe)?;
        let ranged = probe.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let total = probe.headers().get(reqwest::header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(range_total);
        // Signed redirect targets (GitHub) are fetched directly by every part.
        let final_url = probe.url().to_string();
        drop(probe);
        let result = match total {
            Some(size) if ranged && size > SPLIT_ABOVE => download_parts(&self.http.client, &final_url, size, dest).await,
            _ => download_whole(&self.http.client, &final_url, dest).await,
        };
        if let Err(e) = result {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(e);
        }
        let path = dest.to_path_buf();
        let actual = tokio::task::spawn_blocking(move || sha256_file(&path)).await.map_err(|e| AppError::new("internal", e.to_string()))??;
        if actual != expected {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(AppError::new("verify_failed", format!("{tool} download did not match its checksum")));
        }
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

    /// ffmpeg, downloading it now if the background download hasn't finished yet. Several
    /// callers at once share one download.
    pub async fn ffmpeg(&self) -> AppResult<PathBuf> {
        let exe = self.ffmpeg_path();
        if exe.exists() {
            return Ok(exe);
        }
        let _guard = self.ffmpeg_lock.lock().await;
        if !exe.exists() {
            self.install_ffmpeg().await?;
        }
        Ok(exe)
    }

    /// Fetches ffmpeg in the background some time after start, so it's there before the first
    /// video import or MP3 export needs it.
    pub fn prefetch_ffmpeg(self: &Arc<Self>) {
        if self.ffmpeg_path().exists() {
            return;
        }
        let tools = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = tools.ffmpeg().await {
                eprintln!("ffmpeg download failed: {e}");
            }
        });
    }

    /// Where to get ffmpeg and its checksum: the GitHub mirror, else gyan.dev.
    async fn ffmpeg_source(&self) -> AppResult<(String, String)> {
        if let Ok(release) = self.http.get_json(FFMPEG_RELEASE_API).await {
            if let Some(found) = github_asset(&release, "essentials_build.zip") {
                return Ok(found);
            }
        }
        let sum = self.http.get_text(&format!("{FFMPEG_ZIP}.sha256")).await?;
        let expected = sum.split_whitespace().next().filter(|h| is_sha256(h));
        let expected = expected.ok_or_else(|| AppError::new("bad_response", "ffmpeg checksum is missing"))?.to_lowercase();
        Ok((FFMPEG_ZIP.to_string(), expected))
    }

    /// Downloads ffmpeg, checks its SHA-256 and keeps only ffmpeg.exe. Windows' own tar.exe
    /// unpacks the zip.
    async fn install_ffmpeg(&self) -> AppResult<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let (url, expected) = self.ffmpeg_source().await?;
        let zip = self.dir.join("ffmpeg.download.zip");
        self.download_verified("ffmpeg", &url, &expected, &zip).await?;
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
        let mut cmd = tokio::process::Command::new(exe);
        cmd.args(["-f", format, "--no-playlist", "--no-warnings", "--print", "url", "--print", "ext", "--print", "filesize"]);
        // Signed in: yt-dlp reads the cookies from a file that only exists for this one run.
        let cookies = self.cookies.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let cookie_file = match cookies {
            Some(text) => {
                let path = self.dir.join(format!("cookies-{}.download", crate::util::uuid_v4()));
                tokio::fs::write(&path, text).await?;
                cmd.arg("--cookies").arg(&path);
                Some(path)
            }
            None => None,
        };
        let out = cmd.arg(format!("https://www.youtube.com/watch?v={video_id}")).creation_flags(CREATE_NO_WINDOW).kill_on_drop(true).output().await;
        if let Some(path) = cookie_file {
            let _ = tokio::fs::remove_file(path).await;
        }
        let out = out?;
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
    fn splits_downloads_into_ranges() {
        assert_eq!(split_ranges(10, 3), vec![(0, 3), (4, 7), (8, 9)]);
        let r = split_ranges(114_768_076, PARTS);
        assert_eq!(r.len(), PARTS as usize);
        assert_eq!(r[0].0, 0);
        assert_eq!(r.last().unwrap().1, 114_768_075);
        assert!(r.windows(2).all(|w| w[0].1 + 1 == w[1].0), "ranges touch without gaps or overlap");
        assert_eq!(range_total("bytes 0-0/23575744"), Some(23_575_744));
        assert_eq!(range_total("bytes */0"), None);
        assert_eq!(range_total("garbage"), None);
    }

    #[test]
    fn reads_github_asset_digests() {
        let hash = "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba";
        let release = serde_json::json!({ "assets": [
            { "name": "ffmpeg-9.0.2-essentials_build.7z", "browser_download_url": "https://github.com/x/y.7z", "digest": format!("sha256:{hash}") },
            { "name": "ffmpeg-9.0.2-essentials_build.zip", "browser_download_url": "https://github.com/x/y.zip", "digest": format!("sha256:{hash}") },
        ]});
        assert_eq!(github_asset(&release, "essentials_build.zip"), Some(("https://github.com/x/y.zip".into(), hash.into())));
        let no_digest = serde_json::json!({ "assets": [{ "name": "a.zip", "browser_download_url": "https://github.com/a.zip" }] });
        assert_eq!(github_asset(&no_digest, ".zip"), None);
        let elsewhere = serde_json::json!({ "assets": [{ "name": "a.zip", "browser_download_url": "https://evil.example/a.zip", "digest": format!("sha256:{hash}") }] });
        assert_eq!(github_asset(&elsewhere, ".zip"), None);
    }

    #[test]
    fn tells_song_problems_from_service_problems() {
        assert_eq!(classify_ytdlp_error("ERROR: [youtube] x: Sign in to confirm your age"), "unavailable");
        assert_eq!(classify_ytdlp_error("ERROR: [youtube] x: Video unavailable"), "unavailable");
        assert_eq!(classify_ytdlp_error("ERROR: Sign in to confirm you're not a bot"), "signin_required");
        assert_eq!(classify_ytdlp_error("ERROR: something new"), "source_failed");
    }
}
