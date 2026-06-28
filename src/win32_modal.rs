//! Hooks the iced window's WndProc to:
//!   - Detect modal message loops (drag/resize/menu) and pause mpv during them.
//!   - Snap the window to monitor edges during drag (WM_MOVING).
//!
//! Uses old-style SetWindowLongPtrW subclassing - no comctl32 v6 / manifest
//! requirement, works on all Windows versions.
#![cfg(target_os = "windows")]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, AtomicU8, Ordering};
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
struct POINT { x: i32, y: i32 }

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
    fn GetCursorPos(pt: *mut POINT) -> i32;
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
/// Euclidean cursor displacement required to release a snapped edge.
/// Must satisfy: SNAP_OUT_PX * cos(max_expected_drag_angle) > SNAP_IN_PX,
/// otherwise the window edge lands back inside snap_in after release and
/// immediately re-snaps. At 60° drag angle: 22 * cos(60°) = 11 > 10. ✓
const SNAP_OUT_PX: i32 = 22;

// ---------------------------------------------------------------------------
// EnumWindows callback - finds our main window by process ID
// ---------------------------------------------------------------------------

static FOUND_HWND: AtomicIsize = AtomicIsize::new(0);

/// Per-edge snap state: stores the cursor position (x,y) at snap time packed
/// into an i64 (high 32 = x, low 32 = y), or i64::MIN when not snapped.
/// Snap-in uses window-edge distance; snap-out uses Euclidean cursor
/// displacement from the snap point so diagonal drags release equally well.
static SNAP_L: AtomicI64 = AtomicI64::new(i64::MIN);
static SNAP_R: AtomicI64 = AtomicI64::new(i64::MIN);
static SNAP_T: AtomicI64 = AtomicI64::new(i64::MIN);
static SNAP_B: AtomicI64 = AtomicI64::new(i64::MIN);
/// Per-edge release guard: counts down WM_MOVING frames after a snap releases,
/// blocking immediate re-snap. Without this, releasing at a shallow angle leaves
/// the window edge still within snap_in, causing a wiggle-snap on the next frame.
static GUARD_L: AtomicU8 = AtomicU8::new(0);
static GUARD_R: AtomicU8 = AtomicU8::new(0);
static GUARD_T: AtomicU8 = AtomicU8::new(0);
static GUARD_B: AtomicU8 = AtomicU8::new(0);

fn pack_pt(x: i32, y: i32) -> i64 { ((x as i64) << 32) | (y as u32 as i64) }
fn unpack_pt(v: i64) -> (i32, i32) { ((v >> 32) as i32, v as i32) }
// Only the perpendicular axis counts toward release — sliding along an edge
// must never loosen the snap.
fn x_disp(snap_pt: i64, cur_x: i32) -> i32 { let (sx, _) = unpack_pt(snap_pt); (cur_x - sx).abs() }
fn y_disp(snap_pt: i64, cur_y: i32) -> i32 { let (_, sy) = unpack_pt(snap_pt); (cur_y - sy).abs() }

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
            // Reset all snap state when drag ends.
            SNAP_L.store(i64::MIN, Ordering::Relaxed);
            SNAP_R.store(i64::MIN, Ordering::Relaxed);
            SNAP_T.store(i64::MIN, Ordering::Relaxed);
            SNAP_B.store(i64::MIN, Ordering::Relaxed);
            GUARD_L.store(0, Ordering::Relaxed);
            GUARD_R.store(0, Ordering::Relaxed);
            GUARD_T.store(0, Ordering::Relaxed);
            GUARD_B.store(0, Ordering::Relaxed);
        }
        WM_MOVING => {
            let r = unsafe { &mut *(l as *mut RECT) };
            let width  = r.right  - r.left;
            let height = r.bottom - r.top;

            let dpi     = unsafe { GetDpiForWindow(hwnd) }.max(96);
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

            // Snap-in: window edge within snap_in px of monitor edge.
            // Snap-out: cursor has moved > snap_out px (Euclidean) from where
            // it was when the edge snapped. This makes diagonal drags release
            // corners just as easily as straight drags — the two axes no longer
            // need to independently exceed the threshold.
            let dist_l = (r.left   - wa.left  ).abs();
            let dist_r = (r.right  - wa.right ).abs();
            let dist_t = (r.top    - wa.top   ).abs();
            let dist_b = (r.bottom - wa.bottom).abs();

            let pt_l = SNAP_L.load(Ordering::Relaxed);
            let pt_r = SNAP_R.load(Ordering::Relaxed);
            let pt_t = SNAP_T.load(Ordering::Relaxed);
            let pt_b = SNAP_B.load(Ordering::Relaxed);

            // Clear release guards once the window edge has moved far enough away —
            // position-based so slow drags don't expire the guard too early.
            let snap_clear = snap_in * 2;
            if GUARD_L.load(Ordering::Relaxed) != 0 && dist_l > snap_clear { GUARD_L.store(0, Ordering::Relaxed); }
            if GUARD_R.load(Ordering::Relaxed) != 0 && dist_r > snap_clear { GUARD_R.store(0, Ordering::Relaxed); }
            if GUARD_T.load(Ordering::Relaxed) != 0 && dist_t > snap_clear { GUARD_T.store(0, Ordering::Relaxed); }
            if GUARD_B.load(Ordering::Relaxed) != 0 && dist_b > snap_clear { GUARD_B.store(0, Ordering::Relaxed); }

            let gd_l = GUARD_L.load(Ordering::Relaxed);
            let gd_r = GUARD_R.load(Ordering::Relaxed);
            let gd_t = GUARD_T.load(Ordering::Relaxed);
            let gd_b = GUARD_B.load(Ordering::Relaxed);

            let mut cur = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut cur) };
            let cur_packed = pack_pt(cur.x, cur.y);

            macro_rules! check_edge {
                ($snap:expr, $pt:expr, $guard:expr, $gd:expr, $dist:expr, $perp_disp:expr, $force:block) => {{
                    if $pt != i64::MIN {
                        // Release only on perpendicular movement — sliding along the
                        // edge contributes zero, so it can never accidentally loosen.
                        if $perp_disp > snap_out {
                            $snap.store(i64::MIN, Ordering::Relaxed);
                            $guard.store(1, Ordering::Relaxed);
                        } else {
                            $force
                        }
                    } else if $gd == 0 && $dist <= snap_in {
                        $snap.store(cur_packed, Ordering::Relaxed);
                        $force
                    }
                }};
            }

            check_edge!(SNAP_L, pt_l, GUARD_L, gd_l, dist_l, x_disp(pt_l, cur.x), { r.left   = wa.left;   r.right  = wa.left   + width;  });
            check_edge!(SNAP_R, pt_r, GUARD_R, gd_r, dist_r, x_disp(pt_r, cur.x), { r.right  = wa.right;  r.left   = wa.right  - width;  });
            check_edge!(SNAP_T, pt_t, GUARD_T, gd_t, dist_t, y_disp(pt_t, cur.y), { r.top    = wa.top;    r.bottom = wa.top    + height; });
            check_edge!(SNAP_B, pt_b, GUARD_B, gd_b, dist_b, y_disp(pt_b, cur.y), { r.bottom = wa.bottom; r.top    = wa.bottom - height; });

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
