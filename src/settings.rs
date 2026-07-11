//! Persistent settings. Saved to the OS-standard config directory:
//!   Windows: %APPDATA%\mpv-ne\config\settings.toml
//!   macOS:   ~/Library/Application Support/mpv-ne/settings.toml
//!   Linux:   ~/.config/mpv-ne/settings.toml
//!
//! Right now we persist just the window size. Other prefs (chrome state,
//! pin state, bindings, etc.) can be added the same way.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub window_w: Option<u32>,
    pub window_h: Option<u32>,
    /// Last window position (top-left corner, logical pixels).
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    /// Whether to resume playback from the last position (default: true).
    #[serde(default = "default_true")]
    pub resume_enabled: bool,
    /// Last used volume (0-200). Restored on next launch.
    #[serde(default = "default_volume")]
    pub volume: f64,
    /// Screenshot save directory. Empty = mpv default.
    #[serde(default)]
    pub screenshot_dir: String,
    /// Dynamic audio normalization (ffmpeg `dynaudnorm` filter).
    #[serde(default)]
    pub audio_normalize: bool,
    /// Preferred audio track language (ISO 639 code, e.g. "eng"). Empty =
    /// no preference (mpv picks its own default). Applied as mpv's `alang`
    /// option, which only influences track auto-selection at file load.
    #[serde(default)]
    pub audio_lang: String,
    /// Same as `audio_lang` but for subtitle tracks (mpv's `slang`).
    #[serde(default)]
    pub sub_lang: String,
    /// When true (default), seeking lands on the exact frame requested
    /// ("absolute+exact") - slower to respond while scrubbing since mpv
    /// has to decode forward from the nearest keyframe. When false, seeks
    /// snap to the nearest keyframe instead ("absolute+keyframes") - near
    /// instant, but can land up to a few seconds off.
    #[serde(default = "default_true")]
    pub precise_seek: bool,
    /// Max height (in pixels) yt-dlp is allowed to grab for network streams
    /// (YouTube/Twitch/etc). 0 = uncapped. See `Player::set_stream_quality`.
    #[serde(default = "default_stream_quality")]
    pub stream_quality_height: u32,
}

fn default_true() -> bool { true }
fn default_volume() -> f64 { 100.0 }
fn default_stream_quality() -> u32 { 1080 }

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
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
        Some((self.window_w? as f32, self.window_h? as f32))
    }
}

fn settings_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "mpv-ne")?;
    Some(dirs.config_dir().join("settings.toml"))
}
