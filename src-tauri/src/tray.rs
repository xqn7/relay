//! The system tray icon and its menu: play/pause, next, previous, private session, the mini
//! player, desktop lyrics lock, show Relay and quit. Labels come from the UI in its language.

use crate::AppState;
use serde::Deserialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

/// Menu and tooltip texts, sent by the UI whenever its language is known or changes.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    pub play: String,
    pub pause: String,
    pub next: String,
    pub previous: String,
    pub private_session: String,
    pub mini_player: String,
    pub unlock_lyrics: String,
    pub lock_lyrics: String,
    pub show: String,
    pub quit: String,
    pub paused: String,
    pub listen_youtube: String,
    pub listen_soundcloud: String,
    pub listen_spotify: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            play: "Play".into(),
            pause: "Pause".into(),
            next: "Next".into(),
            previous: "Previous".into(),
            private_session: "Private session".into(),
            mini_player: "Mini player".into(),
            unlock_lyrics: "Unlock desktop lyrics".into(),
            lock_lyrics: "Lock desktop lyrics".into(),
            show: "Show Relay".into(),
            quit: "Quit Relay".into(),
            paused: "Paused".into(),
            listen_youtube: "Listen on YouTube".into(),
            listen_soundcloud: "Listen on SoundCloud".into(),
            listen_spotify: "Listen on Spotify".into(),
        }
    }
}

struct Items {
    _icon: TrayIcon<Wry>,
    toggle: MenuItem<Wry>,
    next: MenuItem<Wry>,
    previous: MenuItem<Wry>,
    private: CheckMenuItem<Wry>,
    mini: MenuItem<Wry>,
    lyrics: MenuItem<Wry>,
    show: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

#[derive(Default)]
pub struct Tray {
    items: Mutex<Option<Items>>,
    pub labels: Mutex<Labels>,
}

fn state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let l = Labels::default();
    let toggle = MenuItem::with_id(app, "toggle", &l.play, true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", &l.next, true, None::<&str>)?;
    let previous = MenuItem::with_id(app, "previous", &l.previous, true, None::<&str>)?;
    let private = CheckMenuItem::with_id(app, "private", &l.private_session, true, false, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", &l.mini_player, true, None::<&str>)?;
    let lyrics = MenuItem::with_id(app, "lyrics", &l.unlock_lyrics, false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", &l.show, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", &l.quit, true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&toggle, &next, &previous, &sep()?, &private, &mini, &lyrics, &sep()?, &show, &quit])?;
    let icon = TrayIconBuilder::with_id("relay")
        .icon(tauri::include_image!("icons/tray.png"))
        .tooltip("Relay")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    let st = state(app);
    *st.desktop.tray.items.lock().unwrap_or_else(|p| p.into_inner()) =
        Some(Items { _icon: icon, toggle, next, previous, private, mini, lyrics, show, quit });
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    let media = |action: &str| {
        let _ = app.emit("media-key", serde_json::json!({ "action": action, "positionMs": null }));
    };
    match id {
        "toggle" => media("toggle"),
        "next" => media("next"),
        "previous" => media("previous"),
        "private" => {
            let st = state(app);
            let on = !st.private.load(Ordering::Relaxed);
            st.private.store(on, Ordering::Relaxed);
            let _ = app.emit("private-changed", on);
            refresh(app);
        }
        "mini" => crate::overlays::toggle_mini(app),
        "lyrics" => {
            let unlocked = crate::overlays::lyrics_unlocked(&state(app));
            crate::overlays::set_lyrics_unlocked(app, !unlocked);
        }
        "show" => crate::show_main(app),
        "quit" => crate::request_quit(app),
        _ => {}
    }
}

/// Brings texts, checks and the tooltip in line with the current state.
pub fn refresh(app: &AppHandle) {
    let st = state(app);
    let l = st.desktop.tray.labels.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let guard = st.desktop.tray.items.lock().unwrap_or_else(|p| p.into_inner());
    let Some(items) = guard.as_ref() else { return };
    let playing = st.desktop.playing.load(Ordering::SeqCst);
    let lyrics_on = st.db.pref::<bool>("desktopLyrics").unwrap_or(false);
    let unlocked = crate::overlays::lyrics_unlocked(&st);
    let _ = items.toggle.set_text(if playing { &l.pause } else { &l.play });
    let _ = items.next.set_text(&l.next);
    let _ = items.previous.set_text(&l.previous);
    let _ = items.private.set_text(&l.private_session);
    let _ = items.private.set_checked(st.private.load(Ordering::Relaxed));
    let _ = items.mini.set_text(&l.mini_player);
    let _ = items.mini.set_enabled(st.db.pref::<bool>("miniPlayer").unwrap_or(true));
    let _ = items.lyrics.set_text(if unlocked { &l.lock_lyrics } else { &l.unlock_lyrics });
    let _ = items.lyrics.set_enabled(lyrics_on);
    let _ = items.show.set_text(&l.show);
    let _ = items.quit.set_text(&l.quit);
    let tip = st
        .desktop
        .now
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .map(|np| tooltip(&np.title, &np.artist))
        .unwrap_or_else(|| "Relay".into());
    let _ = items._icon.set_tooltip(Some(tip));
}

/// Windows cuts tray tooltips at 127 characters.
fn tooltip(title: &str, artist: &str) -> String {
    let text = if artist.is_empty() { title.to_string() } else { format!("{title} – {artist}") };
    if text.chars().count() <= 127 {
        return text;
    }
    let mut cut: String = text.chars().take(126).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::tooltip;

    #[test]
    fn tooltips_fit_windows_limit() {
        assert_eq!(tooltip("Song", "Artist"), "Song – Artist");
        assert_eq!(tooltip("Song", ""), "Song");
        let long = tooltip(&"x".repeat(200), "y");
        assert_eq!(long.chars().count(), 127);
        assert!(long.ends_with('…'));
    }
}
