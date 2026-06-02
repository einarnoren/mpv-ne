use std::{
    ffi::{CStr, CString},
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        Condvar, Mutex,
    },
};

use futures::stream::{self, BoxStream};
use libmpv_sys as sys;
use tokio::sync::mpsc::UnboundedSender;

/// One subtitle track as reported by mpv's `track-list`. `id` is mpv's
/// track id (1-based for real tracks); we reserve `0` for the special
/// "Off" entry that always appears at the top of the list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubTrack {
    pub id: i64,
    pub label: String,
}

/// One audio track from `track-list`. Same shape as `SubTrack` but a distinct
/// type so `Message::AudioTrackSelected` and `SubTrackSelected` stay unambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioTrack {
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub time: f64,
    /// Chapter name from the container. Not yet displayed in the UI but kept
    /// for future tooltip use on the seek-bar tick marks.
    #[allow(dead_code)]
    pub title: Option<String>,
}

impl std::fmt::Display for SubTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

impl std::fmt::Display for AudioTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Position(f64),
    Duration(f64),
    FileLoaded,
    EndFile,
    Pause(bool),
    Frame(Vec<u8>, u32, u32), // rgba pixels, width, height
    Width(i64),
    Height(i64),
    VideoCodec(String),
    AudioCodec(String),
    AudioChannels(i64),
    HwDec(String),
    Primaries(String),
    SubVisible(bool),
    SubTracks(Vec<SubTrack>),
    CurrentSid(i64),
    Chapters(Vec<Chapter>),
    AudioTracks(Vec<AudioTrack>),
    CurrentAid(i64),
    Speed(f64),
    SubDelay(f64),
    AudioDelay(f64),
    SubFontSize(i64),
    SubPos(i64),
    LoopFile(bool),
    LoopPlaylist(bool),
    Deinterlace(bool),
    VideoZoom(f64),
    CacheTime(f64),
}

// mpv handles are fully thread-safe per the mpv docs.
pub struct Handle(*mut sys::mpv_handle);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Hash for Handle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as usize).hash(state);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { sys::mpv_terminate_destroy(self.0) }
    }
}

impl Handle {
    pub fn is_paused(&self) -> bool {
        let name = CString::new("pause").unwrap();
        let mut flag: std::os::raw::c_int = 0;
        unsafe {
            sys::mpv_get_property(
                self.0,
                name.as_ptr(),
                sys::mpv_format_MPV_FORMAT_FLAG,
                &mut flag as *mut _ as *mut _,
            );
        }
        flag != 0
    }

    pub fn set_pause(&self, paused: bool) {
        let name = CString::new("pause").unwrap();
        let mut flag: std::os::raw::c_int = paused as _;
        unsafe {
            sys::mpv_set_property(
                self.0,
                name.as_ptr(),
                sys::mpv_format_MPV_FORMAT_FLAG,
                &mut flag as *mut _ as *mut _,
            );
        }
    }
}

pub struct RenderSize {
    pub w: AtomicU32,
    pub h: AtomicU32,
    /// Set true + notified whenever the size changes OR mpv signals a new
    /// frame, so the render loop wakes either way.
    pub wake_flag: Mutex<bool>,
    pub wake_cv: Condvar,
}

impl RenderSize {
    fn signal(&self) {
        *self.wake_flag.lock().unwrap() = true;
        self.wake_cv.notify_one();
    }
}

/// Passed to `Subscription::run_with` as the stable key.
#[derive(Clone)]
pub struct StreamKey {
    pub handle: Arc<Handle>,
    pub render_size: Arc<RenderSize>,
}

impl Hash for StreamKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.handle.0 as usize).hash(state);
    }
}

pub struct Player {
    handle: Arc<Handle>,
    render_size: Arc<RenderSize>,
    pub path: Option<String>,
    pub paused: bool,
    pub volume: f64,
    pub position: f64,
    pub duration: f64,
    pub width: i64,
    pub height: i64,
    pub muted: bool,
    pub video_codec: String,
    pub audio_codec: String,
    pub audio_channels: i64,
    pub hwdec: String,
    pub primaries: String,
    pub sub_visible: bool,
    pub sub_tracks: Vec<SubTrack>,
    /// Currently active subtitle track id (0 = none / off).
    pub current_sid: i64,
    pub chapters: Vec<Chapter>,
    pub audio_tracks: Vec<AudioTrack>,
    /// Currently active audio track id (0 = none / off).
    pub current_aid: i64,
    pub speed: f64,
    pub sub_delay: f64,
    pub audio_delay: f64,
    pub sub_font_size: i64,
    pub sub_pos: i64,
    pub loop_file: bool,
    pub loop_playlist: bool,
    pub deinterlace: bool,
    /// mpv `video-zoom` property (log2 scale: 0.0 = 100%, 1.0 = 200%, -1.0 = 50%).
    pub video_zoom: f64,
    /// Furthest buffered timestamp (from demuxer-cache-time). 0 = unknown/local.
    pub cache_time: f64,
    pub brightness: i64,
    pub contrast: i64,
    pub saturation: i64,
    pub hue: i64,
    pub gamma: i64,
    /// Our active labeled filters. Rebuilt and set atomically to avoid flicker.
    vf_slots: std::collections::HashMap<&'static str, String>,
}

