//! In-app updates from GitHub Releases, in Tauri's update format: `latest.json` names the new
//! version, its notes, and a setup.exe with a minisign signature. The installer is downloaded
//! in the background, checked against the public key below and the signed version, and only
//! run when the user picks "Restart to install".

use crate::error::{AppError, AppResult};
use crate::http::check_status;
use crate::AppState;
use minisign_verify::{PublicKey, Signature};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

const LATEST_URL: &str = "https://github.com/xqn7/relay/releases/latest/download/latest.json";
const RELEASES_API: &str = "https://api.github.com/repos/xqn7/relay/releases";
/// Release notes are a few lines; anything far longer isn't shown whole anyway.
const MAX_NOTES: usize = 8000;
/// The release signing key's public half (the private half never leaves the release PC).
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEZBMkMyRTdCNjQ1NjIwNTEKUldSUklGWmtleTRzK2kvNFQ5R2dOV0lhcXhUS1V0ZGVJOUFtVDUrWW5qOE01bmYxNmY5MVNsUUgK";
const PLATFORM: &str = "windows-x86_64";
const MAX_INSTALLER_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
}

pub struct Updater {
    dir: PathBuf,
    ready: Mutex<Option<(UpdateInfo, PathBuf)>>,
    busy: tokio::sync::Mutex<()>,
}

impl Updater {
    pub fn new(local_dir: &std::path::Path) -> Self {
        Self { dir: local_dir.join("update"), ready: Mutex::new(None), busy: tokio::sync::Mutex::new(()) }
    }
}

/// What `latest.json` announces for Windows.
#[derive(Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub signature: String,
}

pub fn parse_latest(json: &Value) -> Option<Release> {
    let version = json["version"].as_str()?.trim_start_matches('v').to_string();
    parse_version(&version)?;
    let platform = &json["platforms"][PLATFORM];
    let url = platform["url"].as_str().filter(|u| u.starts_with("https://") && u.len() < 2048)?.to_string();
    let signature = platform["signature"].as_str().filter(|s| !s.is_empty() && s.len() < 4096)?.to_string();
    let notes = json["notes"].as_str().unwrap_or("").chars().take(4000).collect();
    Some(Release { version, notes, url, signature })
}

/// "1.2.3" (pre-release suffixes ignored) as comparable numbers.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(remote: &str, current: &str) -> bool {
    matches!((parse_version(remote), parse_version(current)), (Some(r), Some(c)) if r > c)
}

/// Standard base64 (with padding) to text; the key and signatures arrive wrapped in it.
fn base64_text(input: &str) -> Option<String> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in input.trim().bytes().filter(|c| !c.is_ascii_whitespace()) {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    String::from_utf8(out).ok()
}

/// Checks the installer's signature, and that it was signed for the announced version (the
/// JSON itself isn't signed, so it could otherwise pair a new number with an old installer).
pub fn verify(data: &[u8], signature_b64: &str, version: &str, public_key_b64: &str) -> AppResult<()> {
    let bad = |what: &str| AppError::new("verify_failed", format!("update {what}"));
    let key = PublicKey::decode(&base64_text(public_key_b64).ok_or_else(|| bad("key unreadable"))?).map_err(|_| bad("key unreadable"))?;
    let sig = Signature::decode(&base64_text(signature_b64).ok_or_else(|| bad("signature unreadable"))?).map_err(|_| bad("signature unreadable"))?;
    key.verify(data, &sig, true).map_err(|_| bad("signature doesn't match"))?;
    if signed_version(sig.trusted_comment()) != Some(version) {
        return Err(bad("was signed for another version"));
    }
    Ok(())
}

/// The version a signature was made for. `tauri signer sign` records the signed file's name
/// ("file:Relay_1.2.3_x64-setup.exe") in the trusted comment; a "version:" field wins if present.
fn signed_version(comment: &str) -> Option<&str> {
    let fields = || comment.split('\t');
    if let Some(v) = fields().find_map(|f| f.strip_prefix("version:")) {
        return Some(v.trim_start_matches('v'));
    }
    fields().find_map(|f| f.strip_prefix("file:")?.strip_prefix("Relay_")?.strip_suffix("_x64-setup.exe"))
}

fn update_dir(state: &AppState) -> PathBuf {
    state.updater.dir.clone()
}

