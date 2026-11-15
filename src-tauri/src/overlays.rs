//! The two small always-on-top windows: the mini player and desktop lyrics. Both load the app's
//! own page in a light mode and follow the main window's player through events; neither plays
//! audio. They never take focus, so showing one over a game doesn't pull the game out of
//! fullscreen. Each is created on first use and destroyed when its setting is turned off.

use crate::AppState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

pub const MINI: &str = "mini";
pub const LYRICS: &str = "lyrics";
/// Logical size of the mini player pill.
const MINI_SIZE: (f64, f64) = (252.0, 40.0);
const LYRICS_SIZE: (f64, f64) = (760.0, 116.0);
const LYRICS_MIN: (f64, f64) = (320.0, 64.0);
/// Gap between an overlay and the screen edge it sits at (logical px).
const EDGE_GAP: f64 = 12.0;
/// Desktop lyrics sit this far above the bottom of the work area, like subtitles.
const LYRICS_BOTTOM_GAP: f64 = 64.0;
/// A drag is saved once the window has been still this long.
const SAVE_AFTER: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Overlays {
    /// The mini player is showing only because the main window was minimized.
    mini_auto: AtomicBool,
    /// Where the app itself last put each window; any other position is the user's drag.
    placed: Mutex<[Option<(i32, i32)>; 2]>,
    /// While unlocked, desktop lyrics can be dragged and resized; otherwise clicks pass through.
    lyrics_unlocked: AtomicBool,
    /// The main window has lyrics to show right now (a song with synced lyrics is playing).
    /// Without them the desktop lyrics window is closed, so it costs no memory.
    lyrics_active: AtomicBool,
    save_seq: AtomicU64,
}

fn slot(label: &str) -> usize {
    usize::from(label == LYRICS)
}

fn custom_key(label: &str) -> &'static str {
    if label == MINI {
        "mini.custom"
    } else {
        "lyrics.custom"
    }
}

fn pref_bool(state: &AppState, key: &str, default: bool) -> bool {
    state.db.pref::<bool>(key).unwrap_or(default)
}

fn pref_str(state: &AppState, key: &str, default: &str) -> String {
    state.db.pref::<String>(key).unwrap_or_else(|| default.to_string())
}

/// X of a window `w` wide at the left / middle / right of an area, `gap` from its edges.
pub fn preset_x(kind: &str, left: i32, area_w: i32, w: i32, gap: i32) -> i32 {
    match kind {
        "left" => left + gap,
        "right" => left + area_w - w - gap,
        _ => left + (area_w - w) / 2,
    }
}

fn state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

fn get_or_build(app: &AppHandle, label: &'static str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }
    match build(app, label) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("{label} window failed: {e}");
            None
        }
    }
}

fn build(app: &AppHandle, label: &'static str) -> tauri::Result<WebviewWindow> {
    let mini = label == MINI;
    let (w, h) = if mini { MINI_SIZE } else { LYRICS_SIZE };
    let st = state(app);
    let on_top = !mini || pref_bool(&st, "miniOnTop", true);
    // Windows 11 can blur what's behind the mini player and round its corners; Windows 10
    // draws the pill itself (the page is told which, so it picks its look).
    let blur = mini && is_windows_11();
    let url = format!("overlay.html?w={label}{}", if blur { "&blur=1" } else { "" });
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(format!("{} {}", crate::app_name(), if mini { "mini player" } else { "lyrics" }))
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(on_top)
        .resizable(!mini)
        .focusable(false)
        .focused(false)
        .visible(false)
        .additional_browser_args(crate::BROWSER_ARGS)
        .on_navigation(crate::is_app_url);
    if !mini {
        builder = builder.min_inner_size(LYRICS_MIN.0, LYRICS_MIN.1);
    }
    if blur {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::Effect;
        builder = builder.effects(WindowEffectsConfig { effects: vec![Effect::Acrylic], ..Default::default() });
    }
    let window = builder.build()?;
    if let Ok(hwnd) = window.hwnd() {
        tool_window(hwnd);
        if blur {
            round_corners(hwnd);
        }
    }
    let app = app.clone();
    window.on_window_event(move |event| match event {
        // Alt+F4 on an overlay hides it; it's not a window with its own life.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if let Some(w) = app.get_webview_window(label) {
                hide(&w);
            }
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => remember_later(&app, label),
        _ => {}
    });
    Ok(window)
}

