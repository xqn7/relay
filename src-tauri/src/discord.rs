//! Discord Rich Presence over Discord's local IPC pipe, without an SDK: a handshake, then
//! SET_ACTIVITY frames. A frame is an opcode and a payload length (both u32 little-endian)
//! followed by JSON. Nothing is retried on a timer: a missing Discord is tried again with the
//! next change (song, play/pause, seek).

use crate::media::NowPlaying;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::sync::watch;

/// Relay's Discord application (its name is what Discord shows as "Listening to …"). Public,
/// not a secret. Empty would mean Rich Presence reports itself as not set up.
pub const APP_ID: &str = "1570104931858382918";
const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// Discord allows about 5 activity updates per 20 s; changes in between are merged.
const MIN_SPACING: Duration = Duration::from_secs(4);
const MAX_FRAME: usize = 64 * 1024;

pub const STATUS_OFF: u8 = 0;
pub const STATUS_NOT_RUNNING: u8 = 1;
pub const STATUS_CONNECTED: u8 = 2;

/// The Rich Presence settings, read from the PC's preferences.
#[derive(Clone, PartialEq, Debug)]
pub struct Options {
    /// "title", "artist" or "both": what Discord shows as the main line.
    pub format: String,
    pub timeline: bool,
    pub artwork: bool,
    pub artist_badge: bool,
    pub show_paused: bool,
    pub buttons: [bool; 3],
}

/// Texts in the UI's language.
pub struct Texts<'a> {
    pub paused: &'a str,
    pub listen: [&'a str; 3],
}

/// Discord needs 2–128 characters per text field.
fn field(text: &str) -> String {
    let s: String = text.trim().chars().take(128).collect();
    format!("{s:<2}")
}

fn image(url: Option<&str>) -> Option<&str> {
    url.filter(|u| u.starts_with("https://") && u.len() <= 256)
}

/// The activity for the playing song, or None to show nothing. `now_ms` is Unix time.
pub fn activity(np: &NowPlaying, playing: bool, position_ms: u64, now_ms: u64, o: &Options, artist_photo: Option<&str>, t: &Texts) -> Option<Value> {
    if !playing && !o.show_paused {
        return None;
    }
    let artist = if np.artist.is_empty() { "-" } else { np.artist.as_str() };
    let (details, mut state, display) = match o.format.as_str() {
        "artist" => (np.title.clone(), artist.to_string(), 1),
        "both" => (format!("{} – {}", np.title, artist), np.album.clone().unwrap_or_else(|| artist.to_string()), 2),
        _ => (np.title.clone(), artist.to_string(), 2),
    };
    if !playing {
        state = format!("{state} · {}", t.paused);
    }
    let mut act = json!({
        "type": 2,
        "status_display_type": display,
        "details": field(&details),
        "state": field(&state),
    });
    if playing && o.timeline && np.duration_ms > 0 {
        let start = now_ms.saturating_sub(position_ms);
        act["timestamps"] = json!({ "start": start, "end": start + np.duration_ms });
    }
    let mut assets = serde_json::Map::new();
    if let Some(cover) = image(np.cover.as_deref()).filter(|_| o.artwork) {
        assets.insert("large_image".into(), json!(cover));
        assets.insert("large_text".into(), json!(field(np.album.as_deref().unwrap_or(&np.title))));
    }
    if let Some(photo) = image(artist_photo).filter(|_| o.artist_badge && o.artwork) {
        assets.insert("small_image".into(), json!(photo));
        assets.insert("small_text".into(), json!(field(artist)));
    }
    if !assets.is_empty() {
        act["assets"] = Value::Object(assets);
    }
    let links = [&np.links.youtube, &np.links.soundcloud, &np.links.spotify];
    let buttons: Vec<Value> = links
        .iter()
        .zip(t.listen)
        .zip(o.buttons)
        .filter_map(|((link, label), on)| {
            let url = link.as_deref().filter(|u| on && u.starts_with("https://") && u.len() <= 512)?;
            Some(json!({ "label": label.chars().take(32).collect::<String>(), "url": url }))
        })
        .take(2)
        .collect();
    if !buttons.is_empty() {
        act["buttons"] = Value::Array(buttons);
    }
    Some(act)
}

