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
}

#[derive(Debug, Clone)]
pub struct FileContextMenu {
    pub path: std::path::PathBuf,
    /// Window coordinates where the right-click occurred.
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}

/// How the video frame is fitted into the window. Cycled with the Z key and
/// applied by the letterbox shader (mpv renders at native size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    fn label(self) -> &'static str {
        match self {
            FrameMode::Fit => "Fit",
            FrameMode::Fill => "Fill",
            FrameMode::Stretch => "Stretch",
        }
    }
}

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

impl Default for Bindings {
    fn default() -> Self {
        let mut keys = HashMap::new();
        keys.insert("space".into(), Action::TogglePause);
        keys.insert("left".into(), Action::SeekRelative(-5.0));
        keys.insert("right".into(), Action::SeekRelative(5.0));
        keys.insert("up".into(), Action::VolumeAdjust(5.0));
        keys.insert("down".into(), Action::VolumeAdjust(-5.0));
        keys.insert("pageup".into(), Action::PrevFile);
        keys.insert("pagedown".into(), Action::NextFile);
        // Escape is NOT bound here - it exits fullscreen/chrome-hidden only,
        // never enters fullscreen. Handled directly in InputKey below.
        keys.insert("m".into(), Action::ToggleMute);
        keys.insert("f".into(), Action::ToggleFullscreen);
        keys.insert("h".into(), Action::ToggleChrome);
        keys.insert("j".into(), Action::CycleSubtitle);
        keys.insert("#".into(), Action::CycleAudio);
        keys.insert("[".into(), Action::SpeedAdjust(-0.1));
        keys.insert("]".into(), Action::SpeedAdjust(0.1));
        keys.insert("\\".into(), Action::SpeedReset);
        keys.insert("v".into(), Action::ToggleSubVisibility);
        keys.insert("i".into(), Action::ToggleHwDec);
        keys.insert("b".into(), Action::AddBookmark);
        keys.insert("s".into(), Action::ToggleStats);
        keys.insert("z".into(), Action::CycleFrameMode);

        Self {
            keys,
            single_left_click: None,
            double_left_click: Some(Action::ToggleFullscreen),
            scroll_up: Some(Action::VolumeAdjust(2.0)),
            scroll_down: Some(Action::VolumeAdjust(-2.0)),
            drag_window_anywhere: true,
        }
    }
}

/// Convert an Action into its equivalent Message so the existing handlers can
/// execute it. Lets us keep the binding lookup path simple while reusing the
/// existing implementations.
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
    }
}

pub const CONTROLS_H: i32 = 76;
const TOP_BAR_H: i32 = 44;
/// Width of the docked side panel in logical pixels. Must match SETTINGS_PANEL_W in ui/mod.rs.
const PANEL_W: f32 = 280.0;

