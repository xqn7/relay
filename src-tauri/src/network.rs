//! Knows whether the internet is reachable, so the app can switch to offline mode (only songs
//! on disk) and back without the user doing anything. Driven by events, never by a timer:
//! Windows tells us when the network changes, and failed requests make us check right away.
//! Also runs the connection check behind the "Connection issue" window.

use crate::sources::{SearchKind, Source};
use crate::AppState;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const CHECK_TIMEOUT: Duration = Duration::from_secs(12);
/// A video that has stayed up since 2005, used to see whether YouTube hands out streams.
const CHECK_VIDEO: &str = "jNQXAC9IVRw";
/// Repeated failures within this window share one probe.
const PROBE_REUSE: Duration = Duration::from_secs(5);
/// Windows' own connectivity test page and its exact answer, plus a second opinion.
const PROBE_URL: &str = "http://www.msftconnecttest.com/connecttest.txt";
const PROBE_BODY: &str = "Microsoft Connect Test";
const PROBE_URL_2: &str = "https://www.google.com/generate_204";

pub struct Network {
    online: AtomicBool,
    last_probe: Mutex<Option<Instant>>,
    probing: tokio::sync::Mutex<()>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// "internet", "youtube", "soundcloud", "spotify", "covers", "lyrics"
    pub id: &'static str,
    pub ok: bool,
    /// Error code when it failed (the UI explains it), "" when it passed.
    pub code: &'static str,
    pub detail: String,
}

impl Network {
    pub fn new() -> Self {
        Self { online: AtomicBool::new(true), last_probe: Mutex::new(None), probing: tokio::sync::Mutex::new(()) }
    }

    pub fn online(&self) -> bool {
        self.online.load(Ordering::SeqCst)
    }
}

fn set(state: &Arc<AppState>, online: bool) {
    if state.net.online.swap(online, Ordering::SeqCst) == online {
        return;
    }
    let _ = state.app.emit("network-status", online);
    if online {
        // Downloads that stopped when the connection dropped continue.
        crate::storage::sync(state);
    }
}

/// True if the probe page answers as expected.
pub fn probe_answer_ok(status: u16, body: &str) -> bool {
    status == 200 && body.trim() == PROBE_BODY
}

async fn reachable(state: &AppState) -> bool {
    let first = async {
        let resp = state.http.client.get(PROBE_URL).timeout(PROBE_TIMEOUT).send().await.ok()?;
        let status = resp.status().as_u16();
        Some(probe_answer_ok(status, &resp.text().await.ok()?))
    };
    if first.await == Some(true) {
        return true;
    }
    // Some networks block Microsoft's test page; a second, unrelated site decides.
    matches!(state.http.client.get(PROBE_URL_2).timeout(PROBE_TIMEOUT).send().await, Ok(r) if r.status().as_u16() == 204)
}

/// Checks the connection now (or reuses a check from the last few seconds) and updates the
/// online state. `force` skips the reuse, for network changes and "Try again".
pub async fn probe(state: &Arc<AppState>, force: bool) -> bool {
    let _one_at_a_time = state.net.probing.lock().await;
    let recent = lock_last(state).is_some_and(|t| t.elapsed() < PROBE_REUSE);
    if recent && !force {
        return state.net.online();
    }
    let ok = reachable(state).await;
    *lock_last(state) = Some(Instant::now());
    set(state, ok);
    ok
}

fn lock_last(state: &AppState) -> std::sync::MutexGuard<'_, Option<Instant>> {
    state.net.last_probe.lock().unwrap_or_else(|p| p.into_inner())
}

/// Before going online for a song: true when online. While offline it checks again (at most
/// every few seconds), so a fixed connection is noticed on the next song.
pub async fn ensure_online(state: &Arc<AppState>) -> bool {
    state.net.online() || probe(state, false).await
}

/// A request failed; if it looks like a connection problem, find out whether we're offline.
pub fn suspect(state: &Arc<AppState>, code: &str) {
    if matches!(code, "offline" | "network" | "timeout") {
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            probe(&state, false).await;
        });
    }
}

