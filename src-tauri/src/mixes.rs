//! Things built from the listening history with help from YouTube Music: song radio (for
//! "Start radio", autoplay and mixes), the daily mixes on Home, and Discover's artists.

use crate::error::{AppError, AppResult};
use crate::history;
use crate::library::{self, is_hidden};
use crate::matcher::{clean_title, fold};
use crate::sources::{is_import_only, ArtistCard, SearchKind, Source, Track, YOUTUBE};
use crate::util::now_secs;
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MIX_COUNT: usize = 6;
const MIX_LEN: usize = 30;
const MIX_MIN_LEN: usize = 8;
/// Own songs per mix; the rest are songs the artist's radio suggests.
const MIX_OWN: usize = 10;
const MIX_MIN_PLAYS: i64 = 3;
/// Mixes and Discover look at this much recent history first (then all of it).
const RECENT_SECS: i64 = 90 * 86_400;
const DISCOVER_SEEDS: usize = 5;
const DISCOVER_MAX: usize = 12;
const DISCOVER_TTL: Duration = Duration::from_secs(3600);

/// Songs YouTube Music would play after this one. Songs from elsewhere borrow the radio of
/// their YouTube match (or the closest YouTube Music song), since only YouTube has radio.
pub async fn radio_for(state: &AppState, track: &Track) -> AppResult<Vec<Track>> {
    let matched = if is_import_only(&track.source) { library::get_match(&state.db, &track.source, &track.source_id)?.flatten() } else { None };
    let seed = match matched.as_ref().unwrap_or(track) {
        t if t.source == YOUTUBE => t.source_id.clone(),
        t => {
            let query = format!("{} {}", t.artist_line(), clean_title(&t.title));
            let found = state.yt.search(&query, SearchKind::Songs).await?;
            found.tracks.first().map(|t| t.source_id.clone()).ok_or_else(|| AppError::new("no_match", "no seed song"))?
        }
    };
    state.yt.radio(&seed).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mix {
    /// 1-based, "Daily Mix 1".
    pub number: usize,
    /// The artists the mix is mostly made of, seed artist first.
    pub artists: Vec<String>,
    pub tracks: Vec<Track>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    mixes: Vec<Mix>,
}

/// Familiar songs spread through the new ones: one own song, then two from the radio.
pub fn interleave(own: &[Track], radio: &[Track], len: usize) -> Vec<Track> {
    let mut out: Vec<Track> = Vec::with_capacity(len);
    let mut seen = HashSet::new();
    let (mut o, mut r) = (own.iter(), radio.iter());
    while out.len() < len {
        let pick = if out.len().is_multiple_of(3) { o.next().or_else(|| r.next()) } else { r.next().or_else(|| o.next()) };
        let Some(t) = pick else { break };
        if seen.insert(t.id.clone()) {
            out.push(t.clone());
        }
    }
    out
}

/// The artists a mix is mostly made of: the seed, then the most frequent others.
fn mix_artists(seed: &str, tracks: &[Track]) -> Vec<String> {
    let mut counts: HashMap<String, (String, usize)> = HashMap::new();
    for a in tracks.iter().filter_map(|t| t.artists.first()) {
        counts.entry(fold(&a.name)).or_insert_with(|| (a.name.clone(), 0)).1 += 1;
    }
    let seed_key = fold(seed);
    let mut others: Vec<(String, usize)> = counts.into_iter().filter(|(k, _)| *k != seed_key).map(|(_, v)| v).collect();
    others.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    std::iter::once(seed.to_string()).chain(others.into_iter().take(2).map(|(n, _)| n)).collect()
}

/// Today's mixes, built once a day (`today` is the local date, e.g. "2026-11-10"). Mixes
/// built without the radio (offline) aren't kept, so they fill out once online again.
pub async fn daily(state: &Arc<AppState>, account: &str, today: &str) -> AppResult<Vec<Mix>> {
    if today.len() != 10 || !today.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err(AppError::new("invalid_input", "bad date"));
    }
    let stored: Option<(String, String)> =
        state.db.with(|c| c.query_row("SELECT day,data FROM mixes WHERE account_id=?1", [account], |r| Ok((r.get(0)?, r.get(1)?))).optional())?;
    if let Some((day, data)) = stored {
        if day == today {
            if let Ok(s) = serde_json::from_str::<Stored>(&data) {
                return Ok(s.mixes);
            }
        }
    }
    let since = now_secs() - RECENT_SECS;
    let mut seeds = history::top_artists(&state.db, account, since, MIX_COUNT)?;
    if seeds.len() < MIX_COUNT {
        seeds = history::top_artists(&state.db, account, 0, MIX_COUNT)?;
    }
    let hidden = library::hidden(&state.db, account)?;
    let mut mixes = Vec::new();
    let mut radio_worked = false;
    for seed in seeds.into_iter().filter(|s| s.plays >= MIX_MIN_PLAYS) {
        let own: Vec<Track> = history::songs_by(&state.db, account, &fold(&seed.name), 0, MIX_OWN)?.into_iter().filter(|t| !is_hidden(&hidden, t)).collect();
        let Some(first) = own.first() else { continue };
        let radio = match radio_for(state, first).await {
            Ok(list) => {
                radio_worked = true;
                list
            }
            Err(_) => Vec::new(),
        };
        let radio: Vec<Track> = radio.into_iter().filter(|t| !is_hidden(&hidden, t)).collect();
        let tracks = interleave(&own, &radio, MIX_LEN);
        if tracks.len() >= MIX_MIN_LEN {
            mixes.push(Mix { number: mixes.len() + 1, artists: mix_artists(&seed.name, &tracks), tracks });
        }
    }
    if radio_worked {
        let data = serde_json::to_string(&Stored { mixes: mixes.clone() })?;
        state.db.with(|c| {
            c.execute(
                "INSERT INTO mixes(account_id,day,data,created_at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(account_id) DO UPDATE SET day=excluded.day, data=excluded.data, created_at=excluded.created_at",
                params![account, today, data, now_secs()],
            )
        })?;
    }
    Ok(mixes)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub artist: ArtistCard,
    /// The artist in the library this one is similar to.
    pub because: String,
}

