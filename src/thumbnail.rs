//! Seekbar thumbnail preview using a lightweight secondary libmpv instance.
//!
//! No ffmpeg, no temp files. A second mpv handle opens the file with
//! `--pause`, seeks to each timestamp, and renders one frame into a pixel
//! buffer via the SW render API - the same path the main player uses.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

pub const THUMB_W: u32 = 160;
pub const THUMB_H: u32 = 90;
const MAX_THUMBS: usize = 30;
const PARALLEL_INSTANCES: usize = 3;

#[derive(Debug, Default)]
pub struct ThumbnailCache {
    pub frames:       Vec<Option<Vec<u8>>>,
    pub current_path: Option<String>,
    pub duration:     f64,
    pub count:        u32,
    pub gen_id:       u64,
}

impl ThumbnailCache {
    pub fn get_nearest(&self, pos: f64) -> Option<Vec<u8>> {
        if self.duration <= 0.0 || self.count == 0 || self.frames.is_empty() {
            return None;
        }
        let step  = self.duration / (self.count as f64 + 1.0);
        let ideal = ((pos / step) - 1.0)
            .floor()
            .clamp(0.0, (self.count.saturating_sub(1)) as f64) as usize;

        let n = self.frames.len();
        for delta in 0..n {
            let lo = ideal.saturating_sub(delta);
            if let Some(Some(px)) = self.frames.get(lo) { return Some(px.clone()); }
            let hi = ideal + delta;
            if hi < n {
                if let Some(Some(px)) = self.frames.get(hi) { return Some(px.clone()); }
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn is_ready(&self) -> bool { self.frames.iter().any(|f| f.is_some()) }
}

pub type SharedCache = Arc<Mutex<ThumbnailCache>>;

pub fn new_cache() -> SharedCache {
    Arc::new(Mutex::new(ThumbnailCache::default()))
}

pub fn spawn_generate(path: String, duration: f64, cache: SharedCache) {
    if duration < 2.0 { return; }

    let gen_id = {
        let mut c = cache.lock().unwrap();
        let count = (duration as usize / 5).clamp(8, MAX_THUMBS) as u32;
        c.frames       = vec![None; count as usize];
        c.current_path = Some(path.clone());
        c.duration     = duration;
        c.count        = count;
        c.gen_id      += 1;
        c.gen_id
    };

    // Split work across parallel mpv instances.
    let path   = Arc::new(path);
    let cache2 = cache.clone();
    for inst in 0..PARALLEL_INSTANCES {
        let path  = path.clone();
        let cache = cache2.clone();
        std::thread::spawn(move || {
            generate((*path).clone(), duration, cache, gen_id, inst, PARALLEL_INSTANCES);
        });
    }
}

// ── mpv render constants (mirror player.rs) ─────────────────────────────────

const RPT_INVALID:  u32 = 0;
const RPT_API_TYPE: u32 = 1;
const RPT_SW_SIZE:  u32 = 17;
const RPT_SW_FORMAT:u32 = 18;
const RPT_SW_STRIDE:u32 = 19;
const RPT_SW_POINTER:u32= 20;
const RPT_FLIP_Y:   u32 = 11;

// ── generation ───────────────────────────────────────────────────────────────

fn generate(path: String, duration: f64, cache: SharedCache, gen_id: u64,
            instance: usize, total_instances: usize) {
    use libmpv_sys as sys;

    let count = { cache.lock().unwrap().count };
    let step  = duration / (count as f64 + 1.0);

    // Create a minimal mpv handle.
    let mpv = unsafe { sys::mpv_create() };
    if mpv.is_null() { return; }

    unsafe {
        set_opt(mpv, "vo",               "libmpv");
        set_opt(mpv, "ao",               "null");
        set_opt(mpv, "vid",              "1");
        set_opt(mpv, "aid",              "no");
        set_opt(mpv, "sid",              "no");
        set_opt(mpv, "pause",            "yes");
        set_opt(mpv, "ytdl",             "no");
        set_opt(mpv, "cache",            "no");
        set_opt(mpv, "vd-lavc-threads",  "2");
        set_opt(mpv, "hwdec",            "no"); // SW decode for consistency
        set_opt(mpv, "msg-level",        "all=no");

        if sys::mpv_initialize(mpv) != 0 {
            sys::mpv_destroy(mpv);
            return;
        }
    }

    // Set up SW render context.
    let mut rctx: *mut sys::mpv_render_context = std::ptr::null_mut();
    let api = b"sw\0";
    let mut init_params = [
        sys::mpv_render_param { type_: RPT_API_TYPE, data: api.as_ptr() as *mut _ },
        sys::mpv_render_param { type_: RPT_INVALID,  data: std::ptr::null_mut() },
    ];
    let rc = unsafe {
        sys::mpv_render_context_create(&mut rctx, mpv, init_params.as_mut_ptr())
    };
    if rc != 0 || rctx.is_null() {
        unsafe { sys::mpv_destroy(mpv); }
        return;
    }

    // Load the file.
    let path_c = CString::new(path.as_str()).unwrap_or_default();
    unsafe {
        let args = [
            b"loadfile\0".as_ptr() as *const i8,
            path_c.as_ptr(),
            std::ptr::null(),
        ];
        sys::mpv_command(mpv, args.as_ptr() as *mut _);
    }

    // Wait for file to load.
    if !wait_for_load(mpv, gen_id, &cache) {
        unsafe {
            sys::mpv_render_context_free(rctx);
            sys::mpv_destroy(mpv);
        }
        return;
    }

    // Each instance handles its own interleaved slice of indices.
    // instance=0 → 0, 2, 4...   instance=1 → 1, 3, 5...
    let mut i = instance as u32;
    while i < count {
        if cache.lock().unwrap().gen_id != gen_id { break; }

        let t = step * (i as f64 + 1.0);
        if let Some(rgba) = seek_and_render(mpv, rctx, t) {
            let mut c = cache.lock().unwrap();
            if c.gen_id == gen_id {
                if let Some(slot) = c.frames.get_mut(i as usize) {
                    *slot = Some(rgba);
                }
            }
        }
        i += total_instances as u32;
    }

    tracing::debug!(path, "thumbnail generation complete");

    unsafe {
        sys::mpv_render_context_free(rctx);
        sys::mpv_destroy(mpv);
    }
}

/// Wait for MPV_EVENT_FILE_LOADED, bail if a newer gen_id appears.
fn wait_for_load(
    mpv: *mut libmpv_sys::mpv_handle,
    gen_id: u64,
    cache: &SharedCache,
) -> bool {
    use libmpv_sys as sys;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if std::time::Instant::now() > deadline { return false; }
        if cache.lock().unwrap().gen_id != gen_id { return false; }
        let ev = unsafe { sys::mpv_wait_event(mpv, 0.1) };
        let id = unsafe { (*ev).event_id };
        if id == sys::mpv_event_id_MPV_EVENT_FILE_LOADED { return true; }
        if id == sys::mpv_event_id_MPV_EVENT_END_FILE    { return false; }
    }
}

/// Seek to `t` accurately using two-pass technique (thumbfast approach):
///   1. Fast keyframe seek to (t - PRE_ROLL)
///   2. Exact seek forward PRE_ROLL seconds to land on the precise frame
/// Much faster than full exact seek while still being frame-accurate.
const PRE_ROLL: f64 = 15.0;

fn seek_and_render(
    mpv: *mut libmpv_sys::mpv_handle,
    rctx: *mut libmpv_sys::mpv_render_context,
    t: f64,
) -> Option<Vec<u8>> {
    use libmpv_sys as sys;

    // Pass 1: fast keyframe seek to just before the target.
    let pre = (t - PRE_ROLL).max(0.0);
    let pre_str = CString::new(format!("{pre:.3}")).unwrap();
    unsafe {
        let args = [
            b"seek\0".as_ptr() as *const i8,
            pre_str.as_ptr(),
            b"absolute\0".as_ptr() as *const i8,
            std::ptr::null(),
        ];
        sys::mpv_command(mpv, args.as_ptr() as *mut _);
    }

    // Drain events after the fast seek.
    drain_events(mpv);

    // Pass 2: exact seek to the precise target from the nearby keyframe.
    let t_str = CString::new(format!("{t:.3}")).unwrap();
    unsafe {
        let args = [
            b"seek\0".as_ptr()    as *const i8,
            t_str.as_ptr(),
            b"absolute+exact\0".as_ptr() as *const i8,
            std::ptr::null(),
        ];
        sys::mpv_command(mpv, args.as_ptr() as *mut _);
    }

    // Wait for MPV_EVENT_PLAYBACK_RESTART which fires when the exact seek settles.
    let settled = wait_for_seek(mpv);
    if !settled { return None; }

    // Render the frame.
    render_frame(rctx)
}

/// Drain all pending events without blocking.
fn drain_events(mpv: *mut libmpv_sys::mpv_handle) {
    loop {
        let ev = unsafe { libmpv_sys::mpv_wait_event(mpv, 0.0) };
        if unsafe { (*ev).event_id } == libmpv_sys::mpv_event_id_MPV_EVENT_NONE { break; }
    }
}

/// Wait for MPV_EVENT_PLAYBACK_RESTART (seek complete) with a timeout.
fn wait_for_seek(mpv: *mut libmpv_sys::mpv_handle) -> bool {
    use libmpv_sys as sys;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
    loop {
        if std::time::Instant::now() > deadline { return false; }
        let ev = unsafe { sys::mpv_wait_event(mpv, 0.05) };
        match unsafe { (*ev).event_id } {
            sys::mpv_event_id_MPV_EVENT_PLAYBACK_RESTART => return true,
            sys::mpv_event_id_MPV_EVENT_END_FILE         => return false,
            _ => {}
        }
    }
}

/// Render the current frame into an RGBA pixel buffer.
fn render_frame(rctx: *mut libmpv_sys::mpv_render_context) -> Option<Vec<u8>> {
    use libmpv_sys as sys;
    let stride = (THUMB_W * 4) as usize;
    let mut buf = vec![0u8; stride * THUMB_H as usize];
    let size   = [THUMB_W as i32, THUMB_H as i32];
    let fmt    = b"rgba\0";
    let flip_y: i32 = 0;

    // Retry briefly in case the render context needs a moment.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    loop {
        if std::time::Instant::now() > deadline { return None; }

        let flags = unsafe { sys::mpv_render_context_update(rctx) };
        if flags & 1 == 0 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            continue;
        }

        let mut rp = [
            sys::mpv_render_param { type_: RPT_SW_SIZE,    data: size.as_ptr()          as *mut _ },
            sys::mpv_render_param { type_: RPT_SW_FORMAT,  data: fmt.as_ptr()            as *mut _ },
            sys::mpv_render_param { type_: RPT_SW_STRIDE,  data: &stride as *const usize as *mut _ },
            sys::mpv_render_param { type_: RPT_SW_POINTER, data: buf.as_mut_ptr()        as *mut _ },
            sys::mpv_render_param { type_: RPT_FLIP_Y,     data: &flip_y as *const i32   as *mut _ },
            sys::mpv_render_param { type_: RPT_INVALID,    data: std::ptr::null_mut() },
        ];
        let rc = unsafe { sys::mpv_render_context_render(rctx, rp.as_mut_ptr()) };
        if rc == 0 && buf.iter().any(|&b| b != 0) {
            return Some(buf);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

unsafe fn set_opt(mpv: *mut libmpv_sys::mpv_handle, key: &str, val: &str) {
    let k = CString::new(key).unwrap();
    let v = CString::new(val).unwrap();
    unsafe { libmpv_sys::mpv_set_option_string(mpv, k.as_ptr(), v.as_ptr()); }
}
