//! Per-song sound data: trims (a custom start/end per song), waveforms for the seek bar, and
//! SponsorBlock's non-music sections of YouTube music videos.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sources::Track;
use crate::util::now_secs;
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Bars in a stored waveform; the UI draws fewer by merging neighbours.
pub const WAVEFORM_BARS: usize = 400;
/// Samples per second ffmpeg hands over for the waveform: plenty for 400 bars, tiny in memory.
const WAVEFORM_RATE: u32 = 2000;
const SPONSOR_API: &str = "https://sponsor.ajay.app/api/skipSegments";
const SPONSOR_TIMEOUT: Duration = Duration::from_secs(6);
/// Shorter sections are not worth a jump.
const MIN_SKIP_S: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trim {
    pub start_ms: u64,
    pub end_ms: Option<u64>,
}

impl Trim {
    pub fn is_valid(&self) -> bool {
        self.start_ms < 24 * 3600 * 1000 && self.end_ms.is_none_or(|e| e > self.start_ms + 1000 && e < 24 * 3600 * 1000)
    }
}

pub fn trims(db: &Db, account: &str) -> AppResult<HashMap<String, Trim>> {
    db.with(|c| {
        let mut s = c.prepare("SELECT track_id,start_ms,end_ms FROM trims WHERE account_id=?1")?;
        let rows = s.query_map([account], |r| {
            let end: Option<i64> = r.get(2)?;
            Ok((r.get::<_, String>(0)?, Trim { start_ms: r.get::<_, i64>(1)?.max(0) as u64, end_ms: end.map(|e| e.max(0) as u64) }))
        })?;
        rows.collect()
    })
}

/// Saves a trim, or removes it with `None`.
pub fn set_trim(db: &Db, account: &str, track: &Track, trim: Option<Trim>) -> AppResult<()> {
    match trim {
        Some(t) if !t.is_valid() => Err(AppError::new("invalid_input", "bad trim")),
        Some(t) => db.with(|c| {
            crate::db::upsert_track_conn(c, track)?;
            c.execute(
                "INSERT INTO trims(account_id,track_id,start_ms,end_ms,updated_at) VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(account_id,track_id) DO UPDATE SET start_ms=excluded.start_ms, end_ms=excluded.end_ms, updated_at=excluded.updated_at",
                params![account, track.id, t.start_ms as i64, t.end_ms.map(|e| e as i64), now_secs()],
            )
            .map(|_| ())
        }),
        None => db.with(|c| c.execute("DELETE FROM trims WHERE account_id=?1 AND track_id=?2", params![account, track.id]).map(|_| ())),
    }
}

// ---- waveform -----------------------------------------------------------------------------

/// Loudness per bar (RMS of each slice), scaled so the loudest bar is 255.
pub fn peaks(samples: &[i16], bars: usize) -> Vec<u8> {
    if samples.is_empty() || bars == 0 {
        return Vec::new();
    }
    let per = samples.len().div_ceil(bars).max(1);
    let rms: Vec<f64> = samples
        .chunks(per)
        .map(|c| (c.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / c.len() as f64).sqrt())
        .collect();
    let max = rms.iter().cloned().fold(0.0, f64::max);
    if max <= 0.0 {
        return vec![0; rms.len()];
    }
    rms.iter().map(|v| ((v / max) * 255.0).round() as u8).collect()
}

/// The song's waveform, computed once from its audio file and stored.
pub async fn waveform(state: &Arc<AppState>, track: &Track) -> AppResult<Vec<u8>> {
    if let Some(found) = stored_waveform(&state.db, &track.id)? {
        return Ok(found);
    }
    // One at a time: ffmpeg decoding a whole song is the heaviest thing the app does.
    let _guard = state.waveform_lock.lock().await;
    if let Some(found) = stored_waveform(&state.db, &track.id)? {
        return Ok(found);
    }
    let path = crate::streams::audio_file(state, track).await?;
    let pcm = state.tools.decode_mono(&path, WAVEFORM_RATE).await?;
    let samples: Vec<i16> = pcm.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
    if samples.len() < WAVEFORM_RATE as usize {
        return Err(AppError::new("convert_failed", "song too short for a waveform"));
    }
    let bars = peaks(&samples, WAVEFORM_BARS);
    state.db.with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO waveforms(track_id,peaks,created_at) VALUES(?1,?2,?3)",
            params![track.id, bars, now_secs()],
        )
    })?;
    Ok(bars)
}

