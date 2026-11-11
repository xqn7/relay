//! Reads the PC's hardware once (CPU, RAM, graphics, screen) and recommends a visual quality
//! from it. Only genuinely weak PCs get a lower level; normal and strong ones get High.

use serde::{Deserialize, Serialize};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

const VENDOR_INTEL: u32 = 0x8086;
const VENDOR_AMD: u32 = 0x1002;
const VENDOR_MICROSOFT: u32 = 0x1414;
const MIB: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub cpu: String,
    /// Logical processors (threads).
    pub cores: u32,
    pub ram_gb: u32,
    pub gpu: String,
    pub gpu_integrated: bool,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub quality: &'static str,
    /// Which explanation the UI shows: "weak", "integratedSmall", "integratedScreen", "strong".
    pub reason: &'static str,
}

pub fn recommend(h: &Hardware) -> Recommendation {
    let (quality, reason) = if h.ram_gb < 6 || h.cores < 4 {
        ("low", "weak")
    } else if h.gpu_integrated && (h.cores <= 4 || h.ram_gb <= 8) {
        ("medium", "integratedSmall")
    } else if h.gpu_integrated && u64::from(h.width) * u64::from(h.height) > 2560 * 1440 {
        ("medium", "integratedScreen")
    } else {
        ("high", "strong")
    };
    Recommendation { quality, reason }
}

/// Integrated chips share system memory: Intel's (except Arc cards), AMD's "Radeon Graphics"
/// APUs, and anything with almost no memory of its own.
pub fn is_integrated(vendor: u32, name: &str, dedicated_mb: u64) -> bool {
    let lower = name.to_ascii_lowercase();
    if dedicated_mb < 512 {
        return true;
    }
    match vendor {
        VENDOR_INTEL => !lower.contains("arc"),
        VENDOR_AMD => lower.ends_with("graphics") && !lower.contains(" rx ") && dedicated_mb < 2048,
        _ => false,
    }
}

pub fn detect() -> Hardware {
    let cpu = crate::winreg::get_string(crate::winreg::Root::Machine, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString")
        .unwrap_or_default();
    let cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
    let (gpu, gpu_integrated) = gpu().unwrap_or_default();
    let (width, height, refresh_hz) = display();
    Hardware { cpu, cores, ram_gb: ram_gb(), gpu, gpu_integrated, width, height, refresh_hz }
}

fn ram_gb() -> u32 {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: the struct carries its own size, as the call requires.
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_err() {
        return 0;
    }
    // Some memory is reserved by the hardware, so "8 GB" reads as ~7.8: round to whole GB.
    ((m.ullTotalPhys as f64) / (1024.0 * MIB as f64)).round() as u32
}

/// The graphics adapter with the most memory of its own (the one doing the work on laptops
/// with two), skipping software renderers.
fn gpu() -> Option<(String, bool)> {
    // SAFETY: DXGI enumeration; every returned object is reference counted by the bindings.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<(String, u32, u64)> = None;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else { break };
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 || desc.VendorId == VENDOR_MICROSOFT {
                continue;
            }
            let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..len]).trim().to_string();
            let dedicated = desc.DedicatedVideoMemory as u64 / MIB;
            if best.as_ref().is_none_or(|b| dedicated > b.2) {
                best = Some((name, desc.VendorId, dedicated));
            }
        }
        best.map(|(name, vendor, mb)| {
            let integrated = is_integrated(vendor, &name, mb);
            (name, integrated)
        })
    }
}

fn display() -> (u32, u32, u32) {
    let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY: the primary display's current mode, into a struct that carries its own size.
    if unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool() {
        (mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency)
    } else {
        (0, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(cores: u32, ram_gb: u32, integrated: bool, width: u32, height: u32) -> Hardware {
        Hardware { cores, ram_gb, gpu_integrated: integrated, width, height, refresh_hz: 60, ..Default::default() }
    }

    #[test]
    fn strong_and_normal_pcs_get_high() {
        assert_eq!(recommend(&pc(16, 16, false, 2560, 1440)).quality, "high");
        assert_eq!(recommend(&pc(8, 16, true, 1920, 1080)).quality, "high");
        assert_eq!(recommend(&pc(4, 8, false, 1920, 1080)).quality, "high");
    }

    #[test]
    fn weak_pcs_step_down() {
        // The low-end target: 4 threads, 8 GB, integrated graphics.
        assert_eq!(recommend(&pc(4, 8, true, 1920, 1080)), Recommendation { quality: "medium", reason: "integratedSmall" });
        assert_eq!(recommend(&pc(8, 16, true, 3840, 2160)), Recommendation { quality: "medium", reason: "integratedScreen" });
        assert_eq!(recommend(&pc(2, 8, true, 1366, 768)).quality, "low");
        assert_eq!(recommend(&pc(8, 4, false, 1920, 1080)).quality, "low");
    }

    #[test]
    fn integrated_graphics_detection() {
        assert!(is_integrated(VENDOR_INTEL, "Intel(R) UHD Graphics 620", 128));
        assert!(!is_integrated(VENDOR_INTEL, "Intel(R) Arc(TM) A770 Graphics", 16_000));
        assert!(is_integrated(VENDOR_AMD, "AMD Radeon(TM) Graphics", 512));
        assert!(!is_integrated(VENDOR_AMD, "AMD Radeon RX 7800 XT", 16_000));
        assert!(!is_integrated(0x10de, "NVIDIA GeForce RTX 3060", 12_000));
        assert!(is_integrated(0x10de, "Some tiny chip", 64));
    }
}
