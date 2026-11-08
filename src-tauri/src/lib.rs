mod accounts;
mod audio_devices;
mod commands;
mod covers;
mod db;
mod error;
mod http;
mod matcher;
mod media;
mod resolver;
mod sources;
mod streams;
mod tools;
mod util;

use accounts::Account;
use db::Db;
use http::Http;
use media::MediaControls;
use sources::{soundcloud::SoundCloud, youtube::YouTube};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
    pub tools: Tools,
    pub media: MediaControls,
    pub covers: covers::Covers,
    pub account: Mutex<Option<Account>>,
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
        .disable_drag_drop_handler()
        .on_navigation(|url| is_app_url(url))
        .build()?;

    let db = Arc::new(Db::open(&data_dir.join("relay.db"))?);
    let http = Arc::new(Http::new());
    let state = AppState {
        yt: YouTube::new(&local_dir.join("youtube"))?,
        sc: SoundCloud::new(http.clone(), db.clone()),
        streams: Streams::new(local_dir.join("buffer"))?,
        tools: Tools::new(local_dir.join("tools")),
        media: MediaControls::attach(&handle, window.hwnd()?),
        account: Mutex::new(None),
        covers: covers::Covers::default(),
        app: handle.clone(),
        db,
        http,
    };
    app.manage(Arc::new(state));
    audio_devices::watch(handle.clone());

    // Closing asks the UI to save the queue and position first; if the UI doesn't answer
    // within a moment (e.g. it crashed), quit anyway.
    window.on_window_event(move |event| {
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
        .register_asynchronous_uri_scheme_protocol("relay", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let response = if request.uri().path().starts_with("/img/") {
                    covers::serve(state, request).await
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Relay");
}