// After creation, overlays are only changed through Win32: Tauri's runtime keeps its own copy
// of the window's state (hidden, since it was never shown through Tauri, which would take focus)
// and writes it back on many calls (resize, click-through, always on top), hiding the window.

fn ex_style(hwnd: HWND, set: u32, clear: u32) {
    // SAFETY: reading and updating the extended style of our own window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((ex | set) & !clear) as isize);
    }
}

/// Unlocked desktop lyrics take the mouse; locked ones let every click through.
fn set_click_through(window: &WebviewWindow, on: bool) {
    let Ok(hwnd) = window.hwnd() else { return };
    let bits = (WS_EX_TRANSPARENT | WS_EX_LAYERED).0;
    if on {
        ex_style(hwnd, bits, 0);
    } else {
        ex_style(hwnd, 0, bits);
    }
}

fn is_shown(window: &WebviewWindow) -> bool {
    // SAFETY: a read-only query about our own window.
    window.hwnd().is_ok_and(|h| unsafe { IsWindowVisible(h) }.as_bool())
}

fn hide(window: &WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        // SAFETY: hides our own window.
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

/// Moves (and optionally resizes) without touching z-order or focus.
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, size: Option<(i32, i32)>) {
    let Ok(hwnd) = window.hwnd() else { return };
    let (w, h, keep) = match size {
        Some((w, h)) => (w, h, SWP_NOACTIVATE | SWP_NOZORDER),
        None => (0, 0, SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE),
    };
    // SAFETY: repositions our own window.
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, keep) };
}

/// Always on top (or not), without showing or activating.
fn set_on_top(window: &WebviewWindow, on_top: bool) {
    let Ok(hwnd) = window.hwnd() else { return };
    let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    // SAFETY: changes only our own window's z-order.
    let _ = unsafe { SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
}

/// Keeps the window out of Alt+Tab.
fn tool_window(hwnd: HWND) {
    ex_style(hwnd, WS_EX_TOOLWINDOW.0, 0);
}

fn round_corners(hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE};
    let pref = DWMWCP_ROUND;
    // SAFETY: the attribute value is a DWORD-sized enum passed by pointer with its size.
    let _ = unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, (&pref as *const DWM_WINDOW_CORNER_PREFERENCE).cast(), std::mem::size_of_val(&pref) as u32)
    };
}

fn is_windows_11() -> bool {
    let build = crate::winreg::get_machine_string(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber");
    build.and_then(|b| b.parse::<u32>().ok()).is_some_and(|b| b >= 22000)
}

/// Shows without taking focus, so a fullscreen game stays in front and fullscreen, and puts it
/// back on top, since a game may have gone topmost since.
fn show_quietly(window: &WebviewWindow, on_top: bool) {
    let Ok(hwnd) = window.hwnd() else { return };
    let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    // SAFETY: shows our own window and sets its z-order; no size, position or focus change.
    let _ = unsafe { SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW) };
}

/// The work area (screen minus taskbar) of the primary monitor, and its scale.
fn work_area(app: &AppHandle) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>, f64)> {
    let m = app.primary_monitor().ok()??;
    let area = m.work_area();
    Some((area.position, area.size, m.scale_factor()))
}

/// A saved spot only counts while it's still on one of the screens.
fn on_some_screen(app: &AppHandle, x: i32, y: i32) -> bool {
    app.available_monitors().unwrap_or_default().iter().any(|m| {
        let (p, s) = (m.position(), m.size());
        x >= p.x && y >= p.y && x < p.x + s.width as i32 - 8 && y < p.y + s.height as i32 - 8
    })
}

fn saved(state: &AppState, label: &str) -> Option<Vec<i32>> {
    state.db.device_get(custom_key(label)).ok().flatten().and_then(|v| serde_json::from_str(&v).ok())
}

