use futures::stream::BoxStream;
use futures::StreamExt as _;
use iced::{Element, Event, Subscription, Task, Theme};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::player::{Player, PlayerEvent, StreamKey};
use crate::resume::{RecentFiles, ResumeDb};
use crate::ui;
use crate::ui::video::VideoFrame;

/// Which panel is currently docked to the right side of the window.
/// A lightweight in-app text-input modal, themed to match the rest of the UI.
#[derive(Debug, Clone)]
pub struct ModalDialog {
    pub title:       &'static str,
    pub prompt:      &'static str,
    pub input:       String,
    pub kind:        ModalKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    JumpToTime,
    OpenUrl,
    /// Add a URL/stream to the playlist without opening it immediately -
    /// see `Message::ModalConfirm`'s arm for this kind.
    AddPlaylistUrl,
}

#[derive(Debug, Clone)]
pub struct FileContextMenu {
    pub path: std::path::PathBuf,
    /// Window coordinates where the right-click occurred.
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AfterPlayback {
    #[default]
    DoNothing,
    NextFile,
    LoopFile,
    ClosePlayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSort {
    Name,
    NameDesc,
    Size,
    SizeDesc,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Playlist,
    Browser,
    Recent,
    Settings,
}

/// Left-nav categories in the standalone App Settings window (kept separate
/// from the docked side panel's Settings tab, which stays playback-only -
/// see `AppSettingsCategory`'s use in `ui::app_settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppSettingsCategory {
    Interface,
    Keyboard,
    Mouse,
}

impl Default for AppSettingsCategory {
    fn default() -> Self {
        Self::Interface
    }
}

/// One entry in the directory browser panel.
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_dir: bool,
}

/// Player-level actions. Inputs (keys, clicks, scroll) and UI buttons both
/// resolve to one of these via [`Bindings`]. Add new variants as features grow
/// - every new control should be an Action so it can be remapped uniformly.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Variants are a public-ish surface for user-defined bindings.
pub enum Action {
    TogglePause,
    Stop,
    SeekRelative(f64),
    VolumeAdjust(f64),
    ToggleMute,
    ToggleFullscreen,
    ToggleChrome,
    CycleSubtitle,
    CycleAudio,
    SpeedAdjust(f64),
    SpeedReset,
    ToggleSubVisibility,
    ToggleHwDec,
    ToggleMaximize,
    TogglePin,
    FitToVisible,
    FitToScale(f32),
    FitToHeight(u32),
    PrevFile,
    NextFile,
    OpenFile,
    AddBookmark,
    ToggleStats,
    CycleFrameMode,
    /// Seek by the configurable step size (`MpvNe::seek_step_secs`),
    /// forward if `true`. Distinct from `SeekRelative` (a fixed amount)
    /// since `KEY_SLOTS` bakes its action at compile time and can't read
    /// runtime app state directly.
    SeekStep(bool),
    /// Step exactly one video frame, forward if `true`.
    FrameStep(bool),
    /// Adjust speed by the configurable step size (`MpvNe::speed_step`),
    /// faster if `true` - same reasoning as `SeekStep`.
    SpeedStep(bool),
    /// Seek to the start of the next/previous subtitle line, forward if `true`.
    SubSeek(bool),
}

/// How the video frame is fitted into the window. Cycled with the Z key and
/// applied by the letterbox shader (mpv renders at native size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FrameMode {
    /// Whole frame visible, black bars on the limiting axis.
    #[default]
    Fit,
    /// Scale up to cover the window, cropping overflow; aspect preserved.
    Fill,
    /// Stretch to fill the window exactly, distorting the aspect ratio.
    Stretch,
}

impl FrameMode {
    fn next(self) -> Self {
        match self {
            FrameMode::Fit => FrameMode::Fill,
            FrameMode::Fill => FrameMode::Stretch,
            FrameMode::Stretch => FrameMode::Fit,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            FrameMode::Fit => "Fit",
            FrameMode::Fill => "Fill",
            FrameMode::Stretch => "Stretch",
        }
    }
}

/// The fixed list of rebindable key actions: (stable slot id - used as the
/// persistence key and never shown to the user, display label, default key
/// name, action). `Bindings::keys` is built from this plus any user
/// overrides rather than hardcoded per-key like before, so a rebind is just
/// "look up this slot's key instead of its default" - see `from_overrides`.
/// Escape is NOT here - it exits fullscreen/chrome-hidden only, never enters
/// fullscreen, and is handled directly in the `InputKey` handler instead.
pub const KEY_SLOTS: &[(&str, &str, &str, Action)] = &[
    ("toggle_pause", "Play / pause", "space", Action::TogglePause),
    ("seek_back", "Seek back (step)", "left", Action::SeekStep(false)),
    ("seek_fwd", "Seek forward (step)", "right", Action::SeekStep(true)),
    ("volume_up", "Volume up", "up", Action::VolumeAdjust(5.0)),
    ("volume_down", "Volume down", "down", Action::VolumeAdjust(-5.0)),
    ("prev_file", "Previous file", "pageup", Action::PrevFile),
    ("next_file", "Next file", "pagedown", Action::NextFile),
    ("toggle_mute", "Mute", "m", Action::ToggleMute),
    ("toggle_fullscreen", "Fullscreen", "f", Action::ToggleFullscreen),
    ("toggle_chrome", "Focus mode", "h", Action::ToggleChrome),
    ("cycle_subtitle", "Cycle subtitle track", "j", Action::CycleSubtitle),
    ("cycle_audio", "Cycle audio track", "#", Action::CycleAudio),
    ("speed_down", "Speed down (step)", "[", Action::SpeedStep(false)),
    ("speed_up", "Speed up (step)", "]", Action::SpeedStep(true)),
    ("speed_reset", "Reset speed", "\\", Action::SpeedReset),
    ("toggle_sub_visibility", "Toggle subtitles", "v", Action::ToggleSubVisibility),
    ("toggle_hwdec", "Toggle hardware decode", "i", Action::ToggleHwDec),
    ("add_bookmark", "Add bookmark", "b", Action::AddBookmark),
    ("toggle_stats", "Stats overlay", "s", Action::ToggleStats),
    ("cycle_frame_mode", "Cycle frame fit", "z", Action::CycleFrameMode),
    ("frame_step_back", "Step back one frame", ",", Action::FrameStep(false)),
    ("frame_step_fwd", "Step forward one frame", ".", Action::FrameStep(true)),
    ("sub_seek_prev", "Previous subtitle", "p", Action::SubSeek(false)),
    ("sub_seek_next", "Next subtitle", "n", Action::SubSeek(true)),
];

/// Maps every input trigger to an optional Action. `None` means the input is
/// unbound - for example, single-left-click defaults to `None` so clicking the
/// video doesn't toggle pause unless the user opts in.
#[derive(Debug)]
pub struct Bindings {
    /// Lowercase-normalised key names: "space", "left", "f", etc.
    pub keys: HashMap<String, Action>,
    pub single_left_click: Option<Action>,
    pub double_left_click: Option<Action>,
    pub scroll_up: Option<Action>,
    pub scroll_down: Option<Action>,
    /// When true, left-click on any non-interactive area starts a window drag.
    /// Off-switch since users may want classic click-to-pause behaviour later.
    pub drag_window_anywhere: bool,
}

/// Preset id for each of the 4 mouse triggers - persisted instead of the
/// raw `Action` (mirrors `KEY_SLOTS`' slot-id approach) since `Action`
/// itself has no `PartialEq`/serialization story, and a fixed preset list
/// is what the Mouse settings page actually offers anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseTrigger {
    SingleClick,
    DoubleClick,
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone)]
pub struct MouseBindingIds {
    pub single_click: String,
    pub double_click: String,
    pub scroll_up: String,
    pub scroll_down: String,
}

impl Default for MouseBindingIds {
    fn default() -> Self {
        Self {
            single_click: "none".into(),
            double_click: "toggle_fullscreen".into(),
            scroll_up: "volume_up_2".into(),
            scroll_down: "volume_down_2".into(),
        }
    }
}

/// (preset id, display label, action). `None` action = "Unbound". Shared by
/// all 4 mouse trigger pickers in the Settings window's Mouse category.
pub const MOUSE_ACTION_PRESETS: &[(&str, &str, Option<Action>)] = &[
    ("none", "Unbound", None),
    ("toggle_pause", "Play / pause", Some(Action::TogglePause)),
    ("toggle_mute", "Mute", Some(Action::ToggleMute)),
    ("toggle_fullscreen", "Fullscreen", Some(Action::ToggleFullscreen)),
    ("toggle_chrome", "Focus mode", Some(Action::ToggleChrome)),
    ("volume_up_2", "Volume +2", Some(Action::VolumeAdjust(2.0))),
    ("volume_down_2", "Volume -2", Some(Action::VolumeAdjust(-2.0))),
    ("volume_up_5", "Volume +5", Some(Action::VolumeAdjust(5.0))),
    ("volume_down_5", "Volume -5", Some(Action::VolumeAdjust(-5.0))),
    ("seek_fwd_5", "Seek +5s", Some(Action::SeekRelative(5.0))),
    ("seek_back_5", "Seek -5s", Some(Action::SeekRelative(-5.0))),
    ("seek_fwd_10", "Seek +10s", Some(Action::SeekRelative(10.0))),
    ("seek_back_10", "Seek -10s", Some(Action::SeekRelative(-10.0))),
    ("next_file", "Next file", Some(Action::NextFile)),
    ("prev_file", "Previous file", Some(Action::PrevFile)),
    ("cycle_subtitle", "Cycle subtitle", Some(Action::CycleSubtitle)),
    ("cycle_audio", "Cycle audio", Some(Action::CycleAudio)),
    ("toggle_stats", "Stats overlay", Some(Action::ToggleStats)),
];

fn resolve_mouse_action(id: &str) -> Option<Action> {
    MOUSE_ACTION_PRESETS.iter().find(|(pid, ..)| *pid == id).and_then(|(_, _, a)| *a)
}

impl Bindings {
    /// Build the key map from `KEY_SLOTS`, applying user overrides
    /// (slot id -> key name, keyed by `Settings.keybindings`). An override
    /// value of `""` means the slot was explicitly cleared (no key bound at
    /// all, not even the default) - see `MpvNe::apply_key_rebind`.
    pub fn from_overrides(overrides: &HashMap<String, String>, mouse: &MouseBindingIds) -> Self {
        let mut keys = HashMap::new();
        for (slot_id, _label, default_key, action) in KEY_SLOTS {
            let key: Option<&str> = match overrides.get(*slot_id) {
                Some(k) if k.is_empty() => None,
                Some(k) => Some(k.as_str()),
                None => Some(*default_key),
            };
            if let Some(key) = key {
                keys.insert(key.to_string(), *action);
            }
        }
        Self {
            keys,
            single_left_click: resolve_mouse_action(&mouse.single_click),
            double_left_click: resolve_mouse_action(&mouse.double_click),
            scroll_up: resolve_mouse_action(&mouse.scroll_up),
            scroll_down: resolve_mouse_action(&mouse.scroll_down),
            drag_window_anywhere: true,
        }
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self::from_overrides(&HashMap::new(), &MouseBindingIds::default())
    }
}

/// Convert an Action into its equivalent Message so the existing handlers can
/// execute it. Lets us keep the binding lookup path simple while reusing the
/// existing implementations.
/// App icon for secondary OS windows (detached panel, App Settings) - without
/// this, Windows falls back to a generic icon in the taskbar/alt-tab instead
/// of the app's own icon. The main window sets this via `boot()`.
fn app_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_file_data(
        include_bytes!("../assets/MPV_NE_icon_hires.png"),
        None,
    ).ok()
}

fn action_to_message(a: Action) -> Message {
    match a {
        Action::TogglePause => Message::TogglePause,
        Action::Stop => Message::Stop,
        Action::SeekRelative(d) => Message::SeekRelative(d),
        Action::VolumeAdjust(d) => Message::VolumeAdjust(d),
        Action::ToggleMute => Message::ToggleMute,
        Action::ToggleFullscreen => Message::ToggleFullscreen,
        Action::ToggleChrome => Message::ToggleChrome,
        Action::CycleSubtitle => Message::CycleSubtitle,
        Action::CycleAudio => Message::CycleAudio,
        Action::SpeedAdjust(d) => Message::SpeedAdjust(d),
        Action::SpeedReset => Message::SpeedReset,
        Action::ToggleSubVisibility => Message::ToggleSubVisibility,
        Action::ToggleHwDec => Message::ToggleHwDec,
        Action::ToggleMaximize => Message::ToggleMaximize,
        Action::TogglePin => Message::TogglePin,
        Action::FitToVisible => Message::FitToVisible,
        Action::FitToScale(s) => Message::FitToScale(s),
        Action::FitToHeight(h) => Message::FitToHeight(h),
        Action::PrevFile => Message::PrevFile,
        Action::NextFile => Message::NextFile,
        Action::OpenFile => Message::OpenFile,
        Action::AddBookmark => Message::AddBookmark,
        Action::ToggleStats => Message::ToggleStats,
        Action::CycleFrameMode => Message::CycleFrameMode,
        Action::SeekStep(fwd) => Message::SeekStep(fwd),
        Action::FrameStep(fwd) => Message::FrameStep(fwd),
        Action::SpeedStep(faster) => Message::SpeedStep(faster),
        Action::SubSeek(fwd) => Message::SubSeek(fwd),
    }
}

pub const CONTROLS_H: i32 = 76;
const TOP_BAR_H: i32 = 44;
/// Width of the docked side panel in logical pixels. Must match SETTINGS_PANEL_W in ui/mod.rs.
const PANEL_W: f32 = 280.0;

/// Whether to disable the OS title bar and draw our own - gives us pin,
/// minimize, maximize, close all on the Nord top bar. When off, falls back
/// to the OS title bar (still shown, just without our min/max/close buttons
/// since the OS provides those). Backed by an atomic rather than a plain
/// bool field because window decorations are read from many places
/// (including free functions with no `&MpvNe`); set once from Settings at
/// startup - toggling it takes a restart, since decorations are fixed at
/// window-creation time.
static CUSTOM_TITLE_BAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn use_custom_title_bar() -> bool {
    CUSTOM_TITLE_BAR.load(std::sync::atomic::Ordering::Relaxed)
}

fn set_custom_title_bar(on: bool) {
    CUSTOM_TITLE_BAR.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Debug)]
pub struct MpvNe {
    pub player: Player,
    pub current_frame: Option<VideoFrame>,
    pending_w: u32,
    pending_h: u32,
    resize_seq: u64,
    render_initialized: bool,
    pub fullscreen: bool,
    window_id: Option<iced::window::Id>,
    /// OS DPI scale factor for the main window, tracked for informational
    /// purposes from the Rescaled event. Not used for size-critical work -
    /// see ResizeDpiQueried and boot() for why (a live window::scale_factor()
    /// query, matching the Fit menu, proved more reliable than this event).
    scale_factor: f32,
    modal_hook_installed: bool,
    pub playlist: Vec<std::path::PathBuf>,
    pub playlist_idx: usize,
    pub bindings: Bindings,
    /// Key-rebind overrides (slot id -> key name), the source of truth
    /// `bindings.keys` is rebuilt from via `Bindings::from_overrides` on
    /// every rebind - see `apply_key_rebind`. Kept separate from `bindings`
    /// itself since `Bindings` only stores the resolved key->Action map, not
    /// which slot each key came from.
    pub keybinding_overrides: HashMap<String, String>,
    /// Slot id awaiting its next key press to bind, or `None` when not
    /// actively capturing. Set by the Keyboard settings page's "Rebind"
    /// button; consumed by the next `InputKey` event.
    pub rebind_capture: Option<&'static str>,
    /// Current preset id per mouse trigger - the source of truth
    /// `bindings.{single_left_click,double_left_click,scroll_up,scroll_down}`
    /// is rebuilt from via `Bindings::from_overrides` whenever a mouse
    /// binding changes (`Message::SetMouseBinding`).
    pub mouse_bindings: MouseBindingIds,
    last_left_press: Option<Instant>,
    /// Set while click-dragging the zoomed-in video to pan it (instead of
    /// moving the window, which is what the same drag-anywhere click zone
    /// does at normal zoom). `(start_cursor_x, start_cursor_y, start_pan_x,
    /// start_pan_y)` captured at drag start; deltas are applied relative to it.
    pan_drag_start: Option<(f32, f32, f64, f64)>,
    /// Manual override - true means hide controls even when not fullscreen.
    /// Effective visibility is `chrome_visible()`; controls are hidden when
    /// in fullscreen *or* when this flag is set.
    pub chrome_force_hidden: bool,
    /// Cursor position in logical pixels relative to the window client area.
    /// `None` when the cursor is outside the window. Used to overlay the
    /// controls on hover while chrome is hidden.
    pub cursor_pos: Option<(f32, f32)>,
    /// Same as `cursor_pos` but for the detached panel window - kept
    /// separate since both windows report `CursorMoved` independently and
    /// sharing one field would let whichever moved last clobber the other's
    /// resize-edge detection.
    panel_cursor_pos: Option<(f32, f32)>,
    /// Last known window dimensions and position in logical pixels.
    pub window_h_logical: f32,
    pub window_w_logical: f32,
    pub window_x_logical: i32,
    pub keyboard_modifiers: iced::keyboard::Modifiers,
    pub window_y_logical: i32,
    /// True when always-on-top is enabled via the pin button.
    pub pinned: bool,
    /// True while Picture-in-Picture mode is active: window shrunk to a
    /// small corner-docked size with chrome hidden and pinned on top,
    /// automating what you'd otherwise do manually with Focus mode + Pin.
    pub pip_active: bool,
    /// Window size and position saved when entering PiP, restored on exit.
    pip_prev_w: f32,
    pip_prev_h: f32,
    pip_prev_x: i32,
    pip_prev_y: i32,
    pip_prev_chrome_hidden: bool,
    pip_prev_pinned: bool,
    /// True while the subtitle picker popup is open; chrome stays visible
    /// and Escape closes it instead of doing whatever Escape is bound to.
    pub subs_menu_open: bool,
    /// True after Stop is pressed: video frame is hidden and any frames
    /// mpv emits before our state catches up are dropped. Cleared on Play
    /// or when a new file finishes loading.
    pub stopped: bool,
    /// Pre-decoded image handles - created once to avoid re-uploading every frame.
    pub img_icon: iced::widget::image::Handle,
    pub img_logo: iced::widget::image::Handle,
    pub thumb_cache: crate::thumbnail::SharedCache,
    /// True while we are transitioning from one file to the next (open() was
    /// called but FileLoaded has not fired yet). During this window EndFile
    /// must not clear player.path or player.paused because they already hold
    /// the new-file values.
    pub transitioning: bool,
    pub after_playback: AfterPlayback,
    pub modal: Option<ModalDialog>,
    pub resume_enabled: bool,
    /// Private/no-trace mode: while on, resume position, recent files, and
    /// per-file memory (audio/subtitle track, volume, bookmarks) stop being
    /// written to disk for the session - see ResumeDb/RecentFiles::private.
    /// Existing history from before it was enabled still reads normally;
    /// only new writes are suppressed.
    pub private_mode: bool,
    /// Dynamic audio normalization (ffmpeg `dynaudnorm`). See `Settings::audio_normalize`.
    pub audio_normalize: bool,
    /// Preferred audio/subtitle track languages (ISO 639 codes, e.g. "eng").
    /// Empty = no preference. See `Settings::audio_lang`/`sub_lang`.
    pub audio_lang: String,
    pub sub_lang: String,
    /// Whether seeking lands on the exact frame ("precise") or the nearest
    /// keyframe (faster, less exact). See `Settings::precise_seek`.
    pub precise_seek: bool,
    /// Seconds a single seek-step covers - Left/Right keys, the transport
    /// skip buttons, and the menu's Back/Forward items all share this.
    /// See `Settings::playback.seek_step_secs`.
    pub seek_step_secs: f64,
    /// Playback-speed increment for the speed up/down keys and settings
    /// nudge buttons. See `Settings::playback.speed_step`.
    pub speed_step: f64,
    /// Max height yt-dlp is allowed to grab for network streams. 0 = uncapped.
    /// See `Settings::stream_quality_height`/`Player::set_stream_quality`.
    pub stream_quality_height: u32,
    pub screenshot_dir: String,
    /// Windows-only window-to-window/edge snapping while dragging. See
    /// `win32_modal::set_snap_enabled`/`Settings::interface.snap_to_edge`.
    pub snap_to_edge: bool,
    /// Restore the last window position/size on launch. See boot().
    pub remember_window: bool,
    /// "Always start pinned" preference - distinct from the live `pinned`
    /// field (which the Pin button toggles mid-session and shouldn't
    /// overwrite this on every press).
    pub start_pinned_pref: bool,
    /// The saved custom-title-bar preference, for display in Settings.
    /// Deliberately separate from the live `use_custom_title_bar()` flag
    /// that windows are actually created with - changing this takes effect
    /// on next launch only, so every window opened *this* session (main,
    /// panel, App Settings) keeps using the same decoration mode instead of
    /// windows opened before vs. after the toggle disagreeing with each other.
    pub custom_title_bar_pref: bool,
    /// Show OSD notification popups (volume/seek/speed/etc.).
    pub osd_enabled: bool,
    /// Generate and show the seekbar thumbnail scrub preview.
    pub thumbnail_preview: bool,
    /// Re-download the latest yt-dlp at every startup instead of only when missing.
    pub auto_update_ytdlp: bool,
    /// Windows-only: minimize the detached panel/App Settings windows along
    /// with the main window. See `Message::MinimizeCheckTick`.
    pub hide_all_on_minimize: bool,
    pub pause_on_focus_lost: bool,
    pub pause_on_minimize: bool,
    /// Windows-only: minimizing the main window hides it to a tray icon
    /// instead of the taskbar.
    pub minimize_to_tray: bool,
    /// Tracks whether the main window was minimized as of the last poll -
    /// so hide/pause-on-minimize only fire on the transition, not every tick.
    main_window_was_minimized: bool,
    /// Queue sibling media files from the same folder into the playlist
    /// when opening a file.
    pub auto_load_siblings: bool,
    /// Windows-only: whether opening a second file hands off to an
    /// already-running instance instead of starting a new process. The
    /// actual claim/hand-off happens in `main()` before this struct even
    /// exists - this field is just for display/toggling in Settings, and
    /// for the polling subscription that receives forwarded files (see
    /// `Message::PollSingleInstance`).
    pub single_instance: bool,
    /// URL waiting on `download_ytdlp` to finish before it can be opened -
    /// set when a URL needs yt-dlp and it isn't available yet - see
    /// `Message::ModalConfirm`'s `ModalKind::OpenUrl` arm and
    /// `Message::YtdlpDownloadResult`.
    pending_ytdl_url: Option<String>,
    /// Suppresses the next volume OSD (used when restoring saved volume at startup).
    suppress_volume_osd: bool,
    suppress_speed_osd: bool,
    pub show_help: bool,
    pub sub_search_open: bool,
    pub sub_search_query: String,
    pub sub_search_results: Vec<crate::opensubs::SubResult>,
    pub sub_search_loading: bool,
    pub opensubtitles_api_key: String,
    file_info_osd_shown: bool,
    /// File size cache: populated once per path in a background task.
    pub size_cache: std::collections::HashMap<std::path::PathBuf, u64>,
    /// Title/duration/uploader probed for playlist URL entries, keyed by
    /// the URL string - see `fetch_url_metadata`. Not persisted; re-probed
    /// each session (playlist URLs are re-fetched on add and on load).
    pub playlist_url_meta: std::collections::HashMap<String, UrlMeta>,
    pub video_rotate: i64,   // 0 / 90 / 180 / 270
    pub video_hflip: bool,
    pub video_vflip: bool,
    pub fit_menu_open: bool,
    pub audio_menu_open: bool,
    /// Cursor X (in window coords) when a popup menu was last opened.
    /// Used to anchor the popup above whichever button was clicked.
    pub popup_anchor_x: f32,
    pub playlist_sort_open: bool,
    /// Context menu for file entries in panels.
    pub file_context_menu: Option<FileContextMenu>,
    /// Position of the modal text field's right-click "Paste" menu, if open.
    pub modal_paste_menu: Option<(f32, f32)>,
    /// The floating main-menu popup: a genuine second OS window (not an
    /// in-window overlay), so it can extend past the main window's edges
    /// like a native right-click menu. `None` when closed. See
    /// `open_main_menu`/`close_main_menu` and `MpvNe::view`.
    pub menu_window_id: Option<iced::window::Id>,
    /// Collapsed/expanded state of the main-menu popup's collapsible
    /// sections (0 = Playback, 1 = Video & Audio). Collapsed by default to
    /// keep the popup short; see `ui::menu_rows`/`ui::menu_window_height`.
    pub menu_section_open: [bool; 2],
    /// Screen-space anchor point the popup was opened at (unclamped), kept
    /// so `ToggleMenuSection` can re-clamp against the monitor work area
    /// whenever the popup's height changes, without drifting from repeated
    /// re-clamping of an already-clamped position.
    menu_anchor: Option<(f32, f32)>,
    /// The docked side panel (Playlist/Browser/Recent/Settings), popped out
    /// into its own resizable/movable OS window instead of docked to the
    /// main window. `None` = docked (normal). `active_panel` still tracks
    /// which tab is showing either way - this only changes where it's drawn.
    pub panel_window_id: Option<iced::window::Id>,
    /// Screen position the detached panel window was last seen at (updated
    /// on every move), so re-detaching after a dock/undock cycle reopens it
    /// where you left it instead of wherever the OS defaults to.
    panel_last_pos: Option<(i32, i32)>,
    /// Size the detached panel window was last resized to - same idea as
    /// `panel_last_pos`, so re-detaching reopens it at the size you left it,
    /// not always back at the docked-panel default.
    panel_last_size: Option<(f32, f32)>,
    /// The standalone App Settings window (Interface/Keyboard/...) - always
    /// its own window, never dockable, unlike the side panel above. `None`
    /// when closed.
    pub app_settings_window_id: Option<iced::window::Id>,
    app_settings_cursor_pos: Option<(f32, f32)>,
    app_settings_last_pos: Option<(i32, i32)>,
    app_settings_last_size: Option<(f32, f32)>,
    pub app_settings_category: AppSettingsCategory,
    /// Current OSD message. Empty string means nothing is shown.
    pub osd_message: String,
    /// Monotonic counter - ClearOsd only clears when its seq matches.
    osd_seq: u64,
    /// Which panel (if any) is docked to the right. Toggling with the same
    /// kind closes it; toggling with a different kind switches without resize.
    pub active_panel: Option<PanelKind>,
    /// The panel to reopen when the panels button is pressed while closed.
    /// Remembers the last panel the user had open; starts on Playlist.
    pub last_panel: PanelKind,
    /// Current directory shown in the browser panel. `None` = drives list.
    pub browser_path: Option<std::path::PathBuf>,
    /// Cached entries for the current browser directory.
    pub browser_entries: Vec<BrowserEntry>,
    /// Locations visited before the current one, most-recent-last - popped
    /// by the Back button. `None` entries mean the drives list. Separate
    /// from "up to parent folder", which is just a shortcut to a specific
    /// relative location, not true navigation history (e.g. jumping to a
    /// different drive and wanting to return to where you just were isn't
    /// "up" from anywhere).
    pub browser_back_stack: Vec<Option<std::path::PathBuf>>,
    /// Locations undone by Back, most-recent-last - popped by the Forward
    /// button. Cleared on any fresh navigation (one that didn't come from
    /// Back/Forward itself), same as a normal desktop file browser: going
    /// somewhere new invalidates whatever you'd have gone "forward" to.
    pub browser_forward_stack: Vec<Option<std::path::PathBuf>>,
    /// Resume position database (loaded once at startup).
    pub resume_db: ResumeDb,
    /// Recently opened files (most-recent-first).
    pub recent_files: RecentFiles,
    /// AB repeat loop points. When both are set, position is snapped back to A
    /// whenever it passes B.
    pub ab_loop_a: Option<f64>,
    pub ab_loop_b: Option<f64>,
    /// True while auto-chasing the live edge after End is pressed.
    pub live_catching_up: bool,
    /// True once the current file has been confirmed as an actively
    /// growing/live stream (its duration has genuinely increased under
    /// `LiveEdgeTick`'s polling, not just a momentary EOF pause). Unlike
    /// `live_catching_up`/`live_edge_paused`, which toggle on and off
    /// through normal playback, this is a stable per-file "is this live"
    /// signal for UI display - reset on the next `FileLoaded`.
    pub stream_is_live: bool,
    /// The target of the most-recent seek issued during a live chase.
    /// Used to debounce: we skip a DurationChanged-triggered seek if the
    /// new target is within 10s of this value (avoids hundreds of seeks
    /// per End press from rapid DurationChanged events).
    pub live_last_seek: f64,
    /// True after EndFile fired while position was at the live edge (keep-open=yes).
    /// DurationChanged will re-seek forward when new content arrives so the user
    /// doesn't have to manually press End after every buffer refill.
    pub live_edge_paused: bool,
    /// `player.duration` the last time LiveEdgeTick checked for growth, and
    /// how many consecutive checks in a row found no growth. Once this hits
    /// the stall threshold, the file is almost certainly not actually
    /// growing/live, so we stop poking play() every 2s — otherwise a normal,
    /// finished video would get poked forever every time it reaches EOF.
    pub live_edge_ref_duration: f64,
    pub live_edge_stall_count: u32,
    /// File-size-based duration estimate for growing files. Updated every few
    /// seconds by reading the file size from disk and extrapolating from the
    /// known bitrate. Zero when unknown. Used so the seekbar and JumpToLive
    /// reflect the true extent of a long recording without mpv having to index
    /// the whole file first.
    pub size_est_duration: f64,
    /// Reference point for the size estimate: the file size and mpv-known
    /// duration at the time we last sampled them together.
    pub size_ref_size: u64,
    pub size_ref_duration: f64,
    /// Floor for the displayed duration, set by the container byte-rate probe on
    /// load. mpv reports its own slowly-climbing forward-index duration which
    /// would otherwise stomp the probed full extent and make the seekbar flicker
    /// back to a tiny value; DurationChanged never drops below this.
    pub probed_duration: f64,
    /// When true, the stats overlay (bitrate/fps/dropped/buffer) is shown and
    /// polled on a timer. Toggled with the S key.
    pub show_stats: bool,
    /// How the video frame is fitted into the window (Fit/Fill/Stretch).
    /// Cycled with the Z key; resets to Fit on file load.
    pub frame_mode: FrameMode,
    /// Latest polled playback stats; only refreshed while `show_stats` is on.
    pub stats: crate::player::PlayerStats,
    /// Monotonically-increasing counter for deferred thumbnail generation.
    /// Each live chase completion increments this; `GenerateThumbnails(id)`
    /// is a no-op when `id != thumb_pending_id`, so only the last End press
    /// in a rapid sequence actually spawns thumbnail workers.
    pub thumb_pending_id: u64,
    /// Window size (logical, video-column width only) saved just before
    /// entering fullscreen so we can restore the exact dimensions on exit.
    pre_fullscreen_w: Option<f32>,
    pre_fullscreen_h: Option<f32>,
}