/// Startup: one quiet check right away, in the background so the window doesn't wait for it.
/// Finds and downloads; the UI hears about it through "update-ready".
pub fn check_on_start(state: &Arc<AppState>) {
    LAST_CHECK.store(crate::util::now_secs(), Ordering::SeqCst);
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        let _ = std::fs::remove_dir_all(update_dir(&state));
        check_logged(&state).await;
    });
}

/// Relay brought back from the tray (or opened again while running): closing the window only
/// hides it, so without this an update would wait for a full restart. At most every RECHECK.
pub fn check_when_shown(state: &Arc<AppState>) {
    let now = crate::util::now_secs();
    if crate::is_dev() || now - LAST_CHECK.load(Ordering::SeqCst) < RECHECK.as_secs() as i64 {
        return;
    }
    LAST_CHECK.store(now, Ordering::SeqCst);
    let state = state.clone();
    tauri::async_runtime::spawn(async move { check_logged(&state).await });
}

static LAST_CHECK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
const RECHECK: Duration = Duration::from_secs(30 * 60);

async fn check_logged(state: &Arc<AppState>) {
    if !state.net.online() {
        eprintln!("update check skipped: offline");
        return;
    }
    // Every outcome is logged, so a missed update can be traced in a problem report.
    match check(state).await {
        Ok(Some(info)) => eprintln!("update {} downloaded", info.version),
        Ok(None) => eprintln!("update check: up to date"),
        Err(e) => eprintln!("update check failed: {e}"),
    }
}

/// An update already downloaded and waiting, if any.
pub fn ready(state: &AppState) -> Option<UpdateInfo> {
    state.updater.ready.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(|(info, _)| info.clone())
}

/// The notes of the release this version came from ("" when it has none).
pub async fn notes(state: &AppState, version: &str) -> AppResult<String> {
    if parse_version(version).is_none() {
        return Err(AppError::new("invalid_input", "bad version"));
    }
    let url = format!("{RELEASES_API}/tags/v{version}");
    let release = state.http.get_json(&url).await?;
    Ok(release["body"].as_str().unwrap_or("").chars().take(MAX_NOTES).collect())
}

/// Looks for a newer release and downloads it. None = already up to date.
pub async fn check(state: &Arc<AppState>) -> AppResult<Option<UpdateInfo>> {
    // Relay Dev is updated by rebuilding it; installing a release would replace the real Relay.
    if crate::is_dev() {
        return Ok(None);
    }
    let _busy = state.updater.busy.lock().await;
    if let Some((info, _)) = state.updater.ready.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        return Ok(Some(info));
    }
    let resp = state.http.send(LATEST_URL, |c| c.get(LATEST_URL)).await?;
    // No release published yet means nothing to update to.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    check_status(&resp)?;
    let json: Value = resp.json().await?;
    let release = parse_latest(&json).ok_or_else(|| AppError::new("bad_response", "latest.json has no usable Windows release"))?;
    let current = state.app.package_info().version.to_string();
    if !is_newer(&release.version, &current) {
        return Ok(None);
    }
    let bytes = download(state, &release.url).await?;
    verify(&bytes, &release.signature, &release.version, PUBLIC_KEY)?;
    let dir = update_dir(state);
    tokio::fs::create_dir_all(&dir).await?;
    let file = dir.join(format!("Relay_{}_setup.exe", release.version));
    tokio::fs::write(&file, &bytes).await?;
    let info = UpdateInfo { version: release.version, notes: release.notes };
    *state.updater.ready.lock().unwrap_or_else(|p| p.into_inner()) = Some((info.clone(), file));
    let _ = state.app.emit("update-ready", &info);
    Ok(Some(info))
}

