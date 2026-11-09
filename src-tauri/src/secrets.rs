//! Secrets (API keys, sign-in tokens) live in the Windows Credential Manager, never in Relay's
//! own files or database, so a backup or a copied data folder never contains them.

use crate::error::{AppError, AppResult};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Security::Credentials::{CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};

const MAX_SECRET_BYTES: usize = 2048;

fn target(name: &str) -> Vec<u16> {
    format!("Relay/{name}").encode_utf16().chain(Some(0)).collect()
}

pub fn get(name: &str) -> Option<String> {
    let target = target(name);
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredReadW fills `cred` with a buffer we own until CredFree; the blob pointer and
    // size come from that same buffer.
    unsafe {
        CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
        let c = &*cred;
        let value = if c.CredentialBlob.is_null() {
            None
        } else {
            String::from_utf8(std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()).ok()
        };
        CredFree(cred as *const _);
        value
    }
}

pub fn set(name: &str, value: &str) -> AppResult<()> {
    if value.len() > MAX_SECRET_BYTES {
        return Err(AppError::new("invalid_input", "secret too long"));
    }
    let mut target = target(name);
    let mut blob = value.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: every pointer in `cred` points into buffers that outlive the call.
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| AppError::new("io", format!("Credential Manager: {e}")))
}

pub fn delete(name: &str) {
    let target = target(name);
    // SAFETY: plain call with a null-terminated name; a missing entry is fine.
    let _ = unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) };
}