/// Two activities that differ only by a little clock drift are the same to a viewer.
pub fn same(a: &Option<Value>, b: &Option<Value>) -> bool {
    let strip = |v: &Option<Value>| {
        v.clone().map(|mut v| {
            let start = v.get("timestamps").and_then(|t| t["start"].as_u64());
            if let Some(obj) = v.as_object_mut() {
                obj.remove("timestamps");
            }
            (v, start)
        })
    };
    match (strip(a), strip(b)) {
        (None, None) => true,
        (Some((va, sa)), Some((vb, sb))) => {
            va == vb
                && match (sa, sb) {
                    (Some(x), Some(y)) => x.abs_diff(y) < 2000,
                    (x, y) => x == y,
                }
        }
        _ => false,
    }
}

pub struct Discord {
    tx: watch::Sender<Option<Value>>,
    status: std::sync::Arc<AtomicU8>,
}

impl Default for Discord {
    fn default() -> Self {
        let (tx, rx) = watch::channel(None);
        let status = std::sync::Arc::new(AtomicU8::new(STATUS_OFF));
        if !APP_ID.is_empty() {
            tauri::async_runtime::spawn(worker(rx, status.clone()));
        }
        Self { tx, status }
    }
}

impl Discord {
    pub fn set(&self, activity: Option<Value>) {
        self.tx.send_if_modified(|cur| {
            let changed = !same(cur, &activity);
            if changed {
                *cur = activity;
            }
            changed
        });
    }

    pub fn status(&self) -> u8 {
        self.status.load(Ordering::SeqCst)
    }
}

async fn worker(mut rx: watch::Receiver<Option<Value>>, status: std::sync::Arc<AtomicU8>) {
    let mut pipe: Option<NamedPipeClient> = None;
    loop {
        let activity = rx.borrow_and_update().clone();
        if pipe.is_none() && activity.is_some() {
            pipe = connect().await;
            status.store(if pipe.is_some() { STATUS_CONNECTED } else { STATUS_NOT_RUNNING }, Ordering::SeqCst);
        }
        if let Some(p) = pipe.as_mut() {
            if let Err(e) = send_activity(p, &activity).await {
                eprintln!("Discord update failed: {e}");
                pipe = None;
                status.store(STATUS_NOT_RUNNING, Ordering::SeqCst);
            }
        }
        if activity.is_none() {
            // Nothing to show: let go of Discord until there is.
            pipe = None;
            status.store(STATUS_OFF, Ordering::SeqCst);
        }
        tokio::time::sleep(MIN_SPACING).await;
        if rx.changed().await.is_err() {
            return;
        }
    }
}

async fn connect() -> Option<NamedPipeClient> {
    for i in 0..10 {
        let Ok(mut pipe) = ClientOptions::new().open(format!(r"\\.\pipe\discord-ipc-{i}")) else { continue };
        let hello = json!({ "v": 1, "client_id": APP_ID });
        if write_frame(&mut pipe, OP_HANDSHAKE, &hello).await.is_ok() && read_frame(&mut pipe).await.is_ok_and(|(op, _)| op == OP_FRAME) {
            return Some(pipe);
        }
    }
    None
}

async fn send_activity(pipe: &mut NamedPipeClient, activity: &Option<Value>) -> std::io::Result<()> {
    let msg = json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": crate::util::uuid_v4(),
    });
    write_frame(pipe, OP_FRAME, &msg).await?;
    let (op, body) = read_frame(pipe).await?;
    if op == OP_CLOSE {
        return Err(std::io::Error::other("Discord closed the connection"));
    }
    if body.get("evt").and_then(Value::as_str) == Some("ERROR") {
        eprintln!("Discord rejected the activity: {}", body["data"]);
    }
    Ok(())
}

async fn write_frame(pipe: &mut NamedPipeClient, op: u32, body: &Value) -> std::io::Result<()> {
    let payload = body.to_string().into_bytes();
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&payload);
    tokio::time::timeout(IO_TIMEOUT, pipe.write_all(&frame)).await.map_err(|_| std::io::Error::other("Discord timed out"))?
}

