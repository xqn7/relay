//! Previous / play-pause / next buttons on the taskbar thumbnail. Windows reports clicks to the
//! main window as WM_COMMAND, so the window is subclassed to catch them. Everything here runs
//! on the main (UI) thread, which owns the window and the taskbar COM object.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_ENABLED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, RegisterWindowMessageW, HICON, ICONINFO, SM_CXSMICON, WM_COMMAND,
};

const SUBCLASS_ID: usize = 0x5245_4c59;
const THBN_CLICKED: u32 = 0x1800;
const ID_PREVIOUS: u32 = 1;
const ID_PLAY: u32 = 2;
const ID_NEXT: u32 = 3;

static APP: OnceLock<AppHandle> = OnceLock::new();
static CREATED_MSG: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
    static STATE: RefCell<Shown> = RefCell::new(Shown::default());
}

struct Bar {
    list: ITaskbarList3,
    hwnd: HWND,
    previous: HICON,
    play: HICON,
    pause: HICON,
    next: HICON,
}

/// What the buttons show; kept so they can be rebuilt when Explorer restarts.
struct Shown {
    playing: bool,
    tips: [String; 4],
}

impl Default for Shown {
    fn default() -> Self {
        Self { playing: false, tips: ["Previous".into(), "Play".into(), "Pause".into(), "Next".into()] }
    }
}

/// Call on the main thread, before the window is first shown.
pub fn attach(app: &AppHandle, hwnd: HWND) {
    let _ = APP.set(app.clone());
    // SAFETY: the subclass procedure is a plain function; the window outlives the app's setup.
    unsafe {
        CREATED_MSG.store(RegisterWindowMessageW(w!("TaskbarButtonCreated")), Ordering::SeqCst);
        if !SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0).as_bool() {
            eprintln!("taskbar buttons unavailable: window subclass failed");
        }
    }
}

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
    let created = CREATED_MSG.load(Ordering::SeqCst);
    if created != 0 && msg == created {
        // The taskbar button exists now (or again, after Explorer restarted).
        if let Err(e) = add_buttons(hwnd) {
            eprintln!("taskbar buttons failed: {e}");
        }
    } else if msg == WM_COMMAND && (wparam.0 >> 16) as u32 & 0xffff == THBN_CLICKED {
        let action = match wparam.0 as u32 & 0xffff {
            ID_PREVIOUS => "previous",
            ID_PLAY => "toggle",
            ID_NEXT => "next",
            _ => return DefSubclassProc(hwnd, msg, wparam, lparam),
        };
        if let Some(app) = APP.get() {
            let _ = app.emit("media-key", serde_json::json!({ "action": action, "positionMs": null }));
        }
        return LRESULT(0);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe fn add_buttons(hwnd: HWND) -> windows::core::Result<()> {
    let list: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
    list.HrInit()?;
    let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 64);
    let bar = Bar {
        list,
        hwnd,
        previous: icon(&shapes(Glyph::Previous), size)?,
        play: icon(&shapes(Glyph::Play), size)?,
        pause: icon(&shapes(Glyph::Pause), size)?,
        next: icon(&shapes(Glyph::Next), size)?,
    };
    let buttons = STATE.with(|s| buttons(&bar, &s.borrow()));
    bar.list.ThumbBarAddButtons(hwnd, &buttons)?;
    BAR.with(|b| *b.borrow_mut() = Some(bar));
    Ok(())
}

fn buttons(bar: &Bar, shown: &Shown) -> [THUMBBUTTON; 3] {
    let (play_icon, play_tip) = if shown.playing { (bar.pause, &shown.tips[2]) } else { (bar.play, &shown.tips[1]) };
    [button(ID_PREVIOUS, bar.previous, &shown.tips[0]), button(ID_PLAY, play_icon, play_tip), button(ID_NEXT, bar.next, &shown.tips[3])]
}

fn button(id: u32, icon: HICON, tip: &str) -> THUMBBUTTON {
    let mut b = THUMBBUTTON { dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS, iId: id, hIcon: icon, dwFlags: THBF_ENABLED, ..Default::default() };
    for (dst, src) in b.szTip.iter_mut().zip(tip.encode_utf16().take(259)) {
        *dst = src;
    }
    b
}

fn refresh() {
    BAR.with(|b| {
        if let Some(bar) = b.borrow().as_ref() {
            let buttons = STATE.with(|s| buttons(bar, &s.borrow()));
            // SAFETY: the taskbar object and window belong to this (main) thread.
            if let Err(e) = unsafe { bar.list.ThumbBarUpdateButtons(bar.hwnd, &buttons) } {
                eprintln!("taskbar button update failed: {e}");
            }
        }
    });
}

