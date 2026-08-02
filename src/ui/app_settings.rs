//! Standalone App Settings window - Interface/Keyboard/... in a left-nav
//! layout (VLC/PotPlayer-style "preferences" dialog), separate from the
//! docked side panel's Settings tab, which stays playback-only. Always its
//! own OS window - unlike the side panel, there's no dock/undock state.

use iced::alignment::Vertical;
use iced::{
    Color, Element, Length,
    widget::{Space, button, column, container, mouse_area, row, scrollable, text},
};

use super::edge_grips::EdgeGrips;
use super::icons;
use super::{accent_green, accent_purple, accent_teal, bg_button, bg_deepest, bg_hover, bg_surface, text_bright, text_muted};
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
    minimize_to_tray: bool,
    auto_retry_download: bool,
    gl_render: bool,
    theme: crate::ui::theme::Theme,
    custom_colors: Vec<String>,
    custom_picker: Option<usize>,
    custom_import: Option<String>,
    /// HSL of the slot being edited, as bits so the snapshot stays hashable.
    custom_hsl: (u32, u32, u32),
    /// Resolved key per `KEY_SLOTS` entry, in the same order - `None` means
    /// that slot is explicitly unbound.
    keybind_keys: Vec<Option<String>>,
    rebind_capture: Option<&'static str>,
    mouse_single_click: String,
    mouse_double_click: String,
    mouse_scroll_up: String,
    mouse_scroll_down: String,
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
            minimize_to_tray: app.minimize_to_tray,
            auto_retry_download: app.auto_retry_download,
            gl_render: app.gl_render,
            theme: app.theme,
            custom_colors: app.custom_colors.clone(),
            custom_picker: app.custom_picker,
            custom_import: app.custom_import.clone(),
            custom_hsl: (
                app.custom_hsl.0.to_bits(),
                app.custom_hsl.1.to_bits(),
                app.custom_hsl.2.to_bits(),
            ),
            keybind_keys: KEY_SLOTS.iter()
                .map(|(id, ..)| app.resolved_key_for_slot(id))
                .collect(),
            rebind_capture: app.rebind_capture,
            mouse_single_click: app.mouse_bindings.single_click.clone(),
            mouse_double_click: app.mouse_bindings.double_click.clone(),
            mouse_scroll_up: app.mouse_bindings.scroll_up.clone(),
            mouse_scroll_down: app.mouse_bindings.scroll_down.clone(),
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

    // Always wrapped, never conditionally: adding or removing a node above
    // the scrollable invalidates its state and snaps it back to the top.
    // Only the message it emits varies.
    let dismiss = if app.custom_picker.is_some() {
        Message::CloseColorPicker
    } else {
        Message::Noop
    };
    let inner: Element<'_, Message> =
        iced::widget::mouse_area(inner).on_press(dismiss).into();

    let outer = container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        // Children paint over the container's own border, so inset them by
        // the stroke width or the outline is drawn and then covered up.
        .padding(1)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(bg_deepest())),
            border: iced::Border {
                color: crate::ui::theme::border(),
                width: 1.0,
                ..Default::default()
            },
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
    let logo = iced::widget::svg(app.img_icon.clone())
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0));
    let logo_btn = container(logo)
        .padding(iced::Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 2.0 })
        .height(Length::Fill)
        .align_y(Vertical::Center);

    let drag_region = mouse_area(super::title_region("Settings".to_string()))
        .on_press(Message::AppSettingsDragWindow);

    let min_btn = icons::tipped_below(
        icons::square_btn(icons::window_minimize()).on_press(Message::AppSettingsMinimize),
        "Minimize",
    );
    let max_btn = icons::tipped_below(
        icons::square_btn(icons::window_maximize()).on_press(Message::AppSettingsToggleMaximize),
        "Maximize",
    );
    let close_btn = icons::tipped_below(
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
        background: Some(iced::Background::Color(bg_surface())),
        ..Default::default()
    })
    .into()
}

