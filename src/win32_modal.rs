//! Hooks the iced window's WndProc to:
//!   - Detect modal message loops (drag/resize/menu) and pause mpv during them.
//!   - Snap the window to monitor edges during drag (WM_MOVING).
//!
//! Uses old-style SetWindowLongPtrW subclassing - no comctl32 v6 / manifest
//! requirement, works on all Windows versions.
#![cfg(target_os = "windows")]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;

type Callback = Box<dyn Fn() + Send + Sync>;

static ON_ENTER:    OnceLock<Callback> = OnceLock::new();
static ON_EXIT:     OnceLock<Callback> = OnceLock::new();
static INSTALLED:   AtomicBool  = AtomicBool::new(false);
/// Original WndProc saved by SetWindowLongPtrW; we chain to it for every
/// message we do not handle ourselves.
static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);

type HWND     = isize;
type HMONITOR = isize;

#[repr(C)]
struct RECT { left: i32, top: i32, right: i32, bottom: i32 }

#[repr(C)]
struct MONITORINFO {
    cb_size:    u32,
    rc_monitor: RECT,
    rc_work:    RECT,
    dw_flags:   u32,
}

const GWLP_WNDPROC: i32 = -4;

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(cb: unsafe extern "system" fn(HWND, isize) -> i32, l: isize) -> i32;
    fn GetWindowThreadProcessId(hwnd: HWND, pid: *mut u32) -> u32;
    fn GetCurrentProcessId() -> u32;
    fn IsWindowVisible(hwnd: HWND) -> i32;
    fn GetWindow(hwnd: HWND, cmd: u32) -> HWND;
    fn SetWindowLongPtrW(hwnd: HWND, index: i32, new_val: isize) -> isize;
    fn CallWindowProcW(prev: isize, hwnd: HWND, msg: u32, w: usize, l: isize) -> isize;
    fn MonitorFromRect(rc: *const RECT, flags: u32) -> HMONITOR;
    fn GetMonitorInfoW(mon: HMONITOR, mi: *mut MONITORINFO) -> i32;
    fn GetDpiForWindow(hwnd: HWND) -> u32;
}

const GW_OWNER:                u32 = 4;
const MONITOR_DEFAULTTONEAREST: u32 = 2;
const WM_ENTERMENULOOP: u32 = 0x0211;
const WM_EXITMENULOOP:  u32 = 0x0212;
#[allow(dead_code)]
const WM_ENTERSIZEMOVE: u32 = 0x0231;
const WM_EXITSIZEMOVE:  u32 = 0x0232;
const WM_MOVING:        u32 = 0x0216;
/// Distance (logical px at 96 DPI) to attract to an edge.
const SNAP_IN_PX:  i32 = 10;
/// Distance you must drag away before the edge releases.
const SNAP_OUT_PX: i32 = 28;

// ---------------------------------------------------------------------------
// EnumWindows callback - finds our main window by process ID
// ---------------------------------------------------------------------------

static FOUND_HWND: AtomicIsize = AtomicIsize::new(0);

/// Per-edge snap state: true while that edge is currently held against the monitor.
static SNAP_L: AtomicBool = AtomicBool::new(false);
static SNAP_R: AtomicBool = AtomicBool::new(false);
static SNAP_T: AtomicBool = AtomicBool::new(false);
static SNAP_B: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn enum_cb(hwnd: HWND, _: isize) -> i32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != unsafe { GetCurrentProcessId() } { return 1; }
    if unsafe { IsWindowVisible(hwnd) } == 0    { return 1; }
    if unsafe { GetWindow(hwnd, GW_OWNER) } != 0 { return 1; }
    FOUND_HWND.store(hwnd, Ordering::SeqCst);
    0 // stop
}