fn place(app: &AppHandle, window: &WebviewWindow, label: &str) {
    let st = state(app);
    let Some((origin, area, scale)) = work_area(app) else { return };
    let custom = saved(&st, label).filter(|v| v.len() >= 2 && on_some_screen(app, v[0], v[1]));
    let (x, y) = if label == MINI {
        let w = (MINI_SIZE.0 * scale) as i32;
        match custom {
            Some(v) => (v[0], v[1]),
            None => {
                let kind = pref_str(&st, "miniPosition", "center");
                (preset_x(&kind, origin.x, area.width as i32, w, (EDGE_GAP * scale) as i32), origin.y + (EDGE_GAP * scale) as i32)
            }
        }
    } else {
        let (w, h) = custom
            .as_ref()
            .filter(|v| v.len() == 4 && v[2] > 0 && v[3] > 0)
            .map(|v| (v[2], v[3]))
            .unwrap_or(((LYRICS_SIZE.0 * scale) as i32, (LYRICS_SIZE.1 * scale) as i32));
        let (x, y) = match custom {
            Some(v) => (v[0], v[1]),
            None => {
                let x = preset_x("center", origin.x, area.width as i32, w, 0);
                let y = if pref_str(&st, "lyricsPosition", "bottom") == "top" {
                    origin.y + (EDGE_GAP * 4.0 * scale) as i32
                } else {
                    origin.y + area.height as i32 - h - (LYRICS_BOTTOM_GAP * scale) as i32
                };
                (x, y)
            }
        };
        st.desktop.overlays.placed.lock().unwrap_or_else(|p| p.into_inner())[slot(label)] = Some((x, y));
        set_bounds(window, x, y, Some((w, h)));
        return;
    };
    st.desktop.overlays.placed.lock().unwrap_or_else(|p| p.into_inner())[slot(label)] = Some((x, y));
    set_bounds(window, x, y, None);
}

/// After the user drags (or resizes) an overlay, its spot is saved once it settles.
fn remember_later(app: &AppHandle, label: &'static str) {
    let st = state(app);
    let seq = st.desktop.overlays.save_seq.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_AFTER).await;
        let st = state(&app);
        if st.desktop.overlays.save_seq.load(Ordering::SeqCst) != seq {
            return;
        }
        let Some(window) = app.get_webview_window(label) else { return };
        let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else { return };
        let placed = st.desktop.overlays.placed.lock().unwrap_or_else(|p| p.into_inner())[slot(label)];
        let resized = label == LYRICS && saved(&st, label).is_some_and(|v| v.len() == 4 && (v[2], v[3]) != (size.width as i32, size.height as i32));
        if placed == Some((pos.x, pos.y)) && !resized {
            return;
        }
        let value = if label == MINI {
            serde_json::json!([pos.x, pos.y])
        } else {
            serde_json::json!([pos.x, pos.y, size.width, size.height])
        };
        let _ = st.db.device_set(custom_key(label), &value.to_string());
        let _ = app.emit("overlay-moved", label);
    });
}

/// Choosing a preset position drops the dragged spot and moves the window there.
fn reset_position_now(app: &AppHandle, label: &'static str) {
    let _ = state(app).db.device_delete(custom_key(label));
    if let Some(w) = app.get_webview_window(label) {
        place(app, &w, label);
    }
}

pub fn has_custom_position(state: &AppState, label: &str) -> bool {
    saved(state, label).is_some()
}

// ---- mini player ---------------------------------------------------------------------------

fn mini_enabled(state: &AppState) -> bool {
    pref_bool(state, "miniPlayer", true)
}

fn show_mini_now(app: &AppHandle) {
    let st = state(app);
    if !mini_enabled(&st) {
        return;
    }
    let Some(w) = get_or_build(app, MINI) else { return };
    if !is_shown(&w) {
        place(app, &w, MINI);
    }
    show_quietly(&w, pref_bool(&st, "miniOnTop", true));
}

