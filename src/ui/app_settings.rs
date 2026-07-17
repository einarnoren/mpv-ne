//! Standalone App Settings window - Interface/Keyboard/... in a left-nav
//! layout (VLC/PotPlayer-style "preferences" dialog), separate from the
//! docked side panel's Settings tab, which stays playback-only. Always its
//! own OS window - unlike the side panel, there's no dock/undock state.

use iced::alignment::Vertical;
use iced::{
    Color, Element, Length,
    widget::{Space, button, column, container, image, mouse_area, row, scrollable, text},
};

use super::edge_grips::EdgeGrips;
use super::icons;
use super::{AURORA_GREEN, AURORA_PURPLE, AURORA_TEAL, BG_BUTTON, BG_DEEPEST, BG_HOVER, BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED};
use crate::app::{AppSettingsCategory, Message, MpvNe, KEY_SLOTS};

/// Everything this window's content reads, copied out of `MpvNe` so it can
/// be memoized via `iced::widget::lazy` - see settings.rs's `SettingsSnapshot`
/// doc comment for why: every video frame otherwise forced a full rebuild of
/// this window too, whether or not anything in it actually changed.
#[derive(Debug, Clone, Hash)]
struct AppSettingsSnapshot {
    category: AppSettingsCategory,
    resume_enabled: bool,
    snap_to_edge: bool,
    drag_anywhere: bool,
    remember_window: bool,
    start_pinned_pref: bool,
    osd_enabled: bool,
    thumbnail_preview: bool,
    custom_title_bar_pref: bool,
    auto_update_ytdlp: bool,
    hide_all_on_minimize: bool,
    pause_on_focus_lost: bool,
    pause_on_minimize: bool,
    auto_load_siblings: bool,
    single_instance: bool,
    /// Resolved key per `KEY_SLOTS` entry, in the same order - `None` means
    /// that slot is explicitly unbound.
    keybind_keys: Vec<Option<String>>,
    rebind_capture: Option<&'static str>,
}

impl AppSettingsSnapshot {
    fn from_app(app: &MpvNe) -> Self {
        Self {
            category: app.app_settings_category,
            resume_enabled: app.resume_enabled,
            snap_to_edge: app.snap_to_edge,
            drag_anywhere: app.bindings.drag_window_anywhere,
            remember_window: app.remember_window,
            start_pinned_pref: app.start_pinned_pref,
            osd_enabled: app.osd_enabled,
            thumbnail_preview: app.thumbnail_preview,
            custom_title_bar_pref: app.custom_title_bar_pref,
            auto_update_ytdlp: app.auto_update_ytdlp,
            hide_all_on_minimize: app.hide_all_on_minimize,
            pause_on_focus_lost: app.pause_on_focus_lost,
            pause_on_minimize: app.pause_on_minimize,
            auto_load_siblings: app.auto_load_siblings,
            single_instance: app.single_instance,
            keybind_keys: KEY_SLOTS.iter()
                .map(|(id, ..)| app.resolved_key_for_slot(id))
                .collect(),
            rebind_capture: app.rebind_capture,
        }
    }
}

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    let body = row![nav(app), content(app)]
        .width(Length::Fill)
        .height(Length::Fill);

    let inner: Element<'_, Message> = if crate::app::use_custom_title_bar() {
        column![title_bar(app), body].width(Length::Fill).height(Length::Fill).into()
    } else {
        body.into()
    };

    let outer = container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_DEEPEST)),
            ..Default::default()
        });

    EdgeGrips::new(outer)
        .enabled(crate::app::use_custom_title_bar())
        .into()
}

