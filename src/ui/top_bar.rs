//! Top bar - acts as a full custom title bar when `USE_CUSTOM_TITLE_BAR` is
//! true (with drag region, pin, fullscreen, minimize, maximize, close). When
//! false it stays as a "below the OS title bar" mini-bar with just pin and
//! fullscreen, leaving the OS to draw min/max/close.

use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Space, container, image, mouse_area, row, stack, text, tooltip},
};

use super::{AURORA_GREEN, AURORA_PURPLE, AURORA_TEAL, BG_DEEPEST, BG_SURFACE, TEXT_BRIGHT, icons};
use crate::app::{Message, MpvNe, USE_CUSTOM_TITLE_BAR};

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    // Title text: always show the current file name (or "MPV-NE" when idle).
    let full_title = app.title_str();

    // Playlist counter shown after title when multiple files are loaded.
    let playlist_counter = if app.playlist.len() > 1 {
        format!("  [{}/{}]", app.playlist_idx + 1, app.playlist.len())
    } else {
        String::new()
    };

    let title_label = text(format!("{}{}", full_title, playlist_counter))
        .size(13)
        .color(TEXT_BRIGHT)
        .wrapping(iced::widget::text::Wrapping::None);

    let title_tip = container(text(format!("{}{}", full_title, playlist_counter)).size(11).color(TEXT_BRIGHT))
        .padding([4, 8])
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_DEEPEST)),
            border: Border {
                radius: iced::border::Radius::new(4.0),
                ..Default::default()
            },
            ..Default::default()
        });

    // We emit DragWindow here unconditionally; the handler in update() defers
    // to edge-grip resize if the cursor happens to be in an edge zone.
    // Fade the title into the background on the right so it never wraps.
    // A 56px gradient overlay (transparent -> BG_SURFACE) sits over the text.
    let fade_bg = Color::from_rgba(
        BG_SURFACE.r, BG_SURFACE.g, BG_SURFACE.b, 0.0,
    );
    let fade = container(Space::new())
        .width(Length::Fixed(56.0))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Gradient(
                iced::Gradient::Linear(
                    iced::gradient::Linear::new(
                        iced::Radians(std::f32::consts::FRAC_PI_2),
                    )
                    .add_stop(0.0, fade_bg)
                    .add_stop(1.0, BG_SURFACE),
                ),
            )),
            ..Default::default()
        });

    let fade_overlay = container(fade)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right);

    let title_stack = stack![
        container(title_label)
            .padding([0, 8])
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true),
        fade_overlay,
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    // Main menu: same content as the video right-click menu, opened at a
    // fixed anchor near this button instead of the cursor.
    let menu_btn = icons::tipped(
        icons::square_toggle(icons::hamburger(), app.menu_window_id.is_some(), AURORA_TEAL)
            .on_press(Message::ToggleMainMenu),
        "Menu",
    );

    let logo = image(app.img_icon.clone())
    .width(Length::Fixed(22.0))
    .height(Length::Fixed(22.0));

    let logo_btn = container(logo)
        .padding(iced::Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 2.0 })
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let drag_region = mouse_area(
        tooltip(
            title_stack,
            title_tip,
            tooltip::Position::Bottom,
        )
        .snap_within_viewport(true),
    )
    .on_press(Message::DragWindow);

    // Focus mode - hides all chrome so it's just the video. Icon shows the
    // *current* state: open eye when chrome is visible (click to enter focus),
    // crossed-out eye when chrome is force-hidden (click to leave focus).
    let focus_glyph = if app.chrome_force_hidden { icons::eye_off() } else { icons::eye() };
    let focus_btn = icons::tipped(
        icons::square_toggle(focus_glyph, app.chrome_force_hidden, AURORA_GREEN)
            .on_press(Message::ToggleChrome),
        "Focus mode (H)",
    );

    let pin_glyph = if app.pinned { icons::pin_active() } else { icons::pin() };
    let pin_btn = icons::tipped(
        icons::square_toggle(pin_glyph, app.pinned, AURORA_PURPLE).on_press(Message::TogglePin),
        "Always on top",
    );

    let pip_btn = icons::tipped(
        icons::square_toggle(icons::pip(), app.pip_active, AURORA_TEAL)
            .on_press(Message::TogglePip),
        "Picture-in-Picture",
    );

    // Fullscreen lives on the bottom controls bar - that's where playback
    // actions belong. Top bar is reserved for window-level toggles.
    let help_btn = icons::tipped(
        icons::square_toggle(icons::help(), app.show_help, AURORA_TEAL)
            .on_press(Message::ShowHelp),
        "Keyboard shortcuts (?)",
    );

    let mut buttons = row![help_btn, focus_btn, pin_btn, pip_btn]
        .spacing(8)
        .align_y(Alignment::Center);

    if app.private_mode {
        let badge = container(text("PRIVATE").size(11).color(Color::WHITE))
            .padding([3, 8])
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.820, 0.290, 0.290))),
                border: Border {
                    radius: iced::border::Radius::new(4.0),
                    ..Default::default()
                },
                ..Default::default()
            });
        buttons = buttons.push(badge);
    }

    if USE_CUSTOM_TITLE_BAR {
        let min_btn = icons::tipped(
            icons::square_btn(icons::window_minimize()).on_press(Message::MinimizeWindow),
            "Minimize",
        );
        let max_btn = icons::tipped(
            icons::square_btn(icons::window_maximize()).on_press(Message::ToggleMaximize),
            "Maximize",
        );
        let close_btn = icons::tipped(
            icons::square_btn(icons::window_close()).on_press(Message::CloseWindow),
            "Close",
        );
        buttons = buttons.push(min_btn).push(max_btn).push(close_btn);
    }

    container(
        row![menu_btn, logo_btn, drag_region, buttons]
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(8)
    .width(Length::Fill)
    .height(Length::Fixed(44.0))
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    })
    .into()
}