impl Default for MpvNe {
    fn default() -> Self {
        let prefs = crate::settings::Settings::load();
        set_custom_title_bar(prefs.interface.custom_title_bar);
        let mouse_ids = MouseBindingIds {
            single_click: prefs.interface.mouse_single_click.clone(),
            double_click: prefs.interface.mouse_double_click.clone(),
            scroll_up: prefs.interface.mouse_scroll_up.clone(),
            scroll_down: prefs.interface.mouse_scroll_down.clone(),
        };
        let mut app = Self {
            player: Player::default(),
            current_frame: None,
            pending_w: 0,
            pending_h: 0,
            resize_seq: 0,
            render_initialized: false,
            fullscreen: false,
            window_id: None,
            scale_factor: 1.0,
            modal_hook_installed: false,
            playlist: Vec::new(),
            playlist_idx: 0,
            bindings: Bindings {
                drag_window_anywhere: prefs.interface.drag_anywhere,
                ..Bindings::from_overrides(&prefs.keybindings, &mouse_ids)
            },
            keybinding_overrides: prefs.keybindings.clone(),
            mouse_bindings: mouse_ids,
            rebind_capture: None,
            last_left_press: None,
            pan_drag_start: None,
            chrome_force_hidden: false,
            cursor_pos: None,
            panel_cursor_pos: None,
            window_h_logical: 0.0,
            window_w_logical: 0.0,
            window_x_logical: 0,
            keyboard_modifiers: iced::keyboard::Modifiers::default(),
            window_y_logical: 0,
            pinned: prefs.interface.start_pinned,
            pip_active: false,
            pip_prev_w: 0.0,
            pip_prev_h: 0.0,
            pip_prev_x: 0,
            pip_prev_y: 0,
            pip_prev_chrome_hidden: false,
            pip_prev_pinned: false,
            subs_menu_open: false,
            stopped: false,
            img_logo: iced::widget::image::Handle::from_bytes(
                include_bytes!("../assets/MPV_NE_logo_hires.png").to_vec(),
            ),
            img_icon: iced::widget::image::Handle::from_bytes(
                include_bytes!("../assets/MPV_NE_icon_hires.png").to_vec(),
            ),
            transitioning: false,
            thumb_cache: crate::thumbnail::new_cache(),
            after_playback: AfterPlayback::default(),
            modal: None,
            resume_enabled: prefs.playback.resume_enabled,
            private_mode: false,
            audio_normalize: prefs.audio.normalize,
            audio_lang: prefs.audio.lang.clone(),
            sub_lang: prefs.subtitles.lang.clone(),
            precise_seek: prefs.playback.precise_seek,
            seek_step_secs: prefs.playback.seek_step_secs,
            speed_step: prefs.playback.speed_step,
            stream_quality_height: prefs.streaming.quality_height,
            screenshot_dir: prefs.playback.screenshot_dir.clone(),
            snap_to_edge: prefs.interface.snap_to_edge,
            remember_window: prefs.interface.remember_window,
            start_pinned_pref: prefs.interface.start_pinned,
            custom_title_bar_pref: prefs.interface.custom_title_bar,
            osd_enabled: prefs.interface.osd_enabled,
            thumbnail_preview: prefs.interface.thumbnail_preview,
            auto_update_ytdlp: prefs.interface.auto_update_ytdlp,
            hide_all_on_minimize: prefs.interface.hide_all_on_minimize,
            pause_on_focus_lost: prefs.interface.pause_on_focus_lost,
            pause_on_minimize: prefs.interface.pause_on_minimize,
            minimize_to_tray: prefs.interface.minimize_to_tray,
            main_window_was_minimized: false,
            auto_load_siblings: prefs.interface.auto_load_siblings,
            single_instance: prefs.interface.single_instance,
            pending_ytdl_url: None,
            suppress_volume_osd: false,
            suppress_speed_osd: true, // suppress the startup 1x event
            show_help: false,
            sub_search_open: false,
            sub_search_query: String::new(),
            sub_search_results: Vec::new(),
            sub_search_loading: false,
            opensubtitles_api_key: String::new(),
            file_info_osd_shown: false,
            size_cache: std::collections::HashMap::new(),
            playlist_url_meta: std::collections::HashMap::new(),
            video_rotate: 0,
            video_hflip: false,
            video_vflip: false,
            fit_menu_open: false,
            audio_menu_open: false,
            popup_anchor_x: 0.0,
            playlist_sort_open: false,
            file_context_menu: None,
            modal_paste_menu: None,
            menu_window_id: None,
            menu_section_open: [false, false],
            menu_anchor: None,
            panel_window_id: None,
            panel_last_pos: None,
            panel_last_size: None,
            app_settings_window_id: None,
            app_settings_cursor_pos: None,
            app_settings_last_pos: None,
            app_settings_last_size: None,
            app_settings_category: AppSettingsCategory::default(),
            osd_message: String::new(),
            osd_seq: 0,
            active_panel: None,
            last_panel: PanelKind::Playlist,
            browser_path: None,
            browser_entries: Vec::new(),
            browser_back_stack: Vec::new(),
            browser_forward_stack: Vec::new(),
            resume_db: ResumeDb::load(),
            recent_files: RecentFiles::load(),
            ab_loop_a: None,
            ab_loop_b: None,
            live_catching_up: false,
            stream_is_live: false,
            live_last_seek: 0.0,
            live_edge_paused: false,
            live_edge_ref_duration: 0.0,
            live_edge_stall_count: 0,
            size_est_duration: 0.0,
            size_ref_size: 0,
            size_ref_duration: 0.0,
            probed_duration: 0.0,
            show_stats: false,
            frame_mode: FrameMode::Fit,
            stats: crate::player::PlayerStats::default(),
            thumb_pending_id: 0,
            pre_fullscreen_w: None,
            pre_fullscreen_h: None,
        };
        app.player.set_audio_normalize(app.audio_normalize);
        app.player.set_lang_priority(&app.audio_lang, &app.sub_lang);
        app.player.set_stream_quality(app.stream_quality_height);
        // Pad/truncate the saved gain list to the current band count rather
        // than erroring - lets `player::EQ_BANDS` grow later without
        // breaking older configs (same reasoning as the Settings migration).
        let mut eq_gains = prefs.audio.eq_gains.clone();
        eq_gains.resize(crate::player::EQ_BANDS.len(), 0.0);
        app.player.eq_gains = eq_gains;
        app.player.set_eq_enabled(prefs.audio.eq_enabled);
        #[cfg(target_os = "windows")]
        crate::win32_modal::set_snap_enabled(app.snap_to_edge);
        // If a previous session already auto-downloaded yt-dlp, use it
        // without waiting to discover that again on first URL open.
        if let Some(path) = ytdl_local_path() {
            if path.exists() {
                app.player.set_ytdl_path(&path.to_string_lossy());
            }
        }
        app
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    OpenFile,
    FileSelected(Option<String>),
    Stop,
    Seek(f64),
    VolumeChanged(f64),
    WindowResized(iced::window::Id, iced::Size),
    ResizeSettled(u64),
    /// Follow-up to ResizeSettled once the live DPI scale factor has been
    /// queried from the OS (seq, dpi) - does the actual settings save + OSD.
    ResizeDpiQueried(u64, f32),
    CloseRequested(iced::window::Id),
    // Forwarded from the mpv event loop
    PositionChanged(f64),
    DurationChanged(f64),
    FileLoaded,
    EndFile,
    EofReached(bool),
    PauseChanged(bool),
    FrameReady(Vec<u8>, u32, u32),
    WidthChanged(i64),
    HeightChanged(i64),
    VideoCodecChanged(String),
    AudioCodecChanged(String),
    AudioChannelsChanged(i64),
    HwDecChanged(String),
    HwDecSet(String),
    PrimariesChanged(String),
    SubVisibleChanged(bool),
    SubTracksChanged(Vec<crate::player::SubTrack>),
    CurrentSidChanged(i64),
    SubTrackSelected(crate::player::SubTrack),
    CurrentSecondarySidChanged(i64),
    SecondarySubTrackSelected(crate::player::SubTrack),
    ChaptersChanged(Vec<crate::player::Chapter>),
    AudioTracksChanged(Vec<crate::player::AudioTrack>),
    CurrentAidChanged(i64),
    AudioTrackSelected(crate::player::AudioTrack),
    SpeedChanged(f64),
    LiveEdgeTick,
    FileSizeTick,
    AddBookmark,
    RemoveBookmark(usize),
    JumpToBookmark(f64),
    // Input
    TogglePause,
    ToggleMute,
    ToggleFullscreen,
    ToggleChrome,
    CycleSubtitle,
    CycleAudio,
    SpeedAdjust(f64),
    SpeedReset,
    ToggleSubVisibility,
    ToggleHwDec,
    ToggleMaximize,
    TogglePin,
    TogglePip,
    TogglePrivateMode,
    ToggleAudioNormalize,
    ToggleSnapToEdge,
    ToggleDragAnywhere,
    /// Takes a restart to actually apply - see `use_custom_title_bar`.
    ToggleCustomTitleBar,
    ToggleRememberWindow,
    ToggleStartPinned,
    ToggleOsdEnabled,
    ToggleThumbnailPreview,
    ToggleAutoUpdateYtdlp,
    /// Keyboard settings page: begin capturing the next key press to bind
    /// to this slot. See `MpvNe::rebind_capture`.
    StartRebind(&'static str),
    CancelRebind,
    ResetRebind(&'static str),
    ResetAllKeybindings,
    ToggleAudioEq,
    EqBandSet(usize, f64),
    AudioEqReset,
    ToggleHideAllOnMinimize,
    TogglePauseOnFocusLost,
    TogglePauseOnMinimize,
    ToggleMinimizeToTray,
    /// Windows-only: polls for a taskbar thumbnail button click (previous/
    /// play-pause/next) - see `win32_modal::take_pending_thumb_action`.
    PollThumbBar,
    ToggleAutoLoadSiblings,
    ToggleSingleInstance,
    /// Windows-only: register MPV-NE as an "Open with" candidate for its
    /// supported media extensions, then open Windows' own Default Apps
    /// settings so the user can pick it - see `win32_modal::register_file_associations`.
    RegisterFileAssociations,
    /// Change one of the 4 mouse triggers to a different preset - see
    /// `MOUSE_ACTION_PRESETS`/`MouseTrigger`.
    SetMouseBinding(MouseTrigger, &'static str),
    /// Windows-only: polls for a file path forwarded from a newer launch -
    /// see `win32_modal::take_pending_open_file`.
    PollSingleInstance,
    /// Windows-only: polls `win32_modal::is_main_window_minimized` to
    /// detect the minimize/restore transition (no direct iced event for
    /// it - see subscription()).
    MinimizeCheckTick,
    AudioLangInput(String),
    SubLangInput(String),
    TogglePreciseSeek,
    SeekStepAdjust(f64),
    SeekStepSet(f64),
    SpeedStepAdjust(f64),
    SpeedStepSet(f64),
    StreamQualitySet(u32),
    FitToVisible,
    FitToScale(f32),
    FitToHeight(u32),
    ToggleFitMenu,
    CloseFitMenu,
    MinimizeWindow,
    CloseWindow,
    DragWindow,
    ToggleSubsMenu,
    CloseSubsMenu,
    ToggleAudioMenu,
    CloseAudioMenu,
    // OSD
    ShowOsd(String),
    ClearOsd(u64),
    /// Deferred thumbnail generation after a live chase.  The u64 is a
    /// generation counter — stale requests (from earlier End presses) are
    /// ignored so only the final chase completion actually spawns workers.
    GenerateThumbnails(u64),
    // Side panel (playlist / browser / recent / settings)
    TogglePanel(PanelKind),
    #[allow(dead_code)]
    CloseSettingsPanel,
    // Browser panel navigation
    BrowserNavigate(std::path::PathBuf),
    BrowserNavigateUp,
    BrowserGoToDrives,
    BrowserBack,
    BrowserForward,
    BrowserOpen(std::path::PathBuf),
    // Playlist
    PlaylistJump(usize),
    // Resume position
    ResumePosition(f64),
    SubDelayAdjust(f64),
    AudioDelayAdjust(f64),
    SubDelayReset,
    AudioDelayReset,
    SubDelayChanged(f64),
    AudioDelayChanged(f64),
    TakeScreenshot,
    /// Toggle the stats overlay on/off.
    ToggleStats,
    /// Cycle the video framing: Fit → Fill → Stretch.
    CycleFrameMode,
    /// Periodic poll to refresh the stats overlay while it is visible.
    StatsTick,
    // Loop / deinterlace toggles
    /// True duration probed from container header/tail for the currently-loaded file.
    LiveDurationProbed(f64),
    /// Background probe results.
    #[allow(dead_code)]
    MetadataProbed(std::path::PathBuf, u64, Option<f64>),
    MetadataBatch(Vec<(std::path::PathBuf, u64, Option<f64>)>),
    ProbeFiles(Vec<std::path::PathBuf>),
    SetAfterPlayback(AfterPlayback),
    JumpToTime,
    ShowHelp,
    OpenSubSearch,
    CloseSubSearch,
    SubSearchQuery(String),
    SubSearch,
    SubSearchResults(Vec<crate::opensubs::SubResult>),
    SubSearchError(String),
    SubDownload(u64, String),
    SubDownloaded(String),
    SavePlaylist,
    PlaylistSaved(String),
    LoadPlaylist,
    PlaylistLoaded(Vec<std::path::PathBuf>),
    OpenModal(ModalKind),
    ModalInput(String),
    /// Right-click on the modal text field: open a small "Paste" menu at
    /// the cursor, rather than pasting silently - a right-click with no
    /// visible response was easy to miss existed at all.
    ModalRightClick,
    CloseModalPasteMenu,
    ModalPasteRequest,
    ModalPasteResult(Option<String>),
    ModalConfirm,
    ModalCancel,
    NextChapter,
    PrevChapter,
    FilesDropped(Vec<std::path::PathBuf>),
    VideoRotateCw,
    VideoRotateCcw,
    VideoHFlip,
    VideoVFlip,
    VideoTransformReset,
    OpenUrl,
    #[allow(dead_code)]
    /// yt-dlp finished downloading (or failed) - resume opening whatever
    /// URL was waiting on it, if any.
    YtdlpDownloadResult(Result<String, String>),
    ChooseScreenshotDir,
    ScreenshotDirSelected(String),
    /// A playlist URL entry's metadata probe finished (title/duration/
    /// uploader, or `None` if it couldn't be determined).
    UrlMetaFetched(String, Option<UrlMeta>),
    ToggleResume,
    ToggleLoopFile,
    LoopFileChanged(bool),
    ToggleLoopPlaylist,
    LoopPlaylistChanged(bool),
    ToggleDeinterlace,
    DeinterlaceChanged(bool),
    // Playlist operations
    ShufflePlaylist,
    SortPlaylist(PlaylistSort),
    TogglePlaylistSort,
    TogglePanelsMenu,
    FileContextMenu(std::path::PathBuf),
    CloseFileContextMenu,
    OpenFileLocation(std::path::PathBuf),
    CopyFilePath(std::path::PathBuf),
    ClearRecent,
    /// Right-click on the video area: open the playback shortcuts menu.
    ShowVideoContextMenu,
    /// Title-bar hamburger button: same menu, opened at a fixed anchor near
    /// the button instead of the cursor. Toggles closed if already open.
    ToggleMainMenu,
    /// Wraps a menu action so a single handler can close the video context
    /// menu and then forward the real message, instead of every action
    /// handler (TogglePause, CycleAudio, …) needing to know about the menu.
    VideoMenuAction(Box<Message>),
    /// Expand/collapse a collapsible section in the main-menu popup (does
    /// NOT close the popup, unlike `VideoMenuAction`).
    ToggleMenuSection(usize),
    /// Internal: actually open the main-menu popup window, once the main
    /// window's live DPI is known (screen_x, screen_y, height, dpi).
    OpenMenuPopup(f32, f32, f32, f32),
    /// Internal: resize/reposition an already-open main-menu popup after a
    /// section was toggled, once the main window's live DPI is known
    /// (popup id, anchor_x, anchor_y, height, dpi).
    RepositionMenuPopup(iced::window::Id, f32, f32, f32, f32),
    /// Pop the docked side panel out into its own OS window.
    DetachPanel,
    /// Dock the popped-out side panel back into the main window, keeping it
    /// open (the panel's own "dock" button - a deliberate opposite of
    /// `DetachPanel`, distinct from actually closing the panel).
    ReattachPanel,
    /// Fully close the panel (whether docked or detached) - closes the
    /// floating window too, if one is open. Triggered by the detached
    /// window's own close button/titlebar X and by the OS close request,
    /// as opposed to `ReattachPanel` which keeps the panel open but docked.
    ClosePanelWindow,
    /// Custom-chrome window controls for the detached panel window - same
    /// idea as MinimizeWindow/DragWindow/ToggleMaximize but targeting
    /// `panel_window_id` instead of the hardcoded main `window_id`.
    PanelMinimize,
    PanelDragWindow,
    PanelToggleMaximize,
    /// Open the standalone App Settings window (Interface/Keyboard/...).
    /// No-op if it's already open.
    OpenAppSettings,
    /// Close the App Settings window. Unlike the side panel, there's no
    /// dock/undock distinction - it's always its own window.
    CloseAppSettingsWindow,
    AppSettingsCategorySelect(AppSettingsCategory),
    AppSettingsMinimize,
    AppSettingsDragWindow,
    AppSettingsToggleMaximize,
    // Video zoom
    VideoZoomSet(f64),
    VideoZoomReset,
    VideoZoomChanged(f64),
    CacheTimeChanged(f64),
    // External subtitle
    LoadSubtitle,
    SubtitleFileSelected(String),
    /// No-op: used as a sentinel when async dialogs are cancelled.
    Noop,
    // Aspect ratio override
    AspectRatioSet(String),
    // Playlist item removal
    PlaylistRemove(usize),
    // Window position tracking
    WindowMoved(iced::window::Id, i32, i32),
    // Video equalizer
    BrightnessSet(i64),
    ContrastSet(i64),
    SaturationSet(i64),
    HueSet(i64),
    GammaSet(i64),
    VideoEqReset,
    SpeedSet(f64),
    // Subtitle appearance
    SubFontSizeSet(i64),
    SubPosSet(i64),
    SubFontSizeChanged(i64),
    SubPosChanged(i64),
    SeekRelative(f64),
    /// Seek by the configurable step size, forward if `true`.
    SeekStep(bool),
    /// Step exactly one video frame, forward if `true`.
    FrameStep(bool),
    /// Adjust speed by the configurable step size, faster if `true`.
    SpeedStep(bool),
    /// Seek to the start of the next/previous subtitle line, forward if `true`.
    SubSeek(bool),
    VolumeAdjust(f64),
    #[allow(dead_code)]
    FileDropped(std::path::PathBuf),
    JumpToLive,
    NextFile,
    PrevFile,
    // Raw input events - resolved through `bindings` in `update`.
    InputKey(String),
    /// `captured` is true when a widget already handled the click. Edge-grip
    /// resize fires regardless; other actions only fire when !captured.
    InputMouseDown(iced::window::Id, iced::mouse::Button, bool),
    InputMouseUp(iced::window::Id, iced::mouse::Button),
    InputScroll(iced::window::Id, f32),
    ModifiersChanged(iced::keyboard::Modifiers),
    /// Carries the source window's id so the handler can ignore movement
    /// over the menu popup window (which isn't the main window's cursor).
    CursorMoved(iced::window::Id, f32, f32),
    CursorLeft(iced::window::Id),
    /// A window (the menu popup) lost OS focus - close it, matching how a
    /// native context menu dismisses when you click elsewhere.
    WindowUnfocused(iced::window::Id),
    /// The main window's OS DPI scale factor changed (or was reported for
    /// the first time on open).
    WindowRescaled(iced::window::Id, f32),
    // AB repeat
    AbLoopSetA,
    AbLoopSetB,
    AbLoopClear,
}

impl MpvNe {
    /// Daemon boot function: constructs the initial state and opens the
    /// main window explicitly, since daemons (needed for the floating menu
    /// popup window) don't open one automatically the way `application`
    /// does. Mirrors what used to be main.rs's `.window(Settings {...})`.
    pub fn boot() -> (Self, Task<Message>) {
        let mut app = Self::default();

        let prefs = crate::settings::Settings::load();
        // settings.toml stores PHYSICAL pixels (see the ResizeSettled save
        // block). The real DPI scale factor isn't known until the window
        // exists and reports it, so this first request uses the saved
        // numbers as-is (correct at 100% scale) and gets corrected below.
        // Skipped entirely when "remember window" is off - always open at
        // the default size, centered.
        let (w, h) = if prefs.interface.remember_window {
            prefs.window_size().unwrap_or((1280.0, 720.0))
        } else {
            (1280.0, 720.0)
        };
        let position = if !prefs.interface.remember_window {
            iced::window::Position::Centered
        } else {
            prefs.window.x
                .zip(prefs.window.y)
                // Reject a saved position that isn't on any currently-connected
                // monitor (e.g. the monitor it was saved on got disconnected or
                // rearranged since) - otherwise the window would open off-screen
                // and be unreachable, the same class of bug as the minimized-
                // window sentinel position fixed earlier. No window exists yet
                // at boot, so this has to be a standalone monitor query, not a
                // window-relative one.
                .filter(|&(x, y)| {
                    #[cfg(target_os = "windows")]
                    { crate::win32_modal::is_position_reachable(x, y) }
                    #[cfg(not(target_os = "windows"))]
                    { true }
                })
                .map(|(x, y)| iced::window::Position::Specific(iced::Point::new(x as f32, y as f32)))
                .unwrap_or(iced::window::Position::Centered)
        };
        let icon = app_icon();

        // `w, h` are saved PHYSICAL pixels; convert to logical using the
        // target monitor's real DPI scale *before* creating the window, so
        // it opens at the correct size immediately - guessing 100% scale
        // here (as before) left a window where the controls bar laid out
        // at the wrong size and looked visibly broken until the later
        // async correction resize landed.
        #[cfg(target_os = "windows")]
        let initial_scale = {
            let query_pos = match position {
                iced::window::Position::Specific(p) => Some((p.x as i32, p.y as i32)),
                _ => None,
            };
            crate::win32_modal::dpi_scale_near(query_pos)
        };
        #[cfg(not(target_os = "windows"))]
        let initial_scale: f32 = 1.0;
        let (logical_w, logical_h) = (w / initial_scale, h / initial_scale);

        let settings = iced::window::Settings {
            size: iced::Size::new(logical_w, logical_h),
            position,
            min_size: Some(iced::Size::new(360.0, 160.0)),
            decorations: !use_custom_title_bar(),
            icon,
            exit_on_close_request: false,
            ..Default::default()
        };
        let (id, open_task) = iced::window::open(settings);
        app.window_id = Some(id);
        // Safety net in case the pre-query above didn't match the DPI iced
        // actually opened the window at (e.g. non-Windows, or the OS placed
        // a "Centered" window on a different monitor than we assumed) -
        // corrects it again against the live, authoritative scale factor.
        let physical = (w, h);
        let correct_size = open_task.then(move |id| {
            iced::window::scale_factor(id).then(move |dpi| {
                let logical = iced::Size::new(physical.0 / dpi, physical.1 / dpi);
                iced::window::resize::<Message>(id, logical)
            })
        });

        let mut startup_tasks = vec![correct_size];
        if app.pinned {
            startup_tasks.push(iced::window::set_level(id, iced::window::Level::AlwaysOnTop));
        }
        if app.auto_update_ytdlp {
            startup_tasks.push(Task::perform(download_ytdlp(), Message::YtdlpDownloadResult));
        }
        // A file/URL passed on the command line (double-clicked via a file
        // association, or "Open with") - single-instance mode's handoff
        // path exits before ever reaching here, so this only fires for a
        // genuinely new process. `FileSelected` reuses the same path as
        // manually picking a file via the Open dialog.
        if let Some(arg) = std::env::args().nth(1) {
            startup_tasks.push(Task::done(Message::FileSelected(Some(arg))));
        }
        (app, Task::batch(startup_tasks))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFile => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter(
                                "Media",
                                &[
                                    "mp4", "mkv", "avi", "mov", "webm", "m4v",
                                    "flv", "wmv", "ts", "mp3", "flac", "ogg",
                                    "wav", "aac", "opus",
                                ],
                            )
                            .pick_file()
                            .await
                            .map(|h| h.path().to_string_lossy().into_owned())
                    },
                    Message::FileSelected,
                );
            }
            Message::FileSelected(Some(path)) => self.load_path(std::path::PathBuf::from(path)),
            Message::FileSelected(None) => {}

            Message::Stop => {
                self.live_catching_up = false;
                self.live_edge_paused = false;
                self.player.stop();
                self.current_frame = None;
                self.stopped = true;
            }
            Message::Seek(pos) => {
                self.live_catching_up = false;
                self.live_edge_paused = false;
                self.player.seek(pos, self.precise_seek);
            }
            Message::VolumeChanged(vol) => {
                self.player.set_volume(vol);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.volume = self.player.volume;
                prefs.save();
                if let Some(path) = &self.player.path.clone() {
                    self.resume_db.record_volume(path, self.player.volume);
                    self.resume_db.save();
                }
                if self.suppress_volume_osd {
                    self.suppress_volume_osd = false;
                } else {
                    return Task::done(Message::ShowOsd(format!("Volume  {:.0}%", self.player.volume)));
                }
            }

            Message::WindowResized(id, size) => {
                if Some(id) == self.panel_window_id {
                    self.panel_last_size = Some((size.width, size.height));
                    return Task::none();
                }
                if Some(id) == self.app_settings_window_id {
                    self.app_settings_last_size = Some((size.width, size.height));
                    return Task::none();
                }
                // The menu popup also fires an initial Resized on creation
                // even though it's non-resizable - ignore anything that
                // isn't the main window so it can't clobber window_id or
                // the layout dimensions everything else reads from.
                if self.window_id.is_some() && Some(id) != self.window_id {
                    return Task::none();
                }
                self.window_id = Some(id);
                self.window_h_logical = size.height;
                self.window_w_logical = size.width;
                #[cfg(target_os = "windows")]
                self.install_modal_hook_once();

                let h_offset = if self.chrome_visible() { CONTROLS_H + TOP_BAR_H } else { 0 };
                let panel_px = if self.active_panel.is_some() { PANEL_W as u32 } else { 0 };
                let w = (size.width as u32).saturating_sub(panel_px);
                let h = (size.height as i32 - h_offset).max(0) as u32;
                self.pending_w = w;
                self.pending_h = h;

                // First resize: kick off rendering and restore saved volume.
                if !self.render_initialized {
                    self.apply_render_size();
                    self.render_initialized = true;
                    let saved_vol = crate::settings::Settings::load().playback.volume;
                    self.suppress_volume_osd = true;
                    self.player.set_volume(saved_vol);
                    // Apply saved screenshot dir if set.
                    if !self.screenshot_dir.is_empty() {
                        self.player.set_screenshot_dir(&self.screenshot_dir);
                    }
                }

                // During a live drag we keep mpv at the previous stable size and
                // let the GPU rescale the existing texture (smooth). 80 ms after
                // the last resize event we commit the new size in one shot.
                self.resize_seq += 1;
                let seq = self.resize_seq;
                return Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        seq
                    },
                    Message::ResizeSettled,
                );
            }