/// Set true to disable the OS title bar and draw our own - gives us pin,
/// minimize, maximize, close all on the Nord top bar. Flip to false to fall
/// back to the OS title bar (the bar will still show, just without the
/// min/max/close buttons since the OS provides those).
pub const USE_CUSTOM_TITLE_BAR: bool = true;

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
    modal_hook_installed: bool,
    pub playlist: Vec<std::path::PathBuf>,
    pub playlist_idx: usize,
    pub bindings: Bindings,
    last_left_press: Option<Instant>,
    /// Manual override - true means hide controls even when not fullscreen.
    /// Effective visibility is `chrome_visible()`; controls are hidden when
    /// in fullscreen *or* when this flag is set.
    pub chrome_force_hidden: bool,
    /// Cursor position in logical pixels relative to the window client area.
    /// `None` when the cursor is outside the window. Used to overlay the
    /// controls on hover while chrome is hidden.
    pub cursor_pos: Option<(f32, f32)>,
    /// Last known window dimensions and position in logical pixels.
    pub window_h_logical: f32,
    pub window_w_logical: f32,
    pub window_x_logical: i32,
    pub keyboard_modifiers: iced::keyboard::Modifiers,
    pub window_y_logical: i32,
    /// True when always-on-top is enabled via the pin button.
    pub pinned: bool,
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
    pub screenshot_dir: String,
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
    pub video_rotate: i64,   // 0 / 90 / 180 / 270
    pub video_hflip: bool,
    pub video_vflip: bool,
    pub fit_menu_open: bool,
    pub audio_menu_open: bool,
    /// Cursor X (in window coords) when a popup menu was last opened.
    /// Used to anchor the popup above whichever button was clicked.
    pub popup_anchor_x: f32,
    pub playlist_sort_open: bool,
    pub panels_menu_open: bool,
    /// Context menu for file entries in panels.
    pub file_context_menu: Option<FileContextMenu>,
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
    /// The target of the most-recent seek issued during a live chase.
    /// Used to debounce: we skip a DurationChanged-triggered seek if the
    /// new target is within 10s of this value (avoids hundreds of seeks
    /// per End press from rapid DurationChanged events).
    pub live_last_seek: f64,
    /// True after EndFile fired while position was at the live edge (keep-open=yes).
    /// DurationChanged will re-seek forward when new content arrives so the user
    /// doesn't have to manually press End after every buffer refill.
    pub live_edge_paused: bool,
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
        Self {
            player: Player::default(),
            current_frame: None,
            pending_w: 0,
            pending_h: 0,
            resize_seq: 0,
            render_initialized: false,
            fullscreen: false,
            window_id: None,
            modal_hook_installed: false,
            playlist: Vec::new(),
            playlist_idx: 0,
            bindings: Bindings::default(),
            last_left_press: None,
            chrome_force_hidden: false,
            cursor_pos: None,
            window_h_logical: 0.0,
            window_w_logical: 0.0,
            window_x_logical: 0,
            keyboard_modifiers: iced::keyboard::Modifiers::default(),
            window_y_logical: 0,
            pinned: false,
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
            resume_enabled: {
                let s = crate::settings::Settings::load();
                s.resume_enabled
            },
            screenshot_dir: crate::settings::Settings::load().screenshot_dir,
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
            video_rotate: 0,
            video_hflip: false,
            video_vflip: false,
            fit_menu_open: false,
            audio_menu_open: false,
            popup_anchor_x: 0.0,
            playlist_sort_open: false,
            panels_menu_open: false,
            file_context_menu: None,
            osd_message: String::new(),
            osd_seq: 0,
            active_panel: None,
            last_panel: PanelKind::Playlist,
            browser_path: None,
            browser_entries: Vec::new(),
            resume_db: ResumeDb::load(),
            recent_files: RecentFiles::load(),
            ab_loop_a: None,
            ab_loop_b: None,
            live_catching_up: false,
            live_last_seek: 0.0,
            live_edge_paused: false,
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
        }
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
    UrlEntered(String),
    ChooseScreenshotDir,
    ScreenshotDirSelected(String),
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
    WindowMoved(i32, i32),
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
    InputMouseDown(iced::mouse::Button, bool),
    InputScroll(f32),
    ModifiersChanged(iced::keyboard::Modifiers),
    CursorMoved(f32, f32),
    CursorLeft,
    // AB repeat
    AbLoopSetA,
    AbLoopSetB,
    AbLoopClear,
}

