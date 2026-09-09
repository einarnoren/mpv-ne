// Only suppress the console window in release builds so debug runs show output.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod debug_log;
mod media_probe;
mod opensubs;
mod thumbnail;
mod player;
#[cfg(target_os = "windows")]
mod win32_input;
mod resume;
mod settings;
mod ui;
#[cfg(target_os = "windows")]
mod win32_modal;
mod gl_render;

use app::MpvNe;
use tracing_subscriber::EnvFilter;

/// Fans tracing output out to stdout (visible when a console is attached -
/// always in debug builds, since main.rs only hides it in release), the
/// always-on in-memory ring buffer (see `debug_log` - powers the in-app Log
/// Viewer window), and, if the "Debug logging" setting is on, a log file
/// next to the exe as well.
struct DebugWriter {
    file: Option<std::fs::File>,
    ring: debug_log::RingWriter,
}

impl std::io::Write for DebugWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stdout().write_all(buf);
        if let Some(f) = &mut self.file {
            let _ = f.write_all(buf);
        }
        let _ = self.ring.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stdout().flush();
        if let Some(f) = &mut self.file {
            let _ = f.flush();
        }
        Ok(())
    }
}

fn main() -> iced::Result {
    let prefs = settings::Settings::load();

    // A log file next to the exe is opt-in (the "Debug logging" Interface
    // setting, or --log for a one-off run) since it writes to disk. The
    // in-app Log Viewer's ring buffer is not opt-in - it's in-memory only,
    // cheap, and bounded, so there's no reason to gate it behind a restart.
    let log_file = if prefs.interface.debug_logging || std::env::args().any(|a| a == "--log") {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();
        let path = exe_dir.join("mpvne-debug.log");
        // Program Files is read-only for regular users; fall back to Temp.
        let writable = if std::fs::File::create(&path).is_ok() {
            path
        } else {
            std::env::temp_dir().join("mpvne-debug.log")
        };
        std::fs::File::create(&writable).ok()
    } else {
        None
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("mpv_ne=debug,fontdb=error,iced_wgpu=warn,iced_winit=warn")
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::sync::Mutex::new(DebugWriter { file: log_file, ring: debug_log::RingWriter::default() }))
        .with_ansi(false)
        .init();

    // Single-instance mode (off by default - see Settings::interface):
    // if another instance is already running, hand off whatever file/URL
    // was passed on the command line to it and exit immediately, before
    // creating any window or touching mpv at all.
    #[cfg(target_os = "windows")]
    {
        if prefs.interface.single_instance {
            if let Some(other_hwnd) = win32_modal::try_claim_single_instance() {
                if let Some(path) = std::env::args().nth(1) {
                    win32_modal::send_open_file_to(other_hwnd, &path);
                } else {
                    win32_modal::send_open_file_to(other_hwnd, "");
                }
                return Ok(());
            }
        }
    }

    // `daemon` (rather than `application`) because the floating main-menu
    // popup needs to be a genuine second OS window with its own content —
    // `application`'s view has no window::Id parameter, so every window
    // would render identically. Daemons don't open a window automatically;
    // MpvNe::boot() opens the main one explicitly as its first Task.
    iced::daemon(MpvNe::boot, MpvNe::update, MpvNe::view)
        .title(MpvNe::title)
        .theme(MpvNe::theme)
        .subscription(MpvNe::subscription)
        .run()
}
