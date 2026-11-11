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
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

const LATEST_URL: &str = "https://github.com/xqn7/relay/releases/latest/download/latest.json";
/// The release signing key's public half (the private half never leaves the release PC).
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEM4MTZDREM3QjE1RjI3RjEKUldUeEoxK3h4ODBXeUtjdnc1OWx4QWZVRUxIQTFQTVdpalhFaEg1enlPcUMxT01WcktxbmQ2eUUK";
const PLATFORM: &str = "windows-x86_64";
const MAX_INSTALLER_BYTES: u64 = 200 * 1024 * 1024;
/// The startup check waits a little, so it never competes with the app's own start.
const STARTUP_DELAY: Duration = Duration::from_secs(20);

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
    let signed = sig.trusted_comment().split('\t').find_map(|f| f.strip_prefix("version:")).map(|v| v.trim_start_matches('v'));
    if signed != Some(version) {
        return Err(bad("was signed for another version"));
    }
    Ok(())
}

fn update_dir(state: &AppState) -> PathBuf {
    state.updater.dir.clone()
}

/// Startup: one quiet check a little after launch. Finds and downloads; the UI hears about it
/// through "update-ready".
pub fn check_later(state: &Arc<AppState>) {
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        let _ = std::fs::remove_dir_all(update_dir(&state));
        tokio::time::sleep(STARTUP_DELAY).await;
        if state.net.online() {
            if let Err(e) = check(&state).await {
                eprintln!("update check failed: {e}");
            }
        }
    });
}

/// Looks for a newer release and downloads it. None = already up to date.
pub async fn check(state: &Arc<AppState>) -> AppResult<Option<UpdateInfo>> {
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
    const FIXTURE_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUeEoxK3h4ODBXeUdIRk5vYjl5YSthWmc0cXp3bjdHN3VTd05EakpieERXT3c4NlVCYUtJWm4rekp6VkFsK2l0VE0wejFmZVE5Mi9BbXEyZGtNM0RycE1yZVp6YmljN1FzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0NDA5NTMyCWZpbGU6Zml4dHVyZS5iaW4KSzBoNk1mRUxhbkwvcFVpb0RoNGFCdnZBaHZvQkdvRkFZT0VhTWNmbEoyUTgyamwwT25xVjVIVUVvZE1FTi9JRytpRkJHTFRWU0N0WldNUU9rdkgxQWc9PQo=";

    #[test]
    fn checks_signature_then_signed_version() {
        let detail = |r: AppResult<()>| r.unwrap_err().detail;
        assert_eq!(detail(verify(b"installer", "abc", "0.2.0", PUBLIC_KEY)), "update signature unreadable");
        assert_eq!(detail(verify(b"relay-update-tampered", FIXTURE_SIG, "0.2.0", PUBLIC_KEY)), "update signature doesn't match");
        // A genuine signature, but not one made for the announced version.
        assert_eq!(detail(verify(b"relay-update-test", FIXTURE_SIG, "0.2.0", PUBLIC_KEY)), "update was signed for another version");
    }
}