// ---------------------------------------------------------------------------
// Our replacement WndProc
// ---------------------------------------------------------------------------

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
    match msg {
        WM_ENTERMENULOOP => {
            // Pause mpv during native menu loops - they block iced's event loop
            // completely so frames can't be presented anyway.
            if let Some(cb) = ON_ENTER.get() { cb(); }
        }
        WM_EXITMENULOOP => {
            if let Some(cb) = ON_EXIT.get() { cb(); }
        }
        WM_EXITSIZEMOVE => {
            // Reset snap state when drag ends. Do NOT pause during move/resize -
            // that caused the video to freeze while dragging the window.
            SNAP_L.store(false, Ordering::Relaxed);
            SNAP_R.store(false, Ordering::Relaxed);
            SNAP_T.store(false, Ordering::Relaxed);
            SNAP_B.store(false, Ordering::Relaxed);
        }
        WM_MOVING => {
            let r = unsafe { &mut *(l as *mut RECT) };
            let width  = r.right  - r.left;
            let height = r.bottom - r.top;

            let dpi    = unsafe { GetDpiForWindow(hwnd) }.max(96);
            let snap_in  = (SNAP_IN_PX  * dpi as i32) / 96;
            let snap_out = (SNAP_OUT_PX * dpi as i32) / 96;

            let mut mi = MONITORINFO {
                cb_size:    std::mem::size_of::<MONITORINFO>() as u32,
                rc_monitor: RECT { left: 0, top: 0, right: 0, bottom: 0 },
                rc_work:    RECT { left: 0, top: 0, right: 0, bottom: 0 },
                dw_flags:   0,
            };
            let hmon = unsafe { MonitorFromRect(r as *const RECT, MONITOR_DEFAULTTONEAREST) };
            if hmon != 0 { unsafe { GetMonitorInfoW(hmon, &mut mi) }; }
            let wa = &mi.rc_work;

            // Each edge: snap in when within snap_in, release only when
            // dragged further than snap_out away.
            let dist_l = (r.left   - wa.left  ).abs();
            let dist_r = (r.right  - wa.right ).abs();
            let dist_t = (r.top    - wa.top   ).abs();
            let dist_b = (r.bottom - wa.bottom).abs();

            let snapped_l = SNAP_L.load(Ordering::Relaxed);
            let snapped_r = SNAP_R.load(Ordering::Relaxed);
            let snapped_t = SNAP_T.load(Ordering::Relaxed);
            let snapped_b = SNAP_B.load(Ordering::Relaxed);

            if  snapped_l && dist_l <= snap_out || !snapped_l && dist_l <= snap_in {
                SNAP_L.store(true, Ordering::Relaxed);
                r.left  = wa.left; r.right  = wa.left  + width;
            } else { SNAP_L.store(false, Ordering::Relaxed); }

            if  snapped_r && dist_r <= snap_out || !snapped_r && dist_r <= snap_in {
                SNAP_R.store(true, Ordering::Relaxed);
                r.right = wa.right; r.left  = wa.right - width;
            } else { SNAP_R.store(false, Ordering::Relaxed); }

            if  snapped_t && dist_t <= snap_out || !snapped_t && dist_t <= snap_in {
                SNAP_T.store(true, Ordering::Relaxed);
                r.top    = wa.top; r.bottom = wa.top    + height;
            } else { SNAP_T.store(false, Ordering::Relaxed); }

            if  snapped_b && dist_b <= snap_out || !snapped_b && dist_b <= snap_in {
                SNAP_B.store(true, Ordering::Relaxed);
                r.bottom = wa.bottom; r.top = wa.bottom - height;
            } else { SNAP_B.store(false, Ordering::Relaxed); }

            return 1; // TRUE
        }
        _ => {}
    }
    let orig = ORIG_WNDPROC.load(Ordering::SeqCst);
    unsafe { CallWindowProcW(orig, hwnd, msg, w, l) }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn install(
    on_enter: impl Fn() + Send + Sync + 'static,
    on_exit:  impl Fn() + Send + Sync + 'static,
) {
    if INSTALLED.swap(true, Ordering::SeqCst) { return; }
    let _ = ON_ENTER.set(Box::new(on_enter));
    let _ = ON_EXIT.set(Box::new(on_exit));

    std::thread::spawn(|| {
        for attempt in 0..50 {
            FOUND_HWND.store(0, Ordering::SeqCst);
            unsafe { EnumWindows(enum_cb, 0) };
            let hwnd = FOUND_HWND.load(Ordering::SeqCst);

            if hwnd != 0 {
                // Replace WndProc with ours; save the original.
                let orig = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as isize) };
                if orig != 0 {
                    ORIG_WNDPROC.store(orig, Ordering::SeqCst);
                    tracing::info!(hwnd, attempt, "modal hook: WndProc installed ok");
                    return;
                }
                tracing::warn!(hwnd, attempt, "modal hook: SetWindowLongPtrW failed, retrying");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        tracing::warn!("modal hook: gave up after 5s");
    });
}