fn nav(app: &MpvNe) -> Element<'_, Message> {
    const ITEMS: &[(&str, AppSettingsCategory)] = &[
        ("Interface", AppSettingsCategory::Interface),
        ("Keyboard", AppSettingsCategory::Keyboard),
        ("Mouse", AppSettingsCategory::Mouse),
    ];

    let buttons: Vec<Element<'_, Message>> = ITEMS
        .iter()
        .map(|(label, cat)| {
            let active = app.app_settings_category == *cat;
            let btn = container(text(*label).size(13).color(if active { accent_teal() } else { text_bright() }))
                .padding([8, 14])
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(if active { bg_hover() } else { Color::TRANSPARENT })),
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
            background: Some(iced::Background::Color(bg_surface())),
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
        AppSettingsCategory::Mouse => {
            container(mouse_category(snap))
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
        toggle_row("Minimize to system tray", Some("Minimizing hides the window to a tray icon instead of the taskbar"), app.minimize_to_tray, Message::ToggleMinimizeToTray),
        toggle_row("Auto-load folder as playlist", Some("Queue other media files from the same folder when opening a file"), app.auto_load_siblings, Message::ToggleAutoLoadSiblings),
        toggle_row("Single instance", Some("Opening another file hands it off to the running window instead of starting a new one - requires restart"), app.single_instance, Message::ToggleSingleInstance),
        toggle_row("Auto-retry failed URLs via download", Some("If a URL fails to open directly, automatically retry it via yt-dlp download instead of just failing"), app.auto_retry_download, Message::ToggleAutoRetryDownload),
        toggle_row("GPU video rendering", Some("Render video on the GPU (OpenGL) instead of the CPU - much smoother for 4K. Takes effect on restart; falls back to CPU automatically if unsupported"), app.gl_render, Message::ToggleGlRender),
    ]
    .spacing(0)
    .width(Length::Fill);

    column![
        text("Interface").size(16).color(text_bright()),
        text("General app behavior - playback-specific settings live in the side panel's Settings tab.")
            .size(12)
            .color(text_muted()),
        gap(),
        container(rows)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(bg_surface())),
                border: iced::Border { radius: iced::border::Radius::new(6.0), ..Default::default() },
                ..Default::default()
            }),
        gap(),
        settings_section(
            "Theme",
            "Colour scheme for the whole app - applies immediately",
            theme_picker(app),
        ),
        gap(),
        settings_section(
            "Restart",
            "Some settings (like GPU video rendering) only take effect after a restart - this reopens the app where you left off",
            action_btn("Restart now", Message::RestartApp, accent_green()),
        ),
        gap(),
        settings_section(
            "File associations",
            "Register MPV-NE as an option in \"Open with\", then pick it as default in the Windows settings that open",
            action_btn("Register file associations", Message::RegisterFileAssociations, accent_teal()),
        ),
    ]
    .spacing(0)
    .into()
}