/// Listens for Windows' network change events and checks the connection on each one.
pub fn watch(app: &tauri::AppHandle) {
    use windows::Networking::Connectivity::{NetworkInformation, NetworkStatusChangedEventHandler};
    let app = app.clone();
    let handler = NetworkStatusChangedEventHandler::new(move |_| {
        let state = app.state::<Arc<AppState>>().inner().clone();
        tauri::async_runtime::spawn(async move {
            probe(&state, true).await;
        });
        Ok(())
    });
    if let Err(e) = NetworkInformation::NetworkStatusChanged(&handler) {
        eprintln!("network change events unavailable: {e}");
    }
}

// ---- connection check ---------------------------------------------------------------------

fn result(id: &'static str, r: Result<(), crate::error::AppError>) -> Check {
    match r {
        Ok(()) => Check { id, ok: true, code: "", detail: String::new() },
        Err(e) => Check { id, ok: false, code: e.code, detail: e.detail },
    }
}

/// Any HTTP answer at all means the host is reachable (a 404 for a made-up path is fine).
async fn answers(state: &AppState, url: &str) -> Result<(), crate::error::AppError> {
    state.http.client.get(url).timeout(CHECK_TIMEOUT).send().await?;
    Ok(())
}

async fn ok_status(state: &AppState, url: &str) -> Result<(), crate::error::AppError> {
    let resp = state.http.client.get(url).timeout(CHECK_TIMEOUT).send().await?;
    crate::http::check_status(&resp)
}

async fn timed<F: std::future::Future<Output = Result<(), crate::error::AppError>>>(f: F) -> Result<(), crate::error::AppError> {
    tokio::time::timeout(CHECK_TIMEOUT, f).await.unwrap_or_else(|_| Err(crate::error::AppError::new("timeout", "no answer in time")))
}

/// Tests everything the app talks to. Services are only tested when the internet works,
/// otherwise each would just repeat "no internet".
pub async fn check_all(state: &Arc<AppState>) -> Vec<Check> {
    let internet = probe(state, true).await;
    if !internet {
        let mut out = vec![Check { id: "internet", ok: false, code: "offline", detail: "connectivity test failed".into() }];
        for id in ["youtube", "soundcloud", "spotify", "covers", "lyrics"] {
            out.push(Check { id, ok: false, code: "skipped", detail: String::new() });
        }
        return out;
    }
    let youtube = timed(async {
        if !state.tools.ytdlp_ready() {
            return Err(crate::error::AppError::new("helper_installing", "yt-dlp is still downloading"));
        }
        state.yt.search_songs("music", false).await?;
        // Search can work while playback is blocked ("confirm you're not a bot", often on
        // VPN addresses), so also ask for a real stream: YouTube's first video, 19 s long.
        state.tools.ytdlp_stream(CHECK_VIDEO, crate::sources::Quality::DataSaver).await.map(|_| ())
    });
    let soundcloud = timed(async { state.sc.search("music", SearchKind::Songs).await.map(|_| ()) });
    let spotify = ok_status(state, "https://open.spotify.com/embed/track/4cOdK2wGLETKBW3PvgPWqT");
    let covers = timed(async {
        answers(state, "https://lh3.googleusercontent.com/").await?;
        answers(state, "https://i1.sndcdn.com/").await
    });
    let lyrics = answers(state, "https://lrclib.net/api/get?artist_name=Relay&track_name=Connection%20check");
    let (y, s, sp, c, l) = tokio::join!(youtube, soundcloud, spotify, covers, lyrics);
    vec![
        Check { id: "internet", ok: true, code: "", detail: String::new() },
        result("youtube", y),
        result("soundcloud", s),
        result("spotify", sp),
        result("covers", c),
        result("lyrics", l),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_needs_the_exact_answer() {
        assert!(probe_answer_ok(200, "Microsoft Connect Test"));
        assert!(probe_answer_ok(200, "Microsoft Connect Test\n"));
        // A captive portal (hotel Wi-Fi login page) answers, but with something else.
        assert!(!probe_answer_ok(200, "<html>Please log in</html>"));
        assert!(!probe_answer_ok(302, "Microsoft Connect Test"));
    }
}
