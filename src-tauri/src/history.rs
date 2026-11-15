//! The account's listening history: one row per song that counted as played (after ~30 s or
//! half the song, decided by the player), with how long it was listened to. Play counts, the
//! Home page lists, mixes, Discover and Wrapped are all read from here.

use crate::db::{upsert_track_conn, Db};
use crate::error::{AppError, AppResult};
use crate::library::decorate;
use crate::matcher::fold;
use crate::sources::Track;
use crate::util::{now_secs, uuid_v4};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;

const DAY_SECS: i64 = 86_400;
/// A single play can't be longer than this; anything above is a broken report.
const MAX_LISTEN_MS: i64 = 6 * 3600 * 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counted {
    pub track: Track,
    pub plays: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistStat {
    pub name: String,
    pub plays: i64,
    pub minutes: i64,
    /// The cover of this artist's most played song, as their picture.
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    /// Local midnight of that day, as Unix seconds.
    pub day: i64,
    pub minutes: i64,
}

/// Listening stats for a period (a Wrapped month or year, or all time on the profile).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub minutes: i64,
    pub plays: i64,
    pub songs: i64,
    pub artists: i64,
    pub top_songs: Vec<Counted>,
    pub top_artists: Vec<ArtistStat>,
    pub top_day: Option<DayStat>,
    /// Most days in a row with music.
    pub longest_streak: i64,
    pub streak_start: Option<i64>,
    /// Time of the first play ever counted in the period.
    pub first_play: Option<i64>,
    /// The local hour (0–23) with the most listening.
    pub peak_hour: Option<i64>,
}

fn track_from(data: &str) -> Option<Track> {
    serde_json::from_str(data).ok()
}

pub fn record(db: &Db, account: &str, track: &Track, ms: i64) -> AppResult<String> {
    let id = uuid_v4();
    let now = now_secs();
    db.with(|c| {
        upsert_track_conn(c, track)?;
        c.execute(
            "INSERT INTO plays(id,account_id,track_id,played_at,ms_listened,updated_at) VALUES(?1,?2,?3,?4,?5,?4)",
            params![id, account, track.id, now, ms.clamp(0, MAX_LISTEN_MS)],
        )
    })?;
    Ok(id)
}

/// Updates how long a recorded play was listened to (it keeps growing after it counted).
pub fn set_listened(db: &Db, account: &str, id: &str, ms: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute(
            "UPDATE plays SET ms_listened=MAX(ms_listened,?1), updated_at=?2 WHERE id=?3 AND account_id=?4",
            params![ms.clamp(0, MAX_LISTEN_MS), now_secs(), id, account],
        )
    })?;
    Ok(())
}

pub fn counts(db: &Db, account: &str) -> AppResult<HashMap<String, i64>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT track_id, COUNT(*) FROM plays WHERE account_id=?1 GROUP BY track_id")?;
        let rows = s.query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    })
}