fn stored_waveform(db: &Db, track_id: &str) -> AppResult<Option<Vec<u8>>> {
    db.with(|c| c.query_row("SELECT peaks FROM waveforms WHERE track_id=?1", [track_id], |r| r.get(0)).optional())
}

// ---- SponsorBlock -------------------------------------------------------------------------

#[derive(Default)]
pub struct Sponsor {
    cache: Mutex<HashMap<String, Vec<(f64, f64)>>>,
}

/// Non-music sections from SponsorBlock's answer for one video, checked and sorted.
pub fn parse_segments(v: &Value, video_id: &str) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e.get("videoID").and_then(Value::as_str) == Some(video_id))
        .flat_map(|e| e.get("segments").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter(|s| s.get("category").and_then(Value::as_str) == Some("music_offtopic"))
        .filter(|s| s.get("actionType").and_then(Value::as_str).is_none_or(|a| a == "skip"))
        .filter_map(|s| {
            let seg = s.get("segment")?.as_array()?;
            let (a, b) = (seg.first()?.as_f64()?, seg.get(1)?.as_f64()?);
            (a.is_finite() && b.is_finite() && a >= 0.0 && b - a >= MIN_SKIP_S && b < 24.0 * 3600.0).then_some((a, b))
        })
        .collect();
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    // Overlapping submissions become one section.
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (a, b) in out {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

/// Asks by the first 4 hex digits of the video id's SHA-256, so SponsorBlock never learns
/// which video is playing.
pub async fn segments(state: &AppState, video_id: &str) -> AppResult<Vec<(f64, f64)>> {
    if video_id.len() != 11 || !video_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::new("invalid_input", "bad YouTube video id"));
    }
    if let Some(hit) = state.sponsor.cache.lock().unwrap_or_else(|p| p.into_inner()).get(video_id) {
        return Ok(hit.clone());
    }
    let prefix: String = Sha256::digest(video_id.as_bytes()).iter().take(2).map(|b| format!("{b:02x}")).collect();
    let url = format!("{SPONSOR_API}/{prefix}?categories=%5B%22music_offtopic%22%5D");
    let resp = state.http.client.get(&url).timeout(SPONSOR_TIMEOUT).send().await?;
    let found = if resp.status() == reqwest::StatusCode::NOT_FOUND {
        Vec::new()
    } else {
        crate::http::check_status(&resp)?;
        parse_segments(&resp.json::<Value>().await?, video_id)
    };
    let mut cache = state.sponsor.cache.lock().unwrap_or_else(|p| p.into_inner());
    if cache.len() > 500 {
        cache.clear();
    }
    cache.insert(video_id.to_string(), found.clone());
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn peaks_scale_to_the_loudest_slice() {
        let mut samples = vec![1000i16; 100];
        samples.extend(vec![-4000i16; 100]);
        let p = peaks(&samples, 2);
        assert_eq!(p, vec![64, 255]);
        assert_eq!(peaks(&[0, 0, 0], 3), vec![0, 0, 0]);
        assert!(peaks(&[], 10).is_empty());
    }

    #[test]
    fn keeps_only_valid_non_music_sections_of_this_video() {
        let v = json!([
            { "videoID": "abcdefghijk", "segments": [
                { "category": "music_offtopic", "actionType": "skip", "segment": [180.5, 230.0] },
                { "category": "music_offtopic", "actionType": "skip", "segment": [0.0, 12.0] },
                { "category": "sponsor", "segment": [40.0, 50.0] },
                { "category": "music_offtopic", "segment": [60.0, 60.4] },
                { "category": "music_offtopic", "segment": [10.0, 20.0] },
                { "category": "music_offtopic", "actionType": "mute", "segment": [90.0, 99.0] }
            ]},
            { "videoID": "zzzzzzzzzzz", "segments": [{ "category": "music_offtopic", "segment": [1.0, 9.0] }] }
        ]);
        assert_eq!(parse_segments(&v, "abcdefghijk"), vec![(0.0, 20.0), (180.5, 230.0)]);
        assert!(parse_segments(&json!({"x":1}), "abcdefghijk").is_empty());
    }

    #[test]
    fn trims_need_a_sensible_range() {
        assert!(Trim { start_ms: 0, end_ms: Some(60_000) }.is_valid());
        assert!(Trim { start_ms: 5_000, end_ms: None }.is_valid());
        assert!(!Trim { start_ms: 5_000, end_ms: Some(5_500) }.is_valid());
    }
}