async fn download(state: &AppState, url: &str) -> AppResult<Vec<u8>> {
    let mut resp = state.http.client.get(url).timeout(Duration::from_secs(600)).send().await?;
    check_status(&resp)?;
    if resp.content_length().is_some_and(|n| n > MAX_INSTALLER_BYTES) {
        return Err(AppError::new("bad_response", "update is unexpectedly large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > MAX_INSTALLER_BYTES {
            return Err(AppError::new("bad_response", "update is unexpectedly large"));
        }
    }
    Ok(bytes)
}

/// Runs the downloaded installer (passive: a progress bar, no questions, Relay starts again
/// afterwards) and quits. The UI has saved the queue before calling this.
pub fn install(state: &AppState) -> AppResult<()> {
    let ready = state.updater.ready.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let (_, file) = ready.ok_or_else(|| AppError::new("not_found", "no update downloaded"))?;
    std::process::Command::new(&file).args(["/P", "/UPDATE", "/R"]).spawn()?;
    crate::desktop::before_exit(state);
    state.app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.0.9"));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn reads_latest_json() {
        let j = json!({
            "version": "v0.2.0",
            "notes": "Mini player fixes",
            "platforms": { "windows-x86_64": { "url": "https://github.com/xqn7/relay/releases/download/v0.2.0/Relay_0.2.0_x64-setup.exe", "signature": "abc" } }
        });
        let r = parse_latest(&j).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.notes, "Mini player fixes");
        assert!(parse_latest(&json!({ "version": "0.2.0", "platforms": {} })).is_none());
        let plain_http = json!({ "version": "0.2.0", "platforms": { "windows-x86_64": { "url": "http://x/y.exe", "signature": "abc" } } });
        assert!(parse_latest(&plain_http).is_none());
    }

    #[test]
    fn base64_decodes() {
        assert_eq!(base64_text("aGVsbG8gd29ybGQ=").as_deref(), Some("hello world"));
        assert!(base64_text("not base64!").is_none());
        assert!(base64_text(PUBLIC_KEY).unwrap().starts_with("untrusted comment: minisign public key"));
    }

    /// `tauri signer sign` with Relay's key over "relay-update-test" (no version in its comment).
    const FIXTURE_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSUklGWmtleTRzK3V3VzdHbk9ieDJqaitNSHFTeDQ4RFNHdHNzOEZvU0lJVy8zVmhKZUx3c2hsVG5vVWo3RFVpc0k2ZkFFZmFPUWwzSXpzYmZDc2pHZkYwN1ltd1F5Wnc4PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0NzQyMDU3CWZpbGU6Zml4dHVyZS5iaW4KRlVad1ozM0tkZ05GYWN4Y1h1V2FoUzJCZXRSSCtmNk0rSi9mT3hreGZhTnA5a2pFL1BPcTB6K29DeERYazJLSDFPLzJyd1hSaEVxSm9IMDZ6c3I1RHc9PQo=";

    #[test]
    fn checks_signature_then_signed_version() {
        let detail = |r: AppResult<()>| r.unwrap_err().detail;
        assert_eq!(detail(verify(b"installer", "abc", "0.2.0", PUBLIC_KEY)), "update signature unreadable");
        assert_eq!(detail(verify(b"relay-update-tampered", FIXTURE_SIG, "0.2.0", PUBLIC_KEY)), "update signature doesn't match");
        // A genuine signature, but not one made for the announced version.
        assert_eq!(detail(verify(b"relay-update-test", FIXTURE_SIG, "0.2.0", PUBLIC_KEY)), "update was signed for another version");
    }

    /// The same bytes signed as "Relay_0.2.0_x64-setup.exe", the way scripts/release.ps1 signs.
    const INSTALLER_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSUklGWmtleTRzK285Vno4TUtpbnh5VlFPM2ZSUEhnMGY3L0FVS0xlb3F6c2t3Zml2bDJXSjBEWjliTy9pOXg0T1hEMkxSa0FMQlZnWXVLampFU1JoQVZmMnI5aHVKWUFjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0NzQyNzc1CWZpbGU6UmVsYXlfMC4yLjBfeDY0LXNldHVwLmV4ZQpJSERGZXNpMGd4RHN4c3FVNVpiQ29PQkRsZFhnRW16ZXhIK3hKWTExRm1zUkVEc2pDeldzS3FTNUMwSkt4VmE0TDB3SWVZQURwdkV3SGo1WVpjbk1EUT09Cg==";

    #[test]
    fn accepts_installer_signed_for_its_version() {
        assert!(verify(b"relay-update-test", INSTALLER_SIG, "0.2.0", PUBLIC_KEY).is_ok());
        assert_eq!(verify(b"relay-update-test", INSTALLER_SIG, "0.3.0", PUBLIC_KEY).unwrap_err().detail, "update was signed for another version");
        assert_eq!(signed_version("timestamp:1\tversion:v1.0.0"), Some("1.0.0"));
        assert_eq!(signed_version("timestamp:1\tfile:other.exe"), None);
    }
}
