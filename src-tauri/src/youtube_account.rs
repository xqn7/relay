//! Optional sign-in to YouTube. Google's own login page opens in a private (InPrivate) window;
//! once it lands on YouTube Music, Relay keeps only the youtube.com cookies, in the Windows
//! Credential Manager (never in Relay's files), and the private window forgets everything.
//! With them, yt-dlp asks for audio as the user (fewer "confirm you're not a bot" blocks) and
//! the user's YouTube Music playlists can be listed.

use crate::error::{AppError, AppResult};
use crate::sources::{Collection, Track};
use crate::AppState;
use rustypipe::client::RustyPipe;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LOGIN_LABEL: &str = "youtube-login";
const LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26app%3Ddesktop%26next%3Dhttps%253A%252F%252Fmusic.youtube.com%252F";
/// The secret holds the number of parts; the parts are "<SECRET>.<n>" (each entry is capped
/// in size, and a full set of YouTube cookies is larger than one).
const SECRET: &str = "youtube.cookies";
const PART_BYTES: usize = 1800;
const MAX_PARTS: usize = 8;
/// Without this cookie YouTube treats requests as signed out.
const KEY_COOKIE: &str = "SAPISID";
const MAX_PLAYLISTS: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    /// Unix seconds; None for session cookies.
    pub expires: Option<i64>,
}

#[derive(Default)]
pub struct YouTubeAccount {
    cookies: Mutex<Option<Vec<StoredCookie>>>,
    /// A rustypipe client for signed-in requests. It keeps the cookies in memory only (the
    /// main client caches to a file, which must never hold them).
    client: tokio::sync::Mutex<Option<RustyPipe>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    signed_in: bool,
    /// Signed in, but YouTube no longer accepts the login.
    expired: bool,
}

// ---- cookie handling (pure) ------------------------------------------------------------------

/// "name=value; …" for request headers.
pub fn cookie_header(cookies: &[StoredCookie]) -> String {
    cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ")
}

/// The Netscape cookie file format yt-dlp reads.
pub fn netscape(cookies: &[StoredCookie]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in cookies {
        let domain = if c.domain.starts_with('.') { c.domain.clone() } else { format!(".{}", c.domain) };
        let flag = |b: bool| if b { "TRUE" } else { "FALSE" };
        out.push_str(&format!("{domain}\tTRUE\t{}\t{}\t{}\t{}\t{}\n", c.path, flag(c.secure), c.expires.unwrap_or(0), c.name, c.value));
    }
    out
}

/// Keeps youtube.com cookies with sane names and values; None if the login cookie is missing.
pub fn keep_youtube(cookies: Vec<StoredCookie>) -> Option<Vec<StoredCookie>> {
    let ok = |s: &str| !s.is_empty() && s.len() < 4096 && !s.contains(['\t', '\n', '\r', ';']);
    let kept: Vec<StoredCookie> = cookies
        .into_iter()
        .filter(|c| (c.domain == "youtube.com" || c.domain.ends_with(".youtube.com")) && ok(&c.name) && ok(&c.value) && !c.name.contains('='))
        .collect();
    kept.iter().any(|c| c.name == KEY_COOKIE).then_some(kept)
}

fn split(text: &str) -> Vec<&str> {
    // Cookies JSON is ASCII, so byte chunks never cut a character.
    text.as_bytes().chunks(PART_BYTES).map(|c| std::str::from_utf8(c).unwrap_or("")).collect()
}

// ---- storage ---------------------------------------------------------------------------------

fn load_secret() -> Option<Vec<StoredCookie>> {
    let parts: usize = crate::secrets::get(SECRET)?.parse().ok()?;
    if parts == 0 || parts > MAX_PARTS {
        return None;
    }
    let text: String = (0..parts).map(|i| crate::secrets::get(&format!("{SECRET}.{i}"))).collect::<Option<Vec<_>>>()?.concat();
    serde_json::from_str(&text).ok()
}