            Message::ResizeSettled(seq) => {
                if seq == self.resize_seq {
                    // Query the DPI scale factor live from the OS rather than
                    // trusting a cached field - this is the same pattern the
                    // Fit menu uses (iced::window::scale_factor(id).then(...))
                    // and it's confirmed correct, whereas the cached
                    // self.scale_factor populated from the Rescaled event
                    // was not reliable enough to trust for what gets saved,
                    // displayed, and (see ResizeDpiQueried) rendered here.
                    if let Some(id) = self.window_id {
                        return iced::window::scale_factor(id)
                            .then(move |dpi| Task::done(Message::ResizeDpiQueried(seq, dpi)));
                    }
                }
            }
            Message::ResizeDpiQueried(seq, dpi) => {
                if seq == self.resize_seq {
                    // Keep the cached factor fresh via this reliable live
                    // query (not the passive Rescaled event) - apply_render_
                    // size() reads it synchronously for the mpv render
                    // texture, where a live query per-call isn't practical.
                    // Set it before applying so this exact resize's texture
                    // uses the freshest possible value, not a stale one from
                    // whatever the factor was on the previous cycle.
                    self.scale_factor = dpi;
                    self.apply_render_size();
                    // Don't persist window size when maximized - we'd save the
                    // screen resolution and the next launch would open huge.
                    let maximized = {
                        #[cfg(target_os = "windows")]
                        { win32_is_maximized(self.window_id) }
                        #[cfg(not(target_os = "windows"))]
                        { false }
                    };
                    if !maximized {
                        // Save video-column width only (strip panel if open).
                        let save_w_logical = if self.active_panel.is_some() {
                            (self.window_w_logical - PANEL_W).max(480.0)
                        } else {
                            self.window_w_logical.max(0.0)
                        };
                        // settings.toml stores PHYSICAL pixels (see
                        // desired_window_physical's doc comment) - convert
                        // from iced's logical pixels using the live-queried
                        // DPI scale factor so what's saved matches what the
                        // monitor actually shows, and what boot() loads back
                        // stays consistent across sessions instead of
                        // compounding a logical/physical mismatch.
                        let save_w = (save_w_logical * dpi).round() as u32;
                        let save_h = (self.window_h_logical.max(0.0) * dpi).round() as u32;
                        crate::settings::Settings {
                            window: crate::settings::WindowSettings {
                                w: Some(save_w),
                                h: Some(save_h),
                                x: Some(self.window_x_logical),
                                y: Some(self.window_y_logical),
                            },
                            playback: crate::settings::PlaybackSettings {
                                resume_enabled: self.resume_enabled,
                                volume: self.player.volume,
                                precise_seek: self.precise_seek,
                                screenshot_dir: self.screenshot_dir.clone(),
                                seek_step_secs: self.seek_step_secs,
                                speed_step: self.speed_step,
                            },
                            audio: crate::settings::AudioSettings {
                                normalize: self.audio_normalize,
                                lang: self.audio_lang.clone(),
                                eq_enabled: self.player.eq_enabled,
                                eq_gains: self.player.eq_gains.clone(),
                            },
                            subtitles: crate::settings::SubtitleSettings {
                                lang: self.sub_lang.clone(),
                            },
                            streaming: crate::settings::StreamingSettings {
                                quality_height: self.stream_quality_height,
                            },
                            interface: crate::settings::InterfaceSettings {
                                snap_to_edge: self.snap_to_edge,
                                drag_anywhere: self.bindings.drag_window_anywhere,
                                custom_title_bar: self.custom_title_bar_pref,
                                remember_window: self.remember_window,
                                start_pinned: self.start_pinned_pref,
                                osd_enabled: self.osd_enabled,
                                thumbnail_preview: self.thumbnail_preview,
                                auto_update_ytdlp: self.auto_update_ytdlp,
                                hide_all_on_minimize: self.hide_all_on_minimize,
                                pause_on_focus_lost: self.pause_on_focus_lost,
                                pause_on_minimize: self.pause_on_minimize,
                                auto_load_siblings: self.auto_load_siblings,
                                single_instance: self.single_instance,
                                minimize_to_tray: self.minimize_to_tray,
                                mouse_single_click: self.mouse_bindings.single_click.clone(),
                                mouse_double_click: self.mouse_bindings.double_click.clone(),
                                mouse_scroll_up: self.mouse_bindings.scroll_up.clone(),
                                mouse_scroll_down: self.mouse_bindings.scroll_down.clone(),
                            },
                            keybindings: self.keybinding_overrides.clone(),
                        }
                        .save();
                    }
                    // resize_seq == 1 is the window's own initial Resized
                    // event on startup, not a user drag - skip the OSD for
                    // that one so it doesn't fire the instant the app opens.
                    if self.resize_seq > 1 {
                        // The rendered video's actual pixel size (fit to the
                        // available space, aspect-preserved) - not the raw OS
                        // window size, which includes chrome/letterbox space
                        // the picture itself doesn't occupy.
                        let (vw, vh) = self.compute_render_size(self.pending_w, self.pending_h);
                        let pw = (vw as f32 * dpi).round() as u32;
                        let ph = (vh as f32 * dpi).round() as u32;
                        return Task::done(Message::ShowOsd(format!("{pw} × {ph}")));
                    }
                }
            }

            Message::PositionChanged(pos) => {
                self.player.position = pos;
                // AB repeat: snap back to A when position passes B.
                if let (Some(a), Some(b)) = (self.ab_loop_a, self.ab_loop_b) {
                    if pos >= b {
                        self.player.seek(a, true); // loop point - always exact
                    }
                }
            }
            Message::DurationChanged(dur) => {
                // Never drop below the probed full extent — mpv's forward-index
                // duration climbs from a small value and would otherwise stomp it.
                self.player.duration = dur.max(self.probed_duration);
                // Ignore stale duration events during file transitions.
                if self.transitioning { return Task::none(); }

                // Keep a reference point for file-size-based duration extrapolation.
                // Update whenever mpv gives us a stable, meaningful duration so the
                // FileSizeTick can compute how many bytes per second the recording uses.
                if dur > 30.0 && dur > self.size_ref_duration {
                    if let Some(path) = &self.player.path.clone() {
                        if let Ok(meta) = std::fs::metadata(path) {
                            self.size_ref_size = meta.len();
                            self.size_ref_duration = dur;
                        }
                    }
                }

                // Live edge chase: each seek causes mpv to read further into
                // the file, firing more DurationChanged events. Keep seeking
                // until we're within 8s of the current known edge.
                if self.live_catching_up {
                    let gap = dur - self.player.position;
                    tracing::debug!(
                        pos = self.player.position,
                        dur,
                        gap,
                        "live chase: gap"
                    );
                    if gap > 8.0 {
                        let target = (dur - 2.0).max(self.player.position);
                        // Debounce: skip if target is within 10s of the last
                        // issued seek.  DurationChanged fires many times per
                        // second, and without this we send hundreds of seeks
                        // per End press from stale position values.
                        if target > self.live_last_seek + 10.0 {
                            self.live_last_seek = target;
                            self.player.seek_to(target);
                        }
                        return Task::none();
                    } else {
                        self.live_catching_up = false;
                        self.live_last_seek = 0.0;
                        self.thumb_pending_id += 1;
                        let pending = self.thumb_pending_id;
                        tracing::info!(pos = self.player.position, dur, "live chase: reached edge");
                        // Seeking during the chase may leave mpv paused (keep-open=yes
                        // pauses when it hits the demuxer boundary mid-chase). Always
                        // resume so the user sees live playback without having to press Space.
                        self.player.play();
                        // Defer thumbnail generation by 5s.  If the user keeps
                        // pressing End, each subsequent completion increments
                        // thumb_pending_id, making earlier GenerateThumbnails
                        // messages stale so they no-op when they fire.
                        return Task::batch([
                            Task::done(Message::ShowOsd("Live edge".into())),
                            Task::perform(
                                async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    pending
                                },
                                Message::GenerateThumbnails,
                            ),
                        ]);
                    }
                }

                // Auto-resume after being paused at the live edge: if the file
                // grew by at least 3s past our last position, seek forward to
                // resume seamlessly without requiring the user to press End.
                if self.live_edge_paused && dur > self.player.position + 3.0 {
                    self.live_edge_paused = false;
                    let target = (dur - 2.0).max(self.player.position);
                    tracing::debug!(
                        pos = self.player.position,
                        dur,
                        target,
                        "live_edge_paused: auto-seeking to new content"
                    );
                    self.player.seek_to(target);
                    self.player.play();
                    return Task::none();
                }

                // Duration grew significantly beyond what thumbnails currently cover
                // (e.g. a live file that's still being recorded).  Extend without
                // clearing existing thumbnails so the user sees no gap.
                if self.thumbnail_preview {
                    let covered = self.thumb_cache.lock().unwrap().covered_to;
                    if dur > covered + 30.0 {
                        if let Some(path) = &self.player.path.clone() {
                            crate::thumbnail::spawn_extend(
                                path.clone(), dur, self.thumb_cache.clone(),
                            );
                        }
                    }
                }

