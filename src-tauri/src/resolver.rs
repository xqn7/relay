//! Finds something playable for a track: on disk → the track's own source → a stand-in from
//! the other source. Each step's failure is reported, never fatal.

use crate::error::{AppError, AppResult};
use crate::matcher;
use crate::sources::{Quality, ResolvedStream, SearchKind, Source, StreamKind, Track, SOUNDCLOUD, YOUTUBE};
use crate::util::now_secs;
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::time::Duration;
use tauri::Emitter;

pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Resolution {
    /// The track whose audio will actually play (differs from the request for stand-ins).
    pub played: Track,
    pub stream: ResolvedStream,
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

/// Resolves within RESOLVE_TIMEOUT or fails with a clear "nothing playable" error.
pub async fn resolve(state: &AppState, track: &Track, quality: Quality, fresh: bool) -> AppResult<Resolution> {
    match tokio::time::timeout(RESOLVE_TIMEOUT, resolve_inner(state, track, quality, fresh)).await {
        Ok(r) => r,
        Err(_) => Err(AppError::new("nothing_playable", "timed out after 20 s")),
    }
}

async fn resolve_inner(state: &AppState, track: &Track, quality: Quality, fresh: bool) -> AppResult<Resolution> {
    let native_err = match native_stream(state, track, quality, fresh).await {
        Ok(stream) => {
            emit_status(state, SourceStatus { source: source_name(&track.source), ok: true, code: "ok", fallback: None });
            return Ok(Resolution { played: track.clone(), stream });
        }
        Err(e) => e,
    };
    let fallback = if track.source == YOUTUBE { SOUNDCLOUD } else { YOUTUBE };
    // A missing or Go+-only song is a fact about the song, not a broken service.
    let service_broken = !matches!(native_err.code, "not_found" | "unavailable" | "preview_only" | "helper_installing");
    emit_status(
        state,
        SourceStatus {
            source: source_name(&track.source),
            ok: !service_broken,
            code: native_err.code,
            fallback: Some(source_name(fallback)),
        },
    );
    match stand_in(state, track, fallback, quality).await {
        Ok(r) => Ok(r),
        Err(e) => Err(AppError::new("nothing_playable", format!("{native_err}; stand-in: {e}"))),
    }
}

fn source_name(s: &str) -> &'static str {
    if s == SOUNDCLOUD { SOUNDCLOUD } else { YOUTUBE }
}

/// The song's own source, using a still-valid cached URL when possible.
async fn native_stream(state: &AppState, track: &Track, quality: Quality, fresh: bool) -> AppResult<ResolvedStream> {
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

async fn stand_in(state: &AppState, track: &Track, source: &str, quality: Quality) -> AppResult<Resolution> {
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
    Ok(Resolution { played: candidate, stream })
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
