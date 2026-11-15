//! The few registry values Relay reads or writes: start with Windows, the CPU name and the
//! Windows build. All in places a normal user may access.

use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_SZ,
    RRF_RT_REG_SZ,
};

/// A text value under HKEY_LOCAL_MACHINE.
pub fn get_machine_string(key: &str, name: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut buf = vec![0u16; 512];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: the buffer and its byte size are passed together; RegGetValueW writes at most that.
    let err = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, &key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut bytes))
    };
    if err.is_err() {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]).trim().to_string())
}

pub fn set_user_string(key: &str, name: &str, value: &str) -> windows::core::Result<()> {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    // SAFETY: the data pointer and its byte length (including the terminating zero) match.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
        )
    }
    .ok()
}

pub fn delete_user_value(key: &str, name: &str) -> windows::core::Result<()> {
    // SAFETY: plain call with two null-terminated strings.
    let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name)) };
    // Already gone is what was wanted.
    if err == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    err.ok()
}