                // First real duration after FileLoaded - show OSD + start thumbnails.
                if dur > 0.0 && !self.file_info_osd_shown {
                    self.file_info_osd_shown = true;

                    // Spawn thumbnails - path is confirmed correct (not transitioning).
                    if self.thumbnail_preview {
                        if let Some(path) = &self.player.path.clone() {
                            crate::thumbnail::spawn_generate(
                                path.clone(),
                                dur,
                                self.thumb_cache.clone(),
                            );
                        }
                    }

                    let p = &self.player;
                    let mut parts = Vec::new();
                    if p.width > 0 && p.height > 0 {
                        parts.push(format!("{}×{}", p.width, p.height));
                    }
                    if !p.video_codec.is_empty() { parts.push(p.video_codec.clone()); }
                    if !p.audio_codec.is_empty() { parts.push(p.audio_codec.clone()); }
                    if !p.hwdec.is_empty() && p.hwdec != "no" {
                        parts.push(format!("hw:{}", p.hwdec));
                    }
                    if !parts.is_empty() {
                        return Task::done(Message::ShowOsd(parts.join("  ")));
                    }
                }
            }
            Message::FileLoaded => {
                self.stopped = false;
                self.transitioning = false;
                self.live_catching_up = false;
                self.live_edge_paused = false;
                self.stream_is_live = false;
                self.size_est_duration = 0.0;
                self.size_ref_size = 0;
                self.size_ref_duration = 0.0;
                self.probed_duration = 0.0;
                self.frame_mode = FrameMode::Fit;
                // Clear AB loop points and video transform when a new file starts.
                self.ab_loop_a = None;
                self.ab_loop_b = None;
                self.video_rotate = 0;
                self.video_hflip = false;
                self.video_vflip = false;
                self.file_info_osd_shown = false;
                // Restore per-file preferences: volume, audio track, subtitle track.
                if let Some(path) = &self.player.path.clone() {
                    if let Some(vol) = self.resume_db.volume(path) {
                        self.player.set_volume(vol);
                    }
                    if let Some(aid) = self.resume_db.audio_track(path) {
                        self.player.set_audio_track(aid);
                    }
                    if let Some(sid) = self.resume_db.sub_track(path) {
                        self.player.set_sub_track(sid);
                    }
                }
                // Kick off a background header/tail probe to discover the true
                // duration immediately — without this, mpv only knows about the
                // first demuxer-buffer-worth of the file (~15 min at 5 Mbps).
                let mut tasks: Vec<Task<Message>> = Vec::new();
                if let Some(path) = self.player.path.clone() {
                    tasks.push(Task::perform(
                        async move {
                            crate::media_probe::probe_duration(std::path::Path::new(&path)).unwrap_or(0.0)
                        },
                        |dur| if dur > 0.0 {
                            Message::LiveDurationProbed(dur)
                        } else {
                            Message::LiveDurationProbed(0.0)
                        },
                    ));
                }
                // Check for a saved resume position and seek to it (if enabled).
                if self.resume_enabled {
                if let Some(path) = &self.player.path.clone() {
                    if let Some(pos) = self.resume_db.get(path) {
                        tasks.push(Task::done(Message::ResumePosition(pos)));
                    }
                }
                } // end resume_enabled
                if !tasks.is_empty() { return Task::batch(tasks); }
            }
            Message::EofReached(true) => {
                // eof-reached fires with keep-open=yes when mpv hits the end of
                // the current file and pauses on the last frame. Use this as the
                // reliable live-edge signal instead of EndFile (which may arrive
                // after the user has already pressed End).
                if !self.transitioning && self.player.position >= self.player.duration - 5.0 {
                    self.live_edge_paused = true;
                    self.live_edge_stall_count = 0;
                    self.live_edge_ref_duration = self.player.duration;
                    tracing::debug!(
                        pos = self.player.position,
                        dur = self.player.duration,
                        "eof-reached at live edge — will auto-resume when buffered"
                    );
                }
            }
            Message::EofReached(false) => {}
            Message::AddBookmark => {
                if let Some(path) = &self.player.path.clone() {
                    let pos = self.player.position;
                    let s = pos as u64;
                    let label = if s >= 3600 {
                        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
                    } else {
                        format!("{:02}:{:02}", s / 60, s % 60)
                    };
                    self.resume_db.add_bookmark(path, pos, label.clone());
                    self.resume_db.save();
                    return Task::done(Message::ShowOsd(format!("Bookmark  {}", label)));
                }
            }
            Message::RemoveBookmark(idx) => {
                if let Some(path) = &self.player.path.clone() {
                    self.resume_db.remove_bookmark(path, idx);
                    self.resume_db.save();
                }
            }
            Message::JumpToBookmark(pos) => {
                self.live_catching_up = false;
                self.live_edge_paused = false;
                self.player.seek(pos, true); // deliberate jump - always exact
            }
            Message::LiveEdgeTick => {
                // mpv's demuxer doesn't fire DurationChanged while keep-open pauses
                // at EOF. Poke play() so mpv resumes reading — it will fire
                // DurationChanged if more content exists (letting the chase continue),
                // or re-pause immediately if we're truly at the live edge.
                if self.live_edge_paused {
                    // If duration hasn't grown since the last check, this is
                    // probably just a normal, finished video — not an actual
                    // growing/live recording. Stop poking after a few stalls
                    // in a row (not just one, so a momentary write-flush gap
                    // in a real recording doesn't fool us) instead of poking
                    // play() forever every 2s.
                    if self.player.duration <= self.live_edge_ref_duration + 0.01 {
                        self.live_edge_stall_count += 1;
                        if self.live_edge_stall_count >= 3 {
                            tracing::debug!(
                                dur = self.player.duration,
                                "live edge: no growth after 3 checks — not a live file, stopping poll"
                            );
                            self.live_edge_paused = false;
                            self.live_catching_up = false;
                            return Task::none();
                        }
                    } else {
                        self.live_edge_stall_count = 0;
                        self.live_edge_ref_duration = self.player.duration;
                        self.stream_is_live = true;
                    }
                    tracing::debug!(
                        pos = self.player.position,
                        chasing = self.live_catching_up,
                        stall = self.live_edge_stall_count,
                        "live edge: poking play"
                    );
                    self.player.play();
                }
            }
            Message::FileSizeTick => {
                // Periodically read the file size from disk and extrapolate a
                // duration estimate so the seekbar reflects the true extent of a
                // growing recording without mpv having to index the whole file.
                if let Some(path) = &self.player.path.clone() {
                    if self.size_ref_size > 0 && self.size_ref_duration > 10.0 {
                        if let Ok(meta) = std::fs::metadata(path) {
                            let current_size = meta.len();
                            if current_size > self.size_ref_size {
                                let est = self.size_ref_duration
                                    * (current_size as f64 / self.size_ref_size as f64);
                                if est > self.size_est_duration {
                                    self.size_est_duration = est;
                                    // Update player.duration so the seekbar and all
                                    // duration-dependent logic sees the full extent.
                                    if est > self.player.duration {
                                        self.player.duration = est;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Message::EndFile => {
                if self.transitioning {
                    // We already saved resume and set the new path before
                    // calling open(); don't touch path/paused here.
                } else {
                    tracing::info!(
                        pos = self.player.position,
                        dur = self.player.duration,
                        path = ?self.player.path,
                        "EndFile"
                    );
                    self.save_resume();
                    match self.after_playback {
                        AfterPlayback::NextFile => {
                            if self.playlist_idx + 1 < self.playlist.len() {
                                self.playlist_idx += 1;
                                let p = self.playlist[self.playlist_idx].clone();
                                self.open_next(p.to_string_lossy().into_owned());
                                return Task::none();
                            }
                            // No next file — fall through to keep-open behaviour.
                            self.player.paused = true;
                        }
                        AfterPlayback::LoopFile => {
                            if let Some(path) = self.player.path.clone() {
                                self.open_next(path);
                                return Task::none();
                            }
                            self.player.paused = true;
                        }
                        AfterPlayback::ClosePlayer => {
                            if let Some(id) = self.window_id {
                                self.player.quit();
                                return iced::window::close(id);
                            }
                        }
                        AfterPlayback::DoNothing => {
                            // With keep-open=yes, mpv stays loaded at the last frame.
                            self.player.paused = true;
                            self.live_edge_paused = true;
                            self.live_edge_stall_count = 0;
                            self.live_edge_ref_duration = self.player.duration;
                            if self.live_catching_up {
                                // Chase is in progress — don't wait for the 2s tick.
                                // Immediately poke play() so mpv resumes reading the
                                // file and fires more DurationChanged events. Each EOF
                                // during a chase means mpv exhausted its demuxer buffer;
                                // play() lets it read the next chunk right away.
                                tracing::debug!(
                                    pos = self.player.position,
                                    dur = self.player.duration,
                                    "EndFile during chase — immediately continuing"
                                );
                                self.player.play();
                            } else {
                                tracing::debug!(
                                    pos = self.player.position,
                                    dur = self.player.duration,
                                    "EndFile at live edge — will auto-resume when buffered"
                                );
                            }
                        }
                    }
                }
            }
            Message::ToggleStats => {
                self.show_stats = !self.show_stats;
                if self.show_stats {
                    // Populate immediately so the overlay isn't empty for a tick.
                    self.stats = self.player.stats();
                }
            }
            Message::StatsTick => {
                if self.show_stats && !self.stopped {
                    self.stats = self.player.stats();
                }
            }
            Message::CycleFrameMode => {
                // Framing is applied by our own letterbox shader (mpv renders at
                // native size), so this only advances the mode the renderer reads.
                self.frame_mode = self.frame_mode.next();
                return Task::done(Message::ShowOsd(self.frame_mode.label().into()));
            }
            Message::LiveDurationProbed(dur) => {
                // Update duration from the container byte-rate probe so the
                // seekbar shows the full file extent immediately on load,
                // without mpv having to index the entire file.
                if dur > self.player.duration {
                    tracing::debug!(
                        probed = dur,
                        mpv = self.player.duration,
                        "LiveDurationProbed: updating duration from probe"
                    );
                    self.player.duration = dur;
                    // Remember as a floor so later mpv DurationChanged events
                    // (which start small) can't pull the seekbar back down.
                    self.probed_duration = dur;
                    self.size_ref_duration = dur;
                    if self.size_ref_size == 0 {
                        if let Some(path) = &self.player.path {
                            if let Ok(meta) = std::fs::metadata(path) {
                                self.size_ref_size = meta.len();
                            }
                        }
                    }
                }
            }
            Message::MetadataProbed(path, size, dur) => {
                self.size_cache.insert(path.clone(), size);
                if let Some(d) = dur {
                    self.resume_db.record_duration(&path.to_string_lossy(), d);
                }
            }
            Message::MetadataBatch(results) => {
                for (path, size, dur) in results {
                    self.size_cache.insert(path.clone(), size);
                    if let Some(d) = dur {
                        self.resume_db.record_duration(&path.to_string_lossy(), d);
                    }
                }
                self.resume_db.save();
            }
            Message::ProbeFiles(paths) => {
                // Kick off background probing for any files not already cached.
                let uncached: Vec<_> = paths.into_iter()
                    .filter(|p| !self.size_cache.contains_key(p))
                    .collect();
                if uncached.is_empty() { return Task::none(); }
                return Task::perform(
                    async move {
                        uncached.into_iter().map(|path| {
                            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let dur  = crate::media_probe::probe_duration(&path);
                            (path, size, dur)
                        }).collect::<Vec<_>>()
                    },
                    |results| {
                        // Batch into multiple messages via a sequence.
                        // Use the first result as the message type - the rest
                        // are sent via a follow-up chain.
                        // Simple: just return the first; rest will re-trigger.
                        // Actually send all at once via Noop + side effects.
                        // Best approach: return a Task::batch in the handler.
                        Message::MetadataBatch(results)
                    },
                );
            }
            Message::SetAfterPlayback(a) => self.after_playback = a,
            Message::PauseChanged(p) => {
                tracing::debug!(paused = p, pos = self.player.position, dur = self.player.duration, "PauseChanged");
                self.player.paused = p;
                #[cfg(target_os = "windows")]
                {
                    crate::win32_modal::update_thumbbar_playpause(p);
                    crate::win32_modal::update_smtc_playback(p);
                }
            }

            Message::FrameReady(pixels, w, h) => {
                if self.stopped {
                    // Drop frames that arrive after Stop until playback resumes.
                    return Task::none();
                }
                self.current_frame = Some(VideoFrame {
                    pixels: Arc::new(pixels),
                    width: w,
                    height: h,
                });
            }

            Message::WidthChanged(w) => {
                self.player.width = w;
                self.apply_render_size();
            }
            Message::HeightChanged(h) => {
                self.player.height = h;
                self.apply_render_size();
            }
            Message::VideoCodecChanged(s) => self.player.video_codec = s,
            Message::AudioCodecChanged(s) => self.player.audio_codec = s,
            Message::AudioChannelsChanged(c) => self.player.audio_channels = c,
            Message::HwDecChanged(s) => self.player.hwdec = s,
            Message::PrimariesChanged(s) => self.player.primaries = s,
            Message::SubVisibleChanged(v) => self.player.sub_visible = v,
            Message::SubTracksChanged(list) => self.player.sub_tracks = list,
            Message::CurrentSidChanged(id) => self.player.current_sid = id,
            Message::CurrentSecondarySidChanged(id) => self.player.current_secondary_sid = id,
            Message::SecondarySubTrackSelected(track) => {
                self.player.set_secondary_sub_track(track.id);
                self.subs_menu_open = false;
                return Task::done(Message::ShowOsd(format!("Secondary subtitles  {}", track.label)));
            }
            Message::ChaptersChanged(list) => self.player.chapters = list,
            Message::SubTrackSelected(track) => {
                self.player.set_sub_track(track.id);
                self.subs_menu_open = false;
                if let Some(path) = &self.player.path.clone() {
                    self.resume_db.record_sub_track(path, track.id);
                    self.resume_db.save();
                }
                return Task::done(Message::ShowOsd(format!("Subtitles  {}", track.label)));
            }
            Message::ToggleSubsMenu => {
                self.subs_menu_open = !self.subs_menu_open;
                if self.subs_menu_open {
                    if let Some((x, _)) = self.cursor_pos { self.popup_anchor_x = x; }
                }
            }
            Message::CloseSubsMenu => self.subs_menu_open = false,
            Message::AudioTracksChanged(list) => self.player.audio_tracks = list,
            Message::CurrentAidChanged(id) => self.player.current_aid = id,
            Message::AudioTrackSelected(track) => {
                self.player.set_audio_track(track.id);
                self.audio_menu_open = false;
                if let Some(path) = &self.player.path.clone() {
                    self.resume_db.record_audio_track(path, track.id);
                    self.resume_db.save();
                }
                return Task::done(Message::ShowOsd(format!("Audio  {}", track.label)));
            }
            Message::CycleAudio => {
                self.player.cycle_audio();
                // OSD will update when CurrentAid fires back from mpv.
            }
            Message::ToggleAudioMenu => {
                self.audio_menu_open = !self.audio_menu_open;
                if self.audio_menu_open {
                    if let Some((x, _)) = self.cursor_pos { self.popup_anchor_x = x; }
                }
            }
            Message::CloseAudioMenu => self.audio_menu_open = false,
            Message::SpeedChanged(s) => {
                self.player.speed = s;
                if self.suppress_speed_osd {
                    self.suppress_speed_osd = false;
                } else {
                    let label = if (s - 1.0).abs() < 0.01 {
                        "Speed  1x (normal)".into()
                    } else {
                        format!("Speed  {:.2}x", s)
                    };
                    return Task::done(Message::ShowOsd(label));
                }
            }
            Message::SpeedAdjust(delta) => {
                let new_speed = (self.player.speed + delta).clamp(0.25, 4.0);
                // Round to nearest 0.05 to avoid floating-point drift.
                let new_speed = (new_speed * 20.0).round() / 20.0;
                self.player.set_speed(new_speed);
            }
            Message::SpeedReset => self.player.set_speed(1.0),
            Message::SpeedStep(faster) => {
                let delta = if faster { self.speed_step } else { -self.speed_step };
                return Task::done(Message::SpeedAdjust(delta));
            }
            Message::SubSeek(forward) => self.player.sub_seek(forward),
            Message::ShowOsd(msg) => {
                if !self.osd_enabled {
                    return Task::none();
                }
                self.osd_message = msg;
                self.osd_seq += 1;
                let seq = self.osd_seq;
                return Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(2000)).await;
                        seq
                    },
                    Message::ClearOsd,
                );
            }
            Message::ClearOsd(seq) => {
                if seq == self.osd_seq {
                    self.osd_message.clear();
                }
            }
            Message::GenerateThumbnails(id) => {
                if id != self.thumb_pending_id || !self.thumbnail_preview { return Task::none(); }
                if let Some(path) = &self.player.path.clone() {
                    tracing::debug!(dur = self.player.duration, "thumbnail: deferred generate");
                    crate::thumbnail::spawn_generate(
                        path.clone(), self.player.duration, self.thumb_cache.clone(),
                    );
                }
            }

            Message::TogglePanel(kind) => {
                return self.toggle_panel(Some(kind));
            }
            Message::CloseSettingsPanel => {
                return self.toggle_panel(None);
            }

            Message::BrowserNavigate(path) => {
                self.browser_go(Some(path));
                let paths: Vec<_> = self.browser_entries.iter()
                    .filter(|e| !e.is_dir).map(|e| e.path.clone()).collect();
                if !paths.is_empty() {
                    return Task::done(Message::ProbeFiles(paths));
                }
            }
            Message::BrowserNavigateUp => {
                let target = match &self.browser_path {
                    Some(cur) => match cur.parent() {
                        Some(p) if p != cur.as_path() => Some(p.to_path_buf()),
                        _ => None, // already at root - go to drives list
                    },
                    None => None, // already at drives
                };
                self.browser_go(target);
            }
            Message::BrowserGoToDrives => {
                self.browser_go(None);
            }
            Message::BrowserBack => {
                if let Some(target) = self.browser_back_stack.pop() {
                    self.browser_forward_stack.push(self.browser_path.clone());
                    self.browser_path = target;
                    self.browser_entries = match &self.browser_path {
                        Some(p) => browser_read_dir(p),
                        None => browser_drives(),
                    };
                }
            }
            Message::BrowserForward => {
                if let Some(target) = self.browser_forward_stack.pop() {
                    self.browser_back_stack.push(self.browser_path.clone());
                    self.browser_path = target;
                    self.browser_entries = match &self.browser_path {
                        Some(p) => browser_read_dir(p),
                        None => browser_drives(),
                    };
                }
            }
            Message::BrowserOpen(path) => {
                // Update browser to show the file's directory first.
                if let Some(dir) = path.parent() {
                    self.browser_go(Some(dir.to_path_buf()));
                }
                self.load_path(path);
            }

            Message::PlaylistJump(idx) => {
                if idx < self.playlist.len() {
                    self.playlist_idx = idx;
                    let p = self.playlist[idx].clone();
                    self.open_next(p.to_string_lossy().into_owned());
                }
            }

            Message::ResumePosition(pos) => {
                self.player.seek(pos, true); // startup resume - always exact
                return Task::done(Message::ShowOsd(format!("Resuming from {}", fmt_time(pos))));
            }

            Message::SubDelayChanged(v) => self.player.sub_delay = v,
            Message::AudioDelayChanged(v) => self.player.audio_delay = v,
            Message::SubDelayAdjust(delta) => {
                let new_val = (self.player.sub_delay + delta) * 10.0 / 10.0;
                let new_val = (new_val * 10.0).round() / 10.0;
                self.player.set_sub_delay(new_val);
                return Task::done(Message::ShowOsd(format!("Sub delay  {:+.1}s", new_val)));
            }
            Message::AudioDelayAdjust(delta) => {
                let new_val = (self.player.audio_delay + delta) * 10.0 / 10.0;
                let new_val = (new_val * 10.0).round() / 10.0;
                self.player.set_audio_delay(new_val);
                return Task::done(Message::ShowOsd(format!("Audio delay  {:+.1}s", new_val)));
            }
            Message::SubDelayReset => {
                self.player.set_sub_delay(0.0);
                return Task::done(Message::ShowOsd("Sub delay  reset".into()));
            }
            Message::AudioDelayReset => {
                self.player.set_audio_delay(0.0);
                return Task::done(Message::ShowOsd("Audio delay  reset".into()));
            }
            Message::AbLoopSetA => {
                self.ab_loop_a = Some(self.player.position);
                let a = self.player.position;
                let osd = if let Some(b) = self.ab_loop_b {
                    format!("AB: A={} B={}", fmt_time(a), fmt_time(b))
                } else {
                    format!("AB: A={} (seek to set B)", fmt_time(a))
                };
                return Task::done(Message::ShowOsd(osd));
            }
            Message::AbLoopSetB => {
                self.ab_loop_b = Some(self.player.position);
                let b = self.player.position;
                let osd = if let Some(a) = self.ab_loop_a {
                    format!("AB: A={} B={} (looping)", fmt_time(a), fmt_time(b))
                } else {
                    format!("AB: B={} (seek to set A)", fmt_time(b))
                };
                return Task::done(Message::ShowOsd(osd));
            }
            Message::AbLoopClear => {
                self.ab_loop_a = None;
                self.ab_loop_b = None;
                return Task::done(Message::ShowOsd("AB repeat cleared".into()));
            }

            Message::ShowHelp => { self.show_help = !self.show_help; }

            Message::OpenSubSearch => {
                // Pre-fill query from current filename.
                if self.sub_search_query.is_empty() {
                    if let Some(path) = &self.player.path {
                        let stem = std::path::Path::new(path)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.sub_search_query = stem;
                    }
                }
                self.sub_search_open = true;
                self.sub_search_results.clear();
            }
            Message::SubSearchQuery(q) => self.sub_search_query = q,
            Message::SubSearch => {
                if self.sub_search_query.is_empty() { return Task::none(); }
                self.sub_search_loading = true;
                self.sub_search_results.clear();
                let query = self.sub_search_query.clone();
                let key   = self.opensubtitles_api_key.clone();
                return Task::perform(
                    async move {
                        crate::opensubs::search(&query, "en", &key).await
                    },
                    |r| match r {
                        Ok(results) => Message::SubSearchResults(results),
                        Err(e)      => Message::SubSearchError(e.to_string()),
                    },
                );
            }
            Message::SubSearchResults(results) => {
                self.sub_search_loading = false;
                self.sub_search_results = results;
            }
            Message::SubSearchError(e) => {
                self.sub_search_loading = false;
                return Task::done(Message::ShowOsd(format!("Subtitle search failed: {e}")));
            }
            Message::SubDownload(file_id, filename) => {
                let key = self.opensubtitles_api_key.clone();
                return Task::perform(
                    async move {
                        crate::opensubs::download_to_temp(file_id, &filename, &key).await
                    },
                    |r| match r {
                        Ok(path) => Message::SubDownloaded(path),
                        Err(e)   => Message::SubSearchError(e.to_string()),
                    },
                );
            }
            Message::CloseSubSearch => { self.sub_search_open = false; }
            Message::SubDownloaded(path) => {
                self.sub_search_open = false;
                if path.is_empty() { return Task::none(); }
                self.player.add_sub_file(&path);
                return Task::done(Message::ShowOsd("Subtitle loaded".into()));
            }

            Message::SavePlaylist => {
                let paths = self.playlist.clone();
                return Task::perform(
                    rfd::AsyncFileDialog::new()
                        .set_title("Save playlist")
                        .add_filter("M3U playlist", &["m3u", "m3u8"])
                        .set_file_name("playlist.m3u")
                        .save_file(),
                    move |f| match f {
                        Some(h) => {
                            let path = h.path().to_path_buf();
                            let content = paths.iter()
                                .map(|p| p.to_string_lossy().into_owned())
                                .collect::<Vec<_>>()
                                .join("\n");
                            let _ = std::fs::write(&path, format!("#EXTM3U\n{content}"));
                            Message::PlaylistSaved(path.to_string_lossy().into_owned())
                        }
                        None => Message::Noop,
                    },
                );
            }
            Message::PlaylistSaved(path) => {
                let name = std::path::Path::new(&path)
                    .file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(path);
                return Task::done(Message::ShowOsd(format!("Playlist saved  {name}")));
            }
            Message::LoadPlaylist => {
                return Task::perform(
                    rfd::AsyncFileDialog::new()
                        .set_title("Load playlist")
                        .add_filter("Playlist files", &["m3u", "m3u8", "pls"])
                        .pick_file(),
                    |f| match f {
                        Some(h) => {
                            let content = std::fs::read_to_string(h.path()).unwrap_or_default();
                            let is_pls = h.path().extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| e.eq_ignore_ascii_case("pls"));
                            let entries: Vec<String> = if is_pls {
                                parse_pls(&content)
                            } else {
                                content.lines()
                                    .filter(|l| !l.starts_with('#') && !l.is_empty())
                                    .map(|l| l.trim().to_string())
                                    .collect()
                            };
                            let paths: Vec<_> = entries.into_iter()
                                // Keep URL entries unconditionally (they never
                                // "exist" as a local path) - only filesystem
                                // paths need the exists() check to drop stale
                                // entries.
                                .filter(|l| {
                                    l.starts_with("http://") || l.starts_with("https://")
                                        || l.starts_with("rtsp://") || l.starts_with("rtmp://")
                                        || std::path::Path::new(l).exists()
                                })
                                .map(std::path::PathBuf::from)
                                .collect();
                            Message::PlaylistLoaded(paths)
                        }
                        None => Message::Noop,
                    },
                );
            }
            Message::PlaylistLoaded(paths) => {
                if !paths.is_empty() {
                    let count = paths.len();
                    let url_fetches: Vec<Task<Message>> = paths.iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
                        .map(|url| Task::perform(fetch_url_metadata(url), |(url, meta)| Message::UrlMetaFetched(url, meta)))
                        .collect();
                    self.playlist = paths;
                    self.playlist_idx = 0;
                    let p = self.playlist[0].clone();
                    self.open_next(p.to_string_lossy().into_owned());
                    return Task::batch(
                        std::iter::once(Task::done(Message::ShowOsd(format!("Loaded {count} files"))))
                            .chain(url_fetches),
                    );
                }
            }

            Message::JumpToTime => {
                return Task::done(Message::OpenModal(ModalKind::JumpToTime));
            }
            Message::OpenModal(kind) => {
                let (title, prompt) = match kind {
                    ModalKind::JumpToTime => ("Jump to time", "Enter time (1:23:45 or seconds)"),
                    ModalKind::OpenUrl    => ("Open URL / stream", "Enter URL or file path"),
                    ModalKind::AddPlaylistUrl => ("Add URL to playlist", "Enter URL or file path"),
                };
                self.modal = Some(ModalDialog { title, prompt, input: String::new(), kind });
            }
            Message::ModalInput(s) => {
                if let Some(m) = &mut self.modal { m.input = s; }
            }
            Message::ModalRightClick => {
                if self.modal.is_some() {
                    self.modal_paste_menu = self.cursor_pos;
                }
            }
            Message::CloseModalPasteMenu => {
                self.modal_paste_menu = None;
            }
            Message::ModalPasteRequest => {
                self.modal_paste_menu = None;
                if self.modal.is_some() {
                    return iced::clipboard::read().map(Message::ModalPasteResult);
                }
            }
            Message::ModalPasteResult(Some(text)) => {
                if let Some(m) = &mut self.modal { m.input = text; }
            }
            Message::ModalPasteResult(None) => {}
            Message::ModalConfirm => {
                self.modal_paste_menu = None;
                if let Some(m) = self.modal.take() {
                    match m.kind {
                        ModalKind::JumpToTime => {
                            if let Some(t) = parse_time(&m.input) {
                                return Task::done(Message::Seek(t));
                            }
                        }
                        ModalKind::OpenUrl => {
                            if !m.input.is_empty() {
                                let path = std::path::PathBuf::from(&m.input);
                                if path.exists() {
                                    self.load_path(path);
                                } else if needs_ytdl(&m.input) && !ytdl_available() {
                                    // mpv's built-in ytdl_hook script needs
                                    // yt-dlp (or youtube-dl) to resolve sites
                                    // like YouTube into an actual playable
                                    // stream - without it, loadfile just
                                    // silently fails. Direct media URLs
                                    // (.m3u8/.mp4/rtsp/etc.) don't need this
                                    // at all, so only fetch it for URLs that
                                    // look like they do. yt-dlp is public
                                    // domain (The Unlicense), so
                                    // auto-fetching a copy has no licensing
                                    // concerns.
                                    self.pending_ytdl_url = Some(m.input);
                                    return Task::batch([
                                        Task::done(Message::ShowOsd("Downloading yt-dlp...".into())),
                                        Task::perform(download_ytdlp(), Message::YtdlpDownloadResult),
                                    ]);
                                } else {
                                    self.player.open_url(&m.input);
                                    self.recent_files.record(&std::path::PathBuf::from(&m.input));
                                    self.recent_files.save();
                                    return Task::done(Message::ShowOsd(format!("Opening: {}", m.input)));
                                }
                            }
                        }
                        ModalKind::AddPlaylistUrl => {
                            if !m.input.is_empty() {
                                let was_empty = self.playlist.is_empty();
                                self.playlist.push(std::path::PathBuf::from(&m.input));
                                if was_empty {
                                    self.playlist_idx = 0;
                                }
                                let url = m.input.clone();
                                return Task::batch([
                                    Task::done(Message::ShowOsd(format!("Added to playlist: {}", m.input))),
                                    Task::perform(fetch_url_metadata(url), |(url, meta)| Message::UrlMetaFetched(url, meta)),
                                ]);
                            }
                        }
                    }
                }
            }
            Message::ModalCancel => {
                self.modal = None;
                self.modal_paste_menu = None;
            }
            Message::NextChapter => {
                if !self.player.chapters.is_empty() {
                    self.player.seek_chapter(1);
                } else {
                    self.open_next(self.playlist[
                        (self.playlist_idx + 1).min(self.playlist.len().saturating_sub(1))
                    ].to_string_lossy().into_owned());
                }
            }
            Message::PrevChapter => {
                if !self.player.chapters.is_empty() {
                    self.player.seek_chapter(-1);
                } else if self.playlist_idx > 0 {
                    self.playlist_idx -= 1;
                    let p = self.playlist[self.playlist_idx].clone();
                    self.open_next(p.to_string_lossy().into_owned());
                }
            }
            Message::FilesDropped(paths) => {
                if paths.is_empty() { return Task::none(); }
                // If cursor is over the side panel, append without replacing current file.
                let over_panel = self.active_panel.is_some()
                    && self.cursor_pos
                        .map(|(x, _)| x >= self.window_w_logical - PANEL_W)
                        .unwrap_or(false);
                if over_panel {
                    // Append all dropped files to the playlist.
                    let mut added = 0usize;
                    for p in paths {
                        if !self.playlist.contains(&p) {
                            self.playlist.push(p);
                            added += 1;
                        }
                    }
                    if added > 0 {
                        return Task::done(Message::ShowOsd(format!("Added {added} file{} to playlist",
                            if added == 1 { "" } else { "s" })));
                    }
                } else {
                    // Drop onto video area: open first file, append rest.
                    let mut it = paths.into_iter();
                    let first = it.next().unwrap();
                    self.load_path(first);
                    for extra in it {
                        if !self.playlist.contains(&extra) {
                            self.playlist.push(extra);
                        }
                    }
                }
            }
            Message::VideoRotateCw => {
                self.video_rotate = (self.video_rotate + 90).rem_euclid(360);
                self.player.set_rotate(self.video_rotate);
            }
            Message::VideoRotateCcw => {
                self.video_rotate = (self.video_rotate - 90).rem_euclid(360);
                self.player.set_rotate(self.video_rotate);
            }
            Message::VideoHFlip => {
                self.video_hflip = !self.video_hflip;
                self.player.toggle_hflip(self.video_hflip);
            }
            Message::VideoVFlip => {
                self.video_vflip = !self.video_vflip;
                self.player.toggle_vflip(self.video_vflip);
            }
            Message::VideoTransformReset => {
                self.video_rotate = 0;
                self.video_hflip = false;
                self.video_vflip = false;
                self.player.set_rotate(0);
                self.player.toggle_hflip(false);
                self.player.toggle_vflip(false);
            }
            Message::OpenUrl => {
                return Task::done(Message::OpenModal(ModalKind::OpenUrl));
            }
            Message::YtdlpDownloadResult(result) => {
                match result {
                    Ok(path) => {
                        self.player.set_ytdl_path(&path);
                        if let Some(url) = self.pending_ytdl_url.take() {
                            self.player.open_url(&url);
                            self.recent_files.record(&std::path::PathBuf::from(&url));
                            self.recent_files.save();
                            return Task::done(Message::ShowOsd(format!("Opening: {url}")));
                        }
                    }
                    Err(e) => {
                        self.pending_ytdl_url = None;
                        return Task::done(Message::ShowOsd(format!("yt-dlp download failed: {e}")));
                    }
                }
            }
            Message::ToggleResume => {
                self.resume_enabled = !self.resume_enabled;
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.resume_enabled = self.resume_enabled;
                prefs.save();
            }
            Message::ToggleLoopFile => {
                let on = !self.player.loop_file;
                self.player.set_loop_file(on);
                let label = if on { "Loop: on" } else { "Loop: off" };
                return Task::done(Message::ShowOsd(label.into()));
            }
            Message::LoopFileChanged(v) => self.player.loop_file = v,

            Message::ToggleLoopPlaylist => {
                let on = !self.player.loop_playlist;
                self.player.set_loop_playlist(on);
                let label = if on { "Loop playlist: on" } else { "Loop playlist: off" };
                return Task::done(Message::ShowOsd(label.into()));
            }
            Message::LoopPlaylistChanged(v) => self.player.loop_playlist = v,

            Message::ShufflePlaylist => {
                if self.playlist.len() > 1 {
                    let current = self.playlist[self.playlist_idx].clone();
                    // Fisher-Yates with a time-seeded LCG (no rand crate needed).
                    let seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos() as u64;
                    let mut rng = seed.wrapping_add(0xdeadbeef);
                    for i in (1..self.playlist.len()).rev() {
                        rng = rng.wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        let j = (rng >> 33) as usize % (i + 1);
                        self.playlist.swap(i, j);
                    }
                    // Keep current file at the front after shuffle.
                    if let Some(pos) = self.playlist.iter().position(|p| p == &current) {
                        self.playlist.swap(0, pos);
                    }
                    self.playlist_idx = 0;
                    return Task::done(Message::ShowOsd("Playlist shuffled".into()));
                }
            }

            Message::FileContextMenu(path) => {
                let (x, y) = self.cursor_pos.unwrap_or((100.0, 100.0));
                self.file_context_menu = Some(crate::app::FileContextMenu { path, x, y });
            }
            Message::CloseFileContextMenu => { self.file_context_menu = None; }
            Message::ShowVideoContextMenu => {
                if self.menu_window_id.is_some() { return Task::none(); }
                let (cx, cy) = self.cursor_pos.unwrap_or((100.0, 100.0));
                let screen_x = self.window_x_logical as f32 + cx;
                let screen_y = self.window_y_logical as f32 + cy;
                return self.open_main_menu(screen_x, screen_y);
            }
            Message::ToggleMainMenu => {
                if self.menu_window_id.is_some() {
                    return self.close_main_menu();
                }
                // Fixed anchor near the hamburger button, rather than the
                // cursor - the button sits in the same spot regardless of
                // where the mouse happens to be at click time.
                let screen_x = self.window_x_logical as f32 + 8.0;
                let screen_y = self.window_y_logical as f32 + TOP_BAR_H as f32 + 4.0;
                return self.open_main_menu(screen_x, screen_y);
            }
            Message::VideoMenuAction(inner) => {
                let close = self.close_main_menu();
                return Task::batch([close, Task::done(*inner)]);
            }
            Message::ToggleMenuSection(idx) => {
                if idx < self.menu_section_open.len() {
                    self.menu_section_open[idx] = !self.menu_section_open[idx];
                }
                if let (Some(win_id), Some((ax, ay)), Some(main_id)) =
                    (self.menu_window_id, self.menu_anchor, self.window_id)
                {
                    let height = ui::menu_window_height(self);
                    return iced::window::scale_factor(main_id)
                        .map(move |dpi| Message::RepositionMenuPopup(win_id, ax, ay, height, dpi));
                }
            }
            Message::OpenMenuPopup(screen_x, screen_y, height, dpi) => {
                let width = 224.0;
                let (x, y) = self.clamp_menu_pos(screen_x, screen_y, width, height, dpi);
                let settings = iced::window::Settings {
                    size: iced::Size::new(width, height),
                    position: iced::window::Position::Specific(iced::Point::new(x, y)),
                    resizable: false,
                    decorations: false,
                    transparent: true,
                    level: iced::window::Level::AlwaysOnTop,
                    exit_on_close_request: false,
                    ..Default::default()
                };
                let (id, open_task) = iced::window::open(settings);
                self.menu_window_id = Some(id);
                return open_task.discard();
            }
            Message::RepositionMenuPopup(win_id, ax, ay, height, dpi) => {
                let width = 224.0;
                let (x, y) = self.clamp_menu_pos(ax, ay, width, height, dpi);
                return Task::batch([
                    iced::window::resize(win_id, iced::Size::new(width, height)),
                    iced::window::move_to(win_id, iced::Point::new(x, y)),
                ]);
            }
            Message::DetachPanel => {
                if self.active_panel.is_some() && self.panel_window_id.is_none() {
                    // First-ever detach: start at the same size it was
                    // docked at, so popping it out isn't a jarring resize.
                    // After that, remember whatever size/position it was
                    // last left at (see `panel_last_size`/`panel_last_pos`).
                    let start_h = if self.window_h_logical > 0.0 { self.window_h_logical } else { 640.0 };
                    let size = self.panel_last_size.unwrap_or((PANEL_W, start_h));
                    let position = match self.panel_last_pos {
                        Some((x, y)) => iced::window::Position::Specific(iced::Point::new(x as f32, y as f32)),
                        None => iced::window::Position::Default,
                    };
                    let settings = iced::window::Settings {
                        size: iced::Size::new(size.0, size.1),
                        position,
                        resizable: true,
                        decorations: !use_custom_title_bar(),
                        icon: app_icon(),
                        exit_on_close_request: false,
                        ..Default::default()
                    };
                    let (id, open_task) = iced::window::open(settings);
                    self.panel_window_id = Some(id);
                    // The docked panel no longer occupies width in the main
                    // window - shrink it back, or the video would stretch
                    // into that now-empty space instead of staying the size
                    // it was before the panel took up room.
                    let resize = self.resize_main_for_panel(false);
                    return Task::batch([open_task.discard(), resize]);
                }
            }
            Message::ReattachPanel => {
                if let Some(id) = self.panel_window_id.take() {
                    // Grow the main window back out to make room for the
                    // panel resuming its docked width - the mirror image of
                    // the shrink in `DetachPanel`.
                    let resize = self.resize_main_for_panel(true);
                    return Task::batch([iced::window::close(id), resize]);
                }
            }
            Message::ClosePanelWindow => {
                self.active_panel = None;
                if let Some(id) = self.panel_window_id.take() {
                    return iced::window::close(id);
                }
            }
            Message::PanelMinimize => {
                if let Some(id) = self.panel_window_id {
                    return iced::window::minimize(id, true);
                }
            }
            Message::PanelToggleMaximize => {
                if let Some(id) = self.panel_window_id {
                    return iced::window::toggle_maximize(id);
                }
            }
            Message::PanelDragWindow => {
                if let Some(id) = self.panel_window_id {
                    return iced::window::drag(id);
                }
            }
            Message::OpenAppSettings => {
                if self.app_settings_window_id.is_none() {
                    let size = self.app_settings_last_size.unwrap_or((760.0, 520.0));
                    let position = match self.app_settings_last_pos {
                        Some((x, y)) => iced::window::Position::Specific(iced::Point::new(x as f32, y as f32)),
                        None => iced::window::Position::Centered,
                    };
                    let settings = iced::window::Settings {
                        size: iced::Size::new(size.0, size.1),
                        position,
                        min_size: Some(iced::Size::new(480.0, 360.0)),
                        resizable: true,
                        decorations: !use_custom_title_bar(),
                        icon: app_icon(),
                        exit_on_close_request: false,
                        ..Default::default()
                    };
                    let (id, open_task) = iced::window::open(settings);
                    self.app_settings_window_id = Some(id);
                    return open_task.discard();
                }
            }
            Message::CloseAppSettingsWindow => {
                if let Some(id) = self.app_settings_window_id.take() {
                    return iced::window::close(id);
                }
            }
            Message::AppSettingsCategorySelect(cat) => {
                self.app_settings_category = cat;
            }
            Message::AppSettingsMinimize => {
                if let Some(id) = self.app_settings_window_id {
                    return iced::window::minimize(id, true);
                }
            }
            Message::AppSettingsToggleMaximize => {
                if let Some(id) = self.app_settings_window_id {
                    return iced::window::toggle_maximize(id);
                }
            }
            Message::AppSettingsDragWindow => {
                if let Some(id) = self.app_settings_window_id {
                    return iced::window::drag(id);
                }
            }
            Message::OpenFileLocation(path) => {
                self.file_context_menu = None;
                if let Some(dir) = path.parent() {
                    #[cfg(target_os = "windows")]
                    {
                        let _ = std::process::Command::new("explorer")
                            .arg(dir)
                            .spawn();
                    }
                }
            }
            Message::CopyFilePath(path) => {
                self.file_context_menu = None;
                let path_str = path.to_string_lossy().into_owned();
                // Use iced's clipboard via a task.
                return iced::clipboard::write(path_str);
            }
            Message::TogglePanelsMenu => {
                // Direct toggle (no picker): close the open panel, or reopen the
                // last-used one. Switch panels via the tab bar once open.
                if self.active_panel.is_some() {
                    return self.toggle_panel(None);
                } else {
                    return self.toggle_panel(Some(self.last_panel));
                }
            }
            Message::TogglePlaylistSort => {
                self.playlist_sort_open = !self.playlist_sort_open;
            }
            Message::SortPlaylist(order) => {
                self.playlist_sort_open = false;
                if !self.playlist.is_empty() {
                    let current = self.playlist[self.playlist_idx].clone();
                    match order {
                        PlaylistSort::Name =>
                            self.playlist.sort_by(|a, b| a.file_name().cmp(&b.file_name())),
                        PlaylistSort::NameDesc =>
                            self.playlist.sort_by(|a, b| b.file_name().cmp(&a.file_name())),
                        PlaylistSort::Size => self.playlist.sort_by_key(|p| {
                            std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
                        }),
                        PlaylistSort::SizeDesc => self.playlist.sort_by(|a, b| {
                            let sa = std::fs::metadata(a).map(|m| m.len()).unwrap_or(0);
                            let sb = std::fs::metadata(b).map(|m| m.len()).unwrap_or(0);
                            sb.cmp(&sa)
                        }),
                        PlaylistSort::Modified => self.playlist.sort_by_key(|p| {
                            std::fs::metadata(p)
                                .and_then(|m| m.modified())
                                .unwrap_or(std::time::UNIX_EPOCH)
                        }),
                    }
                    if let Some(pos) = self.playlist.iter().position(|p| p == &current) {
                        self.playlist_idx = pos;
                    }
                }
            }

            Message::ClearRecent => {
                self.recent_files.paths.clear();
                self.recent_files.save();
            }

            Message::VideoZoomSet(v) => {
                self.player.set_video_zoom(v);
            }
            Message::VideoZoomReset => {
                self.player.set_video_zoom(0.0);
                self.player.set_video_pan(0.0, 0.0);
                self.player.video_pan_x = 0.0;
                self.player.video_pan_y = 0.0;
            }
            Message::VideoZoomChanged(v) => self.player.video_zoom = v,
            Message::CacheTimeChanged(v) => self.player.cache_time = v,

            Message::AspectRatioSet(ratio) => {
                self.player.set_aspect_ratio(&ratio);
                let label = if ratio.is_empty() { "Aspect: Auto".into() } else { format!("Aspect: {ratio}") };
                return Task::done(Message::ShowOsd(label));
            }

            Message::PlaylistRemove(idx) => {
                if idx < self.playlist.len() {
                    self.playlist.remove(idx);
                    // Keep playlist_idx valid.
                    if self.playlist.is_empty() {
                        self.playlist_idx = 0;
                    } else if self.playlist_idx >= self.playlist.len() {
                        self.playlist_idx = self.playlist.len() - 1;
                    } else if idx < self.playlist_idx {
                        self.playlist_idx = self.playlist_idx.saturating_sub(1);
                    }
                }
            }

            Message::WindowMoved(id, x, y) => {
                // Guard against the menu popup's own Moved event overwriting
                // the main window's tracked screen position - the popup's
                // open position is computed FROM these fields, so letting
                // its own Moved event feed back in here would make each
                // reopen drift further from the true main-window position.
                if Some(id) == self.window_id {
                    // Windows reports a window's position as a sentinel far
                    // off-screen (around -32000, -32000) while it's
                    // minimized - saving that as if it were a real position
                    // would make the window unreachable on next launch.
                    // Any real monitor position is well within this range.
                    const SENTINEL_THRESHOLD: i32 = -10_000;
                    if x > SENTINEL_THRESHOLD && y > SENTINEL_THRESHOLD {
                        self.window_x_logical = x;
                        self.window_y_logical = y;
                    }
                } else if Some(id) == self.panel_window_id {
                    const SENTINEL_THRESHOLD: i32 = -10_000;
                    if x > SENTINEL_THRESHOLD && y > SENTINEL_THRESHOLD {
                        self.panel_last_pos = Some((x, y));
                    }
                } else if Some(id) == self.app_settings_window_id {
                    const SENTINEL_THRESHOLD: i32 = -10_000;
                    if x > SENTINEL_THRESHOLD && y > SENTINEL_THRESHOLD {
                        self.app_settings_last_pos = Some((x, y));
                    }
                }
            }

            Message::LoadSubtitle => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Load subtitle file")
                            .add_filter("Subtitles", &["srt","ass","ssa","vtt","sub","idx","sup"])
                            .add_filter("All files", &["*"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_string_lossy().into_owned())
                    },
                    |opt| match opt {
                        Some(path) => Message::SubtitleFileSelected(path),
                        None => Message::Noop,
                    },
                );
            }
            Message::SubtitleFileSelected(path) => {
                self.player.add_sub_file(&path);
                return Task::done(Message::ShowOsd("Subtitle loaded".into()));
            }

            Message::ToggleDeinterlace => {
                let on = !self.player.deinterlace;
                self.player.set_deinterlace(on);
                let label = if on { "Deinterlace: on" } else { "Deinterlace: off" };
                return Task::done(Message::ShowOsd(label.into()));
            }
            Message::DeinterlaceChanged(v) => self.player.deinterlace = v,

            Message::BrightnessSet(v) => {
                self.player.brightness = v.clamp(-100, 100);
                let (b,c,s,h,g) = (self.player.brightness, self.player.contrast, self.player.saturation, self.player.hue, self.player.gamma);
                self.player.apply_video_eq(b,c,s,h,g);
            }
            Message::ContrastSet(v) => {
                self.player.contrast = v.clamp(-100, 100);
                let (b,c,s,h,g) = (self.player.brightness, self.player.contrast, self.player.saturation, self.player.hue, self.player.gamma);
                self.player.apply_video_eq(b,c,s,h,g);
            }
            Message::SaturationSet(v) => {
                self.player.saturation = v.clamp(-100, 100);
                let (b,c,s,h,g) = (self.player.brightness, self.player.contrast, self.player.saturation, self.player.hue, self.player.gamma);
                self.player.apply_video_eq(b,c,s,h,g);
            }
            Message::HueSet(v) => {
                self.player.hue = v.clamp(-100, 100);
                let (b,c,s,h,g) = (self.player.brightness, self.player.contrast, self.player.saturation, self.player.hue, self.player.gamma);
                self.player.apply_video_eq(b,c,s,h,g);
            }
            Message::GammaSet(v) => {
                self.player.gamma = v.clamp(-100, 100);
                let (b,c,s,h,g) = (self.player.brightness, self.player.contrast, self.player.saturation, self.player.hue, self.player.gamma);
                self.player.apply_video_eq(b,c,s,h,g);
            }
            Message::VideoEqReset => {
                self.player.brightness = 0;
                self.player.contrast   = 0;
                self.player.saturation = 0;
                self.player.hue        = 0;
                self.player.gamma      = 0;
                self.player.apply_video_eq(0, 0, 0, 0, 0);
            }
            // EQ Changed events are no longer emitted (we manage state directly)
            // but keep arms to avoid exhaustiveness errors.
            Message::SpeedSet(v)        => self.player.set_speed(v),
            Message::SubFontSizeSet(v)  => self.player.set_sub_font_size(v),
            Message::SubPosSet(v)       => self.player.set_sub_pos(v),
            Message::SubFontSizeChanged(v) => self.player.sub_font_size = v,
            Message::SubPosChanged(v)      => self.player.sub_pos = v,

            Message::TakeScreenshot => {
                self.player.screenshot();
                let dir = if self.screenshot_dir.is_empty() { "default folder".into() }
                          else { self.screenshot_dir.clone() };
                return Task::done(Message::ShowOsd(format!("Screenshot saved  {dir}")));
            }
            Message::ChooseScreenshotDir => {
                return Task::perform(
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose screenshot folder")
                        .pick_folder(),
                    |f| match f {
                        Some(h) => Message::ScreenshotDirSelected(h.path().to_string_lossy().into_owned()),
                        None    => Message::Noop,
                    },
                );
            }
            Message::ScreenshotDirSelected(dir) => {
                self.screenshot_dir = dir.clone();
                self.player.set_screenshot_dir(&dir);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.screenshot_dir = dir;
                prefs.save();
            }
            Message::UrlMetaFetched(url, meta) => {
                if let Some(meta) = meta {
                    self.playlist_url_meta.insert(url, meta);
                }
            }

            Message::Noop => {}

            Message::CloseRequested(id) => {
                if Some(id) == self.window_id {
                    self.save_resume();
                    self.player.quit();
                    // Daemons (needed for the floating menu popup window)
                    // don't auto-quit when their windows close, unlike a
                    // plain single-window Application - exit explicitly.
                    // Explicitly close the detached panel window too (if
                    // open) rather than counting on iced::exit() to take it
                    // down cleanly - it's a separate OS window and we'd
                    // rather not risk it lingering as an orphan.
                    let mut tasks = vec![iced::window::close(id)];
                    if let Some(panel_id) = self.panel_window_id {
                        tasks.push(iced::window::close(panel_id));
                    }
                    if let Some(settings_id) = self.app_settings_window_id {
                        tasks.push(iced::window::close(settings_id));
                    }
                    tasks.push(iced::exit());
                    return Task::batch(tasks);
                } else if Some(id) == self.menu_window_id {
                    return self.close_main_menu();
                } else if Some(id) == self.panel_window_id {
                    return Task::done(Message::ClosePanelWindow);
                } else if Some(id) == self.app_settings_window_id {
                    return Task::done(Message::CloseAppSettingsWindow);
                }
            }

            Message::TogglePause => {
                if self.player.paused {
                    self.stopped = false;
                    self.player.play();
                } else {
                    // User explicitly paused — stop the live-edge auto-resume loop.
                    self.live_edge_paused = false;
                    self.player.pause();
                }
            }
            Message::ToggleMute => {
                self.player.toggle_mute();
                let msg = if self.player.muted { "Muted".into() } else { "Unmuted".into() };
                return Task::done(Message::ShowOsd(msg));
            }
            Message::CycleSubtitle => self.player.cycle_subtitle(),
            Message::ToggleSubVisibility => self.player.toggle_sub_visibility(),
            Message::ToggleHwDec => {
                self.player.toggle_hwdec();
                return Task::done(Message::ShowOsd(format!("HW decode  {}",
                    if self.player.hwdec == "no" || self.player.hwdec.is_empty() { "off" } else { "on" }
                )));
            }
            Message::HwDecSet(mode) => {
                self.player.set_hwdec(&mode);
                return Task::done(Message::ShowOsd(format!("HW decode  {mode}")));
            }
            Message::ToggleMaximize => {
                if let Some(id) = self.window_id {
                    return iced::window::toggle_maximize(id);
                }
            }
            Message::FitToVisible => {
                if let (Some(id), Some(size)) = (self.window_id, self.fit_to_visible_size()) {
                    self.fit_menu_open = false;
                    return iced::window::resize(id, size);
                }
            }
            Message::FitToScale(scale) => {
                if self.player.width <= 0 || self.player.height <= 0 || scale <= 0.0 {
                    return Task::none();
                }
                if let Some(id) = self.window_id {
                    self.fit_menu_open = false;
                    let phys_w = self.player.width as f32 * scale;
                    let phys_h = self.player.height as f32 * scale;
                    let chrome_h = if self.chrome_visible() {
                        (CONTROLS_H + TOP_BAR_H) as f32
                    } else {
                        0.0
                    };
                    let panel_w = if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                    return iced::window::scale_factor(id).then(move |dpi| {
                        let w = phys_w / dpi as f32 + panel_w;
                        let h = phys_h / dpi as f32 + chrome_h;
                        iced::window::resize(id, iced::Size::new(w, h))
                    });
                }
            }
            Message::FitToHeight(target_h) => {
                if target_h == 0 {
                    return Task::none();
                }
                // Use the video's native aspect when we have one, otherwise
                // fall back to 16:9 so the menu still works before a file is
                // open.
                let aspect = if self.player.width > 0 && self.player.height > 0 {
                    self.player.width as f32 / self.player.height as f32
                } else {
                    16.0_f32 / 9.0
                };
                let chrome_h = if self.chrome_visible() {
                    (CONTROLS_H + TOP_BAR_H) as f32
                } else {
                    0.0
                };
                if let Some(id) = self.window_id {
                    self.fit_menu_open = false;
                    let panel_w = if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                    return iced::window::scale_factor(id).then(move |dpi| {
                        let h_log = target_h as f32 / dpi as f32;
                        let w_log = h_log * aspect + panel_w;
                        iced::window::resize(
                            id,
                            iced::Size::new(w_log, h_log + chrome_h),
                        )
                    });
                }
            }
            Message::ToggleFitMenu => {
                self.fit_menu_open = !self.fit_menu_open;
                if self.fit_menu_open {
                    if let Some((x, _)) = self.cursor_pos { self.popup_anchor_x = x; }
                }
            }
            Message::CloseFitMenu => self.fit_menu_open = false,
            Message::MinimizeWindow => {
                if let Some(id) = self.window_id {
                    return iced::window::minimize(id, true);
                }
            }
            Message::CloseWindow => {
                self.save_resume();
                self.player.quit();
                if let Some(id) = self.window_id {
                    return Task::batch([iced::window::close(id), iced::exit()]);
                }
            }
            Message::DragWindow => {
                // If the cursor is sitting in an edge grip, InputMouseDown
                // will trigger drag_resize for it; skip the window-move drag
                // so the two don't fight over the same press.
                if self.cursor_edge_direction().is_some() {
                    return Task::none();
                }
                if let Some(id) = self.window_id {
                    return iced::window::drag(id);
                }
            }
            Message::TogglePin => {
                self.pinned = !self.pinned;
                if let Some(id) = self.window_id {
                    let level = if self.pinned {
                        iced::window::Level::AlwaysOnTop
                    } else {
                        iced::window::Level::Normal
                    };
                    return iced::window::set_level(id, level);
                }
            }
            Message::TogglePip => {
                // Doesn't make sense combined with fullscreen - ignore.
                if self.fullscreen { return Task::none(); }
                let Some(id) = self.window_id else { return Task::none(); };

                if self.pip_active {
                    // Exit: restore chrome/pin state, size, and position.
                    self.pip_active = false;
                    let mut tasks: Vec<Task<Message>> = Vec::new();
                    if self.chrome_force_hidden != self.pip_prev_chrome_hidden {
                        tasks.push(Task::done(Message::ToggleChrome));
                    }
                    if self.pinned != self.pip_prev_pinned {
                        tasks.push(Task::done(Message::TogglePin));
                    }
                    tasks.push(iced::window::resize(
                        id, iced::Size::new(self.pip_prev_w, self.pip_prev_h),
                    ));
                    tasks.push(iced::window::move_to(
                        id, iced::Point::new(self.pip_prev_x as f32, self.pip_prev_y as f32),
                    ));
                    return Task::batch(tasks);
                }

                // Enter: save current state to restore on exit, then shrink
                // the window down to exactly the video's current size
                // (trimming the chrome height/panel width that's about to
                // become dead space now that chrome hides), pin, and dock
                // to a screen corner. Chrome hides via the same mechanism
                // Focus mode uses, but the PiP view replaces it with a
                // minimal play/pause + close overlay instead of reusing the
                // full app chrome - see ui::mod's player_col branch.
                self.pip_active = true;
                self.pip_prev_w = self.window_w_logical;
                self.pip_prev_h = self.window_h_logical;
                self.pip_prev_x = self.window_x_logical;
                self.pip_prev_y = self.window_y_logical;
                self.pip_prev_chrome_hidden = self.chrome_force_hidden;
                self.pip_prev_pinned = self.pinned;
                let panel_w = if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                self.active_panel = None;

                let mut tasks: Vec<Task<Message>> = Vec::new();
                if !self.chrome_force_hidden {
                    tasks.push(Task::done(Message::ToggleChrome));
                }
                if !self.pinned {
                    tasks.push(Task::done(Message::TogglePin));
                }

                // Shrink to the actual rendered video picture, not just the
                // video widget area - the area can be wider/taller than the
                // picture itself (letterbox bars) if the window's current
                // aspect doesn't match the video's, and PiP should never
                // carry that dead space along.
                let chrome_h = (CONTROLS_H + TOP_BAR_H) as f32;
                let area_w = ((self.window_w_logical - panel_w).max(1.0)) as u32;
                let area_h = ((self.window_h_logical - chrome_h).max(1.0)) as u32;
                let (render_w, render_h) = self.compute_render_size(area_w, area_h);
                let target_w = (render_w as f32).max(160.0);
                let target_h = (render_h as f32).max(90.0);
                let cur_x = self.window_x_logical;
                let cur_y = self.window_y_logical;
                const MARGIN_PHYSICAL: f32 = 20.0;
                let resize_and_move = iced::window::scale_factor(id).then(move |dpi| {
                    let margin_log = MARGIN_PHYSICAL / dpi;
                    #[cfg(target_os = "windows")]
                    let (_, _, wa_r, wa_b) = crate::win32_modal::work_area_near(cur_x, cur_y);
                    #[cfg(not(target_os = "windows"))]
                    let (wa_r, wa_b): (i32, i32) = (1920, 1080);
                    let x_log = wa_r as f32 / dpi - target_w - margin_log;
                    let y_log = wa_b as f32 / dpi - target_h - margin_log;
                    Task::batch([
                        iced::window::resize(id, iced::Size::new(target_w, target_h)),
                        iced::window::move_to(id, iced::Point::new(x_log, y_log)),
                    ])
                });
                tasks.push(resize_and_move);
                return Task::batch(tasks);
            }
            Message::TogglePrivateMode => {
                self.private_mode = !self.private_mode;
                self.resume_db.set_private(self.private_mode);
                self.recent_files.set_private(self.private_mode);
                let label = if self.private_mode {
                    "Private mode on - nothing will be remembered"
                } else {
                    "Private mode off"
                };
                return Task::done(Message::ShowOsd(label.into()));
            }
            Message::ToggleAudioNormalize => {
                self.audio_normalize = !self.audio_normalize;
                self.player.set_audio_normalize(self.audio_normalize);
                let mut prefs = crate::settings::Settings::load();
                prefs.audio.normalize = self.audio_normalize;
                prefs.save();
            }
            Message::ToggleAudioEq => {
                self.player.set_eq_enabled(!self.player.eq_enabled);
                let mut prefs = crate::settings::Settings::load();
                prefs.audio.eq_enabled = self.player.eq_enabled;
                prefs.save();
            }
            Message::EqBandSet(band, gain) => {
                self.player.set_eq_band(band, gain);
                let mut prefs = crate::settings::Settings::load();
                prefs.audio.eq_gains = self.player.eq_gains.clone();
                prefs.save();
            }
            Message::AudioEqReset => {
                self.player.reset_eq();
                let mut prefs = crate::settings::Settings::load();
                prefs.audio.eq_gains = self.player.eq_gains.clone();
                prefs.save();
            }
            Message::ToggleHideAllOnMinimize => {
                self.hide_all_on_minimize = !self.hide_all_on_minimize;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.hide_all_on_minimize = self.hide_all_on_minimize;
                prefs.save();
            }
            Message::TogglePauseOnFocusLost => {
                self.pause_on_focus_lost = !self.pause_on_focus_lost;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.pause_on_focus_lost = self.pause_on_focus_lost;
                prefs.save();
            }
            Message::TogglePauseOnMinimize => {
                self.pause_on_minimize = !self.pause_on_minimize;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.pause_on_minimize = self.pause_on_minimize;
                prefs.save();
            }
            Message::ToggleMinimizeToTray => {
                self.minimize_to_tray = !self.minimize_to_tray;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.minimize_to_tray = self.minimize_to_tray;
                prefs.save();
            }
            Message::ToggleAutoLoadSiblings => {
                self.auto_load_siblings = !self.auto_load_siblings;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.auto_load_siblings = self.auto_load_siblings;
                prefs.save();
            }
            Message::ToggleSingleInstance => {
                // Only the persisted preference changes here - the actual
                // mutex claim happens once at startup in main(), so this
                // takes effect on next launch, same as custom_title_bar.
                self.single_instance = !self.single_instance;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.single_instance = self.single_instance;
                prefs.save();
                return Task::done(Message::ShowOsd(
                    if self.single_instance { "Restart to enable single-instance mode".into() }
                    else { "Restart to allow multiple instances again".into() }
                ));
            }
            Message::RegisterFileAssociations => {
                #[cfg(target_os = "windows")]
                {
                    let ok = crate::win32_modal::register_file_associations();
                    crate::win32_modal::open_default_apps_settings();
                    return Task::done(Message::ShowOsd(
                        if ok { "Registered - pick MPV-NE in the Default Apps settings that just opened".into() }
                        else { "Couldn't register file associations".into() }
                    ));
                }
                #[cfg(not(target_os = "windows"))]
                {
                    return Task::done(Message::ShowOsd("Not supported on this platform yet".into()));
                }
            }
            Message::SetMouseBinding(trigger, preset_id) => {
                let field = match trigger {
                    MouseTrigger::SingleClick => &mut self.mouse_bindings.single_click,
                    MouseTrigger::DoubleClick => &mut self.mouse_bindings.double_click,
                    MouseTrigger::ScrollUp => &mut self.mouse_bindings.scroll_up,
                    MouseTrigger::ScrollDown => &mut self.mouse_bindings.scroll_down,
                };
                *field = preset_id.to_string();
                self.rebuild_bindings();
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.mouse_single_click = self.mouse_bindings.single_click.clone();
                prefs.interface.mouse_double_click = self.mouse_bindings.double_click.clone();
                prefs.interface.mouse_scroll_up = self.mouse_bindings.scroll_up.clone();
                prefs.interface.mouse_scroll_down = self.mouse_bindings.scroll_down.clone();
                prefs.save();
            }
            #[cfg(target_os = "windows")]
            Message::PollSingleInstance => {
                if let Some(path) = crate::win32_modal::take_pending_open_file() {
                    if !path.is_empty() {
                        self.load_path(std::path::PathBuf::from(path));
                    }
                }
            }
            #[cfg(not(target_os = "windows"))]
            Message::PollSingleInstance => {}
            #[cfg(target_os = "windows")]
            Message::PollThumbBar => {
                if let Some(action) = crate::win32_modal::take_pending_thumb_action() {
                    return Task::done(match action {
                        0 => Message::PrevFile,
                        2 => Message::NextFile,
                        _ => Message::TogglePause,
                    });
                }
            }
            #[cfg(not(target_os = "windows"))]
            Message::PollThumbBar => {}
            Message::MinimizeCheckTick => {
                #[cfg(target_os = "windows")]
                {
                    let now_minimized = crate::win32_modal::is_main_window_minimized();
                    if now_minimized && !self.main_window_was_minimized {
                        self.main_window_was_minimized = true;
                        if self.pause_on_minimize && !self.player.paused {
                            self.player.pause();
                        }
                        if self.minimize_to_tray {
                            crate::win32_modal::minimize_to_tray();
                        }
                        if self.hide_all_on_minimize {
                            let mut tasks = Vec::new();
                            if let Some(id) = self.panel_window_id {
                                tasks.push(iced::window::minimize(id, true));
                            }
                            if let Some(id) = self.app_settings_window_id {
                                tasks.push(iced::window::minimize(id, true));
                            }
                            return Task::batch(tasks);
                        }
                    } else if !now_minimized && self.main_window_was_minimized {
                        self.main_window_was_minimized = false;
                        if self.hide_all_on_minimize {
                            let mut tasks = Vec::new();
                            if let Some(id) = self.panel_window_id {
                                tasks.push(iced::window::minimize(id, false));
                            }
                            if let Some(id) = self.app_settings_window_id {
                                tasks.push(iced::window::minimize(id, false));
                            }
                            return Task::batch(tasks);
                        }
                    }
                }
            }
            Message::ToggleSnapToEdge => {
                self.snap_to_edge = !self.snap_to_edge;
                #[cfg(target_os = "windows")]
                crate::win32_modal::set_snap_enabled(self.snap_to_edge);
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.snap_to_edge = self.snap_to_edge;
                prefs.save();
            }
            Message::ToggleDragAnywhere => {
                self.bindings.drag_window_anywhere = !self.bindings.drag_window_anywhere;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.drag_anywhere = self.bindings.drag_window_anywhere;
                prefs.save();
            }
            Message::ToggleCustomTitleBar => {
                // Deliberately does NOT call set_custom_title_bar() - see
                // `custom_title_bar_pref`'s doc comment. Only the saved
                // preference changes; the live flag (and therefore every
                // window's decorations for the rest of this session) stays
                // put until restart.
                self.custom_title_bar_pref = !self.custom_title_bar_pref;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.custom_title_bar = self.custom_title_bar_pref;
                prefs.save();
                return Task::done(Message::ShowOsd("Restart MPV-NE to apply".into()));
            }
            Message::ToggleRememberWindow => {
                self.remember_window = !self.remember_window;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.remember_window = self.remember_window;
                prefs.save();
            }
            Message::ToggleStartPinned => {
                self.start_pinned_pref = !self.start_pinned_pref;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.start_pinned = self.start_pinned_pref;
                prefs.save();
            }
            Message::ToggleOsdEnabled => {
                self.osd_enabled = !self.osd_enabled;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.osd_enabled = self.osd_enabled;
                prefs.save();
            }
            Message::ToggleThumbnailPreview => {
                self.thumbnail_preview = !self.thumbnail_preview;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.thumbnail_preview = self.thumbnail_preview;
                prefs.save();
            }
            Message::ToggleAutoUpdateYtdlp => {
                self.auto_update_ytdlp = !self.auto_update_ytdlp;
                let mut prefs = crate::settings::Settings::load();
                prefs.interface.auto_update_ytdlp = self.auto_update_ytdlp;
                prefs.save();
            }
            Message::StartRebind(slot_id) => {
                self.rebind_capture = Some(slot_id);
            }
            Message::CancelRebind => {
                self.rebind_capture = None;
            }
            Message::ResetRebind(slot_id) => {
                self.keybinding_overrides.remove(slot_id);
                self.rebuild_bindings();
                self.persist_keybindings();
            }
            Message::ResetAllKeybindings => {
                self.keybinding_overrides.clear();
                self.rebuild_bindings();
                self.persist_keybindings();
            }
            Message::AudioLangInput(s) => {
                self.audio_lang = s;
                self.player.set_lang_priority(&self.audio_lang, &self.sub_lang);
                let mut prefs = crate::settings::Settings::load();
                prefs.audio.lang = self.audio_lang.clone();
                prefs.save();
            }
            Message::SubLangInput(s) => {
                self.sub_lang = s;
                self.player.set_lang_priority(&self.audio_lang, &self.sub_lang);
                let mut prefs = crate::settings::Settings::load();
                prefs.subtitles.lang = self.sub_lang.clone();
                prefs.save();
            }
            Message::TogglePreciseSeek => {
                self.precise_seek = !self.precise_seek;
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.precise_seek = self.precise_seek;
                prefs.save();
            }
            Message::SeekStepAdjust(delta) => {
                self.seek_step_secs = (self.seek_step_secs + delta).clamp(1.0, 60.0);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.seek_step_secs = self.seek_step_secs;
                prefs.save();
            }
            Message::SeekStepSet(secs) => {
                self.seek_step_secs = secs.clamp(1.0, 60.0);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.seek_step_secs = self.seek_step_secs;
                prefs.save();
            }
            Message::SpeedStepAdjust(delta) => {
                self.speed_step = (self.speed_step + delta).clamp(0.05, 1.0);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.speed_step = self.speed_step;
                prefs.save();
            }
            Message::SpeedStepSet(step) => {
                self.speed_step = step.clamp(0.05, 1.0);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.speed_step = self.speed_step;
                prefs.save();
            }
            Message::StreamQualitySet(height) => {
                self.stream_quality_height = height;
                self.player.set_stream_quality(height);
                let mut prefs = crate::settings::Settings::load();
                prefs.streaming.quality_height = height;
                prefs.save();
            }
            Message::ToggleChrome => {
                let was_visible = self.chrome_visible();
                self.chrome_force_hidden = !self.chrome_force_hidden;
                let now_visible = self.chrome_visible();
                // Visibility flipped - adjust pending_h to reclaim/reserve CONTROLS_H
                // so the video area matches the new layout immediately.
                if was_visible != now_visible {
                    let delta = CONTROLS_H + TOP_BAR_H;
                    if was_visible {
                        self.pending_h = (self.pending_h as i32 + delta).max(0) as u32;
                    } else {
                        self.pending_h = (self.pending_h as i32 - delta).max(0) as u32;
                    }
                    self.apply_render_size();
                }
            }
            Message::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                if let Some(id) = self.window_id {
                    let mut tasks = Vec::new();

                    if self.fullscreen {
                        // Save current window size (video-column width + height)
                        // before the OS/iced overwrites window_w/h_logical with
                        // the screen resolution.
                        let panel_w = if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                        self.pre_fullscreen_w = Some(self.window_w_logical - panel_w);
                        self.pre_fullscreen_h = Some(self.window_h_logical);
                        // Panel stays open - it shares space with the video
                        // inside the fullscreen window, no resize needed.
                        // AlwaysOnTop so we sit above the taskbar.
                        tasks.push(iced::window::set_mode(id, iced::window::Mode::Fullscreen));
                        tasks.push(iced::window::set_level(id, iced::window::Level::AlwaysOnTop));
                    } else {
                        // Restore saved size. Use the panel state as it is now
                        // (user may have opened/closed it while in fullscreen).
                        let saved_video_w = self.pre_fullscreen_w.take().unwrap_or(800.0);
                        let saved_h      = self.pre_fullscreen_h.take().unwrap_or(600.0);
                        let panel_w = if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                        let restore_w = (saved_video_w + panel_w).max(480.0);
                        let level = if self.pinned {
                            iced::window::Level::AlwaysOnTop
                        } else {
                            iced::window::Level::Normal
                        };
                        tasks.push(iced::window::set_mode(id, iced::window::Mode::Windowed));
                        tasks.push(iced::window::set_level(id, level));
                        tasks.push(iced::window::resize(id, iced::Size::new(restore_w, saved_h)));
                    }

                    return Task::batch(tasks);
                }
            }
            Message::SeekRelative(delta) => {
                self.live_catching_up = false;
                self.live_edge_paused = false;
                self.player.seek_relative(delta);
            }
            Message::SeekStep(forward) => {
                self.live_catching_up = false;
                self.live_edge_paused = false;
                let delta = if forward { self.seek_step_secs } else { -self.seek_step_secs };
                self.player.seek_relative(delta);
            }
            Message::FrameStep(forward) => {
                if forward {
                    self.player.frame_step();
                } else {
                    self.player.frame_back_step();
                }
            }
            Message::JumpToLive => {
                if self.player.duration < 1.0 {
                    tracing::warn!(
                        duration = self.player.duration,
                        "JumpToLive: duration unknown, nothing to do"
                    );
                    return Task::none();
                }
                // Phase 1: instant jump to 100% of what mpv has indexed so far.
                // For a file mpv fully knows, this lands at the live edge immediately.
                // Phase 2: DurationChanged-chase continues from here — as mpv reads
                // beyond the indexed portion it fires DurationChanged events, and we
                // keep seeking forward until the gap is < 8s. This handles the common
                // case where a growing file is longer than mpv's current index
                // (e.g. a 1.5h recording where only the first 11 min are indexed).
                self.live_catching_up = true;
                self.live_last_seek = self.player.duration;
                self.live_edge_paused = true;
                self.live_edge_stall_count = 0;
                self.live_edge_ref_duration = self.player.duration;
                tracing::info!(
                    pos = self.player.position,
                    dur = self.player.duration,
                    "JumpToLive: instant jump + chase"
                );
                self.player.seek_to_end();
                return Task::done(Message::ShowOsd("Catching up to live edge…".into()));
            }
            Message::VolumeAdjust(delta) => {
                self.player.set_volume(self.player.volume + delta);
                let mut prefs = crate::settings::Settings::load();
                prefs.playback.volume = self.player.volume;
                prefs.save();
                return Task::done(Message::ShowOsd(format!("Volume  {:.0}%", self.player.volume)));
            }
            Message::FileDropped(path) => return Task::done(Message::FilesDropped(vec![path])),

            Message::CursorMoved(id, x, y) => {
                if Some(id) == self.window_id {
                    self.cursor_pos = Some((x, y));
                    if let Some((start_x, start_y, start_pan_x, start_pan_y)) = self.pan_drag_start {
                        let w = (self.window_w_logical - if self.active_panel.is_some() { PANEL_W } else { 0.0 }).max(1.0);
                        let h = (self.window_h_logical - TOP_BAR_H as f32 - CONTROLS_H as f32).max(1.0);
                        let new_x = (start_pan_x + ((x - start_x) / w) as f64).clamp(-1.5, 1.5);
                        let new_y = (start_pan_y + ((y - start_y) / h) as f64).clamp(-1.5, 1.5);
                        self.player.set_video_pan(new_x, new_y);
                        self.player.video_pan_x = new_x;
                        self.player.video_pan_y = new_y;
                    }
                } else if Some(id) == self.panel_window_id {
                    self.panel_cursor_pos = Some((x, y));
                } else if Some(id) == self.app_settings_window_id {
                    self.app_settings_cursor_pos = Some((x, y));
                }
            }
            Message::CursorLeft(id) => {
                if Some(id) == self.window_id {
                    self.cursor_pos = None;
                } else if Some(id) == self.panel_window_id {
                    self.panel_cursor_pos = None;
                } else if Some(id) == self.app_settings_window_id {
                    self.app_settings_cursor_pos = None;
                }
            }
            Message::InputMouseUp(_id, button) => {
                if button == iced::mouse::Button::Left {
                    self.pan_drag_start = None;
                }
            }
            Message::WindowUnfocused(id) => {
                if Some(id) == self.menu_window_id {
                    return self.close_main_menu();
                }
                if Some(id) == self.window_id && self.pause_on_focus_lost && !self.player.paused {
                    self.player.pause();
                }
            }
            Message::WindowRescaled(id, factor) => {
                // Kept for informational tracking only - the startup size
                // correction is handled directly in boot()'s task chain via
                // a live window::scale_factor() query (the pattern proven
                // correct by the Fit menu), not this event, which wasn't
                // reliable enough to trust for that.
                if Some(id) == self.window_id {
                    self.scale_factor = factor;
                }
            }
            Message::InputKey(name) => {
                // A rebind is in progress - this key press is the new
                // binding (or, for Escape, a cancel), not a normal action
                // trigger. Consume it here so it can't also fire whatever
                // it used to be bound to (e.g. rebinding "m" wouldn't also
                // mute while this is happening).
                if let Some(slot_id) = self.rebind_capture.take() {
                    if name != "escape" {
                        self.apply_key_rebind(slot_id, name);
                    }
                    return Task::none();
                }
                // Escape closes the modal first.
                if name == "escape" && self.modal.is_some() {
                    self.modal = None;
                    return Task::none();
                }
                // Escape closes other popups.
                if name == "escape"
                    && (self.subs_menu_open || self.fit_menu_open
                        || self.audio_menu_open
                        || self.active_panel.is_some())
                {
                    self.subs_menu_open = false;
                    self.fit_menu_open = false;
                    self.audio_menu_open = false;
                    if self.active_panel.is_some() {
                        return self.toggle_panel(None);
                    }
                    return Task::none();
                }
                // ? opens help; Escape also closes it.
                if name == "?" || name == "/" {
                    self.show_help = !self.show_help;
                    return Task::none();
                }
                if name == "escape" && self.show_help {
                    self.show_help = false;
                    return Task::none();
                }
                if name == "escape" && self.file_context_menu.is_some() {
                    self.file_context_menu = None;
                    return Task::none();
                }
                if name == "escape" && self.modal_paste_menu.is_some() {
                    self.modal_paste_menu = None;
                    return Task::none();
                }
                if name == "escape" && self.menu_window_id.is_some() {
                    return self.close_main_menu();
                }
                if name == "escape" && self.sub_search_open {
                    self.sub_search_open = false;
                    return Task::none();
                }
                // Escape exits fullscreen or un-hides chrome - never enters.
                if name == "escape" {
                    if self.fullscreen {
                        return Task::done(Message::ToggleFullscreen);
                    }
                    if self.chrome_force_hidden {
                        return Task::done(Message::ToggleChrome);
                    }
                    return Task::none();
                }
                if name == "end" {
                    return Task::done(Message::JumpToLive);
                }
                if let Some(action) = self.bindings.keys.get(&name).copied() {
                    return Task::done(action_to_message(action));
                }
            }
            Message::ModifiersChanged(m) => self.keyboard_modifiers = m,

            Message::InputScroll(event_window, dy) => {
                // Only the main window's video/controls area drives
                // volume/seek via scroll - the detached panel and App
                // Settings windows never should, even when the scroll
                // lands on non-scrollable empty space in them.
                if Some(event_window) != self.window_id {
                    return Task::none();
                }
                let over_panel = self.active_panel.is_some()
                    && self.cursor_pos
                        .map(|(x, _)| x > self.window_w_logical - 280.0)
                        .unwrap_or(false);
                if over_panel {
                    return Task::none();
                }
                // Ctrl+scroll = seek ±5s per notch.
                if self.keyboard_modifiers.control() {
                    let secs = if dy > 0.0 { 5.0 } else { -5.0 };
                    self.player.seek_relative(secs);
                    return Task::none();
                }
                let action = if dy > 0.0 {
                    self.bindings.scroll_up
                } else if dy < 0.0 {
                    self.bindings.scroll_down
                } else {
                    None
                };
                if let Some(action) = action {
                    return Task::done(action_to_message(action));
                }
            }
            Message::InputMouseDown(event_window, button, captured) => {
                if button == iced::mouse::Button::Left {
                    // Edge-grip resize fires regardless of captured state.
                    // The panel window has no other click behavior to
                    // preserve, so check it first and bail out early.
                    if Some(event_window) == self.panel_window_id {
                        if let Some(direction) = self.panel_cursor_edge_direction() {
                            return iced::window::drag_resize(event_window, direction);
                        }
                        return Task::none();
                    }
                    if Some(event_window) == self.app_settings_window_id {
                        if let Some(direction) = self.app_settings_cursor_edge_direction() {
                            return iced::window::drag_resize(event_window, direction);
                        }
                        return Task::none();
                    }
                    if let Some(direction) = self.cursor_edge_direction() {
                        if let Some(id) = self.window_id {
                            return iced::window::drag_resize(id, direction);
                        }
                    }
                    // All other actions only fire for uncaptured clicks.
                    if !captured {
                        let now = Instant::now();
                        let is_double = self
                            .last_left_press
                            .map(|t| now.duration_since(t) < Duration::from_millis(400))
                            .unwrap_or(false);
                        self.last_left_press = Some(now);

                        if is_double {
                            // Double-click in top bar = maximize/restore (like Windows title bar).
                            let in_top_bar = self.cursor_pos
                                .map(|(_, y)| y <= TOP_BAR_H as f32)
                                .unwrap_or(false);
                            if in_top_bar && !self.fullscreen {
                                return Task::done(Message::ToggleMaximize);
                            }
                            if let Some(action) = self.bindings.double_left_click {
                                return Task::done(action_to_message(action));
                            }
                        }
                        // Zoomed in: click-drag over the video pans it instead
                        // of moving the window (takes priority over
                        // drag_window_anywhere, and works regardless of
                        // whether that setting is on).
                        if self.player.video_zoom != 0.0 {
                            let over_video = self.cursor_pos.map(|(x, y)| {
                                let controls_top = self.window_h_logical - CONTROLS_H as f32;
                                let panel_left = self.window_w_logical
                                    - if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                                y > TOP_BAR_H as f32 && y < controls_top && x < panel_left
                            }).unwrap_or(false);
                            if over_video {
                                if let Some((x, y)) = self.cursor_pos {
                                    self.pan_drag_start = Some((x, y, self.player.video_pan_x, self.player.video_pan_y));
                                }
                                return Task::none();
                            }
                        }
                        let in_drag_zone = if self.bindings.drag_window_anywhere && !self.fullscreen {
                            if let Some((_, y)) = self.cursor_pos {
                                let controls_top = self.window_h_logical - CONTROLS_H as f32;
                                let panel_left = self.window_w_logical
                                    - if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                                y < controls_top && self.cursor_pos.map(|(x, _)| x < panel_left).unwrap_or(true)
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if in_drag_zone {
                            if let Some(id) = self.window_id {
                                return iced::window::drag(id);
                            }
                        }
                        if let Some(action) = self.bindings.single_left_click {
                            return Task::done(action_to_message(action));
                        }
                    }
                } else if button == iced::mouse::Button::Right && !captured {
                    // Only over the video area itself - not the top bar,
                    // controls bar, or a docked panel.
                    let over_video = self.cursor_pos.map(|(x, y)| {
                        let controls_top = self.window_h_logical - CONTROLS_H as f32;
                        let panel_left = self.window_w_logical
                            - if self.active_panel.is_some() { PANEL_W } else { 0.0 };
                        y > TOP_BAR_H as f32 && y < controls_top && x < panel_left
                    }).unwrap_or(false);
                    if over_video {
                        return Task::done(Message::ShowVideoContextMenu);
                    }
                }
            }

            Message::NextFile => {
                if self.playlist_idx + 1 < self.playlist.len() {
                    self.playlist_idx += 1;
                    let p = self.playlist[self.playlist_idx].clone();
                    self.open_next(p.to_string_lossy().into_owned());
                }
            }
            Message::PrevFile => {
                if self.playlist_idx > 0 {
                    self.playlist_idx -= 1;
                    let p = self.playlist[self.playlist_idx].clone();
                    self.open_next(p.to_string_lossy().into_owned());
                }
            }
        }
        Task::none()
    }

    pub fn view(&self, window: iced::window::Id) -> Element<'_, Message> {
        if Some(window) == self.menu_window_id {
            ui::menu_window_view(self)
        } else if Some(window) == self.panel_window_id {
            ui::panel_window_view(self)
        } else if Some(window) == self.app_settings_window_id {
            ui::app_settings::view(self)
        } else {
            ui::view(self)
        }
    }

    /// Open the floating main-menu popup at the given *screen* coordinates
    /// (not window-relative - the popup is a separate OS window). Height is
    /// estimated from the current (collapsed-by-default) row list via
    /// `ui::menu_window_height`, and the position is clamped to the nearest
    /// monitor's work area so the popup never extends off-screen.
    fn open_main_menu(&mut self, screen_x: f32, screen_y: f32) -> Task<Message> {
        self.menu_section_open = [false, false];
        self.menu_anchor = Some((screen_x, screen_y));
        let height = ui::menu_window_height(self);
        let Some(main_id) = self.window_id else { return Task::none() };
        iced::window::scale_factor(main_id)
            .map(move |dpi| Message::OpenMenuPopup(screen_x, screen_y, height, dpi))
    }

    /// Close the floating main-menu popup, if open. Safe to call when it's
    /// already closed.
    fn close_main_menu(&mut self) -> Task<Message> {
        self.menu_anchor = None;
        if let Some(id) = self.menu_window_id.take() {
            iced::window::close(id)
        } else {
            Task::none()
        }
    }

    /// Clamp a popup top-left position + size so it stays fully within the
    /// work area of the monitor nearest `(x, y)`, shifting up/left rather
    /// than letting the far edge run off-screen. `(x, y)`, `w`/`h`, and the
    /// returned point are logical coordinates (matching `window_x_logical`
    /// and `iced::window::Position::Specific`); `work_area_near` returns
    /// physical pixels, so its bounds are converted via `dpi` before
    /// comparing - see `Message::TogglePip` for the same conversion.
    fn clamp_menu_pos(&self, x: f32, y: f32, w: f32, h: f32, dpi: f32) -> (f32, f32) {
        #[cfg(target_os = "windows")]
        let (wa_l, wa_t, wa_r, wa_b) = {
            let (l, t, r, b) = crate::win32_modal::work_area_near((x * dpi) as i32, (y * dpi) as i32);
            (l as f32 / dpi, t as f32 / dpi, r as f32 / dpi, b as f32 / dpi)
        };
        #[cfg(not(target_os = "windows"))]
        let (wa_l, wa_t, wa_r, wa_b): (f32, f32, f32, f32) = (0.0, 0.0, 1920.0, 1080.0);

        let mut nx = x;
        let mut ny = y;
        if ny + h > wa_b { ny = wa_b - h; }
        if nx + w > wa_r { nx = wa_r - w; }
        if ny < wa_t { ny = wa_t; }
        if nx < wa_l { nx = wa_l; }
        (nx, ny)
    }

    /// The key currently bound to `slot_id`, or `None` if that slot was
    /// explicitly cleared. Used by the Keyboard settings page to display the
    /// current binding without duplicating `Bindings::from_overrides`' logic.
    pub fn resolved_key_for_slot(&self, slot_id: &str) -> Option<String> {
        let (_, _, default_key, _) = KEY_SLOTS.iter().find(|(id, ..)| *id == slot_id)?;
        match self.keybinding_overrides.get(slot_id) {
            Some(k) if k.is_empty() => None,
            Some(k) => Some(k.clone()),
            None => Some((*default_key).to_string()),
        }
    }

    /// Rebind `slot_id` to `key`, stealing the key from whichever other slot
    /// currently holds it (if any) since two actions can't share one
    /// physical key - the other slot becomes explicitly unbound rather than
    /// silently falling back to its default and colliding again.
    fn apply_key_rebind(&mut self, slot_id: &'static str, key: String) {
        for (other_id, _, default_key, _) in KEY_SLOTS {
            if *other_id == slot_id {
                continue;
            }
            let other_key = self.keybinding_overrides.get(*other_id)
                .map(|s| s.as_str())
                .unwrap_or(default_key);
            if other_key == key {
                self.keybinding_overrides.insert(other_id.to_string(), String::new());
            }
        }
        self.keybinding_overrides.insert(slot_id.to_string(), key);
        self.rebuild_bindings();
        self.persist_keybindings();
    }

    fn rebuild_bindings(&mut self) {
        let drag_window_anywhere = self.bindings.drag_window_anywhere;
        self.bindings = Bindings {
            drag_window_anywhere,
            ..Bindings::from_overrides(&self.keybinding_overrides, &self.mouse_bindings)
        };
    }

    fn persist_keybindings(&self) {
        let mut prefs = crate::settings::Settings::load();
        prefs.keybindings = self.keybinding_overrides.clone();
        prefs.save();
    }

    /// True when the controls bar should be drawn. Hidden in fullscreen or
    /// when the user has manually toggled chrome off.
    pub fn chrome_visible(&self) -> bool {
        !self.fullscreen && !self.chrome_force_hidden
    }

    /// In hidden mode (fullscreen / force-hidden), reveal the controls when
    /// the cursor is in the bottom band of the window. The video stays at full
    /// size and the bar is drawn as an overlay so playback doesn't reflow.
    /// Also stays up while the subtitle picker popup is open, otherwise the
    /// popup would vanish the instant the cursor leaves the chrome zone.
    pub fn chrome_overlay_visible(&self) -> bool {
        if self.chrome_visible() {
            return false; // covered by the normal layout
        }
        if self.subs_menu_open || self.fit_menu_open || self.audio_menu_open || self.active_panel.is_some() {
            return true;
        }
        let Some((_, y)) = self.cursor_pos else {
            return false;
        };
        const HOVER_ZONE: f32 = 120.0;
        y >= self.window_h_logical - HOVER_ZONE
    }

    /// When the OS chrome is replaced by our custom title bar, edges of the
    /// window no longer have OS-drawn resize handles. Detect cursor near an
    /// edge so we can trigger `iced::window::drag_resize` manually.
    ///
    /// Corner grip is larger than edge grip so diagonal resize is easy to
    /// land. Both numbers are tuned to be roughly equivalent to native
    /// Windows resize zones (a couple of logical pixels outside the visible
    /// border + a few inside).
    pub fn cursor_edge_direction(&self) -> Option<iced::window::Direction> {
        if !use_custom_title_bar() || self.fullscreen {
            return None;
        }
        edge_direction_for(self.cursor_pos?, self.window_w_logical, self.window_h_logical)
    }

    /// Same idea as `cursor_edge_direction` but for the detached panel
    /// window - it also has no OS-drawn resize handles (custom chrome), so
    /// it needs the same manual edge detection to be resizable at all.
    pub fn panel_cursor_edge_direction(&self) -> Option<iced::window::Direction> {
        if !use_custom_title_bar() {
            return None;
        }
        let (w, h) = self.panel_last_size.unwrap_or((PANEL_W, 640.0));
        edge_direction_for(self.panel_cursor_pos?, w, h)
    }

    /// Same idea, for the standalone App Settings window.
    pub fn app_settings_cursor_edge_direction(&self) -> Option<iced::window::Direction> {
        if !use_custom_title_bar() {
            return None;
        }
        let (w, h) = self.app_settings_last_size.unwrap_or((760.0, 520.0));
        edge_direction_for(self.app_settings_cursor_pos?, w, h)
    }

    /// Same idea for the top bar - cursor in the top band reveals it as an
    /// overlay when chrome is otherwise hidden.

    /// Pick a render size that matches the video's native aspect ratio and
    /// fits inside `widget_w` × `widget_h`. With this, mpv's output texture
    /// never contains internal letterbox bars - our shader provides the only
    /// letterbox, so the picture stays put through resizes.
    fn compute_render_size(&self, widget_w: u32, widget_h: u32) -> (u32, u32) {
        if widget_w == 0 || widget_h == 0 {
            return (widget_w, widget_h);
        }
        if self.player.width <= 0 || self.player.height <= 0 {
            // Video dimensions unknown yet - render at widget size; mpv will
            // letterbox internally for one or two frames until the size update
            // arrives below.
            return (widget_w, widget_h);
        }
        let va = self.player.width as f32 / self.player.height as f32;
        let wa = widget_w as f32 / widget_h as f32;
        if wa > va {
            let h = widget_h;
            let w = (h as f32 * va).round() as u32;
            (w.max(1), h.max(1))
        } else {
            let w = widget_w;
            let h = (w as f32 / va).round() as u32;
            (w.max(1), h.max(1))
        }
    }

    /// Navigate the file browser to `target` (None = drives list), pushing
    /// the current location onto the back stack first so BrowserBack can
    /// return to it. The single entry point every browser navigation
    /// (Navigate, NavigateUp, GoToDrives, Open) goes through, so history
    /// stays consistent no matter which one triggered the move.
    fn browser_go(&mut self, target: Option<std::path::PathBuf>) {
        if target == self.browser_path {
            return; // no-op navigation - don't pollute history with it
        }
        self.browser_back_stack.push(self.browser_path.clone());
        const MAX_BACK_STACK: usize = 50;
        if self.browser_back_stack.len() > MAX_BACK_STACK {
            self.browser_back_stack.remove(0);
        }
        // A fresh navigation (not Back/Forward itself) invalidates whatever
        // was available to go forward to - same as a normal desktop browser.
        self.browser_forward_stack.clear();
        self.browser_entries = match &target {
            Some(p) => browser_read_dir(p),
            None => browser_drives(),
        };
        self.browser_path = target;
    }

    /// Toggle, switch, or close the docked side panel. Returns a resize Task
    /// only when the panel transitions between open and closed (not when
    /// switching between panel kinds, which keeps the same width).
    /// Grow (`grow = true`) or shrink the main window's width by `PANEL_W`,
    /// same rule `toggle_panel` uses when a docked panel opens/closes: skip
    /// it in fullscreen/maximized (nothing to reclaim/make room for there).
    /// Shared by `DetachPanel`/`ReattachPanel` so popping the panel in/out
    /// of its own window resizes the main window the same way toggling it
    /// docked would.
    fn resize_main_for_panel(&self, grow: bool) -> Task<Message> {
        let maximized = {
            #[cfg(target_os = "windows")]
            { win32_is_maximized(self.window_id) }
            #[cfg(not(target_os = "windows"))]
            { false }
        };
        if self.fullscreen || maximized {
            return Task::none();
        }
        let Some(id) = self.window_id else { return Task::none() };
        let delta: f32 = if grow { PANEL_W } else { -PANEL_W };
        let new_size = iced::Size::new(
            (self.window_w_logical + delta).max(480.0),
            self.window_h_logical,
        );
        iced::window::resize(id, new_size)
    }

    fn toggle_panel(&mut self, kind: Option<PanelKind>) -> Task<Message> {
        let was_open = self.active_panel.is_some();

        match kind {
            Some(k) if self.active_panel == Some(k) => {
                // Same panel button pressed again - close it.
                self.active_panel = None;
            }
            Some(k) => {
                // Open a different panel (or open from closed).
                if k == PanelKind::Browser && self.browser_entries.is_empty() {
                    self.browser_entries = browser_drives();
                }
                self.active_panel = Some(k);
                self.last_panel = k; // remember for the next quick toggle
            }
            None => {
                self.active_panel = None;
            }
        }

        let is_open = self.active_panel.is_some();

        // Build probe task for newly opened panels.
        let probe_task: Task<Message> = if is_open && !was_open {
            if let Some(k) = self.active_panel {
                let paths: Vec<std::path::PathBuf> = match k {
                    PanelKind::Playlist => self.playlist.clone(),
                    PanelKind::Recent   => self.recent_files.paths.clone(),
                    PanelKind::Browser  => self.browser_entries.iter()
                        .filter(|e| !e.is_dir).map(|e| e.path.clone()).collect(),
                    _ => vec![],
                };
                if !paths.is_empty() {
                    Task::done(Message::ProbeFiles(paths))
                } else { Task::none() }
            } else { Task::none() }
        } else { Task::none() };

        if was_open != is_open {
            // Detached: the panel isn't occupying any of the main window's
            // width, so resizing it here would just be a spurious stretch -
            // and the "open/close" button pressed from the main window's
            // chrome has no docked target to affect. Treat it as closing
            // the floating window instead (the button-name "open/close"
            // implies fully dismissing it, same as the panel's own close).
            if let Some(id) = self.panel_window_id {
                if !is_open {
                    self.panel_window_id = None;
                    return Task::batch([iced::window::close(id), probe_task]);
                }
                return probe_task;
            }

            let resize_task = self.resize_main_for_panel(is_open);
            return Task::batch([resize_task, probe_task]);
        }
        probe_task
    }

    /// Open a file and scan its folder so Prev/Next can navigate sibling media.
    /// Save resume position + resolution for the currently playing file.
    fn save_resume(&mut self) {
        if let Some(path) = self.player.path.clone() {
            self.resume_db.record(&path, self.player.position, self.player.duration);
            if self.player.width > 0 && self.player.height > 0 {
                self.resume_db.record_resolution(
                    &path,
                    self.player.width as u32,
                    self.player.height as u32,
                );
            }
            self.resume_db.record_audio_track(&path, self.player.current_aid);
            self.resume_db.record_sub_track(&path, self.player.current_sid);
            self.resume_db.record_volume(&path, self.player.volume);
            self.resume_db.save();
        }
    }

    fn load_path(&mut self, path: std::path::PathBuf) {
        self.recent_files.record(&path);
        self.recent_files.save();
        let (list, idx) = if self.auto_load_siblings {
            build_folder_playlist(&path)
        } else {
            (vec![path.clone()], 0)
        };
        self.playlist = list;
        self.playlist_idx = idx;
        self.open_next(path.to_string_lossy().into_owned());
    }

    /// Save resume for whatever is currently playing, mark a transition in
    /// progress, then tell mpv to open the new path. The EndFile that mpv
    /// fires for the outgoing file will be ignored (transitioning = true) so
    /// it cannot clear the new path or paused state we are about to set.
    fn open_next(&mut self, path: String) {
        self.save_resume();
        self.transitioning = true;
        let pb = std::path::PathBuf::from(&path);
        self.recent_files.record(&pb);
        self.recent_files.save();
        #[cfg(target_os = "windows")]
        {
            let name = pb.file_name().and_then(|n| n.to_str()).unwrap_or(&path).to_string();
            crate::win32_modal::update_smtc_metadata(&name);
        }
        self.player.open(path);
    }

    /// Shrink the window to match the video area that is *currently visible*
    /// after letterboxing. If the window is wider than the video aspect, this
    /// trims the pillarbox bars; if taller, trims the letterbox bars.
    pub fn fit_to_visible_size(&self) -> Option<iced::Size> {
        if self.player.width <= 0 || self.player.height <= 0 {
            return None;
        }
        if self.pending_w == 0 || self.pending_h == 0 {
            return None;
        }
        // pending_w/h is the video widget area (excludes panel if open).
        let va = self.player.width as f32 / self.player.height as f32;
        let wa = self.pending_w as f32 / self.pending_h as f32;
        let (vis_w, vis_h) = if wa > va {
            let h = self.pending_h as f32;
            (h * va, h)
        } else {
            let w = self.pending_w as f32;
            (w, w / va)
        };
        let chrome_h = if self.chrome_visible() {
            (CONTROLS_H + TOP_BAR_H) as f32
        } else {
            0.0
        };
        // If a panel is open, keep it alongside the video at its current width.
        let panel_w = if self.active_panel.is_some() { PANEL_W } else { 0.0 };
        Some(iced::Size::new(vis_w + panel_w, vis_h + chrome_h))
    }

    fn apply_render_size(&mut self) {
        let (w, h) = self.compute_render_size(self.pending_w, self.pending_h);
        // Render the actual mpv texture at PHYSICAL pixel resolution so
        // video stays crisp on HiDPI displays. compute_render_size's
        // aspect-fit math works in iced's logical pixels (widget_w/h come
        // from pending_w/h, which are logical) - without this, mpv would
        // render at a lower resolution than the screen actually shows, and
        // the GPU would stretch/blur it to fill the real physical viewport.
        // self.scale_factor is kept fresh via ResizeDpiQueried's live query,
        // not the passive Rescaled event (see its doc comment for why).
        let pw = ((w as f32) * self.scale_factor).round().max(1.0) as u32;
        let ph = ((h as f32) * self.scale_factor).round().max(1.0) as u32;
        self.player.set_render_size(pw, ph);
    }

    #[cfg(target_os = "windows")]
    fn install_modal_hook_once(&mut self) {
        if self.modal_hook_installed {
            return;
        }
        self.modal_hook_installed = true;

        let saved = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let h_enter = self.player.handle_arc();
        let s_enter = Arc::clone(&saved);
        let on_enter = move || {
            s_enter.store(h_enter.is_paused(), std::sync::atomic::Ordering::SeqCst);
            h_enter.set_pause(true);
        };

        let h_exit = self.player.handle_arc();
        let s_exit = Arc::clone(&saved);
        let on_exit = move || {
            h_exit.set_pause(s_exit.load(std::sync::atomic::Ordering::SeqCst));
        };

        crate::win32_modal::install(on_enter, on_exit);
        crate::win32_modal::install_tray();
        crate::win32_modal::install_thumbbar();
        crate::win32_modal::install_smtc();
    }

    pub fn theme(&self, _window: iced::window::Id) -> Theme {
        Theme::Nord
    }

    /// Daemon API wrapper - the window id is unused since the title never
    /// varies per-window. `title_str` below is the real logic, callable
    /// directly from UI code that doesn't have (and doesn't need) an id.
    pub fn title(&self, window: iced::window::Id) -> String {
        if Some(window) == self.app_settings_window_id {
            "MPV-NE - Settings".to_string()
        } else if Some(window) == self.panel_window_id {
            "MPV-NE - Panel".to_string()
        } else {
            self.title_str()
        }
    }

    pub fn title_str(&self) -> String {
        let prefix = if self.private_mode { "[Private] " } else { "" };
        if let Some(path) = &self.player.path {
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path.as_str());
            format!("{prefix}MPV-NE | {}", name)
        } else {
            format!("{prefix}MPV-NE")
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs: Vec<Subscription<Message>> = vec![
            Subscription::run_with(self.player.stream_key(), mpv_stream),
            iced::event::listen_with(on_window_event),
        ];
        // While paused at the live edge, mpv's demuxer stops firing DurationChanged.
        // Poll every 2 seconds and poke play() so mpv resumes automatically once
        // new content is available.
        if self.live_edge_paused {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(2))
                    .map(|_| Message::LiveEdgeTick),
            );
        }
        // Poll file size every 3s while a file is loaded so we can extrapolate
        // the true duration of a growing recording without mpv indexing it all.
        if self.player.path.is_some() && !self.stopped {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(3))
                    .map(|_| Message::FileSizeTick),
            );
        }
        // Refresh the stats overlay ~2x/sec while it's visible.
        if self.show_stats && !self.stopped {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(500))
                    .map(|_| Message::StatsTick),
            );
        }
        // Poll for the main window minimize/restore transition - no direct
        // iced event for it (see `win32_modal::is_main_window_minimized`).
        // Only runs when a preference that cares about it is on.
        #[cfg(target_os = "windows")]
        if self.hide_all_on_minimize || self.pause_on_minimize || self.minimize_to_tray {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(400))
                    .map(|_| Message::MinimizeCheckTick),
            );
        }
        // Poll for a file forwarded from a newer launch (single-instance
        // mode) - only runs when that mode is actually on.
        #[cfg(target_os = "windows")]
        if self.single_instance {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(500))
                    .map(|_| Message::PollSingleInstance),
            );
        }
        // Poll for taskbar thumbnail button clicks - always on (Windows only,
        // negligible cost, and the buttons exist regardless of other settings).
        #[cfg(target_os = "windows")]
        subs.push(
            iced::time::every(std::time::Duration::from_millis(150))
                .map(|_| Message::PollThumbBar),
        );
        Subscription::batch(subs)
    }
}

