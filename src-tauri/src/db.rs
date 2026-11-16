use crate::error::AppResult;
use crate::sources::Track;
use crate::util::now_secs;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// Single SQLite connection behind a mutex. Queries are tiny and fast, so one connection
/// keeps things simple and avoids cross-connection locking surprises.
pub struct Db(Mutex<Connection>);

#[cfg(debug_assertions)]
mod reentry {
    use std::cell::Cell;

    thread_local!(static INSIDE: Cell<bool> = const { Cell::new(false) });

    pub struct Guard;

    pub fn enter() -> Guard {
        assert!(!INSIDE.replace(true), "Db::with called from inside Db::with (would deadlock)");
        Guard
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            INSIDE.set(false);
        }
    }
}

/// Each entry upgrades the schema by one version. Never edit a shipped entry; append a new one.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE accounts(
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    recovery_hash TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_path TEXT,
    banner_path TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE tracks(
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    source_id TEXT NOT NULL,
    data TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(source, source_id)
);
CREATE TABLE account_settings(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, key)
);
CREATE TABLE device_settings(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE queue_state(
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE search_history(
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    query TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(account_id, query)
);
CREATE TABLE stream_cache(
    track_id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    kind TEXT NOT NULL,
    mime TEXT NOT NULL,
    origin_id TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    size INTEGER,
    loudness_db REAL
);
"#, "ALTER TABLE stream_cache ADD COLUMN user_agent TEXT;", r#"
CREATE TABLE matches(
    source TEXT NOT NULL,
    source_id TEXT NOT NULL,
    track_id TEXT,
    manual INTEGER NOT NULL DEFAULT 0,
    score REAL NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(source, source_id)
);
CREATE TABLE folders(
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    parent_id TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);
CREATE TABLE playlists(
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    cover TEXT,
    folder_id TEXT,
    origin TEXT,
    origin_url TEXT,
    report TEXT,
    plays INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);
CREATE INDEX playlists_account ON playlists(account_id);
CREATE TABLE playlist_items(
    id TEXT PRIMARY KEY,
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX playlist_items_list ON playlist_items(playlist_id, position);
CREATE TABLE likes(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    liked_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, track_id)
);
CREATE TABLE hidden(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    value TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, kind, value)
);
CREATE TABLE follows(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    name TEXT NOT NULL,
    avatar TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, source, artist_id)
);
CREATE TABLE local_files(
    track_id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    trashed_at INTEGER,
    trash_path TEXT
);
"#, r#"
CREATE TABLE audio_files(
    track_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    file TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    mime TEXT NOT NULL,
    played TEXT NOT NULL,
    stand_in INTEGER NOT NULL DEFAULT 0,
    loudness_db REAL,
    last_used INTEGER NOT NULL
);
CREATE INDEX audio_files_lru ON audio_files(kind, last_used);
CREATE TABLE offline(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, track_id)
);
ALTER TABLE playlists ADD COLUMN offline INTEGER NOT NULL DEFAULT 0;
CREATE TABLE hd_covers(
    track_id TEXT PRIMARY KEY,
    url TEXT,
    checked_at INTEGER NOT NULL
);
"#, r#"
CREATE TABLE lyrics(
    track_id TEXT PRIMARY KEY,
    synced TEXT,
    plain TEXT,
    instrumental INTEGER NOT NULL DEFAULT 0,
    source TEXT,
    credit TEXT,
    user_edited INTEGER NOT NULL DEFAULT 0,
    checked_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE translations(
    track_id TEXT NOT NULL,
    lang TEXT NOT NULL,
    lines TEXT NOT NULL,
    source_lang TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(track_id, lang)
);
"#, r#"
CREATE TABLE trims(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, track_id)
);
CREATE TABLE waveforms(
    track_id TEXT PRIMARY KEY,
    peaks BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
"#, r#"
CREATE TABLE plays(
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    played_at INTEGER NOT NULL,
    ms_listened INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX plays_account_time ON plays(account_id, played_at);
CREATE INDEX plays_account_track ON plays(account_id, track_id);
CREATE TABLE speed_dial(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    item_id TEXT NOT NULL,
    title TEXT NOT NULL,
    subtitle TEXT NOT NULL,
    cover TEXT,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, kind, source, item_id)
);
CREATE TABLE mixes(
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    day TEXT NOT NULL,
    data TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(account_id)
);
CREATE TABLE backgrounds(
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    file TEXT NOT NULL,
    look TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#, r#"
ALTER TABLE accounts ADD COLUMN lock_hash TEXT;
ALTER TABLE accounts ADD COLUMN lock_hello INTEGER NOT NULL DEFAULT 0;
"#];

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            // Rollback journal, not WAL: one connection doesn't need WAL's concurrency, and a
            // hard-killed process can never leave a stale -wal file to be replayed later.
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=3000;",
        )?;
        migrate(&conn)?;
        Ok(Self(Mutex::new(conn)))
    }

    /// Opens the library, checking it first. A damaged file (a power cut mid-write, a bad disk)
    /// is renamed, not deleted, and a fresh library starts; `true` says that happened, so the
    /// app can offer the latest backup.
    pub fn open_checked(path: &Path) -> AppResult<(Self, bool)> {
        let check = Connection::open(path).and_then(|c| c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0)));
        // Only real damage counts: a file that's merely busy or unreadable for a moment (a backup
        // or virus scan holding it) must never be set aside; opening it reports that as usual.
        let damaged = match &check {
            Ok(answer) => answer != "ok",
            Err(e) => matches!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)),
        };
        if !damaged || !path.exists() {
            return Ok((Self::open(path)?, false));
        }
        let aside = path.with_file_name(format!("relay.damaged-{}.db", crate::util::now_secs()));
        std::fs::rename(path, &aside)?;
        eprintln!("library file was damaged; kept as {}", aside.display());
        Ok((Self::open(path)?, true))
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        migrate(&conn).unwrap();
        Self(Mutex::new(conn))
    }

    /// Runs `f` with the connection. A poisoned lock only means another thread panicked
    /// mid-query; SQLite itself stays consistent, so we keep going.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> AppResult<T> {
        // The lock isn't reentrant: calling back into the Db from inside `f` hangs the app for
        // good, so debug builds and tests fail loudly instead.
        #[cfg(debug_assertions)]
        let _guard = reentry::enter();
        let conn = self.0.lock().unwrap_or_else(|p| p.into_inner());
        Ok(f(&conn)?)
    }

    pub fn device_get(&self, key: &str) -> AppResult<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT value FROM device_settings WHERE key=?1", [key], |r| r.get(0)).optional()
        })
    }

    pub fn device_set(&self, key: &str, value: &str) -> AppResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO device_settings(key,value,updated_at) VALUES(?1,?2,?3)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![key, value, now_secs()],
            )
        })?;
        Ok(())
    }

    /// A UI preference stored for this PC ("pref.<key>", JSON), if set and of the right type.
    pub fn pref<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.device_get(&format!("pref.{key}")).ok().flatten().and_then(|v| serde_json::from_str(&v).ok())
    }

    pub fn device_delete(&self, key: &str) -> AppResult<()> {
        self.with(|c| c.execute("DELETE FROM device_settings WHERE key=?1", [key]))?;
        Ok(())
    }

    pub fn upsert_track(&self, t: &Track) -> AppResult<()> {
        self.with(|c| upsert_track_conn(c, t))
    }

    pub fn track(&self, id: &str) -> AppResult<Option<Track>> {
        let data: Option<String> =
            self.with(|c| c.query_row("SELECT data FROM tracks WHERE id=?1", [id], |r| r.get(0)).optional())?;
        Ok(data.and_then(|d| serde_json::from_str::<Track>(&d).ok()).map(Track::with_split_credits))
    }
}

