//! Album-art lock screen: while music plays, the Windows lock screen shows an image the UI
//! draws (cover, title, artist). The user's own picture is saved first and put back when
//! playback pauses or stops, the option is turned off, or Relay closes (or, after a crash, on
//! the next start). Windows Spotlight can't be switched back on by an app, so that case is
//! reported once instead.

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows::core::HSTRING;
use windows::Storage::StorageFile;
use windows::Storage::Streams::DataReader;
use windows::System::UserProfile::LockScreen;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

/// Present while Relay's picture is on the lock screen.
const ACTIVE: &str = "active";
const MAX_IMAGE_BYTES: usize = 30 * 1024 * 1024;

pub struct LockScreenArt {
    dir: PathBuf,
    /// Serializes changes; Windows copies the file during each call.
    lock: Mutex<u32>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// Spotlight was on before; it has to be picked again by hand afterwards.
    pub spotlight: bool,
}

fn win(e: windows::core::Error) -> AppError {
    AppError::new("lockscreen", e.to_string())
}

fn init_winrt() {
    // SAFETY: joins this worker thread to the multithreaded apartment; "already done" is fine.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
}

fn spotlight_on() -> bool {
    crate::winreg::get_dword(
        crate::winreg::Root::User,
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        "RotatingLockScreenEnabled",
    ) == Some(1)
}

fn apply(path: &Path) -> AppResult<()> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str())).map_err(win)?.join().map_err(win)?;
    LockScreen::SetImageFileAsync(&file).map_err(win)?.join().map_err(win)
}

fn original_path(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_stem().is_some_and(|s| s == "original"))
}

/// Copies the current lock screen picture, once, before Relay replaces it.
fn save_original(dir: &Path) -> AppResult<()> {
    if original_path(dir).is_some() {
        return Ok(());
    }
    let stream = LockScreen::GetImageStream().map_err(win)?;
    let size = stream.Size().map_err(win)? as usize;
    if size == 0 || size > MAX_IMAGE_BYTES {
        return Err(AppError::new("lockscreen", "the current lock screen picture can't be read"));
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(win)?).map_err(win)?;
    reader.LoadAsync(size as u32).map_err(win)?.join().map_err(win)?;
    let mut bytes = vec![0u8; size];
    reader.ReadBytes(&mut bytes).map_err(win)?;
    // Windows picks the decoder by file extension, so it has to match the content.
    let ext = if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(b"BM") {
        "bmp"
    } else {
        "jpg"
    };
    std::fs::write(dir.join(format!("original.{ext}")), bytes)?;
    Ok(())
}

impl LockScreenArt {
    pub fn new(data_dir: &Path) -> Self {
        Self { dir: data_dir.join("lockscreen"), lock: Mutex::new(0) }
    }

    /// Puts a PNG drawn by the UI on the lock screen.
    pub fn set(&self, png: &[u8]) -> AppResult<Applied> {
        if png.len() > MAX_IMAGE_BYTES || !png.starts_with(b"\x89PNG") {
            return Err(AppError::new("invalid_input", "expected a PNG image"));
        }
        let mut n = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        init_winrt();
        std::fs::create_dir_all(&self.dir)?;
        let first = !self.dir.join(ACTIVE).exists();
        let spotlight = first && spotlight_on();
        save_original(&self.dir)?;
        // Two names in turn: Windows may still be reading the previous file.
        *n = (*n + 1) % 2;
        let path = self.dir.join(format!("now-{}.png", *n));
        std::fs::write(&path, png)?;
        apply(&path)?;
        std::fs::write(self.dir.join(ACTIVE), b"")?;
        Ok(Applied { spotlight })
    }

    /// Puts the user's own picture back, if Relay's is showing.
    pub fn restore(&self) -> AppResult<()> {
        let _n = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if !self.dir.join(ACTIVE).exists() {
            return Ok(());
        }
        init_winrt();
        if let Some(original) = original_path(&self.dir) {
            apply(&original)?;
        }
        // The saved original stays until it's been put back, then the folder is cleared.
        let _ = std::fs::remove_dir_all(&self.dir);
        Ok(())
    }
}