/// Returns true when the iced application window is currently maximized.
/// Uses `IsZoomed` from Win32 - no extra crate needed.
#[cfg(target_os = "windows")]
/// Shared edge-detection math for `MpvNe::cursor_edge_direction`/
/// `panel_cursor_edge_direction` - which window it's for doesn't matter,
/// just its cursor position and current size. Corner grip is larger than
/// edge grip so diagonal resize is easy to land; both are tuned to be
/// roughly equivalent to native Windows resize zones.
fn edge_direction_for(cursor: (f32, f32), w: f32, h: f32) -> Option<iced::window::Direction> {
    const EDGE_GRIP: f32 = 10.0;
    const CORNER_GRIP: f32 = 16.0;
    let (x, y) = cursor;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    // Corners first - they win whenever both nearby axes are in range.
    let near_l_c = x <= CORNER_GRIP;
    let near_r_c = x >= w - CORNER_GRIP;
    let near_t_c = y <= CORNER_GRIP;
    let near_b_c = y >= h - CORNER_GRIP;
    use iced::window::Direction::*;
    if near_l_c && near_t_c {
        return Some(NorthWest);
    }
    if near_r_c && near_t_c {
        return Some(NorthEast);
    }
    if near_l_c && near_b_c {
        return Some(SouthWest);
    }
    if near_r_c && near_b_c {
        return Some(SouthEast);
    }

    // Otherwise: straight edges.
    if x <= EDGE_GRIP {
        return Some(West);
    }
    if x >= w - EDGE_GRIP {
        return Some(East);
    }
    if y <= EDGE_GRIP {
        return Some(North);
    }
    if y >= h - EDGE_GRIP {
        return Some(South);
    }
    None
}