impl Player {
    pub fn new() -> Self {
        unsafe {
            let h = sys::mpv_create();
            assert!(!h.is_null(), "mpv_create failed");

            set_opt_str(h, "vo", "libmpv");
            // Enable hardware decoding. -copy variants pull decoded frames
            // back to system memory so they work with our SW render path.
            // auto-copy-safe picks the best available decoder per platform:
            //   Windows → d3d11va-copy / dxva2-copy
            //   macOS   → videotoolbox-copy
            //   Linux   → nvdec-copy / vaapi-copy
            set_opt_str(h, "hwdec", "auto-copy-safe");

            let rc = sys::mpv_initialize(h);
            tracing::info!(rc, "mpv_initialize");
            assert_eq!(rc, 0, "mpv_initialize failed: {rc}");

            observe(h, 1, "time-pos", sys::mpv_format_MPV_FORMAT_DOUBLE);
            observe(h, 2, "duration", sys::mpv_format_MPV_FORMAT_DOUBLE);
            observe(h, 3, "pause", sys::mpv_format_MPV_FORMAT_FLAG);
            observe(h, 4, "width", sys::mpv_format_MPV_FORMAT_INT64);
            observe(h, 5, "height", sys::mpv_format_MPV_FORMAT_INT64);
            observe(h, 6, "video-format", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 7, "audio-codec-name", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 8, "audio-params/channel-count", sys::mpv_format_MPV_FORMAT_INT64);
            observe(h, 9, "hwdec-current", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 10, "video-params/primaries", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 11, "sub-visibility", sys::mpv_format_MPV_FORMAT_FLAG);
            observe(h, 12, "sid", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 13, "aid", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 14, "speed",       sys::mpv_format_MPV_FORMAT_DOUBLE);
            observe(h, 15, "sub-delay",   sys::mpv_format_MPV_FORMAT_DOUBLE);
            observe(h, 16, "audio-delay", sys::mpv_format_MPV_FORMAT_DOUBLE);
            observe(h, 24, "sub-font-size", sys::mpv_format_MPV_FORMAT_INT64);
            observe(h, 25, "sub-pos",       sys::mpv_format_MPV_FORMAT_INT64);
            observe(h, 17, "loop-file",   sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 18, "deinterlace", sys::mpv_format_MPV_FORMAT_FLAG);
            // brightness/contrast/saturation/hue/gamma are managed via the
            // @mpvne_eq lavfi filter in apply_video_eq(); no observers needed.
            observe(h, 26, "loop-playlist", sys::mpv_format_MPV_FORMAT_STRING);
            observe(h, 27, "video-zoom",         sys::mpv_format_MPV_FORMAT_DOUBLE);
            observe(h, 28, "demuxer-cache-time", sys::mpv_format_MPV_FORMAT_DOUBLE);

            Self {
                handle: Arc::new(Handle(h)),
                render_size: Arc::new(RenderSize {
                    w: AtomicU32::new(0),
                    h: AtomicU32::new(0),
                    wake_flag: Mutex::new(false),
                    wake_cv: Condvar::new(),
                }),
                path: None,
                paused: true,
                volume: 100.0,
                position: 0.0,
                duration: 0.0,
                width: 0,
                height: 0,
                muted: false,
                video_codec: String::new(),
                audio_codec: String::new(),
                audio_channels: 0,
                hwdec: String::new(),
                primaries: String::new(),
                sub_visible: true,
                sub_tracks: vec![SubTrack { id: 0, label: "Off".to_string() }],
                current_sid: 0,
                chapters: Vec::new(),
                audio_tracks: vec![AudioTrack { id: 0, label: "Off".to_string() }],
                current_aid: 0,
                speed: 1.0,
                sub_delay: 0.0,
                audio_delay: 0.0,
                sub_font_size: 55,
                sub_pos: 100,
                loop_file: false,
                loop_playlist: false,
                deinterlace: false,
                brightness: 0,
                contrast: 0,
                saturation: 0,
                hue: 0,
                gamma: 0,
                video_zoom: 0.0,
                cache_time: 0.0,
                vf_slots: std::collections::HashMap::new(),
            }
        }
    }

    pub fn handle_arc(&self) -> Arc<Handle> {
        Arc::clone(&self.handle)
    }

    pub fn stream_key(&self) -> StreamKey {
        StreamKey {
            handle: Arc::clone(&self.handle),
            render_size: Arc::clone(&self.render_size),
        }
    }

