//! Windows media overlay (SMTC): shows the current song in the volume flyout / lock screen and
//! routes hardware media keys to the player.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use windows::core::{factory, Ref, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

pub struct MediaControls(Mutex<Option<SystemMediaTransportControls>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaKey {
    action: &'static str,
    position_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub cover: Option<String>,
    pub duration_ms: u64,
}

/// TimeSpan counts 100-nanosecond ticks.
fn span(ms: u64) -> TimeSpan {
    TimeSpan { Duration: ms as i64 * 10_000 }
}

impl MediaControls {
    pub fn attach(app: &AppHandle, hwnd: HWND) -> Self {
        match Self::create(app, hwnd) {
            Ok(smtc) => Self(Mutex::new(Some(smtc))),
            Err(e) => {
                eprintln!("media overlay unavailable: {e}");
                Self(Mutex::new(None))
            }
        }
    }

    fn create(app: &AppHandle, hwnd: HWND) -> windows::core::Result<SystemMediaTransportControls> {
        let interop = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };
        smtc.SetIsEnabled(true)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsNextEnabled(true)?;
        smtc.SetIsPreviousEnabled(true)?;
        smtc.SetIsStopEnabled(true)?;

        let handle = app.clone();
        smtc.ButtonPressed(&TypedEventHandler::new(
            move |_: Ref<SystemMediaTransportControls>, args: Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
                let action = match args.ok()?.Button()? {
                    SystemMediaTransportControlsButton::Play => "play",
                    SystemMediaTransportControlsButton::Pause => "pause",
                    SystemMediaTransportControlsButton::Stop => "pause",
                    SystemMediaTransportControlsButton::Next => "next",
                    SystemMediaTransportControlsButton::Previous => "previous",
                    _ => return Ok(()),
                };
                let _ = handle.emit("media-key", MediaKey { action, position_ms: None });
                Ok(())
            },
        ))?;

        let handle = app.clone();
        smtc.PlaybackPositionChangeRequested(&TypedEventHandler::new(
            move |_: Ref<SystemMediaTransportControls>, args: Ref<PlaybackPositionChangeRequestedEventArgs>| {
                let ticks = args.ok()?.RequestedPlaybackPosition()?.Duration.max(0) as u64;
                let _ = handle.emit("media-key", MediaKey { action: "seek", position_ms: Some(ticks / 10_000) });
                Ok(())
            },
        ))?;
        Ok(smtc)
    }

    fn with(&self, f: impl FnOnce(&SystemMediaTransportControls) -> windows::core::Result<()>) {
        let guard = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(smtc) = guard.as_ref() {
            // The overlay is cosmetic: a failure here must never affect playback.
            if let Err(e) = f(smtc) {
                eprintln!("media overlay update failed: {e}");
            }
        }
    }

    pub fn set_track(&self, np: &NowPlaying) {
        self.with(|smtc| {
            let du = smtc.DisplayUpdater()?;
            du.SetType(MediaPlaybackType::Music)?;
            let props = du.MusicProperties()?;
            props.SetTitle(&HSTRING::from(np.title.as_str()))?;
            props.SetArtist(&HSTRING::from(np.artist.as_str()))?;
            props.SetAlbumTitle(&HSTRING::from(np.album.as_deref().unwrap_or("")))?;
            match np.cover.as_deref().filter(|c| c.starts_with("https://")) {
                Some(url) => du.SetThumbnail(&RandomAccessStreamReference::CreateFromUri(&Uri::CreateUri(&HSTRING::from(url))?)?)?,
                None => du.SetThumbnail(None)?,
            }
            du.Update()
        });
        self.set_position(0, np.duration_ms);
    }

    pub fn set_playing(&self, playing: bool) {
        self.with(|smtc| {
            smtc.SetPlaybackStatus(if playing { MediaPlaybackStatus::Playing } else { MediaPlaybackStatus::Paused })
        });
    }

    pub fn set_position(&self, position_ms: u64, duration_ms: u64) {
        self.with(|smtc| {
            let tl = SystemMediaTransportControlsTimelineProperties::new()?;
            tl.SetStartTime(span(0))?;
            tl.SetMinSeekTime(span(0))?;
            tl.SetEndTime(span(duration_ms))?;
            tl.SetMaxSeekTime(span(duration_ms))?;
            tl.SetPosition(span(position_ms.min(duration_ms)))?;
            smtc.UpdateTimelineProperties(&tl)
        });
    }

    pub fn clear(&self) {
        self.with(|smtc| {
            smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;
            smtc.DisplayUpdater()?.ClearAll()
        });
    }
}