/// Mirrors `panel_title_bar`'s structure (logo, height, padding) so this
/// window reads as part of the same app rather than a bolted-on dialog. No
/// dock button - this window is never dockable.
fn title_bar(app: &MpvNe) -> Element<'_, Message> {
    let logo = image(app.img_icon.clone())
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0));
    let logo_btn = container(logo)
        .padding(iced::Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 2.0 })
        .height(Length::Fill)
        .align_y(Vertical::Center);

    let title_label = text("Settings").size(13).color(TEXT_BRIGHT);
    let drag_region = mouse_area(
        container(title_label)
            .padding([0, 8])
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Vertical::Center),
    )
    .on_press(Message::AppSettingsDragWindow);

    let min_btn = icons::tipped(
        icons::square_btn(icons::window_minimize()).on_press(Message::AppSettingsMinimize),
        "Minimize",
    );
    let max_btn = icons::tipped(
        icons::square_btn(icons::window_maximize()).on_press(Message::AppSettingsToggleMaximize),
        "Maximize",
    );
    let close_btn = icons::tipped(
        icons::square_btn(icons::window_close()).on_press(Message::CloseAppSettingsWindow),
        "Close",
    );
    let buttons = row![min_btn, max_btn, close_btn]
        .spacing(8)
        .align_y(iced::Alignment::Center);

    container(
        row![logo_btn, drag_region, buttons]
            .spacing(8)
            .align_y(iced::Alignment::Center)
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

fn nav(app: &MpvNe) -> Element<'_, Message> {
    const ITEMS: &[(&str, AppSettingsCategory)] = &[
        ("Interface", AppSettingsCategory::Interface),
        ("Keyboard", AppSettingsCategory::Keyboard),
    ];

    let buttons: Vec<Element<'_, Message>> = ITEMS
        .iter()
        .map(|(label, cat)| {
            let active = app.app_settings_category == *cat;
            let btn = container(text(*label).size(13).color(if active { AURORA_TEAL } else { TEXT_BRIGHT }))
                .padding([8, 14])
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(if active { BG_HOVER } else { Color::TRANSPARENT })),
                    border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                    ..Default::default()
                });
            mouse_area(btn).on_press(Message::AppSettingsCategorySelect(*cat)).into()
        })
        .collect();

    container(column(buttons).spacing(2).padding(8).width(Length::Fill))
        .width(Length::Fixed(160.0))
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            ..Default::default()
        })
        .into()
}

fn content(app: &MpvNe) -> Element<'_, Message> {
    let snapshot = AppSettingsSnapshot::from_app(app);
    iced::widget::lazy(snapshot, |snap| -> Element<'static, Message> { match snap.category {
        AppSettingsCategory::Interface => {
            // A stable id, same reasoning as the side panel's settings_scroll -
            // without one iced can lose track of the scroll offset across
            // rebuilds and snap back to the top.
            scrollable(container(interface_category(snap)).width(Length::Fill).padding(20))
                .id("app_settings_interface_scroll")
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
        AppSettingsCategory::Keyboard => {
            container(keyboard_category(snap))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(20)
                .into()
        }
    }})
    .into()
}

fn interface_category(app: &AppSettingsSnapshot) -> Element<'static, Message> {
    let rows = column![
        toggle_row("Resume playback", None, app.resume_enabled, Message::ToggleResume),
        toggle_row("Window snapping", Some("Snap to screen edges and other MPV-NE windows while dragging"), app.snap_to_edge, Message::ToggleSnapToEdge),
        toggle_row("Drag window from anywhere", Some("Click-drag empty video area to move the window, not just the title bar"), app.drag_anywhere, Message::ToggleDragAnywhere),
        toggle_row("Remember window position/size", None, app.remember_window, Message::ToggleRememberWindow),
        toggle_row("Start pinned (always on top)", None, app.start_pinned_pref, Message::ToggleStartPinned),
        toggle_row("OSD notifications", Some("On-screen popups for volume, seek, speed, and similar changes"), app.osd_enabled, Message::ToggleOsdEnabled),
        toggle_row("Seekbar thumbnail preview", Some("Video preview when hovering the seek bar"), app.thumbnail_preview, Message::ToggleThumbnailPreview),
        toggle_row("Custom title bar", Some("App-drawn top bar instead of the OS one - requires restart"), app.custom_title_bar_pref, Message::ToggleCustomTitleBar),
        toggle_row("Auto-update yt-dlp", Some("Re-download the latest yt-dlp at every startup"), app.auto_update_ytdlp, Message::ToggleAutoUpdateYtdlp),
        toggle_row("Hide all windows when minimized", Some("Minimize the detached panel and Settings windows together with the main window"), app.hide_all_on_minimize, Message::ToggleHideAllOnMinimize),
        toggle_row("Pause when window loses focus", None, app.pause_on_focus_lost, Message::TogglePauseOnFocusLost),
        toggle_row("Pause when minimized", None, app.pause_on_minimize, Message::TogglePauseOnMinimize),
        toggle_row("Auto-load folder as playlist", Some("Queue other media files from the same folder when opening a file"), app.auto_load_siblings, Message::ToggleAutoLoadSiblings),
        toggle_row("Single instance", Some("Opening another file hands it off to the running window instead of starting a new one - requires restart"), app.single_instance, Message::ToggleSingleInstance),
    ]
    .spacing(0)
    .width(Length::Fill);

    column![
        text("Interface").size(16).color(TEXT_BRIGHT),
        text("General app behavior - playback-specific settings live in the side panel's Settings tab.")
            .size(12)
            .color(TEXT_MUTED),
        gap(),
        container(rows)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG_SURFACE)),
                border: iced::Border { radius: iced::border::Radius::new(6.0), ..Default::default() },
                ..Default::default()
            }),
    ]
    .spacing(0)
    .into()
}

fn gap<'a>() -> Element<'a, Message> {
    Space::new().height(Length::Fixed(10.0)).width(Length::Fill).into()
}

