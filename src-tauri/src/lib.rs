mod accounts;
mod audio_devices;
mod backup;
mod commands;
mod covers;
mod db;
mod error;
mod http;
mod importer;
mod library;
mod library_commands;
mod local;
mod m3u;
mod matcher;
mod matching;
mod media;
mod network;
mod resolver;
mod sources;
mod storage;
mod storage_commands;
mod streams;
mod tools;
mod util;

use accounts::Account;
use db::Db;
use http::Http;
use media::MediaControls;
use sources::{soundcloud::SoundCloud, youtube::YouTube};
use std::collections::HashMap;
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
    pub artist_cache: Mutex<HashMap<String, (Instant, sources::ArtistPage)>>,
}

/// Only the app's own pages may load in the window; everything else opens in the browser.
fn is_app_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => matches!(url.host_str(), Some("tauri.localhost")) || (cfg!(dev) && url.host_str() == Some("localhost")),
        _ => false,
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = app.path().app_data_dir()?;
    let local_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&local_dir)?;

    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Relay")
        .inner_size(1280.0, 800.0)
        .min_inner_size(940.0, 600.0)
        .decorations(false)
        .center()
        .background_color(tauri::window::Color(10, 10, 11, 255))
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        // WebView2 would otherwise register its own media session and grab the media keys,
        // pausing the <audio> elements behind the app's back. Relay's own SMTC session handles
        // them instead. The first three flags are wry's defaults, which this setting replaces.
        .additional_browser_args(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,HardwareMediaKeyHandling,MediaSessionService",
        )
        .on_navigation(is_app_url)
        .build()?;

    let db = Arc::new(Db::open(&data_dir.join("relay.db"))?);
    let http = Arc::new(Http::new());
    let tools = Arc::new(Tools::new(local_dir.join("tools"), http.clone(), handle.clone()));
    let state = AppState {
        local: local::LocalMusic::new(&handle, &db)?,
        imports: importer::Imports::default(),
        storage: storage::Storage::new(&local_dir)?,
        net: network::Network::new(),
        artist_cache: Mutex::new(HashMap::new()),
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
    network::watch(&handle);
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
        });
    }

    // Closing asks the UI to save the queue and position first; if the UI doesn't answer
    // within a moment (e.g. it crashed), quit anyway.
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
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = handle.emit("app-closing", ());
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                handle.exit(0);
            });
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .register_asynchronous_uri_scheme_protocol("relay", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let path = request.uri().path();
                let response = if path.starts_with("/img/") {
                    covers::serve(state, request).await
                } else if path.starts_with("/art/") {
                    covers::serve_local(&state, path)
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Relay");
}