    pub fn set_render_size(&self, w: u32, h: u32) {
        self.render_size.w.store(w, Ordering::Relaxed);
        self.render_size.h.store(h, Ordering::Relaxed);
        // Wake the render loop so it re-renders at the new size even if mpv
        // hasn't produced a new frame (e.g. while paused).
        self.render_size.signal();
    }

    pub fn open(&mut self, path: impl Into<String>) {
        let path = path.into();
        self.path = Some(path.clone());
        self.paused = false;
        self.vf_slots.clear(); // filters are reset by mpv on file load
        command(self.handle.0, &["loadfile", &path]);
    }

    pub fn play(&mut self) {
        self.set_pause(false);
    }

    pub fn pause(&mut self) {
        self.set_pause(true);
    }

    /// Tell mpv to shut down. Unblocks the event loop so the app can exit.
    pub fn quit(&self) {
        command(self.handle.0, &["quit"]);
    }

    /// Pause and rewind to the start. Keeps the file loaded so a subsequent
    /// Play just resumes from frame one.
    pub fn stop(&mut self) {
        self.position = 0.0;
        command(self.handle.0, &["seek", "0", "absolute"]);
        self.set_pause(true);
    }

    pub fn seek(&mut self, pos: f64) {
        self.position = pos;
        // "absolute+exact" = seek to the precise timestamp, not nearest keyframe.
        command(self.handle.0, &["seek", &pos.to_string(), "absolute+exact"]);
    }

    /// Seek by `delta` seconds from the current position. mpv clamps for us.
    pub fn seek_relative(&self, delta: f64) {
        command(self.handle.0, &["seek", &delta.to_string(), "relative"]);
    }

    /// Cycle through subtitle tracks. mpv treats "no subtitles" as one of the
    /// cycle states, so this both toggles visibility and switches tracks.
    pub fn cycle_subtitle(&self) {
        command(self.handle.0, &["cycle", "sub"]);
    }

