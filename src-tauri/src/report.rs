//! Problem reports (Settings → About) and automatic error reports, sent to the developer's
//! Discord webhook. Before anything leaves the PC it's cleaned: the Windows user name, home
//! folder paths and email addresses are replaced, so a report says what broke, not who it was.

use crate::error::{AppError, AppResult};
use crate::AppState;
use reqwest::multipart::{Form, Part};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Baked in at build time from outside the repository (see build.rs).
const WEBHOOK: Option<&str> = option_env!("RELAY_REPORT_WEBHOOK");
/// Errors that come close together go out as one message.
const GATHER: Duration = Duration::from_secs(60);
/// At most this many automatic reports per run, so a broken network can't flood the channel.
const MAX_AUTO_PER_RUN: usize = 5;
/// Discord's limits: embed description 4096 characters, attachments a few MB.
const MAX_TEXT: usize = 3500;
pub const MAX_PICTURE_BYTES: usize = 8 * 1024 * 1024;
/// Error codes that only mean "no internet" or "this song doesn't exist": not bugs.
const NOT_BUGS: &[&str] = &["offline", "not_found", "unavailable", "no_match", "invalid_input", "not_signed_in"];

#[derive(Default)]
pub struct Reports {
    pending: Mutex<Vec<String>>,
    seen: Mutex<std::collections::HashSet<String>>,
    sent: std::sync::atomic::AtomicUsize,
}

fn webhook() -> Option<&'static str> {
    WEBHOOK.filter(|u| u.starts_with("https://discord.com/api/webhooks/"))
}

/// Replaces anything that could tell who the user is.
pub fn scrub(text: &str) -> String {
    let mut out = text.to_string();
    for var in ["USERPROFILE", "LOCALAPPDATA", "APPDATA", "OneDrive"] {
        if let Some(p) = std::env::var_os(var).map(|v| v.to_string_lossy().into_owned()).filter(|p| p.len() > 3) {
            out = out.replace(&p, &format!("%{var}%"));
            out = out.replace(&p.replace('\\', "/"), &format!("%{var}%"));
        }
    }
    if let Some(user) = std::env::var_os("USERNAME").map(|v| v.to_string_lossy().into_owned()).filter(|u| u.len() > 1) {
        out = out.replace(&user, "<user>");
    }
    out.split_inclusive(char::is_whitespace).map(|word| if looks_like_email(word.trim()) { word.replace(word.trim(), "<email>") } else { word.to_string() }).collect()
}

fn looks_like_email(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    matches!(word.split_once('@'), Some((a, b)) if !a.is_empty() && b.contains('.') && !b.starts_with('.'))
}

fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// Version, Windows build and app name: enough to reproduce, nothing personal.
fn about(state: &AppState) -> String {
    let windows = crate::winreg::get_machine_string(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "DisplayVersion").unwrap_or_default();
    let build = crate::winreg::get_machine_string(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber").unwrap_or_default();
    format!(
        "{} {} · Windows {windows} (build {build}) · {}",
        crate::app_name(),
        state.app.package_info().version,
        if state.net.online() { "online" } else { "offline" }
    )
}

async fn send(state: &AppState, title: &str, text: &str, picture: Option<(Vec<u8>, &'static str)>) -> AppResult<()> {
    let Some(url) = webhook() else { return Err(AppError::new("reports_off", "this build has no report address")) };
    let payload = json!({
        "username": crate::app_name(),
        "embeds": [{ "title": title, "description": clip(&scrub(text), MAX_TEXT), "footer": { "text": about(state) } }],
    });
    let mut form = Form::new()
        .text("payload_json", payload.to_string())
        .part("files[0]", Part::bytes(scrub(&crate::log::tail()).into_bytes()).file_name("relay.log").mime_str("text/plain")?);
    if let Some((bytes, mime)) = picture {
        let name = if mime == "image/png" { "screenshot.png" } else { "screenshot.jpg" };
        form = form.part("files[1]", Part::bytes(bytes).file_name(name).mime_str(mime)?);
    }
    let resp = state.http.client.post(url).multipart(form).timeout(Duration::from_secs(30)).send().await?;
    crate::http::check_status(&resp)
}

/// "Send a problem report": the user's words, the log and an optional screenshot.
pub async fn problem(state: &AppState, message: &str, picture: Option<Vec<u8>>) -> AppResult<()> {
    let message = message.trim();
    if message.is_empty() {
        return Err(AppError::new("invalid_input", "empty report"));
    }
    let picture = match picture.filter(|p| !p.is_empty()) {
        None => None,
        Some(p) if p.len() > MAX_PICTURE_BYTES => return Err(AppError::new("too_large", "picture over 8 MB")),
        Some(p) if p.starts_with(b"\x89PNG") => Some((p, "image/png")),
        Some(p) if p.starts_with(&[0xFF, 0xD8]) => Some((p, "image/jpeg")),
        Some(_) => return Err(AppError::new("bad_file", "not a PNG or JPEG")),
    };
    send(state, "Problem report", message, picture).await
}

/// Something went wrong that shouldn't: noted, and sent with others from the next minute.
/// The same error is sent once per run.
pub fn error(state: &Arc<AppState>, code: &str, what: &str) {
    if NOT_BUGS.contains(&code) || webhook().is_none() {
        return;
    }
    let line = format!("[{code}] {what}");
    let key = clip(&line, 200);
    if !state.reports.seen.lock().unwrap_or_else(|p| p.into_inner()).insert(key) {
        return;
    }
    let first = {
        let mut pending = state.reports.pending.lock().unwrap_or_else(|p| p.into_inner());
        pending.push(line);
        pending.len() == 1
    };
    if !first {
        return;
    }
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(GATHER).await;
        let lines = std::mem::take(&mut *state.reports.pending.lock().unwrap_or_else(|p| p.into_inner()));
        if lines.is_empty() || state.reports.sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= MAX_AUTO_PER_RUN {
            return;
        }
        if let Err(e) = send(&state, "Error report", &lines.join("\n"), None).await {
            eprintln!("error report not sent: {e}");
        }
    });
}

/// Crashes are reported too: written to the log as usual, then sent before the app goes.
pub fn catch_panics(state: &Arc<AppState>) {
    let state = Arc::downgrade(state);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        if let Some(state) = state.upgrade() {
            error(&state, "panic", &info.to_string());
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_removed() {
        assert_eq!(scrub("sign-in failed for anna.k@gmail.com today"), "sign-in failed for <email> today");
        assert_eq!(scrub("no @ here, a@b"), "no @ here, a@b");
    }

    #[test]
    fn long_text_is_cut() {
        assert_eq!(clip("abcdef", 3), "abc…");
        assert_eq!(clip("abc", 3), "abc");
    }
}
