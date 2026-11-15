//! The release build has no console, so its error messages (`eprintln!`, panics) would vanish.
//! They go to relay.log in the app's local folder instead, where the connection check can
//! include them in its copied details. The previous run's log is kept as relay.old.log.

use std::os::windows::io::IntoRawHandle;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};

const LOG: &str = "relay.log";
const OLD_LOG: &str = "relay.old.log";
/// The copied details carry this many of the newest log lines.
const TAIL_LINES: usize = 80;

static PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Sends this process's stderr to the log file. Rust looks the handle up on every write, so
/// everything printed from now on lands there.
pub fn start(dir: &Path) {
    // A debug run prints to the terminal it was started from.
    if cfg!(debug_assertions) {
        return;
    }
    let path = dir.join(LOG);
    let _ = std::fs::rename(&path, dir.join(OLD_LOG));
    let file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let handle = HANDLE(file.into_raw_handle());
    // SAFETY: the handle is a freshly opened, owned file handle that is never closed, so it
    // stays valid for the life of the process.
    if unsafe { SetStdHandle(STD_ERROR_HANDLE, handle) }.is_ok() {
        let _ = PATH.set(path);
        eprintln!("{} {} started", crate::app_name(), env!("CARGO_PKG_VERSION"));
    }
}

/// The newest lines of this run's log ("" without a log).
pub fn tail() -> String {
    let Some(text) = PATH.get().and_then(|p| std::fs::read_to_string(p).ok()) else { return String::new() };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}
