//! App lock: an optional PIN (and Windows Hello) asked for when Relay starts with a remembered
//! account. While locked, every account command fails (see `commands::current`), so the lock
//! is more than a screen in front of the library. The PIN is stored hashed like the password.

use crate::accounts;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use windows::core::HSTRING;
use windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::IUserConsentVerifierInterop;

/// Wrong PINs in a row before a pause, and how long the pause is.
const TRIES_BEFORE_WAIT: u32 = 5;
const WAIT_SECS: i64 = 30;

#[derive(Default)]
pub struct Lock {
    locked: AtomicBool,
    wrong: AtomicU32,
    blocked_until: AtomicI64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub pin: bool,
    pub hello: bool,
    pub hello_available: bool,
    pub locked: bool,
    /// Seconds until another PIN can be tried (after too many wrong ones).
    pub wait_secs: i64,
}

impl Lock {
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    pub fn set_locked(&self, locked: bool) {
        self.locked.store(locked, Ordering::SeqCst);
    }

    fn wait_secs(&self) -> i64 {
        (self.blocked_until.load(Ordering::SeqCst) - crate::util::now_secs()).max(0)
    }
}

/// The account's PIN hash and whether Windows Hello may unlock it.
pub fn settings_of(db: &Db, account_id: &str) -> AppResult<(Option<String>, bool)> {
    Ok(db
        .with(|c| c.query_row("SELECT lock_hash, lock_hello FROM accounts WHERE id=?1", [account_id], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))).optional())?
        .unwrap_or((None, false)))
}

pub fn status(db: &Db, lock: &Lock, account_id: &str) -> AppResult<Status> {
    let (pin, hello) = settings_of(db, account_id)?;
    Ok(Status { pin: pin.is_some(), hello: hello && pin.is_some(), hello_available: hello_available(), locked: lock.is_locked(), wait_secs: lock.wait_secs() })
}

fn valid_pin(pin: &str) -> AppResult<()> {
    if (4..=8).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err(AppError::new("bad_pin", "4 to 8 digits"))
    }
}

/// Turns the lock on (or changes the PIN). The account password is asked first.
pub fn set_pin(db: &Db, account_id: &str, password: &str, pin: &str) -> AppResult<()> {
    accounts::check_password(db, account_id, password)?;
    valid_pin(pin)?;
    let hash = accounts::hash_secret(pin)?;
    db.with(|c| c.execute("UPDATE accounts SET lock_hash=?1 WHERE id=?2", params![hash, account_id]))?;
    Ok(())
}

/// Turns the lock off. The account password is asked first.
pub fn turn_off(db: &Db, account_id: &str, password: &str) -> AppResult<()> {
    accounts::check_password(db, account_id, password)?;
    db.with(|c| c.execute("UPDATE accounts SET lock_hash=NULL, lock_hello=0 WHERE id=?1", [account_id]))?;
    Ok(())
}

pub fn set_hello(db: &Db, account_id: &str, on: bool) -> AppResult<()> {
    if on && settings_of(db, account_id)?.0.is_none() {
        return Err(AppError::new("no_pin", "set a PIN first"));
    }
    db.with(|c| c.execute("UPDATE accounts SET lock_hello=?1 WHERE id=?2", params![on as i64, account_id]))?;
    Ok(())
}

/// Checks a PIN. After TRIES_BEFORE_WAIT wrong ones, the next try waits WAIT_SECS.
pub fn unlock_with_pin(db: &Db, lock: &Lock, account_id: &str, pin: &str) -> AppResult<()> {
    if lock.wait_secs() > 0 {
        return Err(AppError::new("lock_wait", lock.wait_secs().to_string()));
    }
    let Some(stored) = settings_of(db, account_id)?.0 else {
        lock.set_locked(false);
        return Ok(());
    };
    if accounts::matches_secret(pin, &stored) {
        lock.wrong.store(0, Ordering::SeqCst);
        lock.set_locked(false);
        return Ok(());
    }
    if lock.wrong.fetch_add(1, Ordering::SeqCst) + 1 >= TRIES_BEFORE_WAIT {
        lock.wrong.store(0, Ordering::SeqCst);
        lock.blocked_until.store(crate::util::now_secs() + WAIT_SECS, Ordering::SeqCst);
    }
    Err(AppError::new("wrong_pin", ""))
}

pub fn hello_available() -> bool {
    UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.join()).is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
}

/// Asks Windows Hello (face, fingerprint or the Windows PIN) over Relay's window. Blocking:
/// call it off the async workers.
pub fn verify_with_hello(window: isize, message: &str) -> AppResult<bool> {
    let interop = windows::core::factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(|e| AppError::new("hello_unavailable", e.to_string()))?;
    // SAFETY: the handle is Relay's own main window, alive while the app runs.
    let op: windows_future::IAsyncOperation<UserConsentVerificationResult> = unsafe { interop.RequestVerificationForWindowAsync(HWND(window as *mut _), &HSTRING::from(message)) }
        .map_err(|e| AppError::new("hello_unavailable", e.to_string()))?;
    Ok(op.join().map_err(|e| AppError::new("hello_unavailable", e.to_string()))? == UserConsentVerificationResult::Verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Db, String) {
        let db = Db::in_memory();
        let id = accounts::create(&db, "lock@example.com", "longenough", true).unwrap().account.id;
        (db, id)
    }

    #[test]
    fn a_pin_needs_the_password_and_digits() {
        let (db, id) = setup();
        assert_eq!(set_pin(&db, &id, "wrong password", "1234").unwrap_err().code, "wrong_password");
        assert_eq!(set_pin(&db, &id, "longenough", "12a4").unwrap_err().code, "bad_pin");
        set_pin(&db, &id, "longenough", "1234").unwrap();
        assert!(settings_of(&db, &id).unwrap().0.is_some());
        turn_off(&db, &id, "longenough").unwrap();
        assert!(settings_of(&db, &id).unwrap().0.is_none());
    }

    #[test]
    fn wrong_pins_keep_it_locked_and_then_wait() {
        let (db, id) = setup();
        set_pin(&db, &id, "longenough", "2468").unwrap();
        let lock = Lock::default();
        lock.set_locked(true);
        for _ in 0..TRIES_BEFORE_WAIT {
            assert_eq!(unlock_with_pin(&db, &lock, &id, "0000").unwrap_err().code, "wrong_pin");
        }
        assert!(lock.is_locked());
        assert_eq!(unlock_with_pin(&db, &lock, &id, "2468").unwrap_err().code, "lock_wait", "even the right PIN waits");
        lock.blocked_until.store(0, Ordering::SeqCst);
        unlock_with_pin(&db, &lock, &id, "2468").unwrap();
        assert!(!lock.is_locked());
    }
}