fn save_secret(cookies: &[StoredCookie]) -> AppResult<()> {
    let text = serde_json::to_string(cookies)?;
    if !text.is_ascii() {
        return Err(AppError::new("invalid_input", "unexpected characters in YouTube cookies"));
    }
    let parts = split(&text);
    if parts.len() > MAX_PARTS {
        return Err(AppError::new("too_large", "YouTube sign-in data is unexpectedly large"));
    }
    delete_secret();
    for (i, part) in parts.iter().enumerate() {
        crate::secrets::set(&format!("{SECRET}.{i}"), part)?;
    }
    crate::secrets::set(SECRET, &parts.len().to_string())
}

fn delete_secret() {
    crate::secrets::delete(SECRET);
    for i in 0..MAX_PARTS {
        crate::secrets::delete(&format!("{SECRET}.{i}"));
    }
}

impl YouTubeAccount {
    pub fn load() -> Self {
        Self { cookies: Mutex::new(load_secret()), client: tokio::sync::Mutex::new(None) }
    }

    pub fn signed_in(&self) -> bool {
        self.cookies.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }

    fn cookies(&self) -> Option<Vec<StoredCookie>> {
        self.cookies.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// What yt-dlp gets: the cookies while signed in and "Play through your account" is on.
pub fn sync_tools(state: &AppState) {
    let on = state.db.pref::<bool>("youtubeUseAccount").unwrap_or(true);
    let file = state.youtube_account.cookies().filter(|_| on).map(|c| netscape(&c));
    state.tools.set_cookies(file);
}

// ---- sign in / out ---------------------------------------------------------------------------

/// Opens Google's login in a private window (or brings it forward).
pub fn open_login(app: &AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        let _ = w.set_focus();
        return Ok(());
    }
    let url = LOGIN_URL.parse().map_err(|_| AppError::new("internal", "bad login URL"))?;
    WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(url))
        .title("Sign in to YouTube")
        .inner_size(480.0, 680.0)
        .center()
        .incognito(true)
        .additional_browser_args(crate::BROWSER_ARGS)
        // Only Google's and YouTube's own pages; nothing else opens in this window.
        .on_navigation(|url| url.scheme() == "https")
        .on_page_load(|window, payload| {
            if payload.event() != tauri::webview::PageLoadEvent::Finished {
                return;
            }
            let landed = matches!(payload.url().host_str(), Some("music.youtube.com" | "www.youtube.com"));
            if landed {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move { capture(&app).await });
            }
        })
        .build()
        .map_err(|e| AppError::new("internal", format!("login window: {e}")))?;
    Ok(())
}

async fn capture(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LOGIN_LABEL) else { return };
    let Ok(url) = "https://www.youtube.com/".parse() else { return };
    let Ok(raw) = window.cookies_for_url(url) else { return };
    let cookies: Vec<StoredCookie> = raw
        .iter()
        .map(|c| StoredCookie {
            name: c.name().to_string(),
            value: c.value().to_string(),
            domain: c.domain().unwrap_or("youtube.com").trim_start_matches('.').to_string(),
            path: c.path().unwrap_or("/").to_string(),
            secure: c.secure().unwrap_or(true),
            expires: c.expires_datetime().map(|d| d.unix_timestamp()),
        })
        .collect();
    // Landing on YouTube signed out (e.g. "Not now") isn't a sign-in; keep the window open.
    let Some(cookies) = keep_youtube(cookies) else { return };
    let state = app.state::<Arc<AppState>>().inner().clone();
    let result = save_secret(&cookies);
    if result.is_ok() {
        *state.youtube_account.cookies.lock().unwrap_or_else(|p| p.into_inner()) = Some(cookies);
        *state.youtube_account.client.lock().await = None;
        sync_tools(&state);
    }
    let _ = window.destroy();
    let _ = app.emit("youtube-account", result.is_ok());
}

pub async fn sign_out(state: &AppState) {
    delete_secret();
    *state.youtube_account.cookies.lock().unwrap_or_else(|p| p.into_inner()) = None;
    *state.youtube_account.client.lock().await = None;
    sync_tools(state);
    let _ = state.app.emit("youtube-account", false);
}