/// Heuristic for "this URL needs an extractor (yt-dlp/youtube-dl) to become
/// a playable stream" - a direct media URL (.m3u8, .mp4, rtsp://, etc.)
/// doesn't, so we don't want to warn about yt-dlp being missing for those.
/// Not exhaustive (yt-dlp supports 1000+ sites), just the common cases
/// people actually paste in here.
fn needs_ytdl(url: &str) -> bool {
    const EXTRACTOR_HOSTS: &[&str] = &[
        "youtube.com", "youtu.be", "twitch.tv", "vimeo.com",
        "dailymotion.com", "twitter.com", "x.com", "tiktok.com",
        "facebook.com", "soundcloud.com", "reddit.com",
    ];
    let lower = url.to_ascii_lowercase();
    EXTRACTOR_HOSTS.iter().any(|h| lower.contains(h))
}

/// Whether yt-dlp or (as a fallback) youtube-dl is reachable on PATH -
/// mpv's ytdl_hook script needs one of them to resolve extractor sites.
fn ytdl_available() -> bool {
    let check = |bin: &str| {
        #[allow(unused_mut)]
        let mut cmd = std::process::Command::new(bin);
        cmd.arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // In a release build (windows_subsystem="windows"), spawning a
        // console binary would otherwise briefly flash an empty console
        // window on screen.
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.status().is_ok_and(|s| s.success())
    };
    check("yt-dlp") || check("youtube-dl") || ytdl_local_path().is_some_and(|p| p.exists())
}

