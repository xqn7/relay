//! Finds something playable for a track: on disk → the track's own source → a stand-in from
//! the other source. Each step's failure is reported, never fatal.

use crate::error::{AppError, AppResult};
use crate::matcher;
use crate::matching;
use crate::sources::{is_import_only, Quality, ResolvedStream, SearchKind, Source, StreamKind, Track, SOUNDCLOUD, YOUTUBE};
use crate::util::now_secs;
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tauri::Emitter;

pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Resolution {
    /// The track whose audio will actually play (differs from the request for imported
    /// songs and stand-ins).
    pub played: Track,
    pub stream: ResolvedStream,
    /// The song's own source failed and another source's version plays instead.
    pub stand_in: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub source: &'static str,
    pub ok: bool,
    pub code: &'static str,
    pub fallback: Option<&'static str>,
}

pub fn emit_status(state: &AppState, status: SourceStatus) {
    let _ = state.app.emit("source-status", status);
}

/// The steps the resolver tries, in order. The app's sources provide them; tests use fakes,
/// so the fallback order is checked without the network.
pub(crate) trait Steps {
    /// The saved (or freshly searched) playable match of an imported song.
    async fn matched(&self, track: &Track) -> AppResult<Option<Track>>;
    /// The song's own source.
    async fn native(&self, track: &Track) -> AppResult<ResolvedStream>;
    /// The same song found on `source`.
    async fn stand_in(&self, track: &Track, source: &'static str) -> AppResult<(Track, ResolvedStream)>;
    fn status(&self, status: SourceStatus);
}

struct Live<'a> {
    state: &'a AppState,
    quality: Quality,
    fresh: bool,
}

impl Steps for Live<'_> {
    async fn matched(&self, track: &Track) -> AppResult<Option<Track>> {
        matching::matched(self.state, track, true).await
    }
    async fn native(&self, track: &Track) -> AppResult<ResolvedStream> {
        native_stream(self.state, track, self.quality, self.fresh).await
    }
    async fn stand_in(&self, track: &Track, source: &'static str) -> AppResult<(Track, ResolvedStream)> {
        stand_in(self.state, track, source, self.quality).await
    }
    fn status(&self, status: SourceStatus) {
        emit_status(self.state, status);
    }
}

/// Resolves within RESOLVE_TIMEOUT or fails with a clear "nothing playable" error.
pub async fn resolve(state: &AppState, track: &Track, quality: Quality, fresh: bool) -> AppResult<Resolution> {
    let steps = Live { state, quality, fresh };
    match tokio::time::timeout(RESOLVE_TIMEOUT, resolve_with(&steps, track)).await {
        Ok(r) => r,
        Err(_) => Err(AppError::new("nothing_playable", "timed out after 20 s")),
    }
}

/// Looks the song up ahead of a likely click (the pointer rests on it) so the click finds its
/// stream link cached. Only the song's own source, quietly: a failure here says nothing,
/// and the real play goes through `resolve` with its fallbacks and messages.
pub async fn warm(state: &AppState, track: &Track, quality: Quality) -> AppResult<()> {
    let matched;
    let track = if is_import_only(&track.source) {
        matched = matching::matched(state, track, true).await?.ok_or_else(|| AppError::new("not_found", "no match"))?;
        &matched
    } else {
        track
    };
    tokio::time::timeout(RESOLVE_TIMEOUT, native_stream(state, track, quality, false)).await.map_err(|_| AppError::new("timeout", "lookup took too long"))??;
    Ok(())
}

async fn resolve_with(steps: &impl Steps, requested: &Track) -> AppResult<Resolution> {
    let matched;
    let track = if is_import_only(&requested.source) {
        // Imported songs play through their saved (or freshly searched) match.
        matched = steps.matched(requested).await?.ok_or_else(|| AppError::new("nothing_playable", "no match on YouTube or SoundCloud"))?;
        &matched
    } else {
        requested
    };
    let native_err = match steps.native(track).await {
        Ok(stream) => {
            steps.status(SourceStatus { source: source_name(&track.source), ok: true, code: "ok", fallback: None });
            return Ok(Resolution { played: track.clone(), stream, stand_in: false });
        }
        Err(e) => e,
    };
    let fallback = if track.source == YOUTUBE { SOUNDCLOUD } else { YOUTUBE };
    // A missing or Go+-only song is a fact about the song, not a broken service.
    let service_broken = !matches!(native_err.code, "not_found" | "unavailable" | "preview_only" | "helper_installing");
    steps.status(SourceStatus {
        source: source_name(&track.source),
        ok: !service_broken,
        code: native_err.code,
        fallback: Some(source_name(fallback)),
    });
    match steps.stand_in(track, fallback).await {
        Ok((played, stream)) => Ok(Resolution { played, stream, stand_in: true }),
        // YouTube asking for a sign-in is worth saying (the UI can offer it) over "nothing found".
        Err(e) => {
            let code = if native_err.code == "signin_required" { "signin_required" } else { "nothing_playable" };
            Err(AppError::new(code, format!("{native_err}; stand-in: {e}")))
        }
    }
}