impl MpvNe {
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
                self.player.seek(pos);
            }
            Message::VolumeChanged(vol) => {
                self.player.set_volume(vol);
                let mut prefs = crate::settings::Settings::load();
                prefs.volume = self.player.volume;
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
                    let saved_vol = crate::settings::Settings::load().volume;
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
                        let save_w = if self.active_panel.is_some() {
                            (self.window_w_logical - PANEL_W).max(480.0) as u32
                        } else {
                            self.window_w_logical.max(0.0) as u32
                        };
                        crate::settings::Settings {
                            window_w: Some(save_w),
                            window_h: Some(self.window_h_logical.max(0.0) as u32),
                            window_x: Some(self.window_x_logical),
                            window_y: Some(self.window_y_logical),
                            resume_enabled: self.resume_enabled,
                            volume: self.player.volume,
                            screenshot_dir: self.screenshot_dir.clone(),
                        }
                        .save();
                    }
                }
            }

            Message::PositionChanged(pos) => {
                self.player.position = pos;
                // AB repeat: snap back to A when position passes B.
                if let (Some(a), Some(b)) = (self.ab_loop_a, self.ab_loop_b) {
                    if pos >= b {
                        self.player.seek(a);
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
                {
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
                    if let Some(path) = &self.player.path.clone() {
                        crate::thumbnail::spawn_generate(
                            path.clone(),
                            dur,
                            self.thumb_cache.clone(),
                        );
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
                self.player.seek(pos);
            }
            Message::LiveEdgeTick => {
                // mpv's demuxer doesn't fire DurationChanged while keep-open pauses
                // at EOF. Poke play() so mpv resumes reading — it will fire
                // DurationChanged if more content exists (letting the chase continue),
                // or re-pause immediately if we're truly at the live edge.
                if self.live_edge_paused {
                    tracing::debug!(
                        pos = self.player.position,
                        chasing = self.live_catching_up,
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
            Message::ShowOsd(msg) => {
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
                if id != self.thumb_pending_id { return Task::none(); }
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
                self.browser_entries = browser_read_dir(&path);
                self.browser_path = Some(path);
                let paths: Vec<_> = self.browser_entries.iter()
                    .filter(|e| !e.is_dir).map(|e| e.path.clone()).collect();
                if !paths.is_empty() {
                    return Task::done(Message::ProbeFiles(paths));
                }
            }
            Message::BrowserNavigateUp => {
                if let Some(ref cur) = self.browser_path.clone() {
                    match cur.parent() {
                        Some(p) if p != cur => {
                            let p = p.to_path_buf();
                            self.browser_entries = browser_read_dir(&p);
                            self.browser_path = Some(p);
                        }
                        _ => {
                            // Already at root - go to drives list.
                            self.browser_path = None;
                            self.browser_entries = browser_drives();
                        }
                    }
                } else {
                    // Already at drives.
                    self.browser_entries = browser_drives();
                }
            }
            Message::BrowserGoToDrives => {
                self.browser_path = None;
                self.browser_entries = browser_drives();
            }
            Message::BrowserOpen(path) => {
                // Update browser to show the file's directory first.
                if let Some(dir) = path.parent() {
                    self.browser_path = Some(dir.to_path_buf());
                    self.browser_entries = browser_read_dir(dir);
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
                self.player.seek(pos);
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
                            let paths: Vec<_> = content.lines()
                                .filter(|l| !l.starts_with('#') && !l.is_empty())
                                .map(|l| std::path::PathBuf::from(l.trim()))
                                .filter(|p| p.exists())
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
                    self.playlist = paths;
                    self.playlist_idx = 0;
                    let p = self.playlist[0].clone();
                    self.open_next(p.to_string_lossy().into_owned());
                    return Task::done(Message::ShowOsd(format!("Loaded {count} files")));
                }
            }

            Message::JumpToTime => {
                return Task::done(Message::OpenModal(ModalKind::JumpToTime));
            }
            Message::OpenModal(kind) => {
                let (title, prompt) = match kind {
                    ModalKind::JumpToTime => ("Jump to time", "Enter time (1:23:45 or seconds)"),
                    ModalKind::OpenUrl    => ("Open URL / stream", "Enter URL or file path"),
                };
                self.modal = Some(ModalDialog { title, prompt, input: String::new(), kind });
            }
            Message::ModalInput(s) => {
                if let Some(m) = &mut self.modal { m.input = s; }
            }
            Message::ModalConfirm => {
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
                                } else {
                                    self.player.open_url(&m.input);
                                    return Task::done(Message::ShowOsd(format!("Opening: {}", m.input)));
                                }
                            }
                        }
                    }
                }
            }
            Message::ModalCancel => { self.modal = None; }
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
            Message::UrlEntered(url) => {
                if !url.is_empty() {
                    let path = std::path::PathBuf::from(&url);
                    if path.exists() {
                        self.load_path(path);
                    } else {
                        // Treat as URL for yt-dlp streaming.
                        self.player.open_url(&url);
                        return Task::done(Message::ShowOsd(format!("Opening: {url}")));
                    }
                }
            }
            Message::ToggleResume => {
                self.resume_enabled = !self.resume_enabled;
                let mut prefs = crate::settings::Settings::load();
                prefs.resume_enabled = self.resume_enabled;
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

            Message::WindowMoved(x, y) => {
                self.window_x_logical = x;
                self.window_y_logical = y;
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
                prefs.screenshot_dir = dir;
                prefs.save();
            }

            Message::Noop => {}

            Message::CloseRequested(id) => {
                if Some(id) == self.window_id {
                    self.save_resume();
                    self.player.quit();
                    return iced::window::close(id);
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
                    return iced::window::close(id);
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
                prefs.volume = self.player.volume;
                prefs.save();
                return Task::done(Message::ShowOsd(format!("Volume  {:.0}%", self.player.volume)));
            }
            Message::FileDropped(path) => return Task::done(Message::FilesDropped(vec![path])),

            Message::CursorMoved(x, y) => self.cursor_pos = Some((x, y)),
            Message::CursorLeft => self.cursor_pos = None,
            Message::InputKey(name) => {
                // Escape closes the modal first.
                if name == "escape" && self.modal.is_some() {
                    self.modal = None;
                    return Task::none();
                }
                // Escape closes other popups.
                if name == "escape"
                    && (self.subs_menu_open || self.fit_menu_open
                        || self.audio_menu_open || self.panels_menu_open
                        || self.active_panel.is_some())
                {
                    self.subs_menu_open = false;
                    self.fit_menu_open = false;
                    self.audio_menu_open = false;
                    self.panels_menu_open = false;
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

            Message::InputScroll(dy) => {
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
            Message::InputMouseDown(button, captured) => {
                if button == iced::mouse::Button::Left {
                    // Edge-grip resize fires regardless of captured state.
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

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
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
        if !USE_CUSTOM_TITLE_BAR {
            return None;
        }
        if self.fullscreen {
            return None;
        }
        const EDGE_GRIP: f32 = 10.0;
        const CORNER_GRIP: f32 = 16.0;
        let (x, y) = self.cursor_pos?;
        let w = self.window_w_logical;
        let h = self.window_h_logical;
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

    /// Toggle, switch, or close the docked side panel. Returns a resize Task
    /// only when the panel transitions between open and closed (not when
    /// switching between panel kinds, which keeps the same width).
    fn toggle_panel(&mut self, kind: Option<PanelKind>) -> Task<Message> {
        self.panels_menu_open = false; // always close the picker when a panel is toggled
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
            let maximized = {
                #[cfg(target_os = "windows")]
                { win32_is_maximized(self.window_id) }
                #[cfg(not(target_os = "windows"))]
                { false }
            };
            if !self.fullscreen && !maximized {
                if let Some(id) = self.window_id {
                    let delta: f32 = if is_open { 280.0 } else { -280.0 };
                    let new_size = iced::Size::new(
                        (self.window_w_logical + delta).max(480.0),
                        self.window_h_logical,
                    );
                    let resize_task = iced::window::resize(id, new_size);
                    return Task::batch([resize_task, probe_task]);
                }
            }
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
        let (list, idx) = build_folder_playlist(&path);
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
        self.player.set_render_size(w, h);
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
    }

    pub fn theme(&self) -> Theme {
        Theme::Nord
    }

    pub fn title(&self) -> String {
        if let Some(path) = &self.player.path {
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path.as_str());
            format!("MPV-NE | {}", name)
        } else {
            "MPV-NE".to_string()
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
        Subscription::batch(subs)
    }
}

/// Returns true when the iced application window is currently maximized.
/// Uses `IsZoomed` from Win32 - no extra crate needed.
#[cfg(target_os = "windows")]
fn win32_is_maximized(window_id: Option<iced::window::Id>) -> bool {
    let _ = window_id; // kept for future use
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
        fn IsZoomed(hwnd: *mut std::ffi::c_void) -> i32;
    }
    unsafe { IsZoomed(GetForegroundWindow()) != 0 }
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
            Some(Message::WindowMoved(point.x as i32, point.y as i32))
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
            Some(Message::InputMouseDown(button, captured))
        }
        Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position.x, position.y))
        }
        Event::Mouse(iced::mouse::Event::CursorLeft) => Some(Message::CursorLeft),
        Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
            let dy = match delta {
                iced::mouse::ScrollDelta::Lines { y, .. } => y,
                iced::mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            Some(Message::InputScroll(dy))
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