#[derive(Default)]
pub struct DiscoverCache(std::sync::Mutex<Option<(String, Instant, Vec<Suggestion>)>>);

/// Artists the library doesn't have yet, similar to the ones listened to most (YouTube Music's
/// related artists). Kept for an hour per account.
pub async fn discover(state: &Arc<AppState>, account: &str) -> AppResult<Vec<Suggestion>> {
    if let Some((who, at, list)) = state.discover.0.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if who == account && at.elapsed() < DISCOVER_TTL {
            return Ok(list.clone());
        }
    }
    let mut seeds = history::top_artists(&state.db, account, now_secs() - RECENT_SECS, DISCOVER_SEEDS)?;
    if seeds.len() < DISCOVER_SEEDS {
        seeds = history::top_artists(&state.db, account, 0, DISCOVER_SEEDS)?;
    }
    let mut known: HashSet<String> = history::heard_artists(&state.db, account)?.into_iter().collect();
    known.extend(library::follows(&state.db, account)?.iter().map(|f| fold(&f.name)));
    known.extend(library::liked(&state.db, account)?.iter().flat_map(|i| i.track.artists.iter().map(|a| fold(&a.name))));
    let hidden: HashSet<String> = library::hidden(&state.db, account)?.into_iter().filter(|h| h.kind == "artist").map(|h| h.value).collect();

    // Artists suggested by several seeds come first.
    let mut found: Vec<(Suggestion, usize)> = Vec::new();
    let mut any_ok = seeds.is_empty();
    for seed in &seeds {
        let Ok(Some(card)) = state.yt.find_artist(&seed.name).await else { continue };
        let Ok(page) = crate::library_commands::cached_artist(state, &card.source, &card.id).await else { continue };
        any_ok = true;
        for similar in page.similar {
            let key = fold(&similar.name);
            if known.contains(&key) || hidden.contains(&key) {
                continue;
            }
            match found.iter_mut().find(|(s, _)| fold(&s.artist.name) == key) {
                Some((_, n)) => *n += 1,
                None => found.push((Suggestion { artist: similar, because: seed.name.clone() }, 1)),
            }
        }
    }
    if !any_ok {
        return Err(AppError::new("network", "no artist pages could be read"));
    }
    // Stable sort keeps the seed order among equals: the most listened artist's picks first.
    found.sort_by_key(|f| std::cmp::Reverse(f.1));
    let list: Vec<Suggestion> = found.into_iter().take(DISCOVER_MAX).map(|(s, _)| s).collect();
    // An empty answer isn't kept: a few more plays may already be enough for suggestions.
    if !list.is_empty() {
        *state.discover.0.lock().unwrap_or_else(|p| p.into_inner()) = Some((account.to_string(), Instant::now(), list.clone()));
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ArtistRef;

    fn t(id: &str, artist: &str) -> Track {
        Track {
            id: format!("youtube:{id}"),
            source: "youtube".into(),
            source_id: id.into(),
            title: id.into(),
            artists: vec![ArtistRef { id: None, name: artist.into() }],
            album: None,
            duration_ms: None,
            cover: None,
            is_video: false,
            link: None,
            via: None,
            listens: None,
        }
    }

    #[test]
    fn interleaves_own_and_new_songs() {
        let own = [t("o1", "A"), t("o2", "A")];
        let radio = [t("r1", "B"), t("o2", "A"), t("r2", "C"), t("r3", "B"), t("r4", "B")];
        let ids: Vec<String> = interleave(&own, &radio, 6).into_iter().map(|x| x.source_id).collect();
        // o2 shows up once although the radio suggests it too.
        assert_eq!(ids, ["o1", "r1", "o2", "r2", "r3", "r4"]);
        assert_eq!(interleave(&own, &[], 10).len(), 2);
    }

    #[test]
    fn names_the_main_artists() {
        let tracks = [t("1", "Seed"), t("2", "B"), t("3", "C"), t("4", "B"), t("5", "D"), t("6", "seed")];
        assert_eq!(mix_artists("Seed", &tracks), ["Seed", "B", "C"]);
    }
}
