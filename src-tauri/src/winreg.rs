//! The few registry values Relay reads or writes: start with Windows, the CPU name, and
//! whether Windows Spotlight runs the lock screen. All in places a normal user may access.

use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_SZ, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ,
};

pub enum Root {
    User,
    Machine,
}

fn hkey(root: Root) -> HKEY {
    match root {
        Root::User => HKEY_CURRENT_USER,
        Root::Machine => HKEY_LOCAL_MACHINE,
    }
}

pub fn get_string(root: Root, key: &str, name: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut buf = vec![0u16; 512];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: the buffer and its byte size are passed together; RegGetValueW writes at most that.
    let err = unsafe {
        RegGetValueW(hkey(root), &key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut bytes))
    };
    if err.is_err() {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]).trim().to_string())
}

pub fn get_dword(root: Root, key: &str, name: &str) -> Option<u32> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut value = 0u32;
    let mut bytes = 4u32;
    // SAFETY: a DWORD-sized buffer for a value restricted to REG_DWORD.
    let err = unsafe {
        RegGetValueW(hkey(root), &key, &name, RRF_RT_REG_DWORD, None, Some((&mut value as *mut u32).cast()), Some(&mut bytes))
    };
    err.is_ok().then_some(value)
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