/// Row of theme buttons, active one highlighted. Small enough a list makes
/// more sense than a dropdown, and it lets each name stay readable.
fn theme_picker(app: &AppSettingsSnapshot) -> Element<'static, Message> {
    use crate::ui::theme::Theme;
    let (active, custom, open, hsl) =
        (app.theme, &app.custom_colors, app.custom_picker, app.custom_hsl);
    let btn = |t: Theme| {
        let is_active = t == active;
        let base = if is_active { accent_green() } else { text_muted() };
        button(text(t.label()).size(11))
            .padding([4, 10])
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => bg_hover(),
                    _ => if is_active { bg_hover() } else { bg_button() },
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: legible(base, bg),
                    border: iced::Border {
                        color: if is_active { Color { a: 0.4, ..accent_green() } } else { Color::TRANSPARENT },
                        width: if is_active { 1.0 } else { 0.0 },
                        radius: iced::border::Radius::new(4.0),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetTheme(t))
    };
    let picker = row(Theme::ALL.iter().map(|t| btn(*t).into()))
        .spacing(4)
        .wrap();

    // The colour editor only appears for the Custom theme - it'd just be
    // noise while a built-in one is selected.
    if active != Theme::Custom {
        return picker.into();
    }

    let label_c = crate::ui::theme::ensure_contrast(text_muted(), bg_surface(), 4.5);

    // One editable colour: label, swatch (which opens the picker) and hex.
    let field = |idx: usize| -> Element<'static, Message> {
        let value = custom.get(idx).cloned().unwrap_or_default();
        let swatch = container(Space::new().width(Length::Fixed(18.0)).height(Length::Fixed(18.0)))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(
                    crate::ui::theme::custom_slot(idx),
                )),
                border: iced::Border {
                    color: text_muted(),
                    width: 1.0,
                    radius: iced::border::Radius::new(3.0),
                },
                ..Default::default()
            });
        // The swatch doubles as the picker toggle - typing hex stays
        // available for anyone who knows the value they want.
        let swatch = button(swatch)
            .padding(0)
            .style(|_, _| iced::widget::button::Style::default())
            .on_press(Message::ToggleColorPicker(idx));

        let head = row![
            text(crate::ui::theme::slot_label(idx))
                .size(11)
                .color(label_c)
                .width(Length::Fixed(96.0)),
            swatch,
            iced::widget::text_input("#RRGGBB", &value)
                .on_input(move |s| Message::SetCustomColor(idx, s))
                .size(11)
                .padding([3, 6])
                .width(Length::Fixed(96.0)),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        if open != Some(idx) {
            return head.into();
        }
        column![head, swatch_grid(idx, hsl)].spacing(6).into()
    };

    // Grouped as plain sections, all visible at once - collapsing them hid
    // colours that are usually chosen in relation to each other.
    let groups = crate::ui::theme::CUSTOM_GROUPS.iter().map(|(name, slots)| {
        let rows = slots.iter().map(|i| field(*i));
        column![
            text(*name).size(10).color(accent_green()),
            column(rows).spacing(4),
        ]
        .spacing(4)
        .into()
    });

    // Palette as one line: copy it out to keep, paste one back to restore.
    // Also the quickest undo there is after a run of bad edits.
    let import_value = app
        .custom_import
        .clone()
        .unwrap_or_else(crate::ui::theme::export_custom);
    let import = column![
        text("Palette - copy to save, paste to load").size(10).color(label_c),
        row![
            iced::widget::text_input("", &import_value)
                .on_input(Message::ImportCustomPalette)
                .size(10)
                .padding([3, 6])
                .width(Length::Fill),
            small_btn("Reset all", Message::ResetAllCustomColors),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(4);

    column![picker, preview_strip(), column(groups).spacing(10), import]
        .spacing(10)
        .into()
}

fn small_btn(label: &'static str, msg: Message) -> iced::widget::Button<'static, Message> {
    button(text(label).size(10))
    .padding([3, 8])
    .style(|_, status| {
        use iced::widget::button::Status;
        let bg = if matches!(status, Status::Hovered | Status::Pressed) {
            bg_hover()
        } else {
            bg_button()
        };
        iced::widget::button::Style {
            background: Some(iced::Background::Color(bg)),
            text_color: legible(text_muted(), bg),
            border: iced::Border {
                radius: iced::border::Radius::new(3.0),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .on_press(msg)
}

/// A miniature of the app drawn in the colours being edited, so their effect
/// is visible without going and finding the real thing.
fn preview_strip() -> Element<'static, Message> {
    use crate::ui::theme as th;
    let chip = |c: Color, w: f32| {
        container(Space::new().width(Length::Fixed(w)).height(Length::Fixed(12.0)))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    radius: iced::border::Radius::new(2.0),
                    ..Default::default()
                },
                ..Default::default()
            })
    };
    // Stands in for a control: button fill, hover fill, and an icon on each.
    let fake_btn = |fill: Color| {
        container(chip(th::icon(), 10.0))
            .padding(5)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(fill)),
                border: iced::Border {
                    radius: iced::border::Radius::new(3.0),
                    ..Default::default()
                },
                ..Default::default()
            })
    };

    let bar = container(
        row![
            chip(th::accent_green(), 12.0),
            text("MPV-NE").size(10).color(th::text_bright()),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding([4, 8])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(th::bg_surface())),
        border: iced::Border {
            color: th::border(),
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    });

    let body = container(
        column![
            row![fake_btn(th::bg_button()), fake_btn(th::bg_hover())].spacing(5),
            text("Body text").size(10).color(th::text_bright()),
            text("Muted text").size(9).color(th::text_muted()),
            row![
                chip(th::accent_green(), 26.0),
                chip(th::accent_teal(), 26.0),
                chip(th::accent_purple(), 26.0),
            ]
            .spacing(4),
        ]
        .spacing(5),
    )
    .padding(8)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(th::bg_deepest())),
        ..Default::default()
    });

    container(column![bar, body])
        .width(Length::Fixed(230.0))
        .style(|_| container::Style {
            border: iced::Border {
                color: th::border(),
                width: 1.0,
                radius: iced::border::Radius::new(5.0),
            },
            ..Default::default()
        })
        .into()
}

