//! The account's library in SQLite: playlists, folders, likes, hidden songs/artists, followed
//! artists, the trash and the import match cache. Every function scopes by account so one
//! person's library never leaks into another's.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sources::{is_import_only, Skipped, Track};
use crate::util::{now_secs, uuid_v4};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const TRASH_DAYS: i64 = 30;
const MAX_NAME: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub cover: Option<String>,
    pub folder_id: Option<String>,
    pub count: i64,
    pub duration_ms: i64,
    pub origin: Option<String>,
    pub origin_url: Option<String>,
    pub plays: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub playlists: Vec<PlaylistSummary>,
    pub folders: Vec<Folder>,
    pub liked_count: i64,
    pub follows_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub item_id: String,
    pub added_at: i64,
    pub track: Track,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetail {
    pub playlist: PlaylistSummary,
    pub items: Vec<Item>,
    /// Songs from the last import that aren't in the playlist, with the reason.
    pub skipped: Vec<Skipped>,
}

/// A playlist row removed from a playlist, kept by the UI so "Undo" can put it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    pub item_id: String,
    pub track_id: String,
    pub position: i64,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hidden {
    pub kind: String,
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Followed {
    pub source: String,
    pub artist_id: String,
    pub name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    /// "playlist", "folder" or "local"
    pub kind: String,
    pub id: String,
    pub name: String,
    pub detail: String,
    pub deleted_at: i64,
}

pub fn clean_name(name: &str) -> AppResult<String> {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    if name.is_empty() {
        return Err(AppError::new("invalid_input", "empty name"));
    }
    Ok(name)
}

fn track_from(data: &str) -> Option<Track> {
    serde_json::from_str(data).ok()
}

/// For imported songs: which source plays them ("none" if nothing matched), and the
/// matched cover when the import service gave none.
pub fn decorate(c: &Connection, tracks: &mut [Track]) -> rusqlite::Result<()> {
    let mut stmt = c.prepare_cached(
        "SELECT m.track_id, t.data FROM matches m LEFT JOIN tracks t ON t.id = m.track_id WHERE m.source=?1 AND m.source_id=?2",
    )?;
    for t in tracks.iter_mut().filter(|t| is_import_only(&t.source)) {
        let row: Option<(Option<String>, Option<String>)> = stmt.query_row(params![t.source, t.source_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        match row {
            Some((Some(_), Some(data))) => {
                if let Some(m) = track_from(&data) {
                    t.via = Some(m.source);
                    if t.cover.is_none() {
                        t.cover = m.cover;
                    }
                }
            }
            Some((None, _)) => t.via = Some("none".into()),
            _ => {}
        }
    }
    Ok(())
}

// ---- playlists ------------------------------------------------------------------------

const SUMMARY_SELECT: &str = "SELECT p.id, p.name,
        COALESCE(p.cover, (SELECT json_extract(t.data,'$.cover') FROM playlist_items i JOIN tracks t ON t.id=i.track_id
                           WHERE i.playlist_id=p.id AND json_extract(t.data,'$.cover') IS NOT NULL ORDER BY i.position LIMIT 1),
                 (SELECT json_extract(mt.data,'$.cover') FROM playlist_items i JOIN tracks t ON t.id=i.track_id
                           JOIN matches m ON m.source=t.source AND m.source_id=t.source_id JOIN tracks mt ON mt.id=m.track_id
                           WHERE i.playlist_id=p.id ORDER BY i.position LIMIT 1)),
        p.folder_id,
        (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id=p.id),
        (SELECT COALESCE(SUM(json_extract(t.data,'$.durationMs')),0) FROM playlist_items i JOIN tracks t ON t.id=i.track_id WHERE i.playlist_id=p.id),
        p.origin, p.origin_url, p.plays
    FROM playlists p";

fn summary_row(r: &rusqlite::Row) -> rusqlite::Result<PlaylistSummary> {
    Ok(PlaylistSummary {
        id: r.get(0)?,
        name: r.get(1)?,
        cover: r.get(2)?,
        folder_id: r.get(3)?,
        count: r.get(4)?,
        duration_ms: r.get(5)?,
        origin: r.get(6)?,
        origin_url: r.get(7)?,
        plays: r.get(8)?,
    })
}

pub fn overview(db: &Db, account: &str) -> AppResult<Overview> {
    db.with(|c| {
        let mut s = c.prepare(&format!(
            "{SUMMARY_SELECT} LEFT JOIN folders f ON f.id=p.folder_id LEFT JOIN folders pf ON pf.id=f.parent_id
             WHERE p.account_id=?1 AND p.deleted_at IS NULL AND f.deleted_at IS NULL AND pf.deleted_at IS NULL
             ORDER BY p.position, p.created_at"
        ))?;
        let playlists = s.query_map([account], summary_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut s = c.prepare(
            "SELECT f.id, f.name, f.parent_id FROM folders f LEFT JOIN folders parent ON parent.id=f.parent_id
             WHERE f.account_id=?1 AND f.deleted_at IS NULL AND parent.deleted_at IS NULL ORDER BY f.position, f.created_at",
        )?;
        let folders = s.query_map([account], |r| Ok(Folder { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)? }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let liked_count = c.query_row("SELECT COUNT(*) FROM likes WHERE account_id=?1", [account], |r| r.get(0))?;
        let follows_count = c.query_row("SELECT COUNT(*) FROM follows WHERE account_id=?1", [account], |r| r.get(0))?;
        Ok(Overview { playlists, folders, liked_count, follows_count })
    })
}

fn owns_playlist(c: &Connection, account: &str, id: &str) -> rusqlite::Result<bool> {
    c.query_row("SELECT 1 FROM playlists WHERE id=?1 AND account_id=?2 AND deleted_at IS NULL", params![id, account], |_| Ok(())).optional().map(|o| o.is_some())
}

fn require_playlist(c: &Connection, account: &str, id: &str) -> AppResult<()> {
    if owns_playlist(c, account, id)? {
        Ok(())
    } else {
        Err(AppError::new("not_found", "playlist not found"))
    }
}

fn require_folder(c: &Connection, account: &str, id: &str) -> AppResult<Option<String>> {
    c.query_row("SELECT parent_id FROM folders WHERE id=?1 AND account_id=?2 AND deleted_at IS NULL", params![id, account], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::new("not_found", "folder not found"))
}

/// Runs `f` inside one transaction, so a failure halfway never leaves a half-changed list.
fn tx<T>(db: &Db, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
    let mut result = None;
    db.with(|c| {
        c.execute_batch("BEGIN IMMEDIATE")?;
        result = Some(f(c));
        if matches!(result, Some(Ok(_))) {
            c.execute_batch("COMMIT")
        } else {
            c.execute_batch("ROLLBACK")
        }
    })?;
    result.expect("closure ran")
}

pub fn create_playlist(db: &Db, account: &str, name: &str, folder: Option<&str>) -> AppResult<String> {
    let name = clean_name(name)?;
    let id = uuid_v4();
    tx(db, |c| {
        if let Some(f) = folder {
            require_folder(c, account, f)?;
        }
        let now = now_secs();
        let pos: i64 = c.query_row("SELECT COALESCE(MAX(position),0)+1 FROM playlists WHERE account_id=?1", [account], |r| r.get(0))?;
        c.execute(
            "INSERT INTO playlists(id,account_id,name,folder_id,position,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",
            params![id, account, name, folder, pos, now],
        )?;
        Ok(())
    })?;
    Ok(id)
}

pub fn rename_playlist(db: &Db, account: &str, id: &str, name: &str) -> AppResult<()> {
    let name = clean_name(name)?;
    let n = db.with(|c| c.execute("UPDATE playlists SET name=?3, updated_at=?4 WHERE id=?1 AND account_id=?2 AND deleted_at IS NULL", params![id, account, name, now_secs()]))?;
    if n == 0 {
        return Err(AppError::new("not_found", "playlist not found"));
    }
    Ok(())
}

pub fn set_playlist_folder(db: &Db, account: &str, id: &str, folder: Option<&str>) -> AppResult<()> {
    tx(db, |c| {
        require_playlist(c, account, id)?;
        if let Some(f) = folder {
            require_folder(c, account, f)?;
        }
        c.execute("UPDATE playlists SET folder_id=?2, updated_at=?3 WHERE id=?1", params![id, folder, now_secs()])?;
        Ok(())
    })
}

/// Moves a playlist to the trash; it stays restorable for TRASH_DAYS.
pub fn trash_playlist(db: &Db, account: &str, id: &str) -> AppResult<()> {
    let n = db.with(|c| c.execute("UPDATE playlists SET deleted_at=?3, updated_at=?3 WHERE id=?1 AND account_id=?2 AND deleted_at IS NULL", params![id, account, now_secs()]))?;
    if n == 0 {
        return Err(AppError::new("not_found", "playlist not found"));
    }
    Ok(())
}

pub fn find_by_origin(db: &Db, account: &str, origin: &str) -> AppResult<Option<String>> {
    db.with(|c| {
        c.query_row("SELECT id FROM playlists WHERE account_id=?1 AND origin=?2 AND deleted_at IS NULL", params![account, origin], |r| r.get(0)).optional()
    })
}

pub fn playlist_detail(db: &Db, account: &str, id: &str) -> AppResult<PlaylistDetail> {
    db.with(|c| {
        let playlist = c
            .query_row(&format!("{SUMMARY_SELECT} WHERE p.id=?1 AND p.account_id=?2 AND p.deleted_at IS NULL"), params![id, account], summary_row)
            .optional()?;
        let Some(playlist) = playlist else { return Ok(None) };
        let report: Option<String> = c.query_row("SELECT report FROM playlists WHERE id=?1", [id], |r| r.get(0))?;
        let mut s = c.prepare(
            "SELECT i.id, i.added_at, t.data FROM playlist_items i JOIN tracks t ON t.id=i.track_id WHERE i.playlist_id=?1 ORDER BY i.position",
        )?;
        let rows: Vec<(String, i64, String)> = s.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut items: Vec<Item> =
            rows.into_iter().filter_map(|(item_id, added_at, data)| Some(Item { item_id, added_at, track: track_from(&data)? })).collect();
        let mut tracks: Vec<Track> = items.iter().map(|i| i.track.clone()).collect();
        decorate(c, &mut tracks)?;
        for (item, t) in items.iter_mut().zip(tracks) {
            item.track = t;
        }
        let skipped = report.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default();
        Ok(Some(PlaylistDetail { playlist, items, skipped }))
    })?
    .ok_or_else(|| AppError::new("not_found", "playlist not found"))
}

/// Appends songs that aren't in the playlist yet; returns how many were added.
pub fn add_tracks(db: &Db, account: &str, playlist: &str, tracks: &[Track]) -> AppResult<usize> {
    tx(db, |c| {
        require_playlist(c, account, playlist)?;
        let mut existing: HashSet<String> = {
            let mut s = c.prepare("SELECT track_id FROM playlist_items WHERE playlist_id=?1")?;
            let ids = s.query_map([playlist], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            ids
        };
        let mut pos: i64 = c.query_row("SELECT COALESCE(MAX(position),-1)+1 FROM playlist_items WHERE playlist_id=?1", [playlist], |r| r.get(0))?;
        let now = now_secs();
        let mut added = 0;
        for t in tracks.iter().filter(|t| t.is_sane()) {
            if !existing.insert(t.id.clone()) {
                continue;
            }
            crate::db::upsert_track_conn(c, t)?;
            c.execute(
                "INSERT INTO playlist_items(id,playlist_id,track_id,position,added_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",
                params![uuid_v4(), playlist, t.id, pos, now],
            )?;
            pos += 1;
            added += 1;
        }
        c.execute("UPDATE playlists SET updated_at=?2 WHERE id=?1", params![playlist, now])?;
        Ok(added)
    })
}

pub fn remove_items(db: &Db, account: &str, playlist: &str, item_ids: &[String]) -> AppResult<Vec<Removed>> {
    tx(db, |c| {
        require_playlist(c, account, playlist)?;
        let mut removed = Vec::new();
        for id in item_ids {
            let row = c
                .query_row("SELECT track_id, position, added_at FROM playlist_items WHERE id=?1 AND playlist_id=?2", params![id, playlist], |r| {
                    Ok(Removed { item_id: id.clone(), track_id: r.get(0)?, position: r.get(1)?, added_at: r.get(2)? })
                })
                .optional()?;
            if let Some(row) = row {
                c.execute("DELETE FROM playlist_items WHERE id=?1", [id])?;
                removed.push(row);
            }
        }
        c.execute("UPDATE playlists SET updated_at=?2 WHERE id=?1", params![playlist, now_secs()])?;
        Ok(removed)
    })
}

/// Undo for `remove_items`: puts the rows back at their old positions.
pub fn restore_items(db: &Db, account: &str, playlist: &str, rows: &[Removed]) -> AppResult<()> {
    tx(db, |c| {
        require_playlist(c, account, playlist)?;
        for r in rows {
            let known: bool = c.query_row("SELECT 1 FROM tracks WHERE id=?1", [&r.track_id], |_| Ok(())).optional()?.is_some();
            if !known {
                continue;
            }
            c.execute("UPDATE playlist_items SET position=position+1 WHERE playlist_id=?1 AND position>=?2", params![playlist, r.position])?;
            c.execute(
                "INSERT OR IGNORE INTO playlist_items(id,playlist_id,track_id,position,added_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![r.item_id, playlist, r.track_id, r.position, r.added_at, now_secs()],
            )?;
        }
        Ok(())
    })
}

fn renumber(c: &Connection, playlist: &str, order: &[String]) -> rusqlite::Result<()> {
    let mut s = c.prepare_cached("UPDATE playlist_items SET position=?2 WHERE id=?1")?;
    for (i, id) in order.iter().enumerate() {
        s.execute(params![id, i as i64])?;
    }
    c.execute("UPDATE playlists SET updated_at=?2 WHERE id=?1", params![playlist, now_secs()])?;
    Ok(())
}

fn item_order(c: &Connection, playlist: &str) -> rusqlite::Result<Vec<String>> {
    let mut s = c.prepare("SELECT id FROM playlist_items WHERE playlist_id=?1 ORDER BY position")?;
    let ids = s.query_map([playlist], |r| r.get(0))?.collect();
    ids
}

pub fn move_item(db: &Db, account: &str, playlist: &str, item_id: &str, to: usize) -> AppResult<()> {
    tx(db, |c| {
        require_playlist(c, account, playlist)?;
        let mut order = item_order(c, playlist)?;
        let from = order.iter().position(|i| i == item_id).ok_or_else(|| AppError::new("not_found", "song not in playlist"))?;
        let id = order.remove(from);
        order.insert(to.min(order.len()), id);
        renumber(c, playlist, &order)?;
        Ok(())
    })
}

/// Makes the playlist match an imported list: keeps songs still in it (with their "added"
/// date), drops songs that are gone, adds new ones, and takes the list's order.
pub fn sync_items(db: &Db, account: &str, playlist: &str, tracks: &[Track]) -> AppResult<(usize, usize)> {
    tx(db, |c| {
        require_playlist(c, account, playlist)?;
        let current: Vec<(String, String)> = {
            let mut s = c.prepare("SELECT id, track_id FROM playlist_items WHERE playlist_id=?1")?;
            let rows = s.query_map([playlist], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        let wanted: HashSet<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        let mut removed = 0;
        let mut have = std::collections::HashMap::new();
        for (item, track) in current {
            if wanted.contains(track.as_str()) && !have.contains_key(&track) {
                have.insert(track, item);
            } else {
                c.execute("DELETE FROM playlist_items WHERE id=?1", [&item])?;
                removed += 1;
            }
        }
        let now = now_secs();
        let mut order = Vec::with_capacity(tracks.len());
        let mut added = 0;
        let mut seen = HashSet::new();
        for t in tracks.iter().filter(|t| t.is_sane() && seen.insert(t.id.clone())) {
            crate::db::upsert_track_conn(c, t)?;
            match have.get(&t.id) {
                Some(item) => order.push(item.clone()),
                None => {
                    let item = uuid_v4();
                    c.execute(
                        "INSERT INTO playlist_items(id,playlist_id,track_id,position,added_at,updated_at) VALUES(?1,?2,?3,0,?4,?4)",
                        params![item, playlist, t.id, now],
                    )?;
                    order.push(item);
                    added += 1;
                }
            }
        }
        renumber(c, playlist, &order)?;
        Ok((added, removed))
    })
}

pub fn set_import_info(db: &Db, playlist: &str, origin: &str, url: &str, cover: Option<&str>, skipped: &[Skipped]) -> AppResult<()> {
    let report = serde_json::to_string(skipped)?;
    db.with(|c| {
        c.execute(
            "UPDATE playlists SET origin=?2, origin_url=?3, cover=COALESCE(?4, cover), report=?5, updated_at=?6 WHERE id=?1",
            params![playlist, origin, url, cover, report, now_secs()],
        )
    })?;
    Ok(())
}

pub fn count_play(db: &Db, account: &str, playlist: &str) -> AppResult<()> {
    db.with(|c| c.execute("UPDATE playlists SET plays=plays+1 WHERE id=?1 AND account_id=?2", params![playlist, account]))?;
    Ok(())
}

/// Combines two playlists. Songs in both are added once. `into` is an existing playlist
/// that receives the other's songs; otherwise a new playlist named `new_name` is made.
pub fn merge(db: &Db, account: &str, a: &str, b: &str, into: Option<&str>, new_name: Option<&str>) -> AppResult<String> {
    let ta = playlist_detail(db, account, a)?.items.into_iter().map(|i| i.track).collect::<Vec<_>>();
    let tb = playlist_detail(db, account, b)?.items.into_iter().map(|i| i.track).collect::<Vec<_>>();
    let target = match (into, new_name) {
        (Some(t), _) if t == a || t == b => t.to_string(),
        (None, Some(name)) => create_playlist(db, account, name, None)?,
        _ => return Err(AppError::new("invalid_input", "merge needs a target")),
    };
    let all: Vec<Track> = ta.into_iter().chain(tb).collect();
    add_tracks(db, account, &target, &all)?;
    Ok(target)
}

// ---- folders --------------------------------------------------------------------------

pub fn create_folder(db: &Db, account: &str, name: &str, parent: Option<&str>) -> AppResult<String> {
    let name = clean_name(name)?;
    let id = uuid_v4();
    tx(db, |c| {
        if let Some(p) = parent {
            if require_folder(c, account, p)?.is_some() {
                return Err(AppError::new("too_deep", "folders nest one level deep"));
            }
        }
        let now = now_secs();
        let pos: i64 = c.query_row("SELECT COALESCE(MAX(position),0)+1 FROM folders WHERE account_id=?1", [account], |r| r.get(0))?;
        c.execute(
            "INSERT INTO folders(id,account_id,name,parent_id,position,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",
            params![id, account, name, parent, pos, now],
        )?;
        Ok(())
    })?;
    Ok(id)
}

pub fn rename_folder(db: &Db, account: &str, id: &str, name: &str) -> AppResult<()> {
    let name = clean_name(name)?;
    let n = db.with(|c| c.execute("UPDATE folders SET name=?3, updated_at=?4 WHERE id=?1 AND account_id=?2 AND deleted_at IS NULL", params![id, account, name, now_secs()]))?;
    if n == 0 {
        return Err(AppError::new("not_found", "folder not found"));
    }
    Ok(())
}

/// Puts a folder inside another (one level only) or back at the top.
pub fn set_folder_parent(db: &Db, account: &str, id: &str, parent: Option<&str>) -> AppResult<()> {
    tx(db, |c| {
        require_folder(c, account, id)?;
        if let Some(p) = parent {
            let has_children: bool = c.query_row("SELECT 1 FROM folders WHERE parent_id=?1 AND deleted_at IS NULL", [id], |_| Ok(())).optional()?.is_some();
            if p == id || require_folder(c, account, p)?.is_some() || has_children {
                return Err(AppError::new("too_deep", "folders nest one level deep"));
            }
        }
        c.execute("UPDATE folders SET parent_id=?2, updated_at=?3 WHERE id=?1", params![id, parent, now_secs()])?;
        Ok(())
    })
}

/// A trashed folder takes its playlists and sub-folders with it; restoring brings them back.
pub fn trash_folder(db: &Db, account: &str, id: &str) -> AppResult<()> {
    let n = db.with(|c| c.execute("UPDATE folders SET deleted_at=?3, updated_at=?3 WHERE id=?1 AND account_id=?2 AND deleted_at IS NULL", params![id, account, now_secs()]))?;
    if n == 0 {
        return Err(AppError::new("not_found", "folder not found"));
    }
    Ok(())
}

// ---- likes ----------------------------------------------------------------------------

pub fn set_like(db: &Db, account: &str, track: &Track, liked: bool) -> AppResult<()> {
    db.with(|c| {
        if liked {
            crate::db::upsert_track_conn(c, track)?;
            c.execute("INSERT OR IGNORE INTO likes(account_id,track_id,liked_at) VALUES(?1,?2,?3)", params![account, track.id, now_secs()])?;
        } else {
            c.execute("DELETE FROM likes WHERE account_id=?1 AND track_id=?2", params![account, track.id])?;
        }
        Ok(())
    })
}

/// Likes many songs at once (Liked Songs import); returns how many were new.
pub fn like_many(db: &Db, account: &str, tracks: &[Track]) -> AppResult<usize> {
    tx(db, |c| {
        let now = now_secs();
        let mut added = 0;
        // Oldest first gets the earliest time, so "date liked" keeps the export's order.
        for (i, t) in tracks.iter().rev().enumerate().filter(|(_, t)| t.is_sane()) {
            crate::db::upsert_track_conn(c, t)?;
            added += c.execute("INSERT OR IGNORE INTO likes(account_id,track_id,liked_at) VALUES(?1,?2,?3)", params![account, t.id, now - i as i64])?;
        }
        Ok(added)
    })
}

pub fn liked(db: &Db, account: &str) -> AppResult<Vec<Item>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT l.track_id, l.liked_at, t.data FROM likes l JOIN tracks t ON t.id=l.track_id WHERE l.account_id=?1 ORDER BY l.liked_at DESC")?;
        let rows: Vec<(String, i64, String)> = s.query_map([account], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut tracks: Vec<Track> = rows.iter().filter_map(|(_, _, d)| track_from(d)).collect();
        decorate(c, &mut tracks)?;
        let dates: std::collections::HashMap<&str, i64> = rows.iter().map(|(id, at, _)| (id.as_str(), *at)).collect();
        Ok(tracks.into_iter().map(|t| Item { item_id: t.id.clone(), added_at: dates.get(t.id.as_str()).copied().unwrap_or(0), track: t }).collect())
    })
}

pub fn liked_ids(db: &Db, account: &str) -> AppResult<Vec<String>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT track_id FROM likes WHERE account_id=?1")?;
        let ids = s.query_map([account], |r| r.get(0))?.collect();
        ids
    })
}

// ---- hidden songs and artists ---------------------------------------------------------

pub fn set_hidden(db: &Db, account: &str, kind: &str, value: &str, label: &str, on: bool) -> AppResult<()> {
    if !matches!(kind, "track" | "artist") || value.is_empty() || value.len() > 300 {
        return Err(AppError::new("invalid_input", "bad hidden entry"));
    }
    db.with(|c| {
        if on {
            c.execute(
                "INSERT OR REPLACE INTO hidden(account_id,kind,value,label,created_at) VALUES(?1,?2,?3,?4,?5)",
                params![account, kind, value, label.chars().take(300).collect::<String>(), now_secs()],
            )
        } else {
            c.execute("DELETE FROM hidden WHERE account_id=?1 AND kind=?2 AND value=?3", params![account, kind, value])
        }
    })?;
    Ok(())
}

pub fn hidden(db: &Db, account: &str) -> AppResult<Vec<Hidden>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT kind, value, label FROM hidden WHERE account_id=?1 ORDER BY created_at DESC")?;
        let rows = s.query_map([account], |r| Ok(Hidden { kind: r.get(0)?, value: r.get(1)?, label: r.get(2)? }))?.collect();
        rows
    })
}

