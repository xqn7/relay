//! Local accounts: email + password, Argon2id hashes, one-time recovery key. Nothing leaves the PC.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::util::{now_secs, random_bytes, uuid_v4};
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

const SIGNED_IN_KEY: &str = "signed_in_account";
const MIN_PASSWORD: usize = 8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub account: Account,
    pub recovery_key: String,
}

fn hash(secret: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::new("internal", e.to_string()))
}

fn verify(secret: &str, stored: &str) -> bool {
    PasswordHash::new(stored).is_ok_and(|h| Argon2::default().verify_password(secret.as_bytes(), &h).is_ok())
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Deliberately loose: the email is only a username here, so we only reject obvious typos.
pub fn is_valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    !local.is_empty()
        && !email.contains(char::is_whitespace)
        && !domain.contains('@')
        && domain.split('.').count() >= 2
        && domain.split('.').all(|p| !p.is_empty())
}

/// 20 characters from an unambiguous alphabet, grouped for reading aloud / writing down.
fn new_recovery_key() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let bytes = random_bytes::<20>();
    let chars: Vec<char> = bytes.iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect();
    chars.chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

fn normalize_recovery_key(key: &str) -> String {
    key.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(AppError::new("password_too_short", ""));
    }
    Ok(())
}

pub fn create(db: &Db, email: &str, password: &str, keep_signed_in: bool) -> AppResult<Created> {
    let email = normalize_email(email);
    if !is_valid_email(&email) {
        return Err(AppError::new("invalid_email", ""));
    }
    validate_password(password)?;
    let exists: bool =
        db.with(|c| c.query_row("SELECT 1 FROM accounts WHERE email=?1", [&email], |_| Ok(true)).optional())?.is_some();
    if exists {
        return Err(AppError::new("email_taken", ""));
    }
    let recovery_key = new_recovery_key();
    let account = Account {
        id: uuid_v4(),
        display_name: email.split('@').next().unwrap_or(&email).to_string(),
        email,
    };
    let (pw_hash, rk_hash) = (hash(password)?, hash(&normalize_recovery_key(&recovery_key))?);
    let now = now_secs();
    db.with(|c| {
        c.execute(
            "INSERT INTO accounts(id,email,password_hash,recovery_hash,display_name,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?6)",
            params![account.id, account.email, pw_hash, rk_hash, account.display_name, now],
        )
    })?;
    remember(db, &account, keep_signed_in)?;
    Ok(Created { account, recovery_key })
}

pub fn sign_in(db: &Db, email: &str, password: &str, keep_signed_in: bool) -> AppResult<Account> {
    let email = normalize_email(email);
    let row: Option<(Account, String)> = db.with(|c| {
        c.query_row(
            "SELECT id,email,display_name,password_hash FROM accounts WHERE email=?1",
            [&email],
            |r| Ok((Account { id: r.get(0)?, email: r.get(1)?, display_name: r.get(2)? }, r.get(3)?)),
        )
        .optional()
    })?;
    let Some((account, stored)) = row else {
        return Err(AppError::new("no_account", ""));
    };
    if !verify(password, &stored) {
        return Err(AppError::new("wrong_password", ""));
    }
    remember(db, &account, keep_signed_in)?;
    Ok(account)
}

pub fn reset_password(db: &Db, email: &str, recovery_key: &str, new_password: &str) -> AppResult<Account> {
    let email = normalize_email(email);
    validate_password(new_password)?;
    let row: Option<(Account, String)> = db.with(|c| {
        c.query_row(
            "SELECT id,email,display_name,recovery_hash FROM accounts WHERE email=?1",
            [&email],
            |r| Ok((Account { id: r.get(0)?, email: r.get(1)?, display_name: r.get(2)? }, r.get(3)?)),
        )
        .optional()
    })?;
    let Some((account, stored)) = row else {
        return Err(AppError::new("no_account", ""));
    };
    if !verify(&normalize_recovery_key(recovery_key), &stored) {
        return Err(AppError::new("wrong_recovery_key", ""));
    }
    let pw_hash = hash(new_password)?;
    db.with(|c| {
        c.execute(
            "UPDATE accounts SET password_hash=?1, updated_at=?2 WHERE id=?3",
            params![pw_hash, now_secs(), account.id],
        )
    })?;
    remember(db, &account, true)?;
    Ok(account)
}

/// Only the account id is remembered: the DB already lives in the user's own profile folder,
/// so a separate session token would protect nothing extra.
fn remember(db: &Db, account: &Account, keep: bool) -> AppResult<()> {
    if keep {
        db.device_set(SIGNED_IN_KEY, &account.id)
    } else {
        db.device_delete(SIGNED_IN_KEY)
    }
}

pub fn remembered(db: &Db) -> AppResult<Option<Account>> {
    let Some(id) = db.device_get(SIGNED_IN_KEY)? else { return Ok(None) };
    db.with(|c| {
        c.query_row("SELECT id,email,display_name FROM accounts WHERE id=?1", [&id], |r| {
            Ok(Account { id: r.get(0)?, email: r.get(1)?, display_name: r.get(2)? })
        })
        .optional()
    })
}

pub fn sign_out(db: &Db) -> AppResult<()> {
    db.device_delete(SIGNED_IN_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation() {
        assert!(is_valid_email("a@b.co"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("a b@c.de"));
        assert!(!is_valid_email("@c.de"));
        assert!(!is_valid_email("a@@c.de"));
        assert!(!is_valid_email("a@c..de"));
    }

    #[test]
    fn account_lifecycle() {
        let db = Db::in_memory();
        let created = create(&db, " Me@Example.com ", "longenough", true).unwrap();
        assert_eq!(created.account.email, "me@example.com");
        assert_eq!(created.recovery_key.len(), 24);
        assert_eq!(remembered(&db).unwrap().unwrap().id, created.account.id);

        assert_eq!(create(&db, "me@example.com", "longenough", true).unwrap_err().code, "email_taken");
        assert_eq!(create(&db, "x@example.com", "short", true).unwrap_err().code, "password_too_short");
        assert_eq!(sign_in(&db, "me@example.com", "wrong-pass", true).unwrap_err().code, "wrong_password");
        assert_eq!(sign_in(&db, "who@example.com", "whatever1", true).unwrap_err().code, "no_account");

        sign_out(&db).unwrap();
        assert!(remembered(&db).unwrap().is_none());
        sign_in(&db, "ME@example.com", "longenough", false).unwrap();
        assert!(remembered(&db).unwrap().is_none());

        let key = created.recovery_key.to_lowercase().replace('-', " ");
        assert_eq!(reset_password(&db, "me@example.com", "AAAA", "newpassword").unwrap_err().code, "wrong_recovery_key");
        reset_password(&db, "me@example.com", &key, "newpassword").unwrap();
        sign_in(&db, "me@example.com", "newpassword", true).unwrap();
    }
}
