mod accounts;
mod audio_devices;
mod backup;
mod commands;
mod covers;
mod db;
mod desktop;
mod desktop_commands;
mod discord;
mod error;
mod export;
mod hardware;
mod history;
mod http;
mod importer;
mod library;
mod library_commands;
mod local;
mod lockscreen;
mod lyrics;
mod lyrics_commands;
mod m3u;
mod matcher;
mod matching;
mod media;
mod mixes;
mod network;
mod os_events;
mod overlays;
mod personal_commands;
mod power;
mod profile;
mod resolver;
mod secrets;
mod sound;
mod sound_commands;
mod sources;
mod storage;
mod storage_commands;
mod streams;
mod taskbar;
mod tools;
mod translate;
mod tray;
mod updater;
mod util;
mod versions;
mod winreg;
mod youtube_account;

use accounts::Account;
use db::Db;
use http::Http;
use media::MediaControls;
use sources::{soundcloud::SoundCloud, youtube::YouTube};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use streams::Streams;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tools::Tools;

pub struct AppState {
    pub app: AppHandle,
    pub db: Arc<Db>,
    pub http: Arc<Http>,
    pub yt: YouTube,
    pub sc: SoundCloud,
    pub streams: Streams,
    pub media: MediaControls,
    pub covers: covers::Covers,
    pub account: Mutex<Option<Account>>,
    pub tools: Arc<Tools>,
    pub local: local::LocalMusic,
    pub imports: importer::Imports,
    pub storage: storage::Storage,
    pub net: network::Network,
    pub lyrics_locks: lyrics::Locks,
    pub artist_cache: Mutex<HashMap<String, (Instant, sources::ArtistPage)>>,
    pub waveform_lock: tokio::sync::Mutex<()>,
    pub sponsor: sound::Sponsor,
    /// The last MP3 export, for "Show in folder".
    pub last_export: Mutex<Option<std::path::PathBuf>>,
    /// Private session: nothing is recorded while on. Never saved, so a restart ends it.
    pub private: AtomicBool,
    /// Profile pictures and background images.
    pub user_dir: std::path::PathBuf,
    pub discover: mixes::DiscoverCache,
    pub desktop: desktop::Desktop,
    pub updater: updater::Updater,
    pub youtube_account: youtube_account::YouTubeAccount,
}

/// WebView2 would otherwise register its own media session and grab the media keys, pausing
/// the <audio> elements behind the app's back; Relay's own SMTC session handles them instead.
/// The first three flags are wry's defaults, which this setting replaces. Every window must
/// pass exactly these: WebView2 refuses to create a webview whose browser arguments differ from
/// the ones the shared browser process started with.
pub(crate) const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,HardwareMediaKeyHandling,MediaSessionService";

const FFMPEG_PREFETCH_DELAY: Duration = Duration::from_secs(60);

/// Only the app's own pages may load in the window; everything else opens in the browser.
pub(crate) fn is_app_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => matches!(url.host_str(), Some("tauri.localhost")) || (cfg!(dev) && url.host_str() == Some("localhost")),
        _ => false,
    }
}

