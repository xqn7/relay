//! Keeps the PC from going to sleep while music plays. Only the system is kept awake, so the
//! screen still turns off on its usual timer. A power request (not SetThreadExecutionState)
//! because it belongs to a handle, not to whichever thread happened to make the call.

use std::sync::Mutex;
use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Power::{PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest};
use windows::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
use windows::Win32::System::Threading::{POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0};

#[derive(Default)]
pub struct KeepAwake(Mutex<Request>);

#[derive(Default)]
struct Request {
    /// The request handle as a number (handles aren't Send); created on first use.
    handle: Option<isize>,
    on: bool,
}

fn create() -> Option<isize> {
    // Shown by `powercfg /requests`, so it says who keeps the PC up and why.
    let mut reason: Vec<u16> = "Relay is playing music".encode_utf16().chain(Some(0)).collect();
    let context = REASON_CONTEXT {
        Version: POWER_REQUEST_CONTEXT_VERSION,
        Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
        Reason: REASON_CONTEXT_0 { SimpleReasonString: PWSTR(reason.as_mut_ptr()) },
    };
    // SAFETY: the reason string outlives the call, which copies it.
    unsafe { PowerCreateRequest(&context) }.ok().map(|h| h.0 as isize)
}

impl KeepAwake {
    pub fn set(&self, on: bool) {
        let mut req = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if req.on == on {
            return;
        }
        if req.handle.is_none() {
            req.handle = create();
        }
        let Some(h) = req.handle else { return };
        let handle = HANDLE(h as *mut _);
        // SAFETY: `handle` is the live power request created above; it's never closed.
        let result = unsafe {
            if on {
                PowerSetRequest(handle, PowerRequestSystemRequired)
            } else {
                PowerClearRequest(handle, PowerRequestSystemRequired)
            }
        };
        match result {
            Ok(()) => req.on = on,
            Err(e) => eprintln!("keep-awake request failed: {e}"),
        }
    }
}
