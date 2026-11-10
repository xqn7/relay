//! The account's own look and pins: profile picture and banner, saved background images (all
//! copied into the app's data folder, so they keep working when the originals move) and the
//! speed dial on Home.

use crate::accounts::{self, Account};
use crate::covers::{respond, sniff};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::clean_name;
use crate::util::{now_secs, uuid_v4};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::http::{Response, StatusCode};

const URL_PREFIX: &str = "http://relay.localhost/user/";
/// Cropped in the app, so these are already small.
const MAX_PROFILE_BYTES: usize = 6 * 1024 * 1024;
/// Animated gifs can be big.
const MAX_BACKGROUND_BYTES: usize = 40 * 1024 * 1024;
const MAX_BACKGROUNDS: usize = 12;
const MAX_LOOK_BYTES: usize = 1024;
const MAX_PINS: usize = 24;
pub const PIN_KINDS: [&str; 3] = ["playlist", "album", "artist"];

pub fn file_url(name: &str) -> String {
    format!("{URL_PREFIX}{name}")
}

/// Only names this module made: a UUID and an image extension. Anything else could reach
/// outside the folder.
fn valid_name(name: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else { return false };
    stem.len() == 36 && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-') && matches!(ext, "jpg" | "png" | "webp" | "gif")
}

fn ext_of(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}

pub fn serve(dir: &Path, path: &str) -> Response<Vec<u8>> {
    let name = path.trim_start_matches("/user/");
    let bytes = valid_name(name).then(|| std::fs::read(dir.join(name)).ok()).flatten();
    match bytes.as_deref().and_then(|b| sniff(b).map(|m| (m, b))) {
        Some((mime, b)) => respond(StatusCode::OK, mime, b.to_vec()),
        None => respond(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec()),
    }
}

/// Writes an image under a new name and returns that name. Only real images are accepted.
fn store(dir: &Path, bytes: &[u8], max: usize, gif_ok: bool) -> AppResult<String> {
    if bytes.len() > max {
        return Err(AppError::new("too_large", "image is too large"));
    }
    let mime = sniff(bytes).filter(|m| gif_ok || *m != "image/gif").ok_or_else(|| AppError::new("bad_file", "not a supported image"))?;
    std::fs::create_dir_all(dir)?;
    let name = format!("{}.{}", uuid_v4(), ext_of(mime));
    std::fs::write(dir.join(&name), bytes)?;
    Ok(name)
}

fn remove(dir: &Path, name: Option<String>) {
    if let Some(n) = name.filter(|n| valid_name(n)) {
        let _ = std::fs::remove_file(dir.join(n));
    }
}

// ---- profile ------------------------------------------------------------------------------

fn image_column(kind: &str) -> AppResult<&'static str> {
    match kind {
        "avatar" => Ok("avatar_path"),
        "banner" => Ok("banner_path"),
        _ => Err(AppError::new("invalid_input", "bad image kind")),
    }
}

/// Sets (or with None, removes) the profile picture or banner.
pub fn set_image(dir: &Path, db: &Db, account: &str, kind: &str, bytes: Option<&[u8]>) -> AppResult<Account> {
    let col = image_column(kind)?;
    let name = bytes.map(|b| store(dir, b, MAX_PROFILE_BYTES, false)).transpose()?;
    let old: Option<String> = db.with(|c| {
        let old = c.query_row(&format!("SELECT {col} FROM accounts WHERE id=?1"), [account], |r| r.get(0)).optional()?.flatten();
        c.execute(&format!("UPDATE accounts SET {col}=?1, updated_at=?2 WHERE id=?3"), params![name, now_secs(), account])?;
        Ok(old)
    })?;
    remove(dir, old);
    accounts::by_id(db, account)?.ok_or_else(|| AppError::new("signed_out", ""))
}

pub fn set_name(db: &Db, account: &str, name: &str) -> AppResult<Account> {
    let name = clean_name(name)?;
    db.with(|c| c.execute("UPDATE accounts SET display_name=?1, updated_at=?2 WHERE id=?3", params![name, now_secs(), account]))?;
    accounts::by_id(db, account)?.ok_or_else(|| AppError::new("signed_out", ""))
}

// ---- background images --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Background {
    pub id: String,
    pub url: String,
    pub animated: bool,
    /// Blur, dim, brightness, saturation, zoom, position, mix: checked by the UI on reading.
    pub look: Value,
}

fn background_row(r: &rusqlite::Row) -> rusqlite::Result<Background> {
    let file: String = r.get(1)?;
    let look: String = r.get(2)?;
    Ok(Background { id: r.get(0)?, animated: file.ends_with(".gif"), url: file_url(&file), look: serde_json::from_str(&look).unwrap_or(Value::Null) })
}