/// Whether autoplay/radio may pick this song. Artists are compared by folded name, because
/// the same artist has a different id on every service.
pub fn is_hidden(list: &[Hidden], t: &Track) -> bool {
    list.iter().any(|h| match h.kind.as_str() {
        "track" => h.value == t.id,
        _ => t.artists.iter().any(|a| crate::matcher::fold(&a.name) == h.value),
    })
}

// ---- followed artists -----------------------------------------------------------------

pub fn set_follow(db: &Db, account: &str, f: &Followed, on: bool) -> AppResult<()> {
    db.with(|c| {
        if on {
            c.execute(
                "INSERT OR REPLACE INTO follows(account_id,source,artist_id,name,avatar,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![account, f.source, f.artist_id, f.name, f.avatar, now_secs()],
            )
        } else {
            c.execute("DELETE FROM follows WHERE account_id=?1 AND source=?2 AND artist_id=?3", params![account, f.source, f.artist_id])
        }
    })?;
    Ok(())
}

pub fn follows(db: &Db, account: &str) -> AppResult<Vec<Followed>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT source, artist_id, name, avatar FROM follows WHERE account_id=?1 ORDER BY name COLLATE NOCASE")?;
        let rows = s.query_map([account], |r| Ok(Followed { source: r.get(0)?, artist_id: r.get(1)?, name: r.get(2)?, avatar: r.get(3)? }))?.collect();
        rows
    })
}

