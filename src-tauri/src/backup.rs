//! "Back up library" and "Restore": the accounts and everything in their libraries, written to
//! one JSON file and read back on this or another PC. Music files and the caches are not part
//! of it. Rows are stored column by column, so backups from older or newer versions still load
//! (unknown columns are skipped, missing ones get their defaults).

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::util::now_secs;
use crate::AppState;
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const FORMAT: u64 = 1;
const APP: &str = "relay-backup";
pub const EXTENSION: &str = "relaybackup";
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const WEEK_SECS: i64 = 7 * 24 * 3600;
const KEEP_AUTO: usize = 4;
const LAST_AUTO_KEY: &str = "backup_last_auto";
const AUTO_PREFIX: &str = "Relay backup ";

/// Parents before children, so foreign keys hold while restoring.
const TABLES: &[&str] = &[
    "accounts",
    "account_settings",
    "folders",
    "playlists",
    "playlist_items",
    "likes",
    "hidden",
    "follows",
    "search_history",
    "offline",
    "matches",
    "tracks",
    "lyrics",
    "trims",
];

fn to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
    }
}

fn to_sql(v: &Value) -> AppResult<SqlValue> {
    Ok(match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => SqlValue::Integer(i),
            None => SqlValue::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => SqlValue::Text(s.clone()),
        _ => return Err(AppError::new("bad_file", "unexpected value in backup")),
    })
}

fn dump(c: &Connection, sql: &str) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = c.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let rows = stmt.query_map([], |r| {
        let mut obj = Map::new();
        for (i, name) in names.iter().enumerate() {
            obj.insert(name.clone(), to_json(r.get_ref(i)?));
        }
        Ok(Value::Object(obj))
    })?;
    rows.collect()
}

/// The whole backup as JSON.
pub fn export(db: &Db) -> AppResult<Value> {
    db.with(|c| {
        let mut tables = Map::new();
        for t in TABLES {
            // Looked-up lyrics are cache; only the user's own lyrics belong in a backup.
            let only = if *t == "lyrics" { " WHERE user_edited=1" } else { "" };
            tables.insert(t.to_string(), Value::Array(dump(c, &format!("SELECT * FROM {t}{only}"))?));
        }
        // Only preferences; internal values (sign-in, client ids) belong to this PC.
        let prefs = dump(c, "SELECT key,value FROM device_settings WHERE key LIKE 'pref.%'")?;
        Ok(json!({ "app": APP, "format": FORMAT, "createdAt": now_secs(), "tables": tables, "preferences": prefs }))
    })
}

fn columns(c: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt.query_map([], |r| r.get::<_, String>(1))?.collect();
    cols
}

/// Rows must be objects of plain values; checked before anything is changed.
fn validate(tables: &Map<String, Value>) -> AppResult<()> {
    for t in TABLES {
        for row in tables.get(*t).and_then(Value::as_array).into_iter().flatten() {
            let obj = row.as_object().ok_or_else(|| AppError::new("bad_file", "row is not an object"))?;
            for v in obj.values() {
                to_sql(v)?;
            }
        }
    }
    Ok(())
}

fn insert_rows(c: &Connection, table: &str, rows: &[Value]) -> rusqlite::Result<()> {
    let known = columns(c, table)?;
    for obj in rows.iter().filter_map(Value::as_object) {
        let cols: Vec<&String> = known.iter().filter(|k| obj.contains_key(k.as_str())).collect();
        if cols.is_empty() {
            continue;
        }
        let values: Vec<SqlValue> = cols.iter().map(|k| to_sql(&obj[k.as_str()]).unwrap_or(SqlValue::Null)).collect();
        let names: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
        let marks = vec!["?"; cols.len()].join(",");
        let sql = format!("INSERT OR REPLACE INTO {table}({}) VALUES({marks})", names.join(","));
        c.execute(&sql, params_from_iter(values))?;
    }
    Ok(())
}

