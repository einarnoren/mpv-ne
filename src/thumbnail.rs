//! Seekbar thumbnail preview using a lightweight secondary libmpv instance.
//!
//! Thumbnails are keyed by their actual timestamp (seconds) in a BTreeMap so
//! lookup is always accurate regardless of what duration was current at
//! generation time.  For growing/live files `spawn_extend` adds new frames for
//! the newly discovered range without discarding the ones already generated.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::sync::{Arc, Mutex};

pub const THUMB_W: u32 = 160;
pub const THUMB_H: u32 = 90;

// Aim for this many thumbnails per generation call.
const TARGET_PER_CALL: usize = 30;
// Never space thumbnails closer than this many seconds.
const MIN_STEP: f64 = 5.0;
// Single worker keeps CPU impact low. Thumbnails fill in sequentially,
// which is fine — the user sees them appear one by one.
const PARALLEL_INSTANCES: usize = 1;

// ── Cache ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ThumbnailCache {
    /// Actual timestamp in whole seconds → 160×90 RGBA pixels.
    pub entries: BTreeMap<u32, Vec<u8>>,
    pub current_path: Option<String>,
    pub gen_id: u64,
    /// Step used when generating the current range (secs between thumbnails).
    pub step: f64,
    /// Furthest timestamp for which thumbnails have been generated.
    pub covered_to: f64,
    /// Furthest timestamp that has been *dispatched* to a worker (≥ covered_to).
    /// Used by spawn_extend to avoid launching a new thread for every tiny duration tick.
    pub scheduled_to: f64,
}

impl ThumbnailCache {
    /// Return the nearest thumbnail to `pos` seconds, or `None` if we don't
    /// have anything close enough (e.g. out-of-range for a live file).
    pub fn get_nearest(&self, pos: f64) -> Option<Vec<u8>> {
        if self.entries.is_empty() { return None; }
        // Don't show a stale thumbnail far beyond what we've generated yet.
        if pos > self.covered_to + self.step * 2.0 { return None; }
        let t = pos.max(0.0) as u32;
        let before = self.entries.range(..=t).next_back().map(|(k, v)| (*k, v.clone()));
        let after  = self.entries.range(t..).next().map(|(k, v)| (*k, v.clone()));
        match (before, after) {
            (Some((k1, v1)), Some((k2, v2))) => {
                if t - k1 <= k2 - t { Some(v1) } else { Some(v2) }
            }
            (Some((_, v)), None) | (None, Some((_, v))) => Some(v),
            (None, None) => None,
        }
    }
}

pub type SharedCache = Arc<Mutex<ThumbnailCache>>;