pub fn backgrounds(db: &Db, account: &str) -> AppResult<Vec<Background>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT id,file,look FROM backgrounds WHERE account_id=?1 ORDER BY created_at")?;
        let rows = s.query_map([account], background_row)?.collect();
        rows
    })
}

pub fn add_background(dir: &Path, db: &Db, account: &str, bytes: &[u8]) -> AppResult<Background> {
    let count: i64 = db.with(|c| c.query_row("SELECT COUNT(*) FROM backgrounds WHERE account_id=?1", [account], |r| r.get(0)))?;
    if count as usize >= MAX_BACKGROUNDS {
        return Err(AppError::new("too_many", "too many backgrounds"));
    }
    let file = store(dir, bytes, MAX_BACKGROUND_BYTES, true)?;
    let id = uuid_v4();
    let now = now_secs();
    db.with(|c| {
        c.execute(
            "INSERT INTO backgrounds(id,account_id,file,look,created_at,updated_at) VALUES(?1,?2,?3,'{}',?4,?4)",
            params![id, account, file, now],
        )
    })?;
    Ok(Background { id, animated: file.ends_with(".gif"), url: file_url(&file), look: Value::Object(Default::default()) })
}

pub fn set_look(db: &Db, account: &str, id: &str, look: &Value) -> AppResult<()> {
    let json = serde_json::to_string(look)?;
    if !look.is_object() || json.len() > MAX_LOOK_BYTES {
        return Err(AppError::new("invalid_input", "bad look"));
    }
    let n = db.with(|c| c.execute("UPDATE backgrounds SET look=?1, updated_at=?2 WHERE id=?3 AND account_id=?4", params![json, now_secs(), id, account]))?;
    if n == 0 {
        return Err(AppError::new("not_found", "background not found"));
    }
    Ok(())
}

pub fn delete_background(dir: &Path, db: &Db, account: &str, id: &str) -> AppResult<()> {
    let file: Option<String> = db.with(|c| {
        let file = c.query_row("SELECT file FROM backgrounds WHERE id=?1 AND account_id=?2", params![id, account], |r| r.get(0)).optional()?;
        c.execute("DELETE FROM backgrounds WHERE id=?1 AND account_id=?2", params![id, account])?;
        Ok(file)
    })?;
    remove(dir, file);
    Ok(())
}

// ---- speed dial ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    /// "playlist", "album" or "artist"
    pub kind: String,
    /// The catalog source for albums and artists; empty for playlists.
    pub source: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub cover: Option<String>,
}

impl Pin {
    fn is_sane(&self) -> bool {
        let id_ok = !self.id.is_empty() && self.id.len() <= 128 && self.id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
        let source_ok = if self.kind == "playlist" { self.source.is_empty() } else { matches!(self.source.as_str(), "youtube" | "soundcloud") };
        PIN_KINDS.contains(&self.kind.as_str())
            && id_ok
            && source_ok
            && !self.title.trim().is_empty()
            && self.title.len() < 512
            && self.subtitle.len() < 512
            && self.cover.as_ref().is_none_or(|c| c.starts_with("https://") && c.len() < 2048)
    }
}

pub fn pins(db: &Db, account: &str) -> AppResult<Vec<Pin>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT kind,source,item_id,title,subtitle,cover FROM speed_dial WHERE account_id=?1 ORDER BY position, created_at")?;
        let rows = s
            .query_map([account], |r| Ok(Pin { kind: r.get(0)?, source: r.get(1)?, id: r.get(2)?, title: r.get(3)?, subtitle: r.get(4)?, cover: r.get(5)? }))?
            .collect();
        rows
    })
}