fn source_name(s: &str) -> &'static str {
    if s == SOUNDCLOUD { SOUNDCLOUD } else { YOUTUBE }
}

/// One lookup per song at a time: a click that lands while the song is being looked up
/// ahead of time (hover, next in queue) waits for that lookup instead of starting another.
static IN_FLIGHT: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = LazyLock::new(Default::default);

struct InFlight {
    id: String,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl InFlight {
    fn new(id: &str) -> Self {
        let mut map = IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner());
        let lock = map.entry(id.to_string()).or_default().clone();
        Self { id: id.to_string(), lock }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let mut map = IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner());
        // Only the map and this guard hold it: nobody else is waiting, so the entry goes.
        if Arc::strong_count(&self.lock) <= 2 {
            map.remove(&self.id);
        }
    }
}

/// The song's own source, using a still-valid cached URL when possible.
async fn native_stream(state: &AppState, track: &Track, quality: Quality, fresh: bool) -> AppResult<ResolvedStream> {
    let flight = InFlight::new(&track.id);
    let _turn = flight.lock.lock().await;
    if !fresh {
        if let Some(s) = cached_stream(state, &track.id)? {
            return Ok(s);
        }
    }
    let result = match track.source.as_str() {
        YOUTUBE => state.yt.resolve(track, quality).await,
        SOUNDCLOUD => state.sc.resolve(track, quality).await,
        _ => Err(AppError::new("invalid_input", "unknown source")),
    }?;
    store_stream(state, &track.id, &result)?;
    Ok(result)
}

async fn stand_in(state: &AppState, track: &Track, source: &str, quality: Quality) -> AppResult<(Track, ResolvedStream)> {
    let query = format!("{} {}", track.artist_line(), matcher::clean_title(&track.title));
    let results = match source {
        YOUTUBE => state.yt.search(&query, SearchKind::Songs).await?,
        _ => state.sc.search(&query, SearchKind::Songs).await?,
    };
    let candidate = matcher::best_match(track, &results.tracks)
        .ok_or_else(|| AppError::new("no_match", format!("no matching song on {source}")))?
        .clone();
    state.db.upsert_track(&candidate)?;
    let stream = native_stream(state, &candidate, quality, false).await?;
    Ok((candidate, stream))
}

fn cached_stream(state: &AppState, track_id: &str) -> AppResult<Option<ResolvedStream>> {
    let min_expiry = now_secs() + 60;
    state.db.with(|c| {
        c.query_row(
            "SELECT url,kind,mime,expires_at,size,loudness_db,user_agent FROM stream_cache WHERE track_id=?1 AND expires_at>?2",
            params![track_id, min_expiry],
            |r| {
                let kind: String = r.get(1)?;
                Ok(ResolvedStream {
                    url: r.get(0)?,
                    kind: if kind == "hls" { StreamKind::Hls } else { StreamKind::Direct },
                    mime: r.get(2)?,
                    expires_at: r.get(3)?,
                    size: r.get::<_, Option<i64>>(4)?.map(|s| s as u64),
                    loudness_db: r.get::<_, Option<f64>>(5)?.map(|l| l as f32),
                    user_agent: r.get(6)?,
                })
            },
        )
        .optional()
    })
}

fn store_stream(state: &AppState, track_id: &str, s: &ResolvedStream) -> AppResult<()> {
    let kind = if s.kind == StreamKind::Hls { "hls" } else { "direct" };
    state.db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO stream_cache(track_id,url,kind,mime,origin_id,expires_at,size,loudness_db,user_agent)
             VALUES(?1,?2,?3,?4,?1,?5,?6,?7,?8)",
            params![track_id, s.url, kind, s.mime, s.expires_at, s.size.map(|x| x as i64), s.loudness_db.map(f64::from), s.user_agent],
        )
    })?;
    Ok(())
}