// ---- trash ----------------------------------------------------------------------------

pub fn trash(db: &Db, account: &str) -> AppResult<Vec<TrashEntry>> {
    db.with(|c| {
        let mut out = Vec::new();
        let mut s = c.prepare(
            "SELECT id, name, deleted_at, (SELECT COUNT(*) FROM playlists p WHERE p.folder_id=f.id AND p.deleted_at IS NULL)
             FROM folders f WHERE account_id=?1 AND deleted_at IS NOT NULL",
        )?;
        for row in s.query_map([account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))? {
            let (id, name, at, n) = row?;
            out.push(TrashEntry { kind: "folder".into(), id, name, detail: n.to_string(), deleted_at: at });
        }
        let mut s = c.prepare(
            "SELECT id, name, deleted_at, (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id=p.id)
             FROM playlists p WHERE account_id=?1 AND deleted_at IS NOT NULL",
        )?;
        for row in s.query_map([account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))? {
            let (id, name, at, n) = row?;
            out.push(TrashEntry { kind: "playlist".into(), id, name, detail: n.to_string(), deleted_at: at });
        }
        let mut s = c.prepare("SELECT l.track_id, t.data, l.trashed_at FROM local_files l JOIN tracks t ON t.id=l.track_id WHERE l.trashed_at IS NOT NULL")?;
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (id, data, at) = row?;
            if let Some(t) = track_from(&data) {
                out.push(TrashEntry { kind: "local".into(), id, detail: t.artist_line(), name: t.title, deleted_at: at });
            }
        }
        out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        Ok(out)
    })
}