/// One compact row: label (+ optional muted note) on the left, a small
/// On/Off text button on the right. Rows sit flush together with a hairline
/// divider between them, matching a typical OS settings list rather than a
/// boxed section per toggle - the boxed-per-toggle layout ate a lot of
/// vertical space for very little content per box.
fn toggle_row(label: &'static str, note: Option<&'static str>, active: bool, msg: Message) -> Element<'static, Message> {
    let label_col: Element<'static, Message> = if let Some(note) = note {
        column![
            text(label).size(12).color(TEXT_BRIGHT),
            text(note).size(10).color(TEXT_MUTED),
        ]
        .spacing(2)
        .into()
    } else {
        text(label).size(12).color(TEXT_BRIGHT).into()
    };

    let row_content = row![
        label_col,
        Space::new().width(Length::Fill),
        onoff_btn(active, msg),
    ]
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    container(row_content)
        .padding([9, 14])
        .width(Length::Fill)
        .style(|_| container::Style {
            border: iced::Border { color: BG_DEEPEST, width: 1.0, radius: iced::border::Radius::new(0.0) },
            ..Default::default()
        })
        .into()
}

/// Compact On/Off text button - same idea as the side panel's toggle
/// buttons, just tighter padding to suit a dense settings list.
fn onoff_btn(active: bool, msg: Message) -> Element<'static, Message> {
    let text_color = if active { AURORA_GREEN } else { TEXT_MUTED };
    button(text(if active { "On" } else { "Off" }).size(11).color(text_color))
        .padding([4, 10])
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => if active { BG_HOVER } else { BG_BUTTON },
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color,
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    color: if active { AURORA_GREEN } else { Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn action_btn<'a>(label: &'static str, msg: Message, color: Color) -> Element<'a, Message> {
    button(text(label).size(11).color(color))
        .padding([4, 10])
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_BUTTON,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: color,
                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn keyboard_category(app: &AppSettingsSnapshot) -> Element<'static, Message> {
    let any_overridden = KEY_SLOTS.iter().zip(app.keybind_keys.iter())
        .any(|((_, _, default_key, _), key)| key.as_deref() != Some(*default_key));

    let rows = KEY_SLOTS.iter().zip(app.keybind_keys.iter()).map(|((slot_id, label, default_key, _), key)| {
        keybind_row(app, slot_id, label, default_key, key.clone())
    });

    let header = row![
        text("Keyboard").size(16).color(TEXT_BRIGHT),
        Space::new().width(Length::Fill),
        if any_overridden {
            action_btn("Reset all", Message::ResetAllKeybindings, AURORA_PURPLE)
        } else {
            Space::new().into()
        },
    ]
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    column![
        header,
        text("Click Rebind, then press the new key. Press Escape to cancel.")
            .size(12)
            .color(TEXT_MUTED),
        scrollable(column(rows).spacing(4).width(Length::Fill))
            .height(Length::Fill),
    ]
    .spacing(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn keybind_row(
    app: &AppSettingsSnapshot,
    slot_id: &'static str,
    label: &'static str,
    default_key: &'static str,
    key: Option<String>,
) -> Element<'static, Message> {
    let capturing = app.rebind_capture == Some(slot_id);
    let overridden = key.as_deref() != Some(default_key);

    let key_display: Element<'static, Message> = if capturing {
        text("Press a key…").size(12).color(AURORA_TEAL).into()
    } else {
        match key {
            Some(k) => container(text(display_key(&k)).size(11).color(TEXT_BRIGHT))
                .padding([3, 8])
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(BG_BUTTON)),
                    border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                    ..Default::default()
                })
                .into(),
            None => text("Unbound").size(11).color(TEXT_MUTED).into(),
        }
    };

    let rebind_btn = action_btn(
        if capturing { "Cancel" } else { "Rebind" },
        if capturing { Message::CancelRebind } else { Message::StartRebind(slot_id) },
        if capturing { AURORA_PURPLE } else { AURORA_TEAL },
    );

    let mut controls = row![key_display, rebind_btn].spacing(8).align_y(iced::Alignment::Center);
    if overridden && !capturing {
        controls = controls.push(action_btn("Reset", Message::ResetRebind(slot_id), TEXT_MUTED));
    }

    container(
        row![
            text(label).size(12).color(TEXT_BRIGHT),
            Space::new().width(Length::Fill),
            controls,
        ]
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
    )
    .padding([8, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
        ..Default::default()
    })
    .into()
}

/// Friendlier display for a few key names that read awkwardly raw.
fn display_key(key: &str) -> String {
    match key {
        "space" => "Space".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "\\" => "\\".into(),
        _ if key.len() == 1 => key.to_uppercase(),
        _ => {
            let mut c = key.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => key.to_string(),
            }
        }
    }
}