pub fn set_pin(db: &Db, account: &str, pin: &Pin, on: bool) -> AppResult<()> {
    if !pin.is_sane() {
        return Err(AppError::new("invalid_input", "bad pin"));
    }
    db.with(|c| {
        if !on {
            c.execute("DELETE FROM speed_dial WHERE account_id=?1 AND kind=?2 AND source=?3 AND item_id=?4", params![account, pin.kind, pin.source, pin.id])?;
            return Ok(Ok(()));
        }
        let (count, next): (i64, i64) =
            c.query_row("SELECT COUNT(*), COALESCE(MAX(position)+1,0) FROM speed_dial WHERE account_id=?1", [account], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if count as usize >= MAX_PINS {
            return Ok(Err(AppError::new("too_many", "speed dial is full")));
        }
        let now = now_secs();
        c.execute(
            "INSERT INTO speed_dial(account_id,kind,source,item_id,title,subtitle,cover,position,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)
             ON CONFLICT(account_id,kind,source,item_id) DO UPDATE SET title=excluded.title, subtitle=excluded.subtitle, cover=excluded.cover, updated_at=excluded.updated_at",
            params![account, pin.kind, pin.source, pin.id, pin.title.trim(), pin.subtitle, pin.cover, next, now],
        )?;
        Ok(Ok(()))
    })?
}

/// Saves a new order; `keys` are "kind:source:id" in the wanted order.
pub fn order_pins(db: &Db, account: &str, keys: &[String]) -> AppResult<()> {
    if keys.len() > MAX_PINS {
        return Err(AppError::new("invalid_input", "too many pins"));
    }
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        for (i, key) in keys.iter().enumerate() {
            let mut parts = key.splitn(3, ':');
            let (Some(kind), Some(source), Some(id)) = (parts.next(), parts.next(), parts.next()) else { continue };
            tx.execute(
                "UPDATE speed_dial SET position=?1, updated_at=?2 WHERE account_id=?3 AND kind=?4 AND source=?5 AND item_id=?6",
                params![i as i64, now_secs(), account, kind, source, id],
            )?;
        }
        tx.commit()
    })?;
    Ok(())
}

/// Where profile pictures and backgrounds are kept.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("user")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(kind: &str, source: &str, id: &str) -> Pin {
        Pin { kind: kind.into(), source: source.into(), id: id.into(), title: "T".into(), subtitle: String::new(), cover: None }
    }

    #[test]
    fn only_own_file_names_are_served() {
        assert!(valid_name("0b5c1a2e-1111-4a2b-8c3d-123456789abc.jpg"));
        assert!(!valid_name("../relay.db"));
        assert!(!valid_name("0b5c1a2e-1111-4a2b-8c3d-123456789abc.exe"));
        assert!(!valid_name("0b5c1a2e-1111-4a2b-8c3d-12345678\\abc.jpg"));
    }

    #[test]
    fn stores_only_images() {
        let dir = std::env::temp_dir().join(format!("relay-test-{}", uuid_v4()));
        assert_eq!(store(&dir, b"not an image", 1000, true).unwrap_err().code, "bad_file");
        let gif = b"GIF89a....";
        assert_eq!(store(&dir, gif, 1000, false).unwrap_err().code, "bad_file");
        let name = store(&dir, gif, 1000, true).unwrap();
        assert!(name.ends_with(".gif") && valid_name(&name));
        assert_eq!(store(&dir, gif, 4, true).unwrap_err().code, "too_large");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pins_keep_their_order() {
        let db = Db::in_memory();
        let me = accounts::create(&db, "me@example.com", "longenough", true).unwrap().account.id;
        set_pin(&db, &me, &pin("playlist", "", "p1"), true).unwrap();
        set_pin(&db, &me, &pin("album", "youtube", "MPREb_x"), true).unwrap();
        set_pin(&db, &me, &pin("artist", "soundcloud", "42"), true).unwrap();
        // Pinning again doesn't duplicate.
        set_pin(&db, &me, &pin("playlist", "", "p1"), true).unwrap();
        assert!(set_pin(&db, &me, &pin("playlist", "youtube", "p1"), true).is_err());
        assert!(set_pin(&db, &me, &pin("song", "youtube", "x"), true).is_err());
        order_pins(&db, &me, &["artist:soundcloud:42".into(), "playlist::p1".into(), "album:youtube:MPREb_x".into()]).unwrap();
        let ids: Vec<String> = pins(&db, &me).unwrap().into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["42", "p1", "MPREb_x"]);
        set_pin(&db, &me, &pin("artist", "soundcloud", "42"), false).unwrap();
        assert_eq!(pins(&db, &me).unwrap().len(), 2);
    }

    #[test]
    fn profile_pictures_replace_the_old_file() {
        let db = Db::in_memory();
        let dir = std::env::temp_dir().join(format!("relay-test-{}", uuid_v4()));
        let me = accounts::create(&db, "me@example.com", "longenough", true).unwrap().account.id;
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        let a = set_image(&dir, &db, &me, "avatar", Some(&jpeg)).unwrap();
        let first = a.avatar.clone().unwrap();
        let b = set_image(&dir, &db, &me, "avatar", Some(&jpeg)).unwrap();
        assert_ne!(b.avatar.as_deref(), Some(first.as_str()));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let c = set_image(&dir, &db, &me, "avatar", None).unwrap();
        assert!(c.avatar.is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        assert!(set_image(&dir, &db, &me, "other", None).is_err());
        assert_eq!(set_name(&db, &me, "  Kai ").unwrap().display_name, "Kai");
        let _ = std::fs::remove_dir_all(dir);
    }
}