/// Stores a track's metadata (shared by all accounts). The per-list "via" field is derived,
/// never stored.
pub fn upsert_track_conn(c: &Connection, t: &Track) -> rusqlite::Result<()> {
    let mut stored = t.clone();
    stored.via = None;
    let data = serde_json::to_string(&stored).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    c.execute(
        "INSERT INTO tracks(id,source,source_id,data,updated_at) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET
           data=CASE WHEN json_extract(excluded.data,'$.listens') IS NULL AND json_extract(tracks.data,'$.listens') IS NOT NULL
                THEN json_set(excluded.data,'$.listens',json_extract(tracks.data,'$.listens')) ELSE excluded.data END,
           updated_at=excluded.updated_at",
        params![t.id, t.source, t.source_id, data, now_secs()],
    )?;
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        conn.execute_batch(&format!("BEGIN; {sql}; PRAGMA user_version={}; COMMIT;", i + 1))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Db;

    #[test]
    fn a_damaged_file_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("relay-dbcheck-{}", crate::util::uuid_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("relay.db");
        std::fs::write(&path, b"this is not a database, just junk left by a crash ...........").unwrap();
        let (db, damaged) = Db::open_checked(&path).unwrap();
        assert!(damaged);
        assert!(db.with(|c| c.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get::<_, i64>(0))).is_ok());
        let kept = std::fs::read_dir(&dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("relay.damaged-"));
        assert!(kept, "the damaged file is kept");
        drop(db);
        let (_, again) = Db::open_checked(&path).unwrap();
        assert!(!again, "a healthy file passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[should_panic(expected = "would deadlock")]
    fn nested_use_fails_loudly_instead_of_hanging() {
        let db = Db::in_memory();
        let _ = db.with(|_| Ok(db.pref::<bool>("x")));
    }

    #[test]
    fn lock_is_usable_again_after_a_call() {
        let db = Db::in_memory();
        db.device_set("a", "1").unwrap();
        assert_eq!(db.device_get("a").unwrap().as_deref(), Some("1"));
    }
}
