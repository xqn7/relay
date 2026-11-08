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