async fn read_frame(pipe: &mut NamedPipeClient) -> std::io::Result<(u32, Value)> {
    let read = async {
        let mut head = [0u8; 8];
        pipe.read_exact(&mut head).await?;
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if len > MAX_FRAME {
            return Err(std::io::Error::other("Discord frame too large"));
        }
        let mut body = vec![0u8; len];
        pipe.read_exact(&mut body).await?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    };
    tokio::time::timeout(IO_TIMEOUT, read).await.map_err(|_| std::io::Error::other("Discord timed out"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::SongLinks;

    fn song() -> NowPlaying {
        NowPlaying {
            title: "Stand By Me".into(),
            artist: "Ben E. King".into(),
            album: Some("Don't Play That Song".into()),
            cover: Some("https://lh3.googleusercontent.com/abc=w544-h544".into()),
            duration_ms: 180_000,
            artist_id: None,
            links: SongLinks {
                youtube: Some("https://music.youtube.com/watch?v=hwZNL7QVJjE".into()),
                soundcloud: None,
                spotify: Some("https://open.spotify.com/track/3SdTKo2uVsxFblQjpScoHy".into()),
            },
        }
    }

    fn opts() -> Options {
        Options { format: "title".into(), timeline: true, artwork: true, artist_badge: false, show_paused: false, buttons: [true; 3] }
    }

    const TEXTS: Texts = Texts { paused: "Paused", listen: ["Listen on YouTube", "Listen on SoundCloud", "Listen on Spotify"] };

    #[test]
    fn playing_song_has_timeline_cover_and_buttons() {
        let a = activity(&song(), true, 30_000, 1_000_000, &opts(), None, &TEXTS).unwrap();
        assert_eq!(a["details"], "Stand By Me");
        assert_eq!(a["state"], "Ben E. King");
        assert_eq!(a["status_display_type"], 2);
        assert_eq!(a["timestamps"]["start"], 970_000);
        assert_eq!(a["timestamps"]["end"], 1_150_000);
        assert_eq!(a["assets"]["large_image"], "https://lh3.googleusercontent.com/abc=w544-h544");
        let buttons = a["buttons"].as_array().unwrap();
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[1]["label"], "Listen on Spotify");
    }

    #[test]
    fn paused_song_is_hidden_or_marked() {
        assert!(activity(&song(), false, 0, 0, &opts(), None, &TEXTS).is_none());
        let o = Options { show_paused: true, ..opts() };
        let a = activity(&song(), false, 0, 0, &o, None, &TEXTS).unwrap();
        assert_eq!(a["state"], "Ben E. King · Paused");
        assert!(a.get("timestamps").is_none());
    }

    #[test]
    fn options_change_what_is_shown() {
        let o = Options { format: "both".into(), timeline: false, artwork: false, buttons: [false; 3], ..opts() };
        let a = activity(&song(), true, 0, 0, &o, Some("https://x/y.jpg"), &TEXTS).unwrap();
        assert_eq!(a["details"], "Stand By Me – Ben E. King");
        assert_eq!(a["state"], "Don't Play That Song");
        assert!(a.get("timestamps").is_none() && a.get("assets").is_none() && a.get("buttons").is_none());
        let o = Options { format: "artist".into(), artist_badge: true, ..opts() };
        let a = activity(&song(), true, 0, 0, &o, Some("https://x/y.jpg"), &TEXTS).unwrap();
        assert_eq!(a["status_display_type"], 1);
        assert_eq!(a["assets"]["small_image"], "https://x/y.jpg");
    }

    #[test]
    fn local_covers_and_short_texts_are_handled() {
        let mut s = song();
        s.cover = Some("http://relay.localhost/art/x.jpg".into());
        s.title = "X".into();
        let a = activity(&s, true, 0, 0, &opts(), None, &TEXTS).unwrap();
        assert!(a.get("assets").is_none());
        assert_eq!(a["details"].as_str().unwrap().chars().count(), 2);
    }

    #[test]
    fn clock_drift_is_not_a_change() {
        let a = activity(&song(), true, 30_000, 1_000_000, &opts(), None, &TEXTS);
        let b = activity(&song(), true, 35_000, 1_005_500, &opts(), None, &TEXTS);
        let seeked = activity(&song(), true, 90_000, 1_005_500, &opts(), None, &TEXTS);
        assert!(same(&a, &b));
        assert!(!same(&a, &seeked));
        assert!(!same(&a, &None));
    }
}