pub fn new_cache() -> SharedCache {
    Arc::new(Mutex::new(ThumbnailCache::default()))
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate thumbnails for a new file from scratch.
/// Clears any previous thumbnails and starts fresh.
pub fn spawn_generate(path: String, duration: f64, cache: SharedCache) {
    if duration < 2.0 { return; }

    let step = (duration / TARGET_PER_CALL as f64).max(MIN_STEP);

    let gen_id = {
        let mut c = cache.lock().unwrap();
        c.entries.clear();
        c.current_path = Some(path.clone());
        c.step = step;
        c.covered_to = 0.0;
        c.scheduled_to = duration;
        c.gen_id += 1;
        c.gen_id
    };

    tracing::debug!(path, duration, step, "thumbnail: spawn_generate");
    spawn_range(path, step, step, duration, cache, gen_id);
}

/// Extend thumbnails to cover a longer duration without clearing existing ones.
/// Uses the same step already in the cache; no-op if already covered.
pub fn spawn_extend(path: String, new_to: f64, cache: SharedCache) {
    if new_to < 2.0 { return; }

    let (gen_id, from, step) = {
        let mut c = cache.lock().unwrap();
        if c.current_path.as_deref() != Some(&path) { return; }
        // Only dispatch if new_to is at least one step beyond what's already scheduled.
        // This prevents spawning a thread for every tiny DurationChanged tick.
        if new_to <= c.scheduled_to + c.step { return; }
        let from = c.scheduled_to + c.step;
        c.scheduled_to = new_to; // claim the range atomically
        (c.gen_id, from, c.step)
    };

    tracing::debug!(path, from, new_to, step, "thumbnail: spawn_extend");
    spawn_range(path, from, step, new_to, cache, gen_id);
}

// ── Internal ──────────────────────────────────────────────────────────────────

/// Collect timestamps in [first, last] at `step` intervals and distribute them
/// across `PARALLEL_INSTANCES` worker threads.
fn spawn_range(
    path: String,
    first: f64,
    step: f64,
    last: f64,
    cache: SharedCache,
    gen_id: u64,
) {
    // Build the list of timestamps to render.
    let mut timestamps: Vec<f64> = Vec::new();
    let mut t = first;
    while t <= last + f64::EPSILON {
        timestamps.push(t);
        t += step;
    }
    if timestamps.is_empty() { return; }

    let path = Arc::new(path);
    let n_inst = PARALLEL_INSTANCES.min(timestamps.len());
    for inst in 0..n_inst {
        let path  = path.clone();
        let cache = cache.clone();
        // Each instance takes every n_inst-th timestamp.
        let mine: Vec<f64> = timestamps.iter().skip(inst).step_by(n_inst).copied().collect();
        std::thread::spawn(move || {
            generate_timestamps((*path).clone(), mine, cache, gen_id);
        });
    }
}

// ── mpv render constants (mirror player.rs) ───────────────────────────────────

const RPT_INVALID:   u32 = 0;
const RPT_API_TYPE:  u32 = 1;
const RPT_SW_SIZE:   u32 = 17;
const RPT_SW_FORMAT: u32 = 18;
const RPT_SW_STRIDE: u32 = 19;
const RPT_SW_POINTER:u32 = 20;
const RPT_FLIP_Y:    u32 = 11;

// ── per-instance worker ───────────────────────────────────────────────────────

fn generate_timestamps(
    path: String,
    timestamps: Vec<f64>,
    cache: SharedCache,
    gen_id: u64,
) {
    use libmpv_sys as sys;

    let mpv = unsafe { sys::mpv_create() };
    if mpv.is_null() { return; }

    unsafe {
        set_opt(mpv, "vo",              "libmpv");
        set_opt(mpv, "ao",              "null");
        set_opt(mpv, "vid",             "1");
        set_opt(mpv, "aid",             "no");
        set_opt(mpv, "sid",             "no");
        set_opt(mpv, "pause",           "yes");
        set_opt(mpv, "ytdl",            "no");
        set_opt(mpv, "cache",           "no");
        set_opt(mpv, "vd-lavc-threads", "1");
        set_opt(mpv, "hwdec",           "no");
        set_opt(mpv, "msg-level",       "all=no");

        if sys::mpv_initialize(mpv) != 0 {
            sys::mpv_destroy(mpv);
            return;
        }
    }

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

    let path_c = CString::new(path.as_str()).unwrap_or_default();
    unsafe {
        let args = [
            b"loadfile\0".as_ptr() as *const i8,
            path_c.as_ptr(),
            std::ptr::null(),
        ];
        sys::mpv_command(mpv, args.as_ptr() as *mut _);
    }

    if !wait_for_load(mpv, gen_id, &cache) {
        unsafe {
            sys::mpv_render_context_free(rctx);
            sys::mpv_destroy(mpv);
        }
        return;
    }

    for t in timestamps {
        if cache.lock().unwrap().gen_id != gen_id { break; }

        if let Some(rgba) = seek_and_render(mpv, rctx, t) {
            let mut c = cache.lock().unwrap();
            if c.gen_id == gen_id {
                c.entries.insert(t as u32, rgba);
                if t > c.covered_to { c.covered_to = t; }
            }
        }
    }

    tracing::debug!(path, "thumbnail: worker done");

    unsafe {
        sys::mpv_render_context_free(rctx);
        sys::mpv_destroy(mpv);
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

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

fn seek_and_render(
    mpv: *mut libmpv_sys::mpv_handle,
    rctx: *mut libmpv_sys::mpv_render_context,
    t: f64,
) -> Option<Vec<u8>> {
    use libmpv_sys as sys;

    // Single keyframe seek is fast enough for thumbnails; exact isn't needed
    // and the two-pass approach doubled I/O without meaningful quality gain.
    let t_str = CString::new(format!("{t:.3}")).unwrap();
    unsafe {
        let args = [
            b"seek\0".as_ptr() as *const i8,
            t_str.as_ptr(),
            b"absolute+keyframes\0".as_ptr() as *const i8,
            std::ptr::null(),
        ];
        sys::mpv_command(mpv, args.as_ptr() as *mut _);
    }

    if !wait_for_seek(mpv) { return None; }
    render_frame(rctx)
}

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

fn render_frame(rctx: *mut libmpv_sys::mpv_render_context) -> Option<Vec<u8>> {
    use libmpv_sys as sys;
    let stride = (THUMB_W * 4) as usize;
    let mut buf = vec![0u8; stride * THUMB_H as usize];
    let size    = [THUMB_W as i32, THUMB_H as i32];
    let fmt     = b"rgba\0";
    let flip_y: i32 = 0;

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
        if rc == 0 && buf.iter().any(|&b| b != 0) { return Some(buf); }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

unsafe fn set_opt(mpv: *mut libmpv_sys::mpv_handle, key: &str, val: &str) {
    let k = CString::new(key).unwrap();
    let v = CString::new(val).unwrap();
    unsafe { libmpv_sys::mpv_set_option_string(mpv, k.as_ptr(), v.as_ptr()); }
}