/// Clickable colours for one slot: the built-in themes' take on it first,
/// then a general palette.
fn swatch_grid(idx: usize, hsl: (u32, u32, u32)) -> Element<'static, Message> {
    let cell = move |c: Color| {
        button(Space::new().width(Length::Fixed(20.0)).height(Length::Fixed(20.0)))
            .padding(0)
            .style(move |_, status| {
                use iced::widget::button::Status;
                let hot = matches!(status, Status::Hovered | Status::Pressed);
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(c)),
                    border: iced::Border {
                        color: if hot { text_bright() } else { Color { a: 0.35, ..text_muted() } },
                        width: if hot { 2.0 } else { 1.0 },
                        radius: iced::border::Radius::new(3.0),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetCustomColor(idx, crate::ui::theme::to_hex(c)))
            .into()
    };

    let from_themes = row(crate::ui::theme::theme_swatches(idx).into_iter().map(cell))
        .spacing(4)
        .wrap();
    let general = row(crate::ui::theme::SWATCHES
        .iter()
        .map(|p| cell(crate::ui::theme::swatch_color(*p))))
        .spacing(4)
        .width(Length::Fixed(200.0))
        .wrap();

    // Everything in this panel is drawn on bg_deepest, which the user may
    // have just set to anything at all. Force these to stay readable.
    let on_panel = |c: Color| crate::ui::theme::ensure_contrast(c, bg_deepest(), 4.5);
    let label_c = on_panel(text_muted());

    let (h, sat, light) = (
        f32::from_bits(hsl.0),
        f32::from_bits(hsl.1),
        f32::from_bits(hsl.2),
    );

    // Hue gets a rainbow rail so the slider shows what it selects; the other
    // two ramp from the current colour's grey to its full-strength form.
    let rainbow = {
        let mut g = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2));
        for i in 0..=6 {
            let t = i as f32 / 6.0;
            g = g.add_stop(t, crate::ui::theme::from_hsl(t * 360.0, 1.0, 0.5));
        }
        iced::Background::Gradient(iced::Gradient::Linear(g))
    };
    let ramp = |from: Color, to: Color| {
        let g = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
            .add_stop(0.0, from)
            .add_stop(1.0, to);
        iced::Background::Gradient(iced::Gradient::Linear(g))
    };

    let axis = move |label: &'static str, value: f32, max: f32, axis: u8, fill: iced::Background| {
        let handle_c = crate::ui::theme::from_hsl(h, sat, light);
        row![
            text(label).size(10).color(label_c).width(Length::Fixed(20.0)),
            iced::widget::slider(0.0..=max, value, move |v| Message::SetCustomHsl(axis, v))
                .step(if max > 10.0 { 1.0 } else { 0.01 })
                .width(Length::Fixed(150.0))
                .style(move |_, _| {
                    use iced::widget::slider::{Handle, HandleShape, Rail, Style};
                    Style {
                        rail: Rail {
                            backgrounds: (fill.clone(), fill.clone()),
                            width: 6.0,
                            border: iced::Border {
                                radius: iced::border::Radius::new(3.0),
                                color: Color { a: 0.35, ..text_muted() },
                                width: 1.0,
                            },
                        },
                        handle: Handle {
                            shape: HandleShape::Circle { radius: 6.0 },
                            background: iced::Background::Color(handle_c),
                            border_width: 2.0,
                            border_color: text_bright(),
                        },
                    }
                }),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
    };

    let sliders = column![
        axis("H", h, 360.0, 0, rainbow),
        axis("S", sat, 1.0, 1, ramp(
            crate::ui::theme::from_hsl(h, 0.0, light),
            crate::ui::theme::from_hsl(h, 1.0, light),
        )),
        axis("L", light, 1.0, 2, ramp(Color::BLACK, Color::WHITE)),
    ]
    .spacing(4);

    // Informational only - the worst pairing this slot lands in. No
    // threshold and no warning; it's there if you want it.
    let note: Element<'static, Message> =
        match crate::ui::theme::custom_contrast_worst(idx) {
            Some(ratio) => text(format!("contrast {ratio:.1}:1"))
                .size(10)
                .color(label_c)
                .into(),
            None => Space::new().into(),
        };

    let reset = button(text("Reset").size(10).color(on_panel(text_muted())))
        .padding([2, 8])
        .style(|_, status| {
            use iced::widget::button::Status;
            iced::widget::button::Style {
                background: Some(iced::Background::Color(
                    if matches!(status, Status::Hovered | Status::Pressed) {
                        bg_hover()
                    } else {
                        bg_button()
                    },
                )),
                border: iced::Border {
                    radius: iced::border::Radius::new(3.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::ResetCustomColor(idx));

    let inner = iced::widget::mouse_area(
        column![
            sliders,
            row![note, Space::new().width(Length::Fill), reset]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            text("From themes").size(10).color(label_c),
            from_themes,
            text("Palette").size(10).color(label_c),
            general,
        ]
        .spacing(5),
    )
    .on_press(Message::Noop);

    container(inner)
    .padding(8)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(bg_deepest())),
        border: iced::Border {
            color: Color { a: 0.5, ..text_muted() },
            width: 1.0,
            radius: iced::border::Radius::new(5.0),
        },
        ..Default::default()
    })
    .into()
}