    /// Pick a subtitle track by id. id <= 0 means "off" (sid=no), id > 0 is a
    /// real track id from `track-list`.
    pub fn set_sub_track(&self, id: i64) {
        let value = if id <= 0 { "no".to_string() } else { id.to_string() };
        let n = CString::new("sid").unwrap();
        let v = CString::new(value).unwrap();
        unsafe { sys::mpv_set_property_string(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    pub fn toggle_sub_visibility(&self) {
        command(self.handle.0, &["cycle", "sub-visibility"]);
    }

    /// Pick an audio track by id. id <= 0 means "off" (aid=no), id > 0 is a
    /// real track id from `track-list`.
    pub fn set_audio_track(&self, id: i64) {
        let value = if id <= 0 { "no".to_string() } else { id.to_string() };
        let n = CString::new("aid").unwrap();
        let v = CString::new(value).unwrap();
        unsafe { sys::mpv_set_property_string(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    /// Cycle through audio tracks (mpv convention key: #).
    pub fn cycle_audio(&self) {
        command(self.handle.0, &["cycle", "audio"]);
    }

    /// Set playback speed. Clamped to 0.25x - 4.0x.
    pub fn set_speed(&self, speed: f64) {
        let s = speed.clamp(0.25, 4.0);
        set_prop_f64(self.handle.0, "speed", s);
    }

    /// Adjust subtitle delay in seconds. Positive = subs appear later.
    pub fn set_sub_font_size(&self, size: i64) {
        let v = size.clamp(10, 200);
        let n = CString::new("sub-font-size").unwrap();
        unsafe { sys::mpv_set_property(self.handle.0, n.as_ptr(), sys::mpv_format_MPV_FORMAT_INT64, &v as *const i64 as *mut _) };
    }

    pub fn set_sub_pos(&self, pos: i64) {
        let v = pos.clamp(0, 150);
        let n = CString::new("sub-pos").unwrap();
        unsafe { sys::mpv_set_property(self.handle.0, n.as_ptr(), sys::mpv_format_MPV_FORMAT_INT64, &v as *const i64 as *mut _) };
    }

    pub fn set_sub_delay(&self, secs: f64) {
        set_prop_f64(self.handle.0, "sub-delay", secs);
    }

    /// Adjust audio delay in seconds. Positive = audio plays later.
    pub fn set_audio_delay(&self, secs: f64) {
        set_prop_f64(self.handle.0, "audio-delay", secs);
    }

    /// Take a screenshot (saves to mpv's default screenshot directory).
    /// Toggle looping the current file.
    pub fn set_loop_playlist(&self, on: bool) {
        let n = CString::new("loop-playlist").unwrap();
        let v = CString::new(if on { "inf" } else { "no" }).unwrap();
        unsafe { sys::mpv_set_property_string(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    pub fn set_video_zoom(&self, zoom: f64) {
        set_prop_f64(self.handle.0, "video-zoom", zoom);
    }

    /// Override the display aspect ratio. Pass `""` to restore auto (-1).
    pub fn set_aspect_ratio(&self, ratio: &str) {
        let n = CString::new("video-aspect-override").unwrap();
        let v = CString::new(if ratio.is_empty() { "-1" } else { ratio }).unwrap();
        unsafe { sys::mpv_set_property_string(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    /// Load an external subtitle file. It becomes the active subtitle track.
    /// Rotate video 0 / 90 / 180 / 270 degrees clockwise.
    /// Set (or clear) a named filter slot and atomically apply the full chain.
    /// Using `vf set` on the combined chain avoids the remove→add flicker gap.
    fn set_vf_slot(&mut self, label: &'static str, spec: Option<String>) {
        match spec {
            Some(s) => { self.vf_slots.insert(label, s); }
            None    => { self.vf_slots.remove(label); }
        }
        let chain: Vec<String> = self.vf_slots.values().cloned().collect();
        if chain.is_empty() {
            command_str(self.handle.0, "vf clr");
        } else {
            command_str(self.handle.0, &format!("vf set {}", chain.join(",")));
        }
    }

    pub fn set_rotate(&mut self, degrees: i64) {
        let spec = if degrees != 0 {
            let transpose = match degrees.rem_euclid(360) {
                90  => "1",
                180 => "1,transpose=1",
                270 => "2",
                _   => return,
            };
            Some(format!("@mpvne_rotate:lavfi=[transpose={transpose}]"))
        } else { None };
        self.set_vf_slot("rotate", spec);
    }

    pub fn toggle_hflip(&mut self, on: bool) {
        self.set_vf_slot("hflip", on.then_some("@mpvne_hflip:lavfi=[hflip]".into()));
    }

    pub fn toggle_vflip(&mut self, on: bool) {
        self.set_vf_slot("vflip", on.then_some("@mpvne_vflip:lavfi=[vflip]".into()));
    }

    pub fn seek_chapter(&self, delta: i64) {
        // add-chapter: positive = next, negative = previous
        command_str(self.handle.0, &format!("add chapter {delta}"));
    }

    pub fn open_url(&self, url: &str) {
        command(self.handle.0, &["loadfile", url]);
    }

    pub fn add_sub_file(&self, path: &str) {
        let path_c = CString::new(path).unwrap_or_default();
        command(self.handle.0, &["sub-add", path_c.to_str().unwrap_or("")]);
    }

    pub fn set_loop_file(&self, on: bool) {
        let val = if on { "inf" } else { "no" };
        set_opt_str(self.handle.0, "loop-file", val);
    }

    /// Toggle deinterlacing filter.
    pub fn set_deinterlace(&self, on: bool) {
        let val = if on { "yes" } else { "no" };
        set_opt_str(self.handle.0, "deinterlace", val);
    }

    /// Apply all video EQ values at once via a labelled lavfi `eq` filter.
    /// In mpv 0.37+ the old integer properties are no-ops without this filter.
    /// The label `@mpvne_eq` lets us remove/replace it without touching other
    /// filters (e.g. deinterlace).
    ///
    /// lavfi eq ranges:
    ///   brightness : -1.0 .. 1.0   (our -100..100 → /100)
    ///   contrast   : -1000 .. 1000 (default 1.0; our scale → 1 + val/100)
    ///   saturation : 0 .. 3        (default 1.0; our scale → 1 + val/100)
    ///   hue        : -3.14 .. 3.14 rad (our -100..100 → * π/100)
    ///   gamma      : 0.1 .. 10     (our -100..100 → 2^(val/50))
    pub fn apply_video_eq(&mut self, brightness: i64, contrast: i64, saturation: i64, hue: i64, gamma: i64) {
        let is_default = brightness == 0 && contrast == 0 && saturation == 0 && hue == 0 && gamma == 0;
        if is_default {
            self.set_vf_slot("eq", None);
            return;
        }
        let b = brightness.clamp(-100, 100) as f64 / 100.0;
        let c = (1.0 + contrast.clamp(-100, 100) as f64 / 100.0).max(0.0);
        let s = (1.0 + saturation.clamp(-100, 100) as f64 / 100.0).max(0.0);
        let h = hue.clamp(-100, 100) as f64 * 1.8; // degrees
        let g = 2.0_f64.powf(gamma.clamp(-100, 100) as f64 / 50.0);
        let graph = if hue != 0 {
            format!("eq=brightness={b:.4}:contrast={c:.4}:saturation={s:.4}:gamma={g:.4},hue=h={h:.2}")
        } else {
            format!("eq=brightness={b:.4}:contrast={c:.4}:saturation={s:.4}:gamma={g:.4}")
        };
        let spec = format!("@mpvne_eq:lavfi=[{graph}]");
        tracing::debug!(spec, "applying EQ filter");
        self.set_vf_slot("eq", Some(spec));
    }

    #[allow(dead_code)]
    pub fn reset_video_eq(&mut self) {
        self.set_vf_slot("eq", None);
    }

    pub fn set_screenshot_dir(&self, dir: &str) {
        set_opt_str(self.handle.0, "screenshot-directory", dir);
    }

    pub fn screenshot(&self) {
        command(self.handle.0, &["screenshot"]);
    }

    /// Switch HW decoding on/off at runtime. We toggle by reading the current
    /// hwdec-current; "no" means SW, anything else means HW is active. The
    /// flip targets auto-copy-safe (HW) or no (forced SW).
    pub fn toggle_hwdec(&self) {
        let target = if self.hwdec == "no" || self.hwdec.is_empty() {
            "auto-copy-safe"
        } else {
            "no"
        };
        let n = CString::new("hwdec").unwrap();
        let v = CString::new(target).unwrap();
        unsafe { sys::mpv_set_property_string(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    pub fn set_hwdec(&self, mode: &str) {
        let n = CString::new("hwdec").unwrap();
        let v = CString::new(mode).unwrap();
        unsafe { sys::mpv_set_property_string(self.handle.0, n.as_ptr(), v.as_ptr()) };
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        let name = CString::new("mute").unwrap();
        let mut flag: std::os::raw::c_int = self.muted as _;
        unsafe {
            sys::mpv_set_property(
                self.handle.0,
                name.as_ptr(),
                sys::mpv_format_MPV_FORMAT_FLAG,
                &mut flag as *mut _ as *mut _,
            );
        }
    }

    pub fn set_volume(&mut self, vol: f64) {
        self.volume = vol.clamp(0.0, 200.0);
        set_prop_f64(self.handle.0, "volume", self.volume);
    }

    fn set_pause(&mut self, paused: bool) {
        self.paused = paused;
        let name = CString::new("pause").unwrap();
        let mut flag: std::os::raw::c_int = paused as _;
        unsafe {
            sys::mpv_set_property(
                self.handle.0,
                name.as_ptr(),
                sys::mpv_format_MPV_FORMAT_FLAG,
                &mut flag as *mut _ as *mut _,
            );
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("path", &self.path)
            .field("paused", &self.paused)
            .field("volume", &self.volume)
            .field("position", &self.position)
            .field("duration", &self.duration)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

pub fn event_stream(key: &StreamKey) -> BoxStream<'static, PlayerEvent> {
    let key = key.clone();

    enum State {
        Start(StreamKey),
        Running(tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>),
    }

    Box::pin(stream::unfold(State::Start(key), |state| async {
        match state {
            State::Start(key) => {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                let h = Arc::clone(&key.handle);
                let tx1 = tx.clone();
                std::thread::spawn(move || event_loop(h, tx1));

                let h = Arc::clone(&key.handle);
                let rs = Arc::clone(&key.render_size);
                let tx2 = tx.clone();
                std::thread::spawn(move || init_and_render_loop(h, rs, tx2));

                rx.recv().await.map(|e| (e, State::Running(rx)))
            }
            State::Running(mut rx) => rx.recv().await.map(|e| (e, State::Running(rx))),
        }
    }))
}

// ── mpv event loop ───────────────────────────────────────────────────────────

fn event_loop(handle: Arc<Handle>, tx: UnboundedSender<PlayerEvent>) {
    loop {
        let ev = unsafe { &*sys::mpv_wait_event(handle.0, -1.0) };
        match ev.event_id {
            sys::mpv_event_id_MPV_EVENT_SHUTDOWN => break,
            sys::mpv_event_id_MPV_EVENT_FILE_LOADED => {
                let _ = tx.send(PlayerEvent::FileLoaded);
                // Refresh track lists whenever a new file loads.
                let sub_tracks = fetch_sub_tracks(handle.0);
                let _ = tx.send(PlayerEvent::SubTracks(sub_tracks));
                let audio_tracks = fetch_audio_tracks(handle.0);
                let _ = tx.send(PlayerEvent::AudioTracks(audio_tracks));
                let chapters = fetch_chapters(handle.0);
                let _ = tx.send(PlayerEvent::Chapters(chapters));
            }
            sys::mpv_event_id_MPV_EVENT_END_FILE => {
                let _ = tx.send(PlayerEvent::EndFile);
            }
            sys::mpv_event_id_MPV_EVENT_PROPERTY_CHANGE => {
                if ev.data.is_null() {
                    continue;
                }
                let prop = unsafe { &*(ev.data as *const sys::mpv_event_property) };
                if prop.name.is_null() || prop.data.is_null() {
                    continue;
                }
                let name = unsafe { CStr::from_ptr(prop.name) }.to_str().unwrap_or("");
                match (name, prop.format) {
                    ("time-pos", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let pos = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::Position(pos));
                    }
                    ("duration", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let dur = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::Duration(dur));
                    }
                    ("pause", sys::mpv_format_MPV_FORMAT_FLAG) => {
                        let flag = unsafe { *(prop.data as *const std::os::raw::c_int) };
                        let _ = tx.send(PlayerEvent::Pause(flag != 0));
                    }
                    ("width", sys::mpv_format_MPV_FORMAT_INT64) => {
                        let v = unsafe { *(prop.data as *const i64) };
                        let _ = tx.send(PlayerEvent::Width(v));
                    }
                    ("height", sys::mpv_format_MPV_FORMAT_INT64) => {
                        let v = unsafe { *(prop.data as *const i64) };
                        let _ = tx.send(PlayerEvent::Height(v));
                    }
                    ("audio-params/channel-count", sys::mpv_format_MPV_FORMAT_INT64) => {
                        let v = unsafe { *(prop.data as *const i64) };
                        let _ = tx.send(PlayerEvent::AudioChannels(v));
                    }
                    ("video-format", sys::mpv_format_MPV_FORMAT_STRING) => {
                        if let Some(s) = read_string_prop(prop.data) {
                            let _ = tx.send(PlayerEvent::VideoCodec(s));
                        }
                    }
                    ("audio-codec-name", sys::mpv_format_MPV_FORMAT_STRING) => {
                        if let Some(s) = read_string_prop(prop.data) {
                            let _ = tx.send(PlayerEvent::AudioCodec(s));
                        }
                    }
                    ("hwdec-current", sys::mpv_format_MPV_FORMAT_STRING) => {
                        if let Some(s) = read_string_prop(prop.data) {
                            let _ = tx.send(PlayerEvent::HwDec(s));
                        }
                    }
                    ("video-params/primaries", sys::mpv_format_MPV_FORMAT_STRING) => {
                        if let Some(s) = read_string_prop(prop.data) {
                            let _ = tx.send(PlayerEvent::Primaries(s));
                        }
                    }
                    ("sub-visibility", sys::mpv_format_MPV_FORMAT_FLAG) => {
                        let v = unsafe { *(prop.data as *const std::os::raw::c_int) };
                        let _ = tx.send(PlayerEvent::SubVisible(v != 0));
                    }
                    ("sid", sys::mpv_format_MPV_FORMAT_STRING) => {
                        // sid is "no" when off, otherwise an integer string.
                        let id = read_string_prop(prop.data)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let _ = tx.send(PlayerEvent::CurrentSid(id));
                    }
                    ("aid", sys::mpv_format_MPV_FORMAT_STRING) => {
                        let id = read_string_prop(prop.data)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let _ = tx.send(PlayerEvent::CurrentAid(id));
                    }
                    ("speed", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let v = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::Speed(v));
                    }
                    ("sub-delay", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let v = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::SubDelay(v));
                    }
                    ("audio-delay", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let v = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::AudioDelay(v));
                    }
                    ("sub-font-size", sys::mpv_format_MPV_FORMAT_INT64) => {
                        let v = unsafe { *(prop.data as *const i64) };
                        let _ = tx.send(PlayerEvent::SubFontSize(v));
                    }
                    ("sub-pos", sys::mpv_format_MPV_FORMAT_INT64) => {
                        let v = unsafe { *(prop.data as *const i64) };
                        let _ = tx.send(PlayerEvent::SubPos(v));
                    }
                    ("loop-file", sys::mpv_format_MPV_FORMAT_STRING) => {
                        let on = read_string_prop(prop.data)
                            .map(|s| s != "no")
                            .unwrap_or(false);
                        let _ = tx.send(PlayerEvent::LoopFile(on));
                    }
                    ("loop-playlist", sys::mpv_format_MPV_FORMAT_STRING) => {
                        let on = read_string_prop(prop.data)
                            .map(|s| s != "no")
                            .unwrap_or(false);
                        let _ = tx.send(PlayerEvent::LoopPlaylist(on));
                    }
                    ("video-zoom", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let v = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::VideoZoom(v));
                    }
                    ("demuxer-cache-time", sys::mpv_format_MPV_FORMAT_DOUBLE) => {
                        let v = unsafe { *(prop.data as *const f64) };
                        let _ = tx.send(PlayerEvent::CacheTime(v));
                    }
                    ("deinterlace", sys::mpv_format_MPV_FORMAT_FLAG) => {
                        let v = unsafe { *(prop.data as *const std::os::raw::c_int) };
                        let _ = tx.send(PlayerEvent::Deinterlace(v != 0));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

// ── SW render loop ───────────────────────────────────────────────────────────

// mpv_render_param_type integer values (from mpv/render.h)
const RPT_INVALID: u32 = 0;
const RPT_API_TYPE: u32 = 1;
const RPT_SW_SIZE: u32 = 17;
const RPT_SW_FORMAT: u32 = 18;
const RPT_SW_STRIDE: u32 = 19;
const RPT_SW_POINTER: u32 = 20;

unsafe extern "C" fn wakeup_cb(ctx: *mut std::ffi::c_void) {
    let arc = unsafe { &*(ctx as *const Arc<RenderSize>) };
    arc.signal();
}

fn init_and_render_loop(
    handle: Arc<Handle>,
    render_size: Arc<RenderSize>,
    tx: UnboundedSender<PlayerEvent>,
) {
    let mut ctx: *mut sys::mpv_render_context = std::ptr::null_mut();
    let api = b"sw\0";
    let mut init_params = [
        sys::mpv_render_param { type_: RPT_API_TYPE, data: api.as_ptr() as *mut _ },
        sys::mpv_render_param { type_: RPT_INVALID, data: std::ptr::null_mut() },
    ];
    let rc = unsafe {
        sys::mpv_render_context_create(&mut ctx, handle.0, init_params.as_mut_ptr())
    };
    if rc != 0 {
        tracing::error!(rc, "mpv_render_context_create failed");
        return;
    }
    tracing::info!("SW render context ready");

    // Leak a Box<Arc<RenderSize>> for the C callback so it has a stable pointer.
    let cb_box = Box::new(Arc::clone(&render_size));
    let cb_ptr = Box::into_raw(cb_box) as *mut std::ffi::c_void;
    unsafe {
        sys::mpv_render_context_set_update_callback(ctx, Some(wakeup_cb), cb_ptr);
    }

    let mut last_w: u32 = 0;
    let mut last_h: u32 = 0;

    loop {
        // Wait for either mpv (new frame) or set_render_size (resize) to signal.
        {
            let mut flag = render_size.wake_flag.lock().unwrap();
            while !*flag {
                flag = render_size.wake_cv.wait(flag).unwrap();
            }
            *flag = false;
        }

        let w = render_size.w.load(Ordering::Relaxed);
        let h = render_size.h.load(Ordering::Relaxed);
        if w == 0 || h == 0 {
            continue;
        }

        let size_changed = (w, h) != (last_w, last_h);
        let flags = unsafe { sys::mpv_render_context_update(ctx) };
        let frame_ready = flags & 1 != 0;

        // Render if mpv has a new frame OR the target size changed (so the
        // current frame is re-rasterised at the new size - handles resize
        // while paused).
        if !frame_ready && !size_changed {
            continue;
        }

        last_w = w;
        last_h = h;

        let stride = (w * 4) as usize;
        // Pre-fill with opaque black so letterbox areas aren't transparent.
        let mut buf = vec![0u8; stride * h as usize];
        for px in buf.chunks_exact_mut(4) {
            px[3] = 255;
        }
        let size = [w as i32, h as i32];
        let fmt = b"rgba\0";

        let mut rp = [
            sys::mpv_render_param { type_: RPT_SW_SIZE,    data: size.as_ptr() as *mut _ },
            sys::mpv_render_param { type_: RPT_SW_FORMAT,  data: fmt.as_ptr() as *mut _ },
            sys::mpv_render_param { type_: RPT_SW_STRIDE,  data: &stride as *const usize as *mut _ },
            sys::mpv_render_param { type_: RPT_SW_POINTER, data: buf.as_mut_ptr() as *mut _ },
            sys::mpv_render_param { type_: RPT_INVALID,    data: std::ptr::null_mut() },
        ];

        let rc = unsafe { sys::mpv_render_context_render(ctx, rp.as_mut_ptr()) };
        if rc == 0 {
            if tx.send(PlayerEvent::Frame(buf, w, h)).is_err() {
                break;
            }
        } else {
            tracing::warn!(rc, "mpv_render_context_render failed");
        }
    }

    unsafe {
        sys::mpv_render_context_set_update_callback(ctx, None, std::ptr::null_mut());
        drop(Box::from_raw(cb_ptr as *mut Arc<RenderSize>));
        sys::mpv_render_context_free(ctx);
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn command(h: *mut sys::mpv_handle, args: &[&str]) {
    let cstrings: Vec<CString> = args.iter().map(|s| CString::new(*s).unwrap()).collect();
    let mut ptrs: Vec<*const std::os::raw::c_char> = cstrings.iter().map(|s| s.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    let rc = unsafe { sys::mpv_command(h, ptrs.as_mut_ptr()) };
    if rc < 0 {
        tracing::warn!(?args, rc, "mpv_command failed");
    }
}

/// Use mpv_command_string which handles complex filter specs with colons/brackets correctly.
fn command_str(h: *mut sys::mpv_handle, cmd: &str) {
    let c = CString::new(cmd).unwrap_or_default();
    let rc = unsafe { sys::mpv_command_string(h, c.as_ptr()) };
    if rc < 0 {
        tracing::warn!(cmd, rc, "mpv_command_string failed");
    }
}

fn set_opt_str(h: *mut sys::mpv_handle, name: &str, val: &str) {
    let n = CString::new(name).unwrap();
    let v = CString::new(val).unwrap();
    unsafe { sys::mpv_set_option_string(h, n.as_ptr(), v.as_ptr()) };
}

#[allow(dead_code)]
fn set_prop_i64(h: *mut sys::mpv_handle, name: &str, val: i64) {
    let n = CString::new(name).unwrap();
    let mut v = val;
    unsafe {
        sys::mpv_set_property(
            h,
            n.as_ptr(),
            sys::mpv_format_MPV_FORMAT_INT64,
            &mut v as *mut i64 as *mut _,
        )
    };
}

fn set_prop_f64(h: *mut sys::mpv_handle, name: &str, val: f64) {
    let n = CString::new(name).unwrap();
    let mut v = val;
    unsafe {
        sys::mpv_set_property(
            h,
            n.as_ptr(),
            sys::mpv_format_MPV_FORMAT_DOUBLE,
            &mut v as *mut f64 as *mut _,
        )
    };
}

/// mpv string-property data is a `char**` - read the inner pointer as a C string.
fn read_string_prop(data: *mut std::ffi::c_void) -> Option<String> {
    if data.is_null() {
        return None;
    }
    let str_ptr = unsafe { *(data as *const *const std::os::raw::c_char) };
    if str_ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(str_ptr) }.to_str().ok()?;
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// Read `track-list` properties and return the subtitle tracks.
/// Always includes a synthetic "Off" entry as id=0 at the top of the list.
fn fetch_sub_tracks(h: *mut sys::mpv_handle) -> Vec<SubTrack> {
    let mut tracks = vec![SubTrack { id: 0, label: "Off".to_string() }];
    let count = get_prop_i64(h, "track-list/count");
    for i in 0..count {
        let kind = get_prop_string(h, &format!("track-list/{i}/type"));
        if kind.as_deref() != Some("sub") {
            continue;
        }
        let id = get_prop_i64(h, &format!("track-list/{i}/id"));
        let lang = get_prop_string(h, &format!("track-list/{i}/lang"));
        let title = get_prop_string(h, &format!("track-list/{i}/title"));
        let label = match (lang, title) {
            (Some(l), Some(t)) => format!("{l}  ({t})"),
            (Some(l), None) => l,
            (None, Some(t)) => t,
            (None, None) => format!("Track {id}"),
        };
        tracks.push(SubTrack { id, label });
    }
    tracks
}

/// Read `track-list` and return the audio tracks.
/// Always includes a synthetic "Off" entry as id=0 at the top of the list.
fn fetch_audio_tracks(h: *mut sys::mpv_handle) -> Vec<AudioTrack> {
    let mut tracks = vec![AudioTrack { id: 0, label: "Off".to_string() }];
    let count = get_prop_i64(h, "track-list/count");
    for i in 0..count {
        let kind = get_prop_string(h, &format!("track-list/{i}/type"));
        if kind.as_deref() != Some("audio") {
            continue;
        }
        let id = get_prop_i64(h, &format!("track-list/{i}/id"));
        let lang = get_prop_string(h, &format!("track-list/{i}/lang"));
        let title = get_prop_string(h, &format!("track-list/{i}/title"));
        let label = match (lang, title) {
            (Some(l), Some(t)) => format!("{l}  ({t})"),
            (Some(l), None) => l,
            (None, Some(t)) => t,
            (None, None) => format!("Track {id}"),
        };
        tracks.push(AudioTrack { id, label });
    }
    tracks
}

fn fetch_chapters(h: *mut sys::mpv_handle) -> Vec<Chapter> {
    let count = get_prop_i64(h, "chapter-list/count");
    let mut chapters = Vec::with_capacity(count.max(0) as usize);
    for i in 0..count {
        let time = get_prop_f64(h, &format!("chapter-list/{i}/time"));
        let title = get_prop_string(h, &format!("chapter-list/{i}/title"));
        if let Some(t) = time {
            chapters.push(Chapter { time: t, title });
        }
    }
    chapters
}

fn get_prop_f64(h: *mut sys::mpv_handle, name: &str) -> Option<f64> {
    let n = CString::new(name).ok()?;
    let mut v: f64 = 0.0;
    let rc = unsafe {
        sys::mpv_get_property(
            h,
            n.as_ptr(),
            sys::mpv_format_MPV_FORMAT_DOUBLE,
            &mut v as *mut f64 as *mut _,
        )
    };
    if rc == 0 { Some(v) } else { None }
}

fn get_prop_i64(h: *mut sys::mpv_handle, name: &str) -> i64 {
    let n = CString::new(name).unwrap();
    let mut v: i64 = 0;
    unsafe {
        sys::mpv_get_property(
            h,
            n.as_ptr(),
            sys::mpv_format_MPV_FORMAT_INT64,
            &mut v as *mut i64 as *mut _,
        );
    }
    v
}

fn get_prop_string(h: *mut sys::mpv_handle, name: &str) -> Option<String> {
    let n = CString::new(name).ok()?;
    let raw = unsafe { sys::mpv_get_property_string(h, n.as_ptr()) };
    if raw.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(raw) }.to_string_lossy().to_string();
    unsafe { sys::mpv_free(raw as *mut _) };
    if s.is_empty() { None } else { Some(s) }
}

fn observe(h: *mut sys::mpv_handle, id: u64, prop: &str, fmt: sys::mpv_format) {
    let name = CString::new(prop).unwrap();
    unsafe { sys::mpv_observe_property(h, id, name.as_ptr(), fmt) };
}