// ---- signed-in requests ----------------------------------------------------------------------

/// Runs `f` with the signed-in client, creating it (and checking the login) on first use.
async fn with_client<T>(state: &AppState, f: impl AsyncFnOnce(&RustyPipe) -> AppResult<T>) -> AppResult<T> {
    let cookies = state.youtube_account.cookies().ok_or_else(|| AppError::new("signed_out", "not signed in to YouTube"))?;
    let mut guard = state.youtube_account.client.lock().await;
    if guard.is_none() {
        let rp = RustyPipe::builder()
            .no_storage()
            .no_botguard()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| AppError::new("source_failed", format!("YouTube: {e}")))?;
        rp.user_auth_set_cookie(cookie_header(&cookies)).await.map_err(|e| AppError::new("youtube_expired", format!("YouTube login: {e}")))?;
        *guard = Some(rp);
    }
    let rp = guard.as_ref().expect("client was just created");
    f(rp).await
}

pub async fn status(state: &AppState) -> AccountStatus {
    if !state.youtube_account.signed_in() {
        return AccountStatus { signed_in: false, expired: false };
    }
    let check = with_client(state, async |rp| rp.user_auth_check_cookie().await.map_err(|e| AppError::new("youtube_expired", e.to_string()))).await;
    let expired = matches!(&check, Err(e) if e.code == "youtube_expired");
    if expired {
        *state.youtube_account.client.lock().await = None;
    }
    AccountStatus { signed_in: true, expired }
}

/// The user's YouTube Music playlists ("Sync library").
pub async fn playlists(state: &AppState) -> AppResult<Vec<Collection>> {
    with_client(state, async |rp| {
        let page = rp.query().authenticated().music_saved_playlists().await.map_err(crate::sources::youtube::map_err)?;
        Ok(page.items.into_iter().filter(|p| !p.is_podcast).take(MAX_PLAYLISTS).map(crate::sources::youtube::map_playlist).collect())
    })
    .await
}

/// A playlist's songs read as the user, so private playlists work too.
pub async fn playlist_tracks(state: &AppState, id: &str) -> AppResult<Vec<Track>> {
    with_client(state, async |rp| {
        let list = rp.query().authenticated().music_playlist(id).await.map_err(crate::sources::youtube::map_err)?;
        Ok(list.tracks.items.into_iter().map(crate::sources::youtube::map_track).filter(Track::is_sane).collect())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, domain: &str) -> StoredCookie {
        StoredCookie { name: name.into(), value: "v1".into(), domain: domain.into(), path: "/".into(), secure: true, expires: Some(1_900_000_000) }
    }

    #[test]
    fn keeps_only_youtube_cookies_with_a_login() {
        let kept = keep_youtube(vec![c("SAPISID", "youtube.com"), c("SID", ".google.com"), c("LOGIN_INFO", "www.youtube.com")]).unwrap();
        assert_eq!(kept.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["SAPISID", "LOGIN_INFO"]);
        assert!(keep_youtube(vec![c("PREF", "youtube.com")]).is_none(), "signed-out cookies aren't a login");
        let mut bad = c("SAPISID", "youtube.com");
        bad.value = "a\tb".into();
        assert!(keep_youtube(vec![bad]).is_none());
    }

    #[test]
    fn writes_header_and_netscape_file() {
        let list = vec![c("SAPISID", "youtube.com"), StoredCookie { secure: false, expires: None, ..c("YSC", ".youtube.com") }];
        assert_eq!(cookie_header(&list), "SAPISID=v1; YSC=v1");
        let file = netscape(&list);
        assert!(file.starts_with("# Netscape HTTP Cookie File\n"));
        assert!(file.contains(".youtube.com\tTRUE\t/\tTRUE\t1900000000\tSAPISID\tv1\n"));
        assert!(file.contains(".youtube.com\tTRUE\t/\tFALSE\t0\tYSC\tv1\n"));
    }

    #[test]
    fn splits_long_data_into_parts() {
        let text = "x".repeat(PART_BYTES * 2 + 5);
        let parts = split(&text);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.concat(), text);
    }
}