/// Called when a URL turned out dead before its stated expiry (403/410 mid-download).
pub fn forget_stream(state: &AppState, track_id: &str) {
    let _ = state.db.with(|c| c.execute("DELETE FROM stream_cache WHERE track_id=?1", [track_id]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Plays only from the listed sources and records every step the resolver takes.
    struct Fake {
        working: Vec<&'static str>,
        native_error: &'static str,
        log: RefCell<Vec<String>>,
        statuses: RefCell<Vec<(&'static str, bool)>>,
    }

    fn fake(working: &[&'static str], native_error: &'static str) -> Fake {
        Fake { working: working.to_vec(), native_error, log: RefCell::default(), statuses: RefCell::default() }
    }

    fn stream() -> ResolvedStream {
        ResolvedStream {
            url: "https://x.test/a".into(),
            kind: StreamKind::Direct,
            mime: "audio/mp4".into(),
            expires_at: 0,
            size: None,
            loudness_db: None,
            user_agent: None,
        }
    }

    fn song(source: &str) -> Track {
        Track::imported(source, "1", "Song", vec!["Artist".into()], Some(200_000), None, None)
    }

    impl Steps for Fake {
        async fn matched(&self, track: &Track) -> AppResult<Option<Track>> {
            self.log.borrow_mut().push(format!("match {}", track.source));
            Ok(self.working.first().map(|s| song(s)))
        }
        async fn native(&self, track: &Track) -> AppResult<ResolvedStream> {
            self.log.borrow_mut().push(format!("native {}", track.source));
            if self.working.contains(&track.source.as_str()) {
                Ok(stream())
            } else {
                Err(AppError::new(self.native_error, "fake failure"))
            }
        }
        async fn stand_in(&self, _track: &Track, source: &'static str) -> AppResult<(Track, ResolvedStream)> {
            self.log.borrow_mut().push(format!("stand-in {source}"));
            if self.working.contains(&source) {
                Ok((song(source), stream()))
            } else {
                Err(AppError::new("no_match", "fake"))
            }
        }
        fn status(&self, status: SourceStatus) {
            self.statuses.borrow_mut().push((status.source, status.ok));
        }
    }

    fn run(steps: &Fake, track: &Track) -> AppResult<Resolution> {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(resolve_with(steps, track))
    }

    #[test]
    fn own_source_first() {
        let f = fake(&[YOUTUBE, SOUNDCLOUD], "network");
        let r = run(&f, &song(YOUTUBE)).unwrap();
        assert!(!r.stand_in);
        assert_eq!(f.log.borrow().as_slice(), ["native youtube"]);
        assert_eq!(f.statuses.borrow().as_slice(), [(YOUTUBE, true)]);
    }

    #[test]
    fn broken_youtube_falls_back_to_soundcloud() {
        let f = fake(&[SOUNDCLOUD], "source_failed");
        let r = run(&f, &song(YOUTUBE)).unwrap();
        assert!(r.stand_in);
        assert_eq!(r.played.source, SOUNDCLOUD);
        assert_eq!(f.log.borrow().as_slice(), ["native youtube", "stand-in soundcloud"]);
        // Reported as a failing service, so the "trying SoundCloud" toast shows.
        assert_eq!(f.statuses.borrow().as_slice(), [(YOUTUBE, false)]);
    }

    #[test]
    fn soundcloud_falls_back_to_youtube_without_blaming_the_service() {
        let f = fake(&[YOUTUBE], "preview_only");
        let r = run(&f, &song(SOUNDCLOUD)).unwrap();
        assert_eq!(r.played.source, YOUTUBE);
        assert_eq!(f.log.borrow().as_slice(), ["native soundcloud", "stand-in youtube"]);
        assert_eq!(f.statuses.borrow().as_slice(), [(SOUNDCLOUD, true)]);
    }

    #[test]
    fn imported_songs_play_through_their_match() {
        let f = fake(&[YOUTUBE], "network");
        let r = run(&f, &song("spotify")).unwrap();
        assert_eq!(r.played.source, YOUTUBE);
        assert!(!r.stand_in);
        assert_eq!(f.log.borrow().as_slice(), ["match spotify", "native youtube"]);
    }

    #[test]
    fn nothing_playable_when_every_step_fails() {
        let f = fake(&[], "network");
        assert_eq!(run(&f, &song(YOUTUBE)).err().unwrap().code, "nothing_playable");
        assert_eq!(f.log.borrow().as_slice(), ["native youtube", "stand-in soundcloud"]);
        let unmatched = fake(&[], "network");
        assert_eq!(run(&unmatched, &song("deezer")).err().unwrap().code, "nothing_playable");
        assert_eq!(unmatched.log.borrow().as_slice(), ["match deezer"]);
    }
}
