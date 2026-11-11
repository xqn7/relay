//! Everything outside the main window that follows the player: the media overlay, taskbar
//! buttons, keep-awake, tray, Discord, overlays and Game mode. The UI reports the song and
//! play state; this fans it out. Also start with Windows.

use crate::discord::{self, Discord};
use crate::lockscreen::LockScreenArt;
use crate::media::NowPlaying;
use crate::overlays::{self, Overlays};
use crate::power::KeepAwake;
use crate::tray::{self, Tray};
use crate::AppState;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Relay";
/// Passed by the Windows startup entry: start hidden in the tray.
pub const TRAY_ARG: &str = "--tray";

pub struct Desktop {
    pub now: Mutex<Option<NowPlaying>>,
    pub playing: AtomicBool,
    /// Position and length at the last report, for Discord's timeline.
    position: Mutex<(u64, u64)>,
    /// A fullscreen game is in front and Game mode is on.
    pub game: AtomicBool,
    pub awake: KeepAwake,
    pub overlays: Overlays,
    pub tray: Tray,
    pub discord: Discord,
    pub lockscreen: LockScreenArt,
    /// Started by Windows at sign-in, hidden in the tray.
    pub started_in_tray: bool,
    /// Artist photos for Discord's corner badge, by YouTube Music artist id.
    artist_photos: Mutex<HashMap<String, Option<String>>>,
}