/// Songs played most recently, each once, newest first.
pub fn recent(db: &Db, account: &str, limit: usize) -> AppResult<Vec<Track>> {
    db.with(|c| {
        let mut s = c.prepare(
            "SELECT t.data FROM (SELECT track_id, MAX(played_at) AS last FROM plays WHERE account_id=?1 GROUP BY track_id) p
             JOIN tracks t ON t.id=p.track_id ORDER BY p.last DESC LIMIT ?2",
        )?;
        let rows: Vec<String> = s.query_map(params![account, limit as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut tracks: Vec<Track> = rows.iter().filter_map(|d| track_from(d)).collect();
        decorate(c, &mut tracks)?;
        Ok(tracks)
    })
}

/// Per song in [from, to): plays and minutes, most played first.
fn per_track(c: &Connection, account: &str, from: i64, to: i64) -> rusqlite::Result<Vec<(Track, i64, i64)>> {
    let mut s = c.prepare(
        "SELECT t.data, COUNT(*) AS n, SUM(p.ms_listened) AS ms FROM plays p JOIN tracks t ON t.id=p.track_id
         WHERE p.account_id=?1 AND p.played_at>=?2 AND p.played_at<?3 GROUP BY p.track_id ORDER BY n DESC, ms DESC",
    )?;
    let rows: Vec<(String, i64, i64)> = s.query_map(params![account, from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows.into_iter().filter_map(|(d, n, ms)| track_from(&d).map(|t| (t, n, ms))).collect())
}

pub fn most_played(db: &Db, account: &str, since: i64, limit: usize) -> AppResult<Vec<Counted>> {
    db.with(|c| {
        let rows = per_track(c, account, since, i64::MAX)?;
        let mut tracks: Vec<Track> = rows.iter().take(limit).map(|(t, _, _)| t.clone()).collect();
        decorate(c, &mut tracks)?;
        Ok(tracks.into_iter().zip(rows).map(|(track, (_, plays, _))| Counted { track, plays }).collect())
    })
}

/// Artists by plays, credited to each song's main (first) artist. Spellings are merged by
/// their folded form; the most played spelling is shown.
pub fn artists_from(rows: &[(Track, i64, i64)]) -> Vec<ArtistStat> {
    let mut by: HashMap<String, ArtistStat> = HashMap::new();
    for (track, plays, ms) in rows {
        let Some(name) = track.artists.first().map(|a| a.name.trim()).filter(|n| !n.is_empty()) else { continue };
        // Rows come most played first, so the first spelling and cover seen are the main ones.
        let entry = by.entry(fold(name)).or_insert_with(|| ArtistStat { name: name.to_string(), plays: 0, minutes: 0, cover: track.cover.clone() });
        entry.plays += plays;
        entry.minutes += ms;
    }
    let mut out: Vec<ArtistStat> = by.into_values().map(|a| ArtistStat { minutes: a.minutes / 60_000, ..a }).collect();
    out.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.minutes.cmp(&a.minutes)).then(a.name.cmp(&b.name)));
    out
}

pub fn top_artists(db: &Db, account: &str, since: i64, limit: usize) -> AppResult<Vec<ArtistStat>> {
    let rows = db.with(|c| per_track(c, account, since, i64::MAX))?;
    let mut list = artists_from(&rows);
    list.truncate(limit);
    Ok(list)
}

/// Songs by an artist (folded name) from the history, most played first.
pub fn songs_by(db: &Db, account: &str, artist: &str, since: i64, limit: usize) -> AppResult<Vec<Track>> {
    db.with(|c| {
        let mut tracks: Vec<Track> = per_track(c, account, since, i64::MAX)?
            .into_iter()
            .filter(|(t, _, _)| t.artists.first().is_some_and(|a| fold(&a.name) == artist))
            .take(limit)
            .map(|(t, _, _)| t)
            .collect();
        decorate(c, &mut tracks)?;
        Ok(tracks)
    })
}

/// Every artist name the account has listened to, folded.
pub fn heard_artists(db: &Db, account: &str) -> AppResult<Vec<String>> {
    let rows = db.with(|c| per_track(c, account, 0, i64::MAX))?;
    Ok(rows.iter().flat_map(|(t, _, _)| t.artists.iter().map(|a| fold(&a.name))).collect())
}

/// The longest run of consecutive days in a sorted list of day numbers: (length, first day).
pub fn longest_run(days: &[i64]) -> (i64, Option<i64>) {
    let (mut best, mut best_start) = (0, None);
    let (mut run, mut start) = (0, 0);
    for (i, &d) in days.iter().enumerate() {
        if i > 0 && d == days[i - 1] + 1 {
            run += 1;
        } else {
            run = 1;
            start = d;
        }
        if run > best {
            best = run;
            best_start = Some(start);
        }
    }
    (best, best_start)
}

/// Stats for plays in [from, to). Days are counted in local time: `tz_minutes` is the
/// offset from UTC (e.g. 60 in Berlin in winter).
pub fn stats(db: &Db, account: &str, from: i64, to: i64, tz_minutes: i64) -> AppResult<Stats> {
    if from >= to || !(-14 * 60..=14 * 60).contains(&tz_minutes) {
        return Err(AppError::new("invalid_input", "bad period"));
    }
    let shift = tz_minutes * 60;
    db.with(|c| {
        let rows = per_track(c, account, from, to)?;
        let mut s = c.prepare(
            "SELECT (played_at+?4)/86400 AS d, SUM(ms_listened) FROM plays
             WHERE account_id=?1 AND played_at>=?2 AND played_at<?3 GROUP BY d ORDER BY d",
        )?;
        let days: Vec<(i64, i64)> = s.query_map(params![account, from, to, shift], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let peak_hour: Option<i64> = c
            .query_row(
                "SELECT ((played_at+?4)%86400)/3600 AS h FROM plays
                 WHERE account_id=?1 AND played_at>=?2 AND played_at<?3 GROUP BY h ORDER BY SUM(ms_listened) DESC LIMIT 1",
                params![account, from, to, shift],
                |r| r.get(0),
            )
            .optional()?;
        let first_play: Option<i64> =
            c.query_row("SELECT MIN(played_at) FROM plays WHERE account_id=?1 AND played_at>=?2 AND played_at<?3", params![account, from, to], |r| r.get(0))?;

        let to_local_midnight = |d: i64| d * DAY_SECS - shift;
        let top_day = days.iter().max_by_key(|(_, ms)| *ms).map(|&(d, ms)| DayStat { day: to_local_midnight(d), minutes: ms / 60_000 });
        let (longest_streak, start) = longest_run(&days.iter().map(|(d, _)| *d).collect::<Vec<_>>());
        let artists = artists_from(&rows);

        let mut top: Vec<Track> = rows.iter().take(5).map(|(t, _, _)| t.clone()).collect();
        decorate(c, &mut top)?;
        let top_songs = top.into_iter().zip(&rows).map(|(track, (_, plays, _))| Counted { track, plays: *plays }).collect();
        Ok(Stats {
            minutes: rows.iter().map(|(_, _, ms)| ms).sum::<i64>() / 60_000,
            plays: rows.iter().map(|(_, n, _)| n).sum(),
            songs: rows.len() as i64,
            artists: artists.len() as i64,
            top_songs,
            top_artists: artists.into_iter().take(5).collect(),
            top_day,
            longest_streak,
            streak_start: start.map(to_local_midnight),
            first_play,
            peak_hour,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ArtistRef;

    fn track(id: &str, artist: &str) -> Track {
        Track {
            id: format!("youtube:{id}"),
            source: "youtube".into(),
            source_id: id.into(),
            title: format!("Song {id}"),
            artists: vec![ArtistRef { id: None, name: artist.into() }],
            album: None,
            duration_ms: Some(200_000),
            cover: Some(format!("https://x.test/{id}.jpg")),
            is_video: false,
            link: None,
            via: None,
            listens: None,
        }
    }

    fn account(db: &Db) -> String {
        crate::accounts::create(db, "me@example.com", "longenough", true).unwrap().account.id
    }

    fn play_at(db: &Db, account: &str, t: &Track, at: i64, ms: i64) {
        let id = record(db, account, t, ms).unwrap();
        db.with(|c| c.execute("UPDATE plays SET played_at=?1 WHERE id=?2", params![at, id])).unwrap();
    }

    #[test]
    fn runs_of_days() {
        assert_eq!(longest_run(&[]), (0, None));
        assert_eq!(longest_run(&[5]), (1, Some(5)));
        assert_eq!(longest_run(&[1, 2, 3, 7, 8]), (3, Some(1)));
        assert_eq!(longest_run(&[1, 3, 4, 5, 6, 9]), (4, Some(3)));
    }

    #[test]
    fn counts_recent_and_most_played() {
        let db = Db::in_memory();
        let me = account(&db);
        let (a, b) = (track("a", "Alpha"), track("b", "Beta"));
        play_at(&db, &me, &a, 100, 60_000);
        play_at(&db, &me, &a, 200, 60_000);
        play_at(&db, &me, &b, 300, 60_000);
        let counts = counts(&db, &me).unwrap();
        assert_eq!(counts["youtube:a"], 2);
        assert_eq!(counts["youtube:b"], 1);
        let recent: Vec<String> = recent(&db, &me, 10).unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(recent, ["youtube:b", "youtube:a"]);
        let most = most_played(&db, &me, 0, 10).unwrap();
        assert_eq!((most[0].track.id.as_str(), most[0].plays), ("youtube:a", 2));
    }

    #[test]
    fn listened_time_only_grows() {
        let db = Db::in_memory();
        let me = account(&db);
        let id = record(&db, &me, &track("a", "Alpha"), 30_000).unwrap();
        set_listened(&db, &me, &id, 120_000).unwrap();
        set_listened(&db, &me, &id, 10_000).unwrap();
        set_listened(&db, "someone-else", &id, 900_000).unwrap();
        let ms: i64 = db.with(|c| c.query_row("SELECT ms_listened FROM plays WHERE id=?1", [&id], |r| r.get(0))).unwrap();
        assert_eq!(ms, 120_000);
    }

    #[test]
    fn stats_for_a_period() {
        let db = Db::in_memory();
        let me = account(&db);
        let day = DAY_SECS;
        let (a, b, c) = (track("a", "Alpha"), track("b", "beta"), track("c", "Béta"));
        // Day 10 and 11 (UTC), then day 13; one play outside the period.
        play_at(&db, &me, &a, 10 * day + 100, 180_000);
        play_at(&db, &me, &a, 11 * day + 100, 180_000);
        play_at(&db, &me, &b, 11 * day + 200, 240_000);
        play_at(&db, &me, &c, 13 * day + 100, 60_000);
        play_at(&db, &me, &b, 40 * day, 60_000);
        let s = stats(&db, &me, 10 * day, 20 * day, 0).unwrap();
        assert_eq!((s.plays, s.songs, s.minutes), (4, 3, 11));
        assert_eq!(s.top_songs[0].track.id, "youtube:a");
        // "beta" and "Béta" are the same artist.
        assert_eq!(s.artists, 2);
        assert_eq!(s.top_artists[0].plays, 2);
        assert_eq!(s.top_day.as_ref().map(|d| (d.day, d.minutes)), Some((11 * day, 7)));
        assert_eq!((s.longest_streak, s.streak_start), (2, Some(10 * day)));
        assert_eq!(s.first_play, Some(10 * day + 100));
        // An hour ahead of UTC moves day boundaries but not this data's days.
        let shifted = stats(&db, &me, 10 * day, 20 * day, 60).unwrap();
        assert_eq!(shifted.streak_start, Some(10 * day - 3600));
        assert!(stats(&db, &me, 5, 5, 0).is_err());
    }
}
