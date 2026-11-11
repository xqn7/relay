//! One thread with a Windows message loop for the two system hooks that need one: the global
//! mini player shortcut (RegisterHotKey) and foreground-window changes, which drive Game mode
//! (SetWinEventHook). Both arrive as messages, so nothing polls.

use crate::AppState;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, KillTimer, PeekMessageW, PostThreadMessageW, SetTimer,
    EVENT_SYSTEM_FOREGROUND, MSG, PM_NOREMOVE, WINEVENT_OUTOFCONTEXT, WM_APP, WM_HOTKEY, WM_TIMER, WM_USER,
};

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;
/// Holding the keys down fires once, not on every key repeat.
const MOD_NOREPEAT: u32 = 0x4000;

const HOTKEY_ID: i32 = 1;
const WM_SET_SHORTCUT: u32 = WM_APP + 1;
const WM_CHECK_GAME: u32 = WM_APP + 2;
/// Games often switch to fullscreen a moment after their window comes to the front, so the
/// check runs once more after this.
const RECHECK_MS: u32 = 3000;

static APP: OnceLock<AppHandle> = OnceLock::new();
static THREAD: AtomicU32 = AtomicU32::new(0);
static SHORTCUT_OK: AtomicBool = AtomicBool::new(true);

thread_local! {
    static RECHECK_TIMER: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shortcut {
    pub mods: u32,
    pub vk: u32,
}

/// Parses "Alt+Shift+O"-style text. At least Ctrl, Alt or Win is required, so the shortcut
/// can't fire while typing.
pub fn parse_shortcut(text: &str) -> Option<Shortcut> {
    let (mut mods, mut vk) = (0, None);
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" => mods |= MOD_WIN,
            key => {
                if vk.is_some() {
                    return None;
                }
                vk = Some(key_code(key)?);
            }
        }
    }
    if mods & (MOD_CONTROL | MOD_ALT | MOD_WIN) == 0 {
        return None;
    }
    Some(Shortcut { mods, vk: vk? })
}

/// Windows virtual-key codes for letters, digits, F1–F24 and Space.
fn key_code(key: &str) -> Option<u32> {
    let bytes = key.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Some(bytes[0].to_ascii_uppercase() as u32);
    }
    if key == "space" {
        return Some(0x20);
    }
    let n: u32 = key.strip_prefix('f')?.parse().ok()?;
    (1..=24).contains(&n).then_some(0x70 + n - 1)
}

pub fn start(app: AppHandle, shortcut: Option<Shortcut>) {
    let _ = APP.set(app);
    let spawned = std::thread::Builder::new().name("os-events".into()).spawn(move || run(shortcut));
    if let Err(e) = spawned {
        eprintln!("system event thread failed to start: {e}");
    }
}

fn run(initial: Option<Shortcut>) {
    let mut msg = MSG::default();
    // SAFETY: standard message-loop calls on this thread's own queue; the hook callback is a
    // plain function that lives for the whole program.
    unsafe {
        // Creates the message queue before anyone posts to this thread.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        apply_shortcut(initial);
        let hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_foreground), 0, 0, WINEVENT_OUTOFCONTEXT);
        if hook.is_invalid() {
            eprintln!("foreground window hook unavailable; Game mode can't detect games");
        }
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            match msg.message {
                WM_HOTKEY => {
                    if let Some(app) = APP.get() {
                        crate::overlays::toggle_mini(app);
                    }
                }
                WM_SET_SHORTCUT => {
                    let vk = msg.wParam.0 as u32;
                    apply_shortcut((vk != 0).then_some(Shortcut { vk, mods: msg.lParam.0 as u32 }));
                }
                WM_CHECK_GAME => check_game(),
                WM_TIMER => {
                    let _ = KillTimer(None, msg.wParam.0);
                    RECHECK_TIMER.with(|t| t.set(0));
                    check_game();
                }
                _ => {}
            }
        }
    }
}

fn apply_shortcut(shortcut: Option<Shortcut>) {
    // SAFETY: registered for this thread's queue (no window); unregistering a missing id is fine.
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
        let ok = match shortcut {
            Some(s) => RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(s.mods | MOD_NOREPEAT), s.vk).is_ok(),
            None => true,
        };
        SHORTCUT_OK.store(ok, Ordering::SeqCst);
        if let Some(app) = APP.get() {
            let _ = app.emit("shortcut-status", ok);
        }
    }
}

/// False when another app already owns the chosen shortcut.
pub fn shortcut_ok() -> bool {
    SHORTCUT_OK.load(Ordering::SeqCst)
}

fn post(msg: u32, wparam: usize, lparam: isize) {
    let thread = THREAD.load(Ordering::SeqCst);
    if thread != 0 {
        // SAFETY: posting plain numbers to our own thread's queue.
        let _ = unsafe { PostThreadMessageW(thread, msg, WPARAM(wparam), LPARAM(lparam)) };
    }
}

pub fn set_shortcut(shortcut: Option<Shortcut>) {
    let (vk, mods) = shortcut.map(|s| (s.vk, s.mods)).unwrap_or((0, 0));
    post(WM_SET_SHORTCUT, vk as usize, mods as isize);
}

/// Re-evaluates Game mode now (e.g. right after the setting changed).
pub fn recheck_game() {
    post(WM_CHECK_GAME, 0, 0);
}

unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    check_game();
    RECHECK_TIMER.with(|t| {
        // SAFETY: thread timers on the loop's own thread; the old one is replaced.
        unsafe {
            if t.get() != 0 {
                let _ = KillTimer(None, t.get());
            }
            t.set(SetTimer(None, 0, RECHECK_MS, None));
        }
    });
}

fn check_game() {
    let Some(app) = APP.get() else { return };
    let state = app.state::<Arc<AppState>>();
    let active = state.db.pref::<bool>("gameMode").unwrap_or(false) && fullscreen_app_in_front();
    crate::desktop::set_game(state.inner(), active);
}

/// A game or other fullscreen app (not Relay itself) is in front.
fn fullscreen_app_in_front() -> bool {
    // SAFETY: read-only queries about the foreground window.
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == std::process::id() {
            return false;
        }
        SHQueryUserNotificationState()
            .map(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_BUSY || s == QUNS_PRESENTATION_MODE)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        assert_eq!(parse_shortcut("Alt+Shift+O"), Some(Shortcut { mods: MOD_ALT | MOD_SHIFT, vk: b'O' as u32 }));
        assert_eq!(parse_shortcut("ctrl + alt + f9"), Some(Shortcut { mods: MOD_CONTROL | MOD_ALT, vk: 0x78 }));
        assert_eq!(parse_shortcut("Win+Space"), Some(Shortcut { mods: MOD_WIN, vk: 0x20 }));
        assert_eq!(parse_shortcut("Ctrl+7").map(|s| s.vk), Some(b'7' as u32));
    }

    #[test]
    fn rejects_unsafe_or_broken_shortcuts() {
        // Shift alone would fire while typing capitals.
        assert_eq!(parse_shortcut("Shift+O"), None);
        assert_eq!(parse_shortcut("O"), None);
        assert_eq!(parse_shortcut("Alt+O+P"), None);
        assert_eq!(parse_shortcut("Alt+F25"), None);
        assert_eq!(parse_shortcut("Alt+Tab"), None);
        assert_eq!(parse_shortcut("Alt"), None);
    }
}