/// Brings the main window forward from anywhere (tray, mini player, second launch).
pub(crate) fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Quitting asks the UI to save the queue and position first; if the UI doesn't answer within
/// a moment (e.g. it crashed), Relay quits anyway.
pub(crate) fn request_quit(app: &AppHandle) {
    let _ = app.emit("app-closing", ());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let state = app.state::<Arc<AppState>>().inner().clone();
        let _ = tokio::task::spawn_blocking(move || desktop::before_exit(&state)).await;
        app.exit(0);
    });
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = app.path().app_data_dir()?;
    let local_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&local_dir)?;

    let in_tray = std::env::args().any(|a| a == desktop::TRAY_ARG);
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Relay")
        .inner_size(1280.0, 800.0)
        .min_inner_size(940.0, 600.0)
        .decorations(false)
        .center()
        .background_color(tauri::window::Color(10, 10, 11, 255))
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .additional_browser_args(BROWSER_ARGS)
        .on_navigation(is_app_url)
        // Hidden until the page is there: a new WebView2 paints one white frame before its
        // background color applies, which flashes on screen at startup.
        .visible(false)
        .on_page_load(move |window, payload| {
            // Started with Windows: stays in the tray (no window, no startup animation).
            if payload.event() == tauri::webview::PageLoadEvent::Finished && !in_tray {
                let _ = window.show();
            }
        })
        .build()?;
    taskbar::attach(&handle, window.hwnd()?);

    let db = Arc::new(Db::open(&data_dir.join("relay.db"))?);
    let http = Arc::new(Http::new());
    let tools = Arc::new(Tools::new(local_dir.join("tools"), http.clone()));
    let state = AppState {
        local: local::LocalMusic::new(&handle, &db)?,
        imports: importer::Imports::default(),
        storage: storage::Storage::new(&local_dir)?,
        net: network::Network::new(),
        lyrics_locks: lyrics::Locks::default(),
        artist_cache: Mutex::new(HashMap::new()),
        waveform_lock: tokio::sync::Mutex::new(()),
        sponsor: sound::Sponsor::default(),
        last_export: Mutex::new(None),
        private: AtomicBool::new(false),
        user_dir: profile::dir(&data_dir),
        discover: mixes::DiscoverCache::default(),
        desktop: desktop::Desktop::new(&data_dir, in_tray),
        updater: updater::Updater::new(&local_dir),
        youtube_account: youtube_account::YouTubeAccount::load(),
        yt: YouTube::new(&local_dir.join("youtube"), tools.clone())?,
        tools,
        sc: SoundCloud::new(http.clone(), db.clone()),
        streams: Streams::new(local_dir.join("buffer"))?,
        media: MediaControls::attach(&handle, window.hwnd()?),
        account: Mutex::new(None),
        covers: covers::Covers::default(),
        app: handle.clone(),
        db,
        http,
    };
    let state = Arc::new(state);
    app.manage(state.clone());
    audio_devices::watch(handle.clone());
    audio_devices::allow_output_names(&window);
    network::watch(&handle);
    if let Err(e) = tray::create(&handle) {
        eprintln!("tray icon failed: {e}");
    }
    os_events::start(handle.clone(), desktop::mini_shortcut(&state));
    desktop::sync_autostart(&state);
    youtube_account::sync_tools(&state);
    overlays::sync_lyrics(&handle);
    updater::check_later(&state);
    {
        // After a crash, Relay's picture may still be on the lock screen.
        let state = state.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _ = state.desktop.lockscreen.restore();
        });
    }
    tauri::async_runtime::spawn(local::start(state.clone()));
    {
        // Startup housekeeping for stored songs, then continue any unfinished downloads.
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            let s = state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                storage::tidy(&s);
                storage::enforce_cache_limit(&s);
            })
            .await;
            network::probe(&state, true).await;
            storage::sync(&state);
            backup::schedule(&state);
            // ffmpeg a minute in, so the first video import or MP3 export doesn't wait for it.
            tokio::time::sleep(FFMPEG_PREFETCH_DELAY).await;
            if state.net.online() && !desktop::is_game(&state) {
                state.tools.prefetch_ffmpeg();
            }
        });
    }

    window.on_window_event(move |event| {
        // Files dragged in from anywhere go to Local music. The page only shows the reaction;
        // the paths never pass through the webview.
        if let tauri::WindowEvent::DragDrop(drop) = event {
            let kind = match drop {
                tauri::DragDropEvent::Enter { .. } => "enter",
                tauri::DragDropEvent::Over { .. } => return,
                tauri::DragDropEvent::Drop { paths, .. } => {
                    local::add(state.clone(), paths.clone());
                    "drop"
                }
                _ => "leave",
            };
            let _ = handle.emit("file-drag", kind);
        }
        if let tauri::WindowEvent::Resized(_) = event {
            if let Some(main) = handle.get_webview_window("main") {
                overlays::main_minimized(&handle, main.is_minimized().unwrap_or(false));
            }
        }
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            request_quit(&handle);
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch (desktop shortcut, Start menu) brings the running player forward
        // instead of starting another one that would fight over the database and audio.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .register_asynchronous_uri_scheme_protocol("relay", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let path = request.uri().path();
                let response = if path.starts_with("/img/") {
                    covers::serve(state, request).await
                } else if path.starts_with("/art/") {
                    covers::serve_local(&state, path)
                } else if path.starts_with("/user/") {
                    profile::serve(&state.user_dir, path)
                } else {
                    streams::serve(state, request).await
                };
                responder.respond(response);
            });
        })
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::auth_state,
            commands::sign_up,
            commands::sign_in,
            commands::reset_password,
            commands::sign_out,
            commands::settings_load,
            commands::setting_set,
            commands::search,
            commands::search_history,
            commands::search_history_add,
            commands::search_history_clear,
            commands::collection_tracks,
            commands::autoplay_tracks,
            commands::prepare_track,
            commands::prefetch_track,
            commands::queue_save,
            commands::queue_load,
            commands::media_track,
            commands::media_state,
            commands::open_external,
            commands::exit_app,
            library_commands::library_overview,
            library_commands::playlist_get,
            library_commands::playlist_create,
            library_commands::playlist_rename,
            library_commands::playlist_set_folder,
            library_commands::playlist_delete,
            library_commands::playlist_add,
            library_commands::playlist_remove,
            library_commands::playlist_restore_items,
            library_commands::playlist_move,
            library_commands::playlist_played,
            library_commands::playlist_merge,
            library_commands::folder_create,
            library_commands::folder_rename,
            library_commands::folder_set_parent,
            library_commands::folder_delete,
            library_commands::like_set,
            library_commands::liked_list,
            library_commands::liked_ids,
            library_commands::hidden_set,
            library_commands::hidden_list,
            library_commands::follow_set,
            library_commands::follows_list,
            library_commands::followed_releases,
            library_commands::artist_page,
            library_commands::artist_find,
            library_commands::album_page,
            library_commands::soundcloud_track,
            library_commands::trash_list,
            library_commands::trash_restore,
            library_commands::trash_delete,
            library_commands::trash_empty,
            library_commands::trash_purge_expired,
            library_commands::import_preview,
            library_commands::import_start,
            library_commands::import_cancel,
            library_commands::import_liked_links,
            library_commands::import_liked_file,
            library_commands::import_m3u_pick,
            library_commands::export_m3u,
            library_commands::match_candidates,
            library_commands::match_get,
            library_commands::match_set,
            library_commands::prematch,
            library_commands::local_list,
            library_commands::local_add_dialog,
            library_commands::local_edit,
            library_commands::local_remove,
            library_commands::local_folder,
            library_commands::local_folder_pick,
            library_commands::local_open_folder,
            storage_commands::offline_marks,
            storage_commands::offline_set,
            storage_commands::playlist_offline_set,
            storage_commands::offline_sync,
            storage_commands::storage_available,
            storage_commands::offline_lists,
            storage_commands::download_status,
            storage_commands::storage_usage,
            storage_commands::storage_clear,
            storage_commands::storage_open,
            storage_commands::net_status,
            storage_commands::net_check,
            storage_commands::connection_check,
            storage_commands::backup_create,
            storage_commands::backup_restore,
            storage_commands::backup_folder_pick,
            storage_commands::cover_hd,
            storage_commands::cover_save,
            storage_commands::cover_drag_prepare,
            storage_commands::cover_drag_start,
            storage_commands::local_use_hd_cover,
            lyrics_commands::lyrics_get,
            lyrics_commands::lyrics_preload,
            lyrics_commands::lyrics_translate,
            lyrics_commands::lyrics_save,
            lyrics_commands::lyrics_reset,
            lyrics_commands::deepl_key_saved,
            lyrics_commands::deepl_key_set,
            library_commands::playlist_replace,
            sound_commands::trims_list,
            sound_commands::trim_set,
            sound_commands::waveform,
            sound_commands::sponsor_segments,
            sound_commands::other_versions,
            sound_commands::export_mp3,
            sound_commands::export_reveal,
            personal_commands::play_record,
            personal_commands::play_listened,
            personal_commands::play_counts,
            personal_commands::home_lists,
            personal_commands::home_mixes,
            personal_commands::discover,
            personal_commands::listening_stats,
            personal_commands::private_get,
            personal_commands::private_set,
            personal_commands::profile_image_set,
            personal_commands::profile_image_clear,
            personal_commands::profile_name_set,
            personal_commands::image_save,
            personal_commands::backgrounds_list,
            personal_commands::background_add,
            personal_commands::background_look_set,
            personal_commands::background_delete,
            personal_commands::speed_dial_list,
            personal_commands::speed_dial_set,
            personal_commands::speed_dial_order,
            desktop_commands::desktop_info,
            desktop_commands::main_window_show,
            desktop_commands::mini_toggle,
            desktop_commands::lyrics_unlock,
            desktop_commands::ui_labels,
            desktop_commands::hardware_info,
            desktop_commands::discord_preview,
            desktop_commands::lockscreen_set,
            desktop_commands::lockscreen_restore,
            desktop_commands::update_check,
            desktop_commands::update_install,
            desktop_commands::ytdlp_version,
            desktop_commands::ytdlp_update,
            desktop_commands::youtube_status,
            desktop_commands::youtube_sign_in,
            desktop_commands::youtube_sign_out,
            desktop_commands::youtube_playlists,
            desktop_commands::youtube_playlist_tracks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Relay");
}
