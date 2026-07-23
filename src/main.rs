// Only suppress the console window in release builds so debug runs show output.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
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

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Default: show our debug logs, silence noisy library spam.
                EnvFilter::new("mpv_ne=debug,fontdb=error,iced_wgpu=warn,iced_winit=warn")
            }),
        )
        .init();

    // Single-instance mode (off by default - see Settings::interface):
    // if another instance is already running, hand off whatever file/URL
    // was passed on the command line to it and exit immediately, before
    // creating any window or touching mpv at all.
    #[cfg(target_os = "windows")]
    {
        let prefs = settings::Settings::load();
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
