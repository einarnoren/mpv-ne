//! Persistent settings. Saved to the OS-standard config directory:
//!   Windows: %APPDATA%\mpv-ne\config\settings.toml
//!   macOS:   ~/Library/Application Support/mpv-ne/settings.toml
//!   Linux:   ~/.config/mpv-ne/settings.toml
//!
//! Grouped into sub-structs by area (window/playback/audio/subtitles/
//! streaming) rather than one flat struct. Every field and sub-struct is
//! `#[serde(default)]`, so adding a new field or even a whole new section
//! later never breaks loading an older config file - anything missing just
//! falls back to its default instead of erroring out.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub window: WindowSettings,
    #[serde(default)]
    pub playback: PlaybackSettings,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub subtitles: SubtitleSettings,
    #[serde(default)]
    pub streaming: StreamingSettings,
    #[serde(default)]
    pub interface: InterfaceSettings,
    /// Key-rebind overrides: slot id (see `app::KEY_SLOTS`) -> key name.
    /// An empty string value means that slot was explicitly cleared (no key
    /// bound). Missing entries fall back to the slot's default key.
    #[serde(default)]
    pub keybindings: std::collections::HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WindowSettings {
    pub w: Option<u32>,
    pub h: Option<u32>,
    /// Last window position (top-left corner, logical pixels).
    pub x: Option<i32>,
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackSettings {
    /// Whether to resume playback from the last position.
    pub resume_enabled: bool,
    /// Last used volume (0-200). Restored on next launch.
    pub volume: f64,
    /// When true (default), seeking lands on the exact frame requested
    /// ("absolute+exact") - slower to respond while scrubbing since mpv
    /// has to decode forward from the nearest keyframe. When false, seeks
    /// snap to the nearest keyframe instead ("absolute+keyframes") - near
    /// instant, but can land up to a few seconds off.
    pub precise_seek: bool,
    /// Screenshot save directory. Empty = mpv default.
    pub screenshot_dir: String,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self { resume_enabled: true, volume: 100.0, precise_seek: true, screenshot_dir: String::new() }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Dynamic audio normalization (ffmpeg `dynaudnorm` filter).
    pub normalize: bool,
    /// Preferred audio track language (ISO 639 code, e.g. "eng"). Empty =
    /// no preference (mpv picks its own default). Applied as mpv's `alang`
    /// option, which only influences track auto-selection at file load.
    pub lang: String,
    /// Graphic equalizer on/off and per-band gains (dB), one entry per
    /// `player::EQ_BANDS`. A short/empty vec (older config, or never used)
    /// is padded with 0.0 at load time rather than erroring - see
    /// `MpvNe`'s startup wiring.
    pub eq_enabled: bool,
    pub eq_gains: Vec<f64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitleSettings {
    /// Same as `AudioSettings::lang` but for subtitle tracks (mpv's `slang`).
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamingSettings {
    /// Max height (in pixels) yt-dlp is allowed to grab for network streams
    /// (YouTube/Twitch/etc). 0 = uncapped. See `Player::set_stream_quality`.
    pub quality_height: u32,
}

impl Default for StreamingSettings {
    fn default() -> Self {
        Self { quality_height: 1080 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfaceSettings {
    /// Windows-only: snap the window to monitor edges and other MPV-NE
    /// windows while dragging. No-op on other platforms.
    pub snap_to_edge: bool,
    /// Left-click-drag on any non-interactive area of the video starts a
    /// window move, not just the title bar - off-switch for classic
    /// click-to-pause-style behavior. See `Bindings::drag_window_anywhere`.
    pub drag_anywhere: bool,
    /// Draw our own title bar (pin/min/max/close on the Nord top bar)
    /// instead of the OS one. Read once at startup - takes a restart to
    /// apply since window decorations are fixed at window-creation time.
    pub custom_title_bar: bool,
    /// Restore the last window position/size on launch. When off, every
    /// launch opens at the default size, centered.
    pub remember_window: bool,
    /// Start with always-on-top (pin) already enabled.
    pub start_pinned: bool,
    /// Show the on-screen notification popups (volume/seek/speed/etc.).
    pub osd_enabled: bool,
    /// Generate and show the seekbar thumbnail scrub preview.
    pub thumbnail_preview: bool,
    /// Re-download the latest yt-dlp at startup instead of only fetching it
    /// once when first needed.
    pub auto_update_ytdlp: bool,
    /// Windows-only: minimize the detached panel/App Settings windows along
    /// with the main window.
    pub hide_all_on_minimize: bool,
    /// Pause playback when the main window loses focus. Does not
    /// auto-resume on refocus - matches PotPlayer's behavior of leaving
    /// that to the user, so a deliberate pause during the time away isn't
    /// silently overridden.
    pub pause_on_focus_lost: bool,
    /// Windows-only: pause playback when the main window is minimized.
    pub pause_on_minimize: bool,
    /// When opening a file, also queue sibling media files from the same
    /// folder into the playlist (PotPlayer's "load previous/next files in
    /// play folder"). On by default - this was previously unconditional
    /// behavior; this setting is the off-switch for it.
    pub auto_load_siblings: bool,
    /// Windows-only: opening a second file/URL while an instance is already
    /// running forwards it to that instance instead of starting a new
    /// process. Off by default - multiple instances is the existing/
    /// expected behavior, this is opt-in.
    pub single_instance: bool,
}

impl Default for InterfaceSettings {
    fn default() -> Self {
        Self {
            snap_to_edge: true,
            drag_anywhere: true,
            custom_title_bar: true,
            remember_window: true,
            start_pinned: false,
            osd_enabled: true,
            thumbnail_preview: true,
            auto_update_ytdlp: false,
            hide_all_on_minimize: true,
            pause_on_focus_lost: false,
            pause_on_minimize: false,
            auto_load_siblings: true,
            single_instance: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };

        // Pre-0.3.5 configs stored everything as flat top-level keys
        // (window_w, resume_enabled, ...) instead of nested tables. Those
        // keys are silently ignored by the nested schema below (unknown
        // fields don't error), which would otherwise reset users back to
        // defaults on upgrade. Detect that shape once and migrate it.
        if let Ok(raw) = content.parse::<toml::Value>() {
            if raw.get("window_w").is_some() || raw.get("resume_enabled").is_some() {
                let migrated = Self::from_legacy_flat(&raw);
                migrated.save();
                return migrated;
            }
        }

        toml::from_str(&content).unwrap_or_default()
    }

    fn from_legacy_flat(raw: &toml::Value) -> Self {
        let u32_of = |k: &str| raw.get(k).and_then(|v| v.as_integer()).map(|v| v as u32);
        let i32_of = |k: &str| raw.get(k).and_then(|v| v.as_integer()).map(|v| v as i32);
        let bool_of = |k: &str, d: bool| raw.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
        let f64_of = |k: &str, d: f64| raw.get(k).and_then(|v| v.as_float()).unwrap_or(d);
        let str_of = |k: &str| raw.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();

        Self {
            window: WindowSettings {
                w: u32_of("window_w"),
                h: u32_of("window_h"),
                x: i32_of("window_x"),
                y: i32_of("window_y"),
            },
            playback: PlaybackSettings {
                resume_enabled: bool_of("resume_enabled", true),
                volume: f64_of("volume", 100.0),
                precise_seek: bool_of("precise_seek", true),
                screenshot_dir: str_of("screenshot_dir"),
            },
            audio: AudioSettings {
                normalize: bool_of("audio_normalize", false),
                lang: str_of("audio_lang"),
                eq_enabled: false,
                eq_gains: Vec::new(),
            },
            subtitles: SubtitleSettings { lang: str_of("sub_lang") },
            streaming: StreamingSettings {
                quality_height: u32_of("stream_quality_height").unwrap_or(1080),
            },
            interface: InterfaceSettings::default(),
            keybindings: std::collections::HashMap::new(),
        }
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&path, content);
        }
    }

    pub fn window_size(&self) -> Option<(f32, f32)> {
        Some((self.window.w? as f32, self.window.h? as f32))
    }
}

fn settings_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "mpv-ne")?;
    Some(dirs.config_dir().join("settings.toml"))
}
