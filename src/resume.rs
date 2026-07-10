//! Per-file resume position + media metadata cache.
//! Stored at: %APPDATA%\mpv-ne\data\resume.json (Windows)
//!
//! All four maps share a common MAX_ENTRIES cap to prevent unbounded growth.
//! `last_played` is only written for sessions >= 5 seconds (meaningful plays).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const MAX_ENTRIES: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub position: f64,
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ResumeDb {
    positions:    HashMap<String, f64>,
    #[serde(default)] durations:    HashMap<String, f64>,
    #[serde(default)] resolutions:  HashMap<String, (u32, u32)>,
    #[serde(default)] last_played:  HashMap<String, u64>,
    #[serde(default)] audio_tracks: HashMap<String, i64>,
    #[serde(default)] sub_tracks:   HashMap<String, i64>,
    #[serde(default)] volumes:      HashMap<String, f64>,
    #[serde(default)] bookmarks:    HashMap<String, Vec<Bookmark>>,
    /// When true, `save()` is a no-op - private/no-trace mode. Not
    /// persisted itself (that would defeat the point); set fresh each
    /// session via `set_private`. In-memory reads/writes still work
    /// normally within the session, only the on-disk file is untouched.
    #[serde(skip)]
    private: bool,
}

impl ResumeDb {
    pub fn load() -> Self {
        let Some(path) = db_path() else { return Self::default() };
        let Ok(bytes) = std::fs::read(&path) else { return Self::default() };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn set_private(&mut self, private: bool) {
        self.private = private;
    }

    pub fn save(&self) {
        if self.private { return; }
        let Some(path) = db_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_vec(self) {
            let _ = std::fs::write(&path, content);
        }
    }

    // ── Getters ──────────────────────────────────────────────────────────────

    /// Resume position, only if >= 5s (not worth resuming from the very start).
    pub fn get(&self, file_path: &str) -> Option<f64> {
        self.positions.get(file_path).copied().filter(|&p| p >= 5.0)
    }

    pub fn duration(&self, file_path: &str) -> Option<f64> {
        self.durations.get(file_path).copied().filter(|&d| d > 0.0)
    }

    pub fn resolution(&self, file_path: &str) -> Option<(u32, u32)> {
        self.resolutions.get(file_path).copied().filter(|&(w, h)| w > 0 && h > 0)
    }

    pub fn last_played(&self, file_path: &str) -> Option<u64> {
        self.last_played.get(file_path).copied().filter(|&t| t > 0)
    }

    pub fn audio_track(&self, file_path: &str) -> Option<i64> {
        self.audio_tracks.get(file_path).copied()
    }

    pub fn sub_track(&self, file_path: &str) -> Option<i64> {
        self.sub_tracks.get(file_path).copied()
    }

    pub fn volume(&self, file_path: &str) -> Option<f64> {
        self.volumes.get(file_path).copied()
    }

    pub fn bookmarks(&self, file_path: &str) -> &[Bookmark] {
        self.bookmarks.get(file_path).map(|v| v.as_slice()).unwrap_or(&[])
    }

    // ── Writers ──────────────────────────────────────────────────────────────

    /// Store duration from a background probe (no position, no last_played update).
    pub fn record_duration(&mut self, file_path: &str, duration: f64) {
        if duration > 0.0 {
            self.durations.insert(file_path.to_string(), duration);
            trim(&mut self.durations);
        }
    }

    /// Store video resolution once it is known from playback.
    /// Called when both width and height are available.
    pub fn record_resolution(&mut self, file_path: &str, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.resolutions.insert(file_path.to_string(), (w, h));
            trim(&mut self.resolutions);
        }
    }

    pub fn record_audio_track(&mut self, file_path: &str, id: i64) {
        self.audio_tracks.insert(file_path.to_string(), id);
        trim(&mut self.audio_tracks);
    }

    pub fn record_sub_track(&mut self, file_path: &str, id: i64) {
        self.sub_tracks.insert(file_path.to_string(), id);
        trim(&mut self.sub_tracks);
    }

    pub fn record_volume(&mut self, file_path: &str, vol: f64) {
        self.volumes.insert(file_path.to_string(), vol);
        trim(&mut self.volumes);
    }

    /// Add a bookmark at `position`. Silently deduplicates within 2 seconds.
    pub fn add_bookmark(&mut self, file_path: &str, position: f64, label: String) {
        let list = self.bookmarks.entry(file_path.to_string()).or_default();
        if list.iter().any(|b| (b.position - position).abs() < 2.0) { return; }
        let idx = list.partition_point(|b| b.position < position);
        list.insert(idx, Bookmark { position, label });
    }

    pub fn remove_bookmark(&mut self, file_path: &str, idx: usize) {
        if let Some(list) = self.bookmarks.get_mut(file_path) {
            if idx < list.len() { list.remove(idx); }
        }
    }

    /// Record end-of-session state. Called at EndFile or app close.
    /// - duration and resolution are always stored (useful for display)
    /// - position is stored only if resumable (>= 5s, < 95% complete)
    /// - last_played is stored only for meaningful sessions (>= 5s watched)
    pub fn record(&mut self, file_path: &str, position: f64, duration: f64) {
        // Duration: always store if known.
        if duration > 0.0 {
            self.durations.insert(file_path.to_string(), duration);
            trim(&mut self.durations);
        }

        // Skip everything else for trivially short sessions.
        if position < 5.0 {
            self.positions.remove(file_path);
            return;
        }

        // Last played: only for sessions where we actually watched something.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_played.insert(file_path.to_string(), now);
        trim(&mut self.last_played);

        // Resume position: skip if near the end (treat as finished).
        if duration > 0.0 && position / duration >= 0.95 {
            self.positions.remove(file_path);
        } else {
            self.positions.insert(file_path.to_string(), position);
            trim(&mut self.positions);
        }
    }
}

/// Drop an arbitrary entry when a map exceeds MAX_ENTRIES.
fn trim<V>(map: &mut HashMap<String, V>) {
    if map.len() > MAX_ENTRIES {
        if let Some(key) = map.keys().next().cloned() {
            map.remove(&key);
        }
    }
}

fn db_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "mpv-ne")?;
    Some(dirs.data_dir().join("resume.json"))
}

// ── Recent files ─────────────────────────────────────────────────────────────

const MAX_RECENT: usize = 30;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RecentFiles {
    pub paths: Vec<PathBuf>,
    /// See `ResumeDb::private` - same private/no-trace mechanism.
    #[serde(skip)]
    private: bool,
}

impl RecentFiles {
    pub fn load() -> Self {
        let Some(p) = recent_path() else { return Self::default() };
        let Ok(bytes) = std::fs::read(&p) else { return Self::default() };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn set_private(&mut self, private: bool) {
        self.private = private;
    }

    pub fn save(&self) {
        if self.private { return; }
        let Some(p) = recent_path() else { return };
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_vec(self) {
            let _ = std::fs::write(&p, content);
        }
    }

    /// Record a newly opened file - deduplicates, keeps most-recent-first.
    pub fn record(&mut self, path: &PathBuf) {
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.clone());
        self.paths.truncate(MAX_RECENT);
    }
}

fn recent_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "mpv-ne")?;
    Some(dirs.data_dir().join("recent.json"))
}
