use crate::error::AppResult;
use crate::sources::Track;
use crate::util::now_secs;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// Single SQLite connection behind a mutex. Queries are tiny and fast, so one connection
/// keeps things simple and avoids cross-connection locking surprises.
pub struct Db(Mutex<Connection>);

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
        Ok(data.and_then(|d| serde_json::from_str(&d).ok()))
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
         ON CONFLICT(id) DO UPDATE SET data=excluded.data, updated_at=excluded.updated_at",
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