fn gap<'a>() -> Element<'a, Message> {
    Space::new().height(Length::Fixed(10.0)).width(Length::Fill).into()
}

/// A boxed section with a label, a small muted explanation line, and
/// arbitrary content below - used for things that aren't a plain toggle
/// (e.g. an action button) so they don't have to squeeze into `toggle_row`.
fn settings_section(label: &'static str, subtext: &'static str, content: Element<'static, Message>) -> Element<'static, Message> {
    container(
        column![
            text(label).size(12).color(text_bright()),
            text(subtext).size(10).color(text_muted()),
            content,
        ]
        .spacing(8),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(bg_surface())),
        border: iced::Border { radius: iced::border::Radius::new(6.0), ..Default::default() },
        ..Default::default()
    })
    .into()
}

/// One compact row: label (+ optional muted note) on the left, a small
/// On/Off text button on the right. Rows sit flush together with a hairline
/// divider between them, matching a typical OS settings list rather than a
/// boxed section per toggle - the boxed-per-toggle layout ate a lot of
/// vertical space for very little content per box.
fn toggle_row(label: &'static str, note: Option<&'static str>, active: bool, msg: Message) -> Element<'static, Message> {
    let label_col: Element<'static, Message> = if let Some(note) = note {
        column![
            text(label).size(12).color(text_bright()),
            text(note).size(10).color(text_muted()),
        ]
        .spacing(2)
        .into()
    } else {
        text(label).size(12).color(text_bright()).into()
    };

    // Give the label/description column the flexible width and a small gap
    // before the button, so long descriptions wrap *within* their column
    // instead of running underneath the On/Off button on the right.
    let row_content = row![
        container(label_col).width(Length::Fill),
        Space::new().width(Length::Fixed(12.0)),
        onoff_btn(active, msg),
    ]
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    container(row_content)
        .padding([9, 14])
        .width(Length::Fill)
        .style(|_| container::Style {
            border: iced::Border { color: bg_deepest(), width: 1.0, radius: iced::border::Radius::new(0.0) },
            ..Default::default()
        })
        .into()
}