impl Desktop {
    pub fn new(data_dir: &Path, started_in_tray: bool) -> Self {
        Self {
            now: Mutex::new(None),
            playing: AtomicBool::new(false),
            position: Mutex::new((0, 0)),
            game: AtomicBool::new(false),
            awake: KeepAwake::default(),
            overlays: Overlays::default(),
            tray: Tray::default(),
            discord: Discord::default(),
            lockscreen: LockScreenArt::new(data_dir),
            started_in_tray,
            artist_photos: Mutex::new(HashMap::new()),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn on_track(state: &Arc<AppState>, np: NowPlaying) {
    state.media.set_track(&np);
    *state.desktop.position.lock().unwrap_or_else(|p| p.into_inner()) = (0, np.duration_ms);
    *state.desktop.now.lock().unwrap_or_else(|p| p.into_inner()) = Some(np);
    tray::refresh(&state.app);
    refresh_discord(state);
}

pub fn on_state(state: &Arc<AppState>, playing: bool, position_ms: u64, duration_ms: u64) {
    state.media.set_playing(playing);
    state.media.set_position(position_ms, duration_ms);
    let d = &state.desktop;
    let was = d.playing.swap(playing, Ordering::SeqCst);
    *d.position.lock().unwrap_or_else(|p| p.into_inner()) = (position_ms, duration_ms);
    if was != playing {
        d.awake.set(playing);
        crate::taskbar::set_playing(&state.app, playing);
        tray::refresh(&state.app);
    }
    refresh_discord(state);
}

fn discord_options(state: &AppState) -> Option<discord::Options> {
    let b = |key: &str, default: bool| state.db.pref::<bool>(key).unwrap_or(default);
    if discord::APP_ID.is_empty() || !b("discord", true) {
        return None;
    }
    Some(discord::Options {
        format: state.db.pref::<String>("discordFormat").unwrap_or_else(|| "title".into()),
        timeline: b("discordTimeline", true),
        artwork: b("discordArtwork", true),
        artist_badge: b("discordArtistBadge", false),
        show_paused: b("discordShowPaused", false),
        buttons: [b("discordButtonYoutube", true), b("discordButtonSoundcloud", true), b("discordButtonSpotify", true)],
    })
}

/// The activity Discord would show right now (also the settings page's preview).
pub fn discord_activity(state: &Arc<AppState>, force_on: bool) -> Option<serde_json::Value> {
    let options = match discord_options(state) {
        Some(o) => o,
        None if force_on => discord::Options {
            format: "title".into(),
            timeline: true,
            artwork: true,
            artist_badge: false,
            show_paused: false,
            buttons: [true; 3],
        },
        None => return None,
    };
    let np = state.desktop.now.lock().unwrap_or_else(|p| p.into_inner()).clone()?;
    let playing = state.desktop.playing.load(Ordering::SeqCst);
    let (pos, _) = *state.desktop.position.lock().unwrap_or_else(|p| p.into_inner());
    let photo = if options.artist_badge { artist_photo(state, np.artist_id.as_deref()) } else { None };
    let labels = state.desktop.tray.labels.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let texts = discord::Texts {
        paused: &labels.paused,
        listen: [&labels.listen_youtube, &labels.listen_soundcloud, &labels.listen_spotify],
    };
    discord::activity(&np, playing, pos, now_ms(), &options, photo.as_deref(), &texts)
}

pub fn refresh_discord(state: &Arc<AppState>) {
    let activity = if discord_options(state).is_some() { discord_activity(state, false) } else { None };
    state.desktop.discord.set(activity);
}

/// The artist's photo if it's known; otherwise looked up once in the background, and Discord
/// updated when it arrives.
fn artist_photo(state: &Arc<AppState>, artist_id: Option<&str>) -> Option<String> {
    let id = artist_id?.to_string();
    if let Some(known) = state.desktop.artist_photos.lock().unwrap_or_else(|p| p.into_inner()).get(&id) {
        return known.clone();
    }
    state.desktop.artist_photos.lock().unwrap_or_else(|p| p.into_inner()).insert(id.clone(), None);
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(page) = crate::library_commands::cached_artist(&state, crate::sources::YOUTUBE, &id).await {
            state.desktop.artist_photos.lock().unwrap_or_else(|p| p.into_inner()).insert(id, page.avatar);
            refresh_discord(&state);
        }
    });
    None
}

pub fn set_game(state: &Arc<AppState>, active: bool) {
    if state.desktop.game.swap(active, Ordering::SeqCst) == active {
        return;
    }
    let _ = state.app.emit("game-mode", active);
    if !active {
        // Downloads paused for the game pick up again.
        crate::storage::sync(state);
    }
}

pub fn is_game(state: &AppState) -> bool {
    state.desktop.game.load(Ordering::SeqCst)
}

// ---- start with Windows ----------------------------------------------------------------------

/// Adds or removes the Windows startup entry. Re-written on every start while on, so it keeps
/// pointing at the installed exe after an update.
pub fn sync_autostart(state: &AppState) {
    let on = state.db.pref::<bool>("startWithWindows").unwrap_or(false);
    let result = if on {
        match std::env::current_exe() {
            Ok(exe) => crate::winreg::set_user_string(RUN_KEY, RUN_VALUE, &format!("\"{}\" {TRAY_ARG}", exe.display())),
            Err(e) => Err(windows::core::Error::new(windows::core::HRESULT(-1), e.to_string())),
        }
    } else {
        crate::winreg::delete_user_value(RUN_KEY, RUN_VALUE)
    };
    if let Err(e) = result {
        eprintln!("start with Windows entry failed: {e}");
    }
}

// ---- settings ---------------------------------------------------------------------------------

/// Applies a changed PC setting that something outside the page depends on.
pub fn on_setting(state: &Arc<AppState>, key: &str) {
    let app = &state.app;
    match key {
        "miniPlayer" => {
            crate::os_events::set_shortcut(mini_shortcut(state));
            overlays::apply_mini_settings(app);
            tray::refresh(app);
        }
        "miniShortcut" => crate::os_events::set_shortcut(mini_shortcut(state)),
        "miniPosition" => overlays::reset_position(app, overlays::MINI),
        "miniOnTop" => overlays::apply_mini_settings(app),
        "desktopLyrics" => overlays::sync_lyrics(app),
        "lyricsPosition" => overlays::reset_position(app, overlays::LYRICS),
        "lockScreen" => {
            if !state.db.pref::<bool>("lockScreen").unwrap_or(false) {
                let _ = state.desktop.lockscreen.restore();
            }
        }
        "gameMode" => crate::os_events::recheck_game(),
        "startWithWindows" => sync_autostart(state),
        k if k.starts_with("discord") => refresh_discord(state),
        _ => {}
    }
}

/// The shortcut to register: none while the mini player is off.
pub fn mini_shortcut(state: &AppState) -> Option<crate::os_events::Shortcut> {
    if !state.db.pref::<bool>("miniPlayer").unwrap_or(true) {
        return None;
    }
    let text = state.db.pref::<String>("miniShortcut").unwrap_or_else(|| "Alt+Shift+O".into());
    crate::os_events::parse_shortcut(&text)
}

/// Last things before the process ends: the user's own lock screen picture comes back.
pub fn before_exit(state: &AppState) {
    state.media.clear();
    if let Err(e) = state.desktop.lockscreen.restore() {
        eprintln!("lock screen restore failed: {e}");
    }
}