/// Loads a backup. Accounts in it replace the same accounts on this PC (matched by id or
/// email), with their whole library; other accounts on this PC are left alone. Returns the
/// number of accounts restored.
pub fn restore(db: &Db, backup: &Value) -> AppResult<usize> {
    if backup.get("app").and_then(Value::as_str) != Some(APP) {
        return Err(AppError::new("bad_file", "not a Relay backup"));
    }
    if backup.get("format").and_then(Value::as_u64).is_none_or(|f| f > FORMAT) {
        return Err(AppError::new("bad_file", "backup from a newer Relay"));
    }
    let tables = backup.get("tables").and_then(Value::as_object).ok_or_else(|| AppError::new("bad_file", "no tables"))?;
    let rows_of = |t: &str| tables.get(t).and_then(Value::as_array).cloned().unwrap_or_default();
    let accounts = rows_of("accounts");
    if accounts.is_empty() {
        return Err(AppError::new("bad_file", "backup has no accounts"));
    }
    validate(tables)?;
    let signed_in = db.device_get("signed_in_account")?;
    // One transaction under one lock: a failure anywhere leaves this PC's data as it was.
    let new_signed_in = db.with(|c| {
        let tx = c.unchecked_transaction()?;
        let mut new_signed_in = None;
        for a in &accounts {
                let id = a.get("id").and_then(Value::as_str).unwrap_or("");
                let email = a.get("email").and_then(Value::as_str).unwrap_or("");
                // The person signed in here may come back under the backup's account id.
            let was: Vec<String> = {
                let mut s = tx.prepare("SELECT id FROM accounts WHERE id=?1 OR email=?2 COLLATE NOCASE")?;
                let ids = s.query_map([id, email], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                ids
            };
            if signed_in.as_ref().is_some_and(|s| was.contains(s)) {
                new_signed_in = Some(id.to_string());
            }
            // Removing the account removes its whole library (foreign keys cascade).
            tx.execute("DELETE FROM accounts WHERE id=?1 OR email=?2 COLLATE NOCASE", [id, email])?;
        }
        for t in TABLES {
            insert_rows(&tx, t, &rows_of(t))?;
        }
        for p in backup.get("preferences").and_then(Value::as_array).into_iter().flatten() {
            let (Some(key), Some(value)) = (p.get("key").and_then(Value::as_str), p.get("value").and_then(Value::as_str)) else { continue };
            if key.starts_with("pref.") && key.len() <= 70 {
                tx.execute(
                    "INSERT INTO device_settings(key,value,updated_at) VALUES(?1,?2,?3)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                    rusqlite::params![key, value, now_secs()],
                )?;
            }
        }
        tx.commit()?;
        Ok(new_signed_in)
    })?;
    if let Some(id) = new_signed_in {
        db.device_set("signed_in_account", &id)?;
    }
    Ok(accounts.len())
}

pub async fn write_file(db: &Db, path: &Path) -> AppResult<()> {
    let text = serde_json::to_vec(&export(db)?)?;
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, text).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

pub async fn read_file(db: &Db, path: &Path) -> AppResult<usize> {
    let meta = tokio::fs::metadata(path).await?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::new("bad_file", "file is too large"));
    }
    let bytes = tokio::fs::read(path).await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| AppError::new("bad_file", "not a Relay backup"))?;
    restore(db, &value)
}

/// "YYYY-MM-DD" for a Unix time (UTC), for backup file names.
pub fn date_string(secs: i64) -> String {
    // Days since 1970 to a civil date (Howard Hinnant's algorithm).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

// ---- weekly backup ------------------------------------------------------------------------

/// Bumped whenever the schedule changes, so an older sleeping timer knows to stop.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn auto_folder(state: &AppState) -> Option<PathBuf> {
    if !state.db.pref::<bool>("autoBackup").unwrap_or(false) {
        return None;
    }
    state.db.pref::<String>("backupFolder").filter(|f| !f.is_empty()).map(PathBuf::from)
}

/// Writes the weekly backup if one is due, then sleeps until the next one. One timer, no
/// polling; changing the settings restarts it.
pub fn schedule(state: &Arc<AppState>) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let Some(folder) = auto_folder(&state) else { return };
            let last: i64 = state.db.device_get(LAST_AUTO_KEY).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0);
            let due = last + WEEK_SECS;
            let now = now_secs();
            if now >= due {
                match auto_backup(&state, &folder).await {
                    Ok(()) => {
                        let _ = state.db.device_set(LAST_AUTO_KEY, &now.to_string());
                    }
                    Err(e) => eprintln!("weekly backup failed: {e}"),
                }
            }
            let wait = (due - now).clamp(3600, WEEK_SECS) as u64;
            tokio::time::sleep(Duration::from_secs(wait)).await;
            if GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
        }
    });
}

