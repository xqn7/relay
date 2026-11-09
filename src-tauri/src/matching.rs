//! Finds the playable song for an imported one (Spotify, Apple Music, Deezer): YouTube Music
//! songs first, then YouTube music videos, then SoundCloud. Results are saved, so every song is
//! searched once; a manual "Fix match" always wins.

use crate::error::AppResult;
use crate::library;
use crate::matcher::{best_scored, clean_title};
use crate::sources::{SearchKind, Source, Track};
use crate::AppState;

fn query_for(track: &Track) -> String {
    let artist = track.artists.first().map(|a| a.name.as_str()).unwrap_or("");
    format!("{artist} {}", clean_title(&track.title)).trim().to_string()
}

/// Searches the catalogs. Ok(None) only when every search worked and nothing matched, so a
/// network error is never remembered as "no match".
pub async fn search(state: &AppState, track: &Track) -> AppResult<Option<(Track, f32)>> {
    let query = query_for(track);
    let songs = state.yt.search_songs(&query, false).await;
    if let Ok(list) = &songs {
        if let Some((m, s)) = best_scored(track, list) {
            return Ok(Some((m.clone(), s)));
        }
    }
    let videos = state.yt.search_songs(&query, true).await;
    if let Ok(list) = &videos {
        if let Some((m, s)) = best_scored(track, list) {
            return Ok(Some((m.clone(), s)));
        }
    }
    let sc = state.sc.search(&query, SearchKind::Songs).await;
    if let Ok(found) = &sc {
        if let Some((m, s)) = best_scored(track, &found.tracks) {
            return Ok(Some((m.clone(), s)));
        }
    }
    // Report the first failure if any search broke; otherwise there really is no match.
    songs?;
    videos?;
    sc?;
    Ok(None)
}

/// The saved match, searching (and saving) when there is none yet. `retry_unmatched` searches
/// again for songs that matched nothing last time.
pub async fn matched(state: &AppState, track: &Track, retry_unmatched: bool) -> AppResult<Option<Track>> {
    match library::get_match(&state.db, &track.source, &track.source_id)? {
        Some(Some(m)) => return Ok(Some(m)),
        Some(None) if !retry_unmatched => return Ok(None),
        _ => {}
    }
    let found = search(state, track).await?;
    library::set_match(&state.db, &track.source, &track.source_id, found.as_ref().map(|(m, _)| m), false, found.as_ref().map_or(0.0, |(_, s)| *s))?;
    Ok(found.map(|(m, _)| m))
}

/// Candidates for "Fix match": the best few from each catalog, scored, best first.
pub async fn candidates(state: &AppState, track: &Track, query: Option<&str>) -> AppResult<Vec<Track>> {
    let query = query.map(str::to_string).unwrap_or_else(|| query_for(track));
    let (songs, videos, sc) = tokio::join!(state.yt.search_songs(&query, false), state.yt.search_songs(&query, true), state.sc.search(&query, SearchKind::Songs));
    let mut all: Vec<Track> = Vec::new();
    all.extend(songs.unwrap_or_default().into_iter().take(8));
    all.extend(videos.unwrap_or_default().into_iter().take(5));
    all.extend(sc.map(|r| r.tracks).unwrap_or_default().into_iter().take(6));
    if all.is_empty() {
        // All three failed: say why instead of showing an empty list.
        state.yt.search_songs(&query, false).await?;
    }
    let mut scored: Vec<(f32, Track)> = all.into_iter().map(|c| (crate::matcher::score(track, &c), c)).collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(scored.into_iter().map(|(_, c)| c).collect())
}