fn hide_mini_now(app: &AppHandle) {
    state(app).desktop.overlays.mini_auto.store(false, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(MINI) {
        hide(&w);
    }
}

/// The global shortcut and the tray: show if hidden, hide if shown.
fn toggle_mini_now(app: &AppHandle) {
    let visible = app.get_webview_window(MINI).is_some_and(|w| is_shown(&w));
    if visible {
        hide_mini_now(app);
    } else {
        state(app).desktop.overlays.mini_auto.store(false, Ordering::SeqCst);
        show_mini_now(app);
    }
}

/// Auto-show: the mini player appears while the main window is minimized and goes away again
/// when it comes back (unless it was opened on purpose).
fn main_minimized_now(app: &AppHandle, minimized: bool) {
    let st = state(app);
    let overlays = &st.desktop.overlays;
    if minimized {
        let visible = app.get_webview_window(MINI).is_some_and(|w| is_shown(&w));
        if !visible && pref_bool(&st, "miniAutoShow", false) && mini_enabled(&st) {
            overlays.mini_auto.store(true, Ordering::SeqCst);
            show_mini_now(app);
        }
    } else if overlays.mini_auto.swap(false, Ordering::SeqCst) {
        hide_mini_now(app);
    }
}

/// After a mini player setting changed.
fn apply_mini_settings_now(app: &AppHandle) {
    let st = state(app);
    if !mini_enabled(&st) {
        if let Some(w) = app.get_webview_window(MINI) {
            let _ = w.destroy();
        }
        return;
    }
    if let Some(w) = app.get_webview_window(MINI) {
        let on_top = pref_bool(&st, "miniOnTop", true);
        set_on_top(&w, on_top);
        if is_shown(&w) {
            place(app, &w, MINI);
            show_quietly(&w, on_top);
        }
    }
}

// ---- desktop lyrics ------------------------------------------------------------------------

/// Shows or removes the desktop lyrics window to match the setting.
fn sync_lyrics_now(app: &AppHandle) {
    let st = state(app);
    if !pref_bool(&st, "desktopLyrics", false) {
        st.desktop.overlays.lyrics_unlocked.store(false, Ordering::SeqCst);
        if let Some(w) = app.get_webview_window(LYRICS) {
            let _ = w.destroy();
        }
        crate::tray::refresh(app);
        return;
    }
    let unlocked = lyrics_unlocked(&st);
    if !unlocked && !st.desktop.overlays.lyrics_active.load(Ordering::SeqCst) {
        if let Some(w) = app.get_webview_window(LYRICS) {
            let _ = w.destroy();
        }
        crate::tray::refresh(app);
        return;
    }
    let Some(w) = get_or_build(app, LYRICS) else { return };
    place(app, &w, LYRICS);
    set_click_through(&w, !unlocked);
    show_quietly(&w, true);
    crate::tray::refresh(app);
}

pub fn lyrics_unlocked(state: &AppState) -> bool {
    state.desktop.overlays.lyrics_unlocked.load(Ordering::SeqCst)
}

/// Unlocked desktop lyrics take the mouse (to drag and resize); locked ones let every click
/// through to whatever is underneath.
fn set_lyrics_unlocked_now(app: &AppHandle, unlocked: bool) {
    let st = state(app);
    let unlocked = unlocked && pref_bool(&st, "desktopLyrics", false);
    st.desktop.overlays.lyrics_unlocked.store(unlocked, Ordering::SeqCst);
    // Unlocking opens the window even with nothing playing, so it can be placed; locking it
    // again closes it if there's still nothing to show.
    sync_lyrics_now(app);
    let _ = app.emit("lyrics-unlocked", unlocked);
    crate::tray::refresh(app);
}

// ---- entry points ----------------------------------------------------------------------------
// Window work always runs on the async runtime: creating a webview from the UI thread (sync
// commands, menu and window events run there) can deadlock on Windows.

fn later(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { f(&app) });
}

pub fn toggle_mini(app: &AppHandle) {
    later(app, toggle_mini_now);
}

pub fn apply_mini_settings(app: &AppHandle) {
    later(app, apply_mini_settings_now);
}

pub fn sync_lyrics(app: &AppHandle) {
    later(app, sync_lyrics_now);
}

pub fn set_lyrics_active(app: &AppHandle, active: bool) {
    let st = state(app);
    if st.desktop.overlays.lyrics_active.swap(active, Ordering::SeqCst) != active {
        sync_lyrics(app);
    }
}

pub fn reset_position(app: &AppHandle, label: &'static str) {
    later(app, move |app| reset_position_now(app, label));
}

pub fn set_lyrics_unlocked(app: &AppHandle, unlocked: bool) {
    later(app, move |app| set_lyrics_unlocked_now(app, unlocked));
}

pub fn main_minimized(app: &AppHandle, minimized: bool) {
    later(app, move |app| main_minimized_now(app, minimized));
}

#[cfg(test)]
mod tests {
    use super::preset_x;

    #[test]
    fn presets_sit_at_the_edges_or_the_middle() {
        assert_eq!(preset_x("left", 0, 1920, 252, 12), 12);
        assert_eq!(preset_x("center", 0, 1920, 252, 12), 834);
        assert_eq!(preset_x("right", 0, 1920, 252, 12), 1656);
        // A second screen to the left of the main one.
        assert_eq!(preset_x("right", -1280, 1280, 200, 10), -210);
    }
}