/// Where we keep an auto-downloaded yt-dlp binary (see `download_ytdlp`),
/// separate from resume.json/settings.toml but in the same app data dir.
fn ytdl_local_path() -> Option<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "mpv-ne")?;
    let name = if cfg!(target_os = "windows") { "yt-dlp.exe" } else { "yt-dlp" };
    Some(dirs.data_dir().join(name))
}

/// Title/duration/uploader probed for a playlist URL entry via yt-dlp's
/// metadata-only mode - see `fetch_url_metadata`.
#[derive(Debug, Clone)]
pub struct UrlMeta {
    pub title: String,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
}

/// Resolve which yt-dlp/youtube-dl binary to invoke, preferring PATH over
/// the auto-downloaded local copy - same search order as `ytdl_available`.
fn ytdl_binary() -> Option<String> {
    let on_path = |bin: &str| {
        #[allow(unused_mut)]
        let mut cmd = std::process::Command::new(bin);
        cmd.arg("--version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.status().is_ok_and(|s| s.success())
    };
    if on_path("yt-dlp") { return Some("yt-dlp".into()); }
    if on_path("youtube-dl") { return Some("youtube-dl".into()); }
    ytdl_local_path().filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned())
}

/// Probe a URL for a display title (and duration/uploader if available) via
/// yt-dlp's metadata-only mode (`-j --skip-download`) - nothing is
/// downloaded or played, just yt-dlp's own extractor info. Returns `None`
/// silently if yt-dlp isn't available or the URL can't be probed - the
/// playlist just keeps showing the raw URL in that case, same as before
/// this existed.
async fn fetch_url_metadata(url: String) -> (String, Option<UrlMeta>) {
    let Some(bin) = ytdl_binary() else { return (url, None) };
    let url_for_cmd = url.clone();
    let output = tokio::task::spawn_blocking(move || {
        #[allow(unused_mut)]
        let mut cmd = std::process::Command::new(&bin);
        cmd.args(["-j", "--no-warnings", "--skip-download", "--no-playlist", &url_for_cmd]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.output()
    }).await;

    let Ok(Ok(output)) = output else { return (url, None) };
    if !output.status.success() {
        return (url, None);
    }
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return (url, None);
    };
    let Some(title) = json.get("title").and_then(|v| v.as_str()).map(str::to_string) else {
        return (url, None);
    };
    let duration = json.get("duration").and_then(|v| v.as_f64());
    let uploader = json.get("uploader").and_then(|v| v.as_str()).map(str::to_string);
    (url, Some(UrlMeta { title, duration, uploader }))
}