pub fn set_playing(app: &AppHandle, playing: bool) {
    let _ = app.run_on_main_thread(move || {
        let changed = STATE.with(|s| std::mem::replace(&mut s.borrow_mut().playing, playing) != playing);
        if changed {
            refresh();
        }
    });
}

/// Translated tooltips: previous, play, pause, next.
pub fn set_tips(app: &AppHandle, tips: [String; 4]) {
    let _ = app.run_on_main_thread(move || {
        STATE.with(|s| s.borrow_mut().tips = tips);
        refresh();
    });
}

// ---- icons ---------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
}

type Point = (f32, f32);

/// The glyphs as filled convex shapes on a 24-unit grid (the app's icon grid), drawn white.
fn shapes(glyph: Glyph) -> Vec<Vec<Point>> {
    let rect = |x0: f32, x1: f32| vec![(x0, 5.0), (x1, 5.0), (x1, 19.0), (x0, 19.0)];
    let next = vec![vec![(5.5, 5.0), (15.5, 12.0), (5.5, 19.0)], rect(16.5, 19.0)];
    match glyph {
        Glyph::Play => vec![vec![(7.0, 4.5), (19.5, 12.0), (7.0, 19.5)]],
        Glyph::Pause => vec![rect(6.0, 10.0), rect(14.0, 18.0)],
        Glyph::Next => next,
        Glyph::Previous => next.into_iter().map(|poly| poly.into_iter().map(|(x, y)| (24.0 - x, y)).collect()).collect(),
    }
}

fn inside(poly: &[Point], (px, py): Point) -> bool {
    let mut sign = 0.0f32;
    for i in 0..poly.len() {
        let (ax, ay) = poly[i];
        let (bx, by) = poly[(i + 1) % poly.len()];
        let cross = (bx - ax) * (py - ay) - (by - ay) * (px - ax);
        if cross != 0.0 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

/// Anti-aliased coverage (0–255) of each pixel of a `size`×`size` image, 4×4 samples per pixel.
fn coverage(polys: &[Vec<Point>], size: i32) -> Vec<u8> {
    const SAMPLES: i32 = 4;
    let scale = 24.0 / size as f32;
    let mut out = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let p = (
                        (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * scale,
                        (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * scale,
                    );
                    if polys.iter().any(|poly| inside(poly, p)) {
                        hits += 1;
                    }
                }
            }
            out.push((hits * 255 / (SAMPLES * SAMPLES)) as u8);
        }
    }
    out
}

unsafe fn icon(polys: &[Vec<Point>], size: i32) -> windows::core::Result<HICON> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            // Negative height: rows top to bottom.
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
    for (px, a) in pixels.iter_mut().zip(coverage(polys, size)) {
        // White, with the shape's coverage as straight alpha (BGRA).
        *px = (a as u32) << 24 | 0x00ff_ffff;
    }
    // The alpha channel decides transparency; the mask only has to exist.
    let mask = CreateBitmap(size, size, 1, 1, None);
    let icon = CreateIconIndirect(&ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: color, ..Default::default() });
    let _ = DeleteObject(color.into());
    let _ = DeleteObject(mask.into());
    icon
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(cov: &[u8], size: i32, x: i32, y: i32) -> u8 {
        cov[(y * size + x) as usize]
    }

    #[test]
    fn glyphs_cover_the_right_pixels() {
        let pause = coverage(&shapes(Glyph::Pause), 24);
        assert_eq!(at(&pause, 24, 7, 12), 255, "inside the left bar");
        assert_eq!(at(&pause, 24, 12, 12), 0, "the gap between the bars");
        assert_eq!(at(&pause, 24, 1, 1), 0, "corner stays clear");

        let play = coverage(&shapes(Glyph::Play), 24);
        assert_eq!(at(&play, 24, 9, 12), 255);
        assert_eq!(at(&play, 24, 21, 12), 0, "right of the tip");
    }

    #[test]
    fn previous_mirrors_next() {
        let next = coverage(&shapes(Glyph::Next), 16);
        let prev = coverage(&shapes(Glyph::Previous), 16);
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(at(&next, 16, x, y), at(&prev, 16, 15 - x, y));
            }
        }
    }

    #[test]
    fn edges_are_antialiased() {
        let play = coverage(&shapes(Glyph::Play), 16);
        assert!(play.iter().any(|&a| a > 0 && a < 255));
    }
}