pub fn restore(db: &Db, account: &str, kind: &str, id: &str) -> AppResult<()> {
    let sql = match kind {
        "playlist" => "UPDATE playlists SET deleted_at=NULL, updated_at=?3 WHERE id=?1 AND account_id=?2",
        "folder" => "UPDATE folders SET deleted_at=NULL, updated_at=?3 WHERE id=?1 AND account_id=?2",
        _ => return Err(AppError::new("invalid_input", "unknown trash kind")),
    };
    db.with(|c| c.execute(sql, params![id, account, now_secs()]))?;
    Ok(())
}

pub fn delete_forever(db: &Db, account: &str, kind: &str, id: &str) -> AppResult<()> {
    tx(db, |c| {
        match kind {
            "playlist" => {
                c.execute("DELETE FROM playlists WHERE id=?1 AND account_id=?2 AND deleted_at IS NOT NULL", params![id, account])?;
            }
            "folder" => {
                let n = c.execute("DELETE FROM folders WHERE id=?1 AND account_id=?2 AND deleted_at IS NOT NULL", params![id, account])?;
                if n > 0 {
                    // Its playlists and sub-folders go with it.
                    c.execute("DELETE FROM playlists WHERE account_id=?1 AND folder_id IN (SELECT id FROM folders WHERE parent_id=?2)", params![account, id])?;
                    c.execute("DELETE FROM playlists WHERE account_id=?1 AND folder_id=?2", params![account, id])?;
                    c.execute("DELETE FROM folders WHERE account_id=?1 AND parent_id=?2", params![account, id])?;
                }
            }
            _ => return Err(AppError::new("invalid_input", "unknown trash kind")),
        }
        Ok(())
    })
}

