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
"#];

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=3000;",
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
        let data = serde_json::to_string(t)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO tracks(id,source,source_id,data,updated_at) VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(id) DO UPDATE SET data=excluded.data, updated_at=excluded.updated_at",
                params![t.id, t.source, t.source_id, data, now_secs()],
            )
        })?;
        Ok(())
    }

    pub fn track(&self, id: &str) -> AppResult<Option<Track>> {
        let data: Option<String> =
            self.with(|c| c.query_row("SELECT data FROM tracks WHERE id=?1", [id], |r| r.get(0)).optional())?;
        Ok(data.and_then(|d| serde_json::from_str(&d).ok()))
    }
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        conn.execute_batch(&format!("BEGIN; {sql}; PRAGMA user_version={}; COMMIT;", i + 1))?;
    }
    Ok(())
}
