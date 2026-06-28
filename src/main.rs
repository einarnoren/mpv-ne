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

    let prefs = settings::Settings::load();
    let (w, h) = prefs.window_size().unwrap_or((1280.0, 720.0));
    let position = prefs.window_x
        .zip(prefs.window_y)
        .map(|(x, y)| iced::window::Position::Specific(iced::Point::new(x as f32, y as f32)))
        .unwrap_or(iced::window::Position::Centered);

    let icon = iced::window::icon::from_file_data(
        include_bytes!("../assets/MPV_NE_icon_hires.png"),
        None,
    ).ok();

    iced::application(MpvNe::default, MpvNe::update, MpvNe::view)
        .title(MpvNe::title)
        .theme(MpvNe::theme)
        .subscription(MpvNe::subscription)
        .exit_on_close_request(false)
        .window(iced::window::Settings {
            size: iced::Size::new(w, h),
            position,
            min_size: Some(iced::Size::new(480.0, 160.0)),
            decorations: !app::USE_CUSTOM_TITLE_BAR,
            icon,
            ..Default::default()
        })
        .run()
}