/// Compact On/Off text button - same idea as the side panel's toggle
/// buttons, just tighter padding to suit a dense settings list.
fn onoff_btn(active: bool, msg: Message) -> Element<'static, Message> {
    let base = if active { accent_green() } else { text_muted() };
    button(text(if active { "On" } else { "Off" }).size(11))
        .padding([4, 10])
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => bg_hover(),
                _ => if active { bg_hover() } else { bg_button() },
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: legible(base, bg),
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    color: if active { accent_green() } else { Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

/// A label colour guaranteed to read against the fill behind it. Accents are
/// free-form under a custom theme, so an accent-on-button-fill label can end
/// up drawn in exactly the fill's own colour and disappear.
fn legible(fg: Color, bg: Color) -> Color {
    crate::ui::theme::ensure_contrast(fg, bg, 4.5)
}

fn action_btn<'a>(label: &'static str, msg: Message, color: Color) -> Element<'a, Message> {
    // No explicit colour on the text: the button's `text_color` is inherited
    // and recomputed per status, so the label follows the hover fill too.
    button(text(label).size(11))
        .padding([4, 10])
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => bg_hover(),
                _ => bg_button(),
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: legible(color, bg),
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
        text("Keyboard").size(16).color(text_bright()),
        Space::new().width(Length::Fill),
        if any_overridden {
            action_btn("Reset all", Message::ResetAllKeybindings, accent_purple())
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
            .color(text_muted()),
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
        text("Press a key…").size(12).color(accent_teal()).into()
    } else {
        match key {
            Some(k) => container(text(display_key(&k)).size(11).color(text_bright()))
                .padding([3, 8])
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(bg_button())),
                    border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                    ..Default::default()
                })
                .into(),
            None => text("Unbound").size(11).color(text_muted()).into(),
        }
    };

    let rebind_btn = action_btn(
        if capturing { "Cancel" } else { "Rebind" },
        if capturing { Message::CancelRebind } else { Message::StartRebind(slot_id) },
        if capturing { accent_purple() } else { accent_teal() },
    );

    let mut controls = row![key_display, rebind_btn].spacing(8).align_y(iced::Alignment::Center);
    if overridden && !capturing {
        controls = controls.push(action_btn("Reset", Message::ResetRebind(slot_id), text_muted()));
    }

    container(
        row![
            text(label).size(12).color(text_bright()),
            Space::new().width(Length::Fill),
            controls,
        ]
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
    )
    .padding([8, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(bg_surface())),
        border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
        ..Default::default()
    })
    .into()
}

/// Friendlier display for a few key names that read awkwardly raw.
fn mouse_category(app: &AppSettingsSnapshot) -> Element<'static, Message> {
    use crate::app::{MouseTrigger, MOUSE_ACTION_PRESETS};

    let row_for = |label: &'static str, trigger: MouseTrigger, current_id: &str| -> Element<'static, Message> {
        let options: Vec<&'static str> = MOUSE_ACTION_PRESETS.iter().map(|(_, label, _)| *label).collect();
        let current_label = MOUSE_ACTION_PRESETS.iter()
            .find(|(id, ..)| *id == current_id)
            .map(|(_, label, _)| *label)
            .unwrap_or("Unbound");

        let picker = iced::widget::pick_list(
            options,
            Some(current_label),
            move |chosen_label: &'static str| {
                let id = MOUSE_ACTION_PRESETS.iter()
                    .find(|(_, label, _)| *label == chosen_label)
                    .map(|(id, ..)| *id)
                    .unwrap_or("none");
                Message::SetMouseBinding(trigger, id)
            },
        )
        .text_size(12)
        .padding([5, 10])
        .width(Length::Fixed(150.0));

        container(
            row![
                text(label).size(12).color(text_bright()),
                Space::new().width(Length::Fill),
                picker,
            ]
            .align_y(iced::Alignment::Center)
            .width(Length::Fill),
        )
        .padding([9, 14])
        .width(Length::Fill)
        .style(|_| container::Style {
            border: iced::Border { color: bg_deepest(), width: 1.0, radius: iced::border::Radius::new(0.0) },
            ..Default::default()
        })
        .into()
    };

    let rows = column![
        row_for("Single click", MouseTrigger::SingleClick, &app.mouse_single_click),
        row_for("Double click", MouseTrigger::DoubleClick, &app.mouse_double_click),
        row_for("Scroll up", MouseTrigger::ScrollUp, &app.mouse_scroll_up),
        row_for("Scroll down", MouseTrigger::ScrollDown, &app.mouse_scroll_down),
    ]
    .spacing(0)
    .width(Length::Fill);

    column![
        text("Mouse").size(16).color(text_bright()),
        text("What each mouse action does when clicked/scrolled over the video.")
            .size(12)
            .color(text_muted()),
        gap(),
        container(rows)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(bg_surface())),
                border: iced::Border { radius: iced::border::Radius::new(6.0), ..Default::default() },
                ..Default::default()
            }),
    ]
    .spacing(0)
    .width(Length::Fill)
    .into()
}

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
