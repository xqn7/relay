//! Tells the UI when Windows switches the default audio output (e.g. headphones unplugged),
//! so playback can pause instead of suddenly coming out of the speakers.

use tauri::{AppHandle, Emitter};
use windows::core::{Result, PCWSTR};
use windows_core::implement;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

#[implement(IMMNotificationClient)]
struct Watcher {
    app: AppHandle,
}

impl IMMNotificationClient_Impl for Watcher_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        // Windows reports one change per role; the console role is enough to react once.
        if flow == eRender && role == eConsole {
            let _ = self.app.emit("output-device-changed", ());
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

/// Registers the watcher on its own COM thread. The thread then sleeps for the app's lifetime,
/// so it costs no CPU; Windows calls back into it only when devices change.
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
                let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let client: IMMNotificationClient = Watcher { app }.into();
                enumerator.RegisterEndpointNotificationCallback(&client)?;
                loop {
                    std::thread::park();
                }
            }
        })();
        if let Err(e) = result {
            eprintln!("audio device watcher unavailable: {e}");
        }
    });
}

/// Chromium only lists audio outputs (ids and names) to pages that hold microphone permission.
/// Setting that permission for the app's own origin makes the output picker work without a
/// prompt; the microphone itself is never opened.
pub fn allow_output_names(window: &tauri::WebviewWindow) {
    let origin = if cfg!(dev) { "http://localhost:1420" } else { "http://tauri.localhost" };
    let _ = window.with_webview(move |webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Profile4, ICoreWebView2_13, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        };
        use windows::core::{Interface, HSTRING};
        let result = (|| -> Result<()> {
            unsafe {
                let core = webview.controller().CoreWebView2()?;
                let profile: ICoreWebView2Profile4 = core.cast::<ICoreWebView2_13>()?.Profile()?.cast()?;
                let done = webview2_com::SetPermissionStateCompletedHandler::create(Box::new(|_| Ok(())));
                profile.SetPermissionState(COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, &HSTRING::from(origin), COREWEBVIEW2_PERMISSION_STATE_ALLOW, &done)
            }
        })();
        if let Err(e) = result {
            eprintln!("audio output names unavailable: {e}");
        }
    });
}