/// Playlists and folders trashed more than TRASH_DAYS ago, for the automatic cleanup.
pub fn expired_trash(db: &Db, account: &str) -> AppResult<Vec<(String, String)>> {
    let cutoff = now_secs() - TRASH_DAYS * 86_400;
    Ok(trash(db, account)?.into_iter().filter(|e| e.deleted_at < cutoff && e.kind != "local").map(|e| (e.kind, e.id)).collect())
}

// ---- import matches -------------------------------------------------------------------

/// The saved match for an imported song: None = never tried, Some(None) = nothing found.
pub fn get_match(db: &Db, source: &str, source_id: &str) -> AppResult<Option<Option<Track>>> {
    db.with(|c| {
        let row: Option<(Option<String>, Option<String>)> = c
            .query_row(
                "SELECT m.track_id, t.data FROM matches m LEFT JOIN tracks t ON t.id=m.track_id WHERE m.source=?1 AND m.source_id=?2",
                params![source, source_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.map(|(id, data)| id.and(data).and_then(|d| track_from(&d))))
    })
}

pub fn set_match(db: &Db, source: &str, source_id: &str, matched: Option<&Track>, manual: bool, score: f32) -> AppResult<()> {
    db.with(|c| {
        if let Some(m) = matched {
            crate::db::upsert_track_conn(c, m)?;
        }
        c.execute(
            "INSERT OR REPLACE INTO matches(source,source_id,track_id,manual,score,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![source, source_id, matched.map(|m| m.id.as_str()), manual as i64, score as f64, now_secs()],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ArtistRef;

    fn setup() -> (Db, String) {
        let db = Db::in_memory();
        db.with(|c| c.execute("INSERT INTO accounts(id,email,password_hash,recovery_hash,display_name,created_at,updated_at) VALUES('a','a@x','h','r','A',0,0)", []))
            .unwrap();
        (db, "a".into())
    }

    fn track(id: &str) -> Track {
        let mut t = Track::imported("spotify", &format!("{id:0>22}"), &format!("Song {id}"), vec!["Artist".into()], Some(180_000), None, None);
        t.link = None;
        t
    }

    fn ids(db: &Db, a: &str, p: &str) -> Vec<String> {
        playlist_detail(db, a, p).unwrap().items.into_iter().map(|i| i.track.source_id.trim_start_matches('0').to_string()).collect()
    }

    #[test]
    fn adds_without_duplicates_and_reorders() {
        let (db, a) = setup();
        let p = create_playlist(&db, &a, "Mix", None).unwrap();
        assert_eq!(add_tracks(&db, &a, &p, &[track("1"), track("2"), track("1")]).unwrap(), 2);
        assert_eq!(add_tracks(&db, &a, &p, &[track("2"), track("3")]).unwrap(), 1);
        assert_eq!(ids(&db, &a, &p), vec!["1", "2", "3"]);
        let items = playlist_detail(&db, &a, &p).unwrap().items;
        move_item(&db, &a, &p, &items[2].item_id, 0).unwrap();
        assert_eq!(ids(&db, &a, &p), vec!["3", "1", "2"]);
        let detail = playlist_detail(&db, &a, &p).unwrap();
        assert_eq!(detail.playlist.count, 3);
        assert_eq!(detail.playlist.duration_ms, 540_000);
    }

    #[test]
    fn remove_and_undo_keeps_position() {
        let (db, a) = setup();
        let p = create_playlist(&db, &a, "Mix", None).unwrap();
        add_tracks(&db, &a, &p, &[track("1"), track("2"), track("3")]).unwrap();
        let mid = playlist_detail(&db, &a, &p).unwrap().items[1].item_id.clone();
        let removed = remove_items(&db, &a, &p, &[mid]).unwrap();
        assert_eq!(ids(&db, &a, &p), vec!["1", "3"]);
        restore_items(&db, &a, &p, &removed).unwrap();
        assert_eq!(ids(&db, &a, &p), vec!["1", "2", "3"]);
    }

    #[test]
    fn reimport_syncs_instead_of_duplicating() {
        let (db, a) = setup();
        let p = create_playlist(&db, &a, "Imported", None).unwrap();
        sync_items(&db, &a, &p, &[track("1"), track("2"), track("3")]).unwrap();
        let first_added = playlist_detail(&db, &a, &p).unwrap().items[0].added_at;
        let (added, removed) = sync_items(&db, &a, &p, &[track("1"), track("3"), track("4")]).unwrap();
        assert_eq!((added, removed), (1, 1));
        assert_eq!(ids(&db, &a, &p), vec!["1", "3", "4"]);
        assert_eq!(playlist_detail(&db, &a, &p).unwrap().items[0].added_at, first_added);
    }

    #[test]
    fn merge_adds_shared_songs_once() {
        let (db, a) = setup();
        let p1 = create_playlist(&db, &a, "A", None).unwrap();
        let p2 = create_playlist(&db, &a, "B", None).unwrap();
        add_tracks(&db, &a, &p1, &[track("1"), track("2")]).unwrap();
        add_tracks(&db, &a, &p2, &[track("2"), track("3")]).unwrap();
        let merged = merge(&db, &a, &p1, &p2, None, Some("A + B")).unwrap();
        assert_eq!(ids(&db, &a, &merged), vec!["1", "2", "3"]);
        merge(&db, &a, &p1, &p2, Some(&p2), None).unwrap();
        assert_eq!(ids(&db, &a, &p2), vec!["2", "3", "1"]);
    }

    #[test]
    fn folders_nest_one_level_and_trash_with_playlists() {
        let (db, a) = setup();
        let top = create_folder(&db, &a, "Top", None).unwrap();
        let sub = create_folder(&db, &a, "Sub", Some(&top)).unwrap();
        assert_eq!(create_folder(&db, &a, "Too deep", Some(&sub)).unwrap_err().code, "too_deep");
        assert_eq!(set_folder_parent(&db, &a, &top, Some(&sub)).unwrap_err().code, "too_deep");
        let p = create_playlist(&db, &a, "In sub", Some(&sub)).unwrap();
        trash_folder(&db, &a, &top).unwrap();
        let o = overview(&db, &a).unwrap();
        assert!(o.folders.is_empty());
        assert!(o.playlists.is_empty(), "playlist inside a trashed folder is hidden");
        restore(&db, &a, "folder", &top).unwrap();
        assert_eq!(overview(&db, &a).unwrap().playlists[0].id, p);
        trash_folder(&db, &a, &top).unwrap();
        delete_forever(&db, &a, "folder", &top).unwrap();
        assert!(playlist_detail(&db, &a, &p).is_err());
    }

    #[test]
    fn playlists_are_per_account() {
        let (db, a) = setup();
        db.with(|c| c.execute("INSERT INTO accounts(id,email,password_hash,recovery_hash,display_name,created_at,updated_at) VALUES('b','b@x','h','r','B',0,0)", []))
            .unwrap();
        let p = create_playlist(&db, &a, "Mine", None).unwrap();
        assert!(playlist_detail(&db, "b", &p).is_err());
        assert!(add_tracks(&db, "b", &p, &[track("1")]).is_err());
        assert!(overview(&db, "b").unwrap().playlists.is_empty());
    }

    #[test]
    fn matches_decorate_imported_songs() {
        let (db, a) = setup();
        let p = create_playlist(&db, &a, "Imported", None).unwrap();
        let (t1, t2) = (track("1"), track("2"));
        add_tracks(&db, &a, &p, &[t1.clone(), t2.clone()]).unwrap();
        let mut yt = Track::imported("youtube", "abcdefghijk", "Song 1", vec!["Artist".into()], Some(180_000), Some("https://i.ytimg.com/x.jpg".into()), None);
        yt.id = Track::make_id("youtube", "abcdefghijk");
        set_match(&db, "spotify", &t1.source_id, Some(&yt), false, 0.9).unwrap();
        set_match(&db, "spotify", &t2.source_id, None, false, 0.0).unwrap();
        let items = playlist_detail(&db, &a, &p).unwrap().items;
        assert_eq!(items[0].track.via.as_deref(), Some("youtube"));
        assert_eq!(items[0].track.cover.as_deref(), Some("https://i.ytimg.com/x.jpg"));
        assert_eq!(items[1].track.via.as_deref(), Some("none"));
        assert_eq!(get_match(&db, "spotify", &t2.source_id).unwrap(), Some(None));
        assert_eq!(get_match(&db, "spotify", "nope").unwrap(), None);
    }

    #[test]
    fn hidden_artists_match_across_services() {
        let list = vec![Hidden { kind: "artist".into(), value: crate::matcher::fold("Beyoncé"), label: "Beyoncé".into() }];
        let mut t = track("1");
        t.artists = vec![ArtistRef { id: None, name: "Beyonce".into() }];
        assert!(is_hidden(&list, &t));
    }
}