/// Download yt-dlp's latest release into `ytdl_local_path()`. yt-dlp is
/// public domain (The Unlicense), so bundling/auto-fetching it has no
/// licensing concerns.
async fn download_ytdlp() -> Result<String, String> {
    let Some(dest) = ytdl_local_path() else {
        return Err("Couldn't determine where to save yt-dlp".into());
    };
    let asset = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp"
    };
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset}");

    let resp = reqwest::get(&url).await.map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("Download failed: {e}"))?;

    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&dest, &bytes).map_err(|e| format!("Couldn't save yt-dlp: {e}"))?;

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o111); // +x for owner/group/other
            let _ = std::fs::set_permissions(&dest, perms);
        }
    }

    Ok(dest.to_string_lossy().into_owned())
}

fn win32_is_maximized(window_id: Option<iced::window::Id>) -> bool {
    let _ = window_id; // kept for future use
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
        fn IsZoomed(hwnd: *mut std::ffi::c_void) -> i32;
    }
    unsafe { IsZoomed(GetForegroundWindow()) != 0 }
}

/// Parse a `.pls` playlist (INI-style: `File1=`, `Title1=`, `Length1=`,
/// `NumberOfEntries=`, `[playlist]` header) into an ordered list of
/// path/URL strings - the same shape a `.m3u`'s lines already produce, so
/// both feed the same downstream filtering in `Message::LoadPlaylist`.
fn parse_pls(content: &str) -> Vec<String> {
    let mut entries: Vec<(u32, String)> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("File") else { continue };
        let Some(eq) = rest.find('=') else { continue };
        let (idx_str, value) = rest.split_at(eq);
        let Ok(idx) = idx_str.parse::<u32>() else { continue };
        let value = value[1..].trim(); // skip the '='
        if !value.is_empty() {
            entries.push((idx, value.to_string()));
        }
    }
    entries.sort_by_key(|(idx, _)| *idx);
    entries.into_iter().map(|(_, v)| v).collect()
}

const MEDIA_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "m4v", "flv", "wmv", "ts",
    "mp3", "flac", "ogg", "wav", "aac", "opus",
];

fn build_folder_playlist(path: &std::path::Path) -> (Vec<std::path::PathBuf>, usize) {
    let Some(parent) = path.parent() else {
        return (vec![path.to_path_buf()], 0);
    };
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(parent)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|s| MEDIA_EXTS.contains(&s.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    let idx = files.iter().position(|p| p == path).unwrap_or(0);
    if files.is_empty() {
        (vec![path.to_path_buf()], 0)
    } else {
        (files, idx)
    }
}

/// Read a directory and return sorted browser entries (dirs first, then media).
pub fn browser_read_dir(path: &std::path::Path) -> Vec<BrowserEntry> {
    let Ok(rd) = std::fs::read_dir(path) else { return Vec::new() };
    let mut dirs: Vec<BrowserEntry> = Vec::new();
    let mut files: Vec<BrowserEntry> = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip hidden files/dirs on all platforms.
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            dirs.push(BrowserEntry { name, path: p, is_dir: true });
        } else if p.is_file() {
            let is_media = p.extension()
                .and_then(|e| e.to_str())
                .map(|s| MEDIA_EXTS.contains(&s.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if is_media {
                files.push(BrowserEntry { name, path: p, is_dir: false });
            }
        }
    }
    dirs.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    files.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    dirs.extend(files);
    dirs
}

/// Return drive letters present on Windows, or root dirs on other platforms.
pub fn browser_drives() -> Vec<BrowserEntry> {
    #[cfg(target_os = "windows")]
    {
        ('A'..='Z')
            .filter_map(|c| {
                let p = std::path::PathBuf::from(format!("{}:\\", c));
                if p.exists() {
                    Some(BrowserEntry {
                        name: format!("{}:\\", c),
                        path: p,
                        is_dir: true,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![BrowserEntry {
            name: "/".to_string(),
            path: std::path::PathBuf::from("/"),
            is_dir: true,
        }]
    }
}

/// Parse a time string like "1:23:45", "83:45", "83.5" into seconds.
pub fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    // Try plain seconds first
    if let Ok(v) = s.parse::<f64>() { return Some(v); }
    // Try h:mm:ss or m:ss
    let parts: Vec<&str> = s.split(':').collect();
    match parts.as_slice() {
        [m, s] => {
            let mins: f64 = m.trim().parse().ok()?;
            let secs: f64 = s.trim().parse().ok()?;
            Some(mins * 60.0 + secs)
        }
        [h, m, s] => {
            let hours: f64 = h.trim().parse().ok()?;
            let mins:  f64 = m.trim().parse().ok()?;
            let secs:  f64 = s.trim().parse().ok()?;
            Some(hours * 3600.0 + mins * 60.0 + secs)
        }
        _ => None,
    }
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let s = s % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn on_window_event(
    event: Event,
    status: iced::event::Status,
    id: iced::window::Id,
) -> Option<Message> {
    match event {
        Event::Window(iced::window::Event::Resized(size)) => {
            Some(Message::WindowResized(id, size))
        }
        Event::Window(iced::window::Event::CloseRequested) => {
            Some(Message::CloseRequested(id))
        }
        Event::Window(iced::window::Event::FileDropped(path)) => {
            Some(Message::FilesDropped(vec![path]))
        }
        Event::Window(iced::window::Event::Moved(point)) => {
            Some(Message::WindowMoved(id, point.x as i32, point.y as i32))
        }
        Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            use iced::keyboard::{Key, key::Named};
            match &key {
                Key::Named(Named::MediaPlayPause)
                | Key::Named(Named::MediaPlay)
                | Key::Named(Named::MediaPause) => return Some(Message::TogglePause),
                Key::Named(Named::MediaStop)           => return Some(Message::Stop),
                Key::Named(Named::MediaTrackNext)      => return Some(Message::NextFile),
                Key::Named(Named::MediaTrackPrevious)  => return Some(Message::PrevFile),
                // Ctrl+G = jump to time
                Key::Character(c) if c.as_str() == "g" && modifiers.control() => {
                    return Some(Message::JumpToTime);
                }
                // Chapter navigation: Ctrl+Left / Ctrl+Right
                Key::Named(Named::ArrowLeft)  if modifiers.control() => {
                    return Some(Message::PrevChapter);
                }
                Key::Named(Named::ArrowRight) if modifiers.control() => {
                    return Some(Message::NextChapter);
                }
                _ => {}
            }
            key_to_name(&key).map(Message::InputKey)
        }
        Event::Mouse(iced::mouse::Event::ButtonPressed(button)) => {
            // Always forward - edge-grip resize must fire even when a panel
            // widget captures the click. The handler skips non-resize actions
            // when captured=true.
            let captured = matches!(status, iced::event::Status::Captured);
            Some(Message::InputMouseDown(id, button, captured))
        }
        Event::Mouse(iced::mouse::Event::ButtonReleased(button)) => {
            Some(Message::InputMouseUp(id, button))
        }
        Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            // A modern mouse can report movement hundreds of times a second;
            // each one triggers a full app rebuild (this app's view() has no
            // partial-update path, like most iced apps). The heavy panels
            // (settings/menu) are now memoized via `lazy`, but the main
            // window's top/controls bar aren't (they need live position
            // updates anyway) - so cursor-driven rebuilds and video-frame-
            // driven rebuilds are two independent triggers on that same
            // still-nontrivial tree. Matching this to the frame-delivery
            // throttle (33ms/~30fps, see init_and_render_loop) instead of a
            // separate faster 60Hz cap keeps the two from compounding to
            // ~90 rebuilds/sec when both are happening at once (playing
            // while moving the mouse) - still smooth for hover/edge-resize
            // purposes.
            static LAST_CURSOR_MOVE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
            const CURSOR_THROTTLE: std::time::Duration = std::time::Duration::from_millis(33);
            let now = std::time::Instant::now();
            let mut last = LAST_CURSOR_MOVE.lock().unwrap();
            if last.is_some_and(|t| now.duration_since(t) < CURSOR_THROTTLE) {
                return None;
            }
            *last = Some(now);
            Some(Message::CursorMoved(id, position.x, position.y))
        }
        Event::Mouse(iced::mouse::Event::CursorLeft) => Some(Message::CursorLeft(id)),
        Event::Window(iced::window::Event::Unfocused) => Some(Message::WindowUnfocused(id)),
        Event::Window(iced::window::Event::Rescaled(factor)) => {
            Some(Message::WindowRescaled(id, factor))
        }
        Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
            // A scrollable widget under the cursor (settings panel, playlist,
            // etc.) already consumed this - don't also treat it as a
            // volume/seek scroll. Without this check, scrolling a list
            // inside the settings panel scrolled the list AND adjusted
            // volume at the same time.
            if matches!(status, iced::event::Status::Captured) {
                return None;
            }
            let dy = match delta {
                iced::mouse::ScrollDelta::Lines { y, .. } => y,
                iced::mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            Some(Message::InputScroll(id, dy))
        }
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => {
            Some(Message::ModifiersChanged(m))
        }
        _ => None,
    }
}

/// Normalise an iced key into a lowercase name suitable for binding lookup.
fn key_to_name(key: &iced::keyboard::Key) -> Option<String> {
    use iced::keyboard::{Key, key::Named};
    Some(match key {
        Key::Named(Named::Space) => "space".into(),
        Key::Named(Named::ArrowLeft) => "left".into(),
        Key::Named(Named::ArrowRight) => "right".into(),
        Key::Named(Named::ArrowUp) => "up".into(),
        Key::Named(Named::ArrowDown) => "down".into(),
        Key::Named(Named::PageUp) => "pageup".into(),
        Key::Named(Named::PageDown) => "pagedown".into(),
        Key::Named(Named::End) => "end".into(),
        Key::Named(Named::Escape) => "escape".into(),
        Key::Named(Named::Enter) => "enter".into(),
        Key::Named(Named::Tab) => "tab".into(),
        Key::Character(c) => {
            // Normalise backslash - on some keyboards iced returns "\\"
            let s = c.as_str();
            if s == "\\" { "\\".into() } else { s.to_ascii_lowercase() }
        }
        _ => return None,
    })
}

fn mpv_stream(key: &StreamKey) -> BoxStream<'static, Message> {
    Box::pin(crate::player::event_stream(key).map(|ev| match ev {
        PlayerEvent::Position(p) => Message::PositionChanged(p),
        PlayerEvent::Duration(d) => Message::DurationChanged(d),
        PlayerEvent::FileLoaded => Message::FileLoaded,
        PlayerEvent::EndFile => Message::EndFile,
        PlayerEvent::EofReached(v) => Message::EofReached(v),
        PlayerEvent::Pause(p) => Message::PauseChanged(p),
        PlayerEvent::Frame(px, w, h) => Message::FrameReady(px, w, h),
        PlayerEvent::Width(w) => Message::WidthChanged(w),
        PlayerEvent::Height(h) => Message::HeightChanged(h),
        PlayerEvent::VideoCodec(s) => Message::VideoCodecChanged(s),
        PlayerEvent::AudioCodec(s) => Message::AudioCodecChanged(s),
        PlayerEvent::AudioChannels(c) => Message::AudioChannelsChanged(c),
        PlayerEvent::HwDec(s) => Message::HwDecChanged(s),
        PlayerEvent::Primaries(s) => Message::PrimariesChanged(s),
        PlayerEvent::SubVisible(v) => Message::SubVisibleChanged(v),
        PlayerEvent::SubTracks(list) => Message::SubTracksChanged(list),
        PlayerEvent::CurrentSid(id) => Message::CurrentSidChanged(id),
        PlayerEvent::CurrentSecondarySid(id) => Message::CurrentSecondarySidChanged(id),
        PlayerEvent::Chapters(list) => Message::ChaptersChanged(list),
        PlayerEvent::AudioTracks(list) => Message::AudioTracksChanged(list),
        PlayerEvent::CurrentAid(id) => Message::CurrentAidChanged(id),
        PlayerEvent::Speed(s) => Message::SpeedChanged(s),
        PlayerEvent::SubDelay(v)    => Message::SubDelayChanged(v),
        PlayerEvent::AudioDelay(v)  => Message::AudioDelayChanged(v),
        PlayerEvent::SubFontSize(v) => Message::SubFontSizeChanged(v),
        PlayerEvent::SubPos(v)      => Message::SubPosChanged(v),
        PlayerEvent::LoopFile(v)     => Message::LoopFileChanged(v),
        PlayerEvent::LoopPlaylist(v) => Message::LoopPlaylistChanged(v),
        PlayerEvent::Deinterlace(v)  => Message::DeinterlaceChanged(v),
        PlayerEvent::VideoZoom(v)    => Message::VideoZoomChanged(v),
        PlayerEvent::CacheTime(v)    => Message::CacheTimeChanged(v),
    }))
}