async fn auto_backup(state: &AppState, folder: &Path) -> AppResult<()> {
    tokio::fs::create_dir_all(folder).await?;
    let name = format!("{AUTO_PREFIX}{}.{EXTENSION}", date_string(now_secs()));
    write_file(&state.db, &folder.join(name)).await?;
    // Keep the newest few automatic backups; the dates in the names sort by time.
    let mut old: Vec<PathBuf> = std::fs::read_dir(folder)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with(AUTO_PREFIX) && n.ends_with(EXTENSION)
        })
        .collect();
    old.sort();
    if old.len() > KEEP_AUTO {
        for p in &old[..old.len() - KEEP_AUTO] {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::Track;

    fn library(db: &Db, email: &str) -> String {
        let acc = crate::accounts::create(db, email, "password1", false).unwrap().account.id;
        let pl = crate::library::create_playlist(db, &acc, "Road trip", None).unwrap();
        let t = Track::imported("deezer", "42", "Song", vec!["Artist".into()], Some(200_000), None, None);
        crate::library::add_tracks(db, &acc, &pl, std::slice::from_ref(&t)).unwrap();
        crate::library::set_like(db, &acc, &t, true).unwrap();
        acc
    }

    #[test]
    fn round_trips_to_another_pc() {
        let home = Db::in_memory();
        let acc = library(&home, "me@test.dev");
        let backup = export(&home).unwrap();

        let other = Db::in_memory();
        let friend = library(&other, "friend@test.dev");
        assert_eq!(restore(&other, &backup).unwrap(), 1);
        let ov = crate::library::overview(&other, &acc).unwrap();
        assert_eq!(ov.playlists.len(), 1);
        assert_eq!(ov.playlists[0].name, "Road trip");
        assert_eq!(ov.liked_count, 1);
        // Signing in still works: the password hash came along.
        assert!(crate::accounts::sign_in(&other, "me@test.dev", "password1", false).is_ok());
        // The friend's account on that PC is untouched.
        assert_eq!(crate::library::overview(&other, &friend).unwrap().playlists.len(), 1);
    }

    #[test]
    fn restore_replaces_the_same_account() {
        let db = Db::in_memory();
        let acc = library(&db, "me@test.dev");
        let backup = export(&db).unwrap();
        crate::library::create_playlist(&db, &acc, "Made after the backup", None).unwrap();
        restore(&db, &backup).unwrap();
        let names: Vec<String> = crate::library::overview(&db, &acc).unwrap().playlists.into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Road trip"]);
    }

    #[test]
    fn rejects_other_files() {
        let db = Db::in_memory();
        assert_eq!(restore(&db, &json!({"hello": 1})).unwrap_err().code, "bad_file");
        assert_eq!(restore(&db, &json!({"app": APP, "format": 99, "tables": {}})).unwrap_err().code, "bad_file");
        let bad = json!({"app": APP, "format": 1, "tables": {"accounts": [{"id": "x", "email": "a@b.c", "password_hash": {"no": 1}}]}});
        assert_eq!(restore(&db, &bad).unwrap_err().code, "bad_file");
        // A row the database refuses rolls everything back, including the removed account.
        let acc = library(&db, "me@test.dev");
        let broken = json!({"app": APP, "format": 1, "tables": {
            "accounts": [{"id": acc, "email": "me@test.dev"}]
        }});
        assert!(restore(&db, &broken).is_err());
        assert_eq!(crate::library::overview(&db, &acc).unwrap().playlists.len(), 1);
    }

    #[test]
    fn formats_dates() {
        assert_eq!(date_string(0), "1970-01-01");
        assert_eq!(date_string(1_762_646_400), "2025-11-09");
        assert_eq!(date_string(951_782_400), "2000-02-29");
    }
}
