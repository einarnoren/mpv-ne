mod browser_panel;
mod controls;
mod edge_grips;
mod icons;
mod playlist_panel;
mod recent_panel;
mod settings;
mod top_bar;
pub mod video;

use edge_grips::EdgeGrips;
use crate::app::USE_CUSTOM_TITLE_BAR;

use iced::alignment::{Horizontal, Vertical};
use iced::{
    Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, mouse_area, pin, row, stack, text, tooltip},
};

use crate::app::{Message, MpvNe, PanelKind};

// Darker-than-Nord base palette with aurora (northern lights) accents.
// Bases trend toward a cool charcoal so the aurora colors pop.
pub const BG_DEEPEST: Color = Color::from_rgb(0.075, 0.085, 0.110); // window backdrop
pub const BG_SURFACE: Color = Color::from_rgb(0.105, 0.115, 0.145); // chrome bars
pub const BG_BUTTON: Color = Color::from_rgb(0.135, 0.150, 0.180);  // idle button
pub const BG_HOVER: Color = Color::from_rgb(0.180, 0.200, 0.235);   // hovered

pub const TEXT_BRIGHT: Color = Color::from_rgb(0.790, 0.825, 0.870);
pub const TEXT_MUTED: Color = Color::from_rgb(0.470, 0.520, 0.585);

// Aurora accents. Each toggle picks one so the chrome lights up like the
// borealis when several are on at once.
pub const AURORA_GREEN: Color = Color::from_rgb(0.380, 0.860, 0.660);
pub const AURORA_TEAL: Color = Color::from_rgb(0.320, 0.780, 0.860);
pub const AURORA_PURPLE: Color = Color::from_rgb(0.700, 0.550, 0.920);

// Width of the docked settings panel in pixels.
pub const SETTINGS_PANEL_W: f32 = 280.0;

/// Format a Unix timestamp as a human-readable relative time.
pub fn fmt_age(unix_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age = now.saturating_sub(unix_secs);
    if age < 60 { return "just now".into(); }
    if age < 3600 { return format!("{} min ago", age / 60); }
    if age < 86400 { return format!("{} hr ago", age / 3600); }
    if age < 86400 * 7 { return format!("{} days ago", age / 86400); }
    if age < 86400 * 30 { return format!("{} wk ago", age / (86400 * 7)); }
    if age < 86400 * 365 { return format!("{} mo ago", age / (86400 * 30)); }
    format!("{} yr ago", age / (86400 * 365))
}

/// Build a compact metadata line: size, duration, resolution.
/// All values come from pre-cached data only - no disk I/O here.
pub fn fmt_meta(
    path: &std::path::Path,
    size_cache: &std::collections::HashMap<std::path::PathBuf, u64>,
    resume_db: &crate::resume::ResumeDb,
) -> String {
    let key = path.to_string_lossy();
    let size = size_cache.get(path).copied().map(fmt_size).unwrap_or_default();
    let dur  = resume_db.duration(&key).map(fmt_duration).unwrap_or_default();
    let res  = resume_db.resolution(&key)
        .map(|(w, h)| fmt_resolution(w, h))
        .unwrap_or_default();
    [size, dur, res].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("  ")
}

/// Format resolution with a common name when applicable.
pub fn fmt_resolution(w: u32, h: u32) -> String {
    let label = match h {
        0..=480  => "SD",
        481..=720  => "HD",
        721..=1080 => "FHD",
        1081..=1440 => "2K",
        1441..=2160 => "4K",
        _ => "8K",
    };
    format!("{w}×{h} {label}")
}

/// Format a byte count as a human-readable size string.
pub fn fmt_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.0} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Format a duration in seconds as h:mm:ss or m:ss.
pub fn fmt_duration(secs: f64) -> String {
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

/// Builds the shared tab bar + content area shown in the docked side panel.
fn panels_popup(app: &MpvNe) -> Element<'_, Message> {
    fn panel_btn<'a>(
        label: &'static str,
        icon: iced::widget::Svg<'a>,
        kind: PanelKind,
        app: &MpvNe,
    ) -> Element<'a, Message> {
        let active = app.active_panel == Some(kind);
        let fg = if active { AURORA_TEAL } else { TEXT_BRIGHT };
        button(
            row![icon, text(label).size(13).color(fg)]
                .spacing(10)
                .align_y(iced::Alignment::Center),
        )
        .padding([8, 14])
        .width(Length::Fill)
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => if active { BG_BUTTON } else { BG_DEEPEST },
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                border: iced::Border {
                    color: if active { iced::Color { a: 0.3, ..AURORA_TEAL } } else { iced::Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                    radius: iced::border::Radius::new(4.0),
                },
                ..Default::default()
            }
        })
        .on_press(Message::TogglePanel(kind))
        .into()
    }

    container(
        column![
            panel_btn("Playlist",  icons::list_music(),  PanelKind::Playlist,  app),
            panel_btn("Browser",   icons::folder_tree(), PanelKind::Browser,   app),
            panel_btn("Recent",    icons::history(),     PanelKind::Recent,    app),
            panel_btn("Settings",  icons::sliders(),     PanelKind::Settings,  app),
        ]
        .spacing(2)
        .width(Length::Fixed(180.0)),
    )
    .padding(6)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        border: iced::Border {
            radius: iced::border::Radius::new(6.0),
            width: 1.0,
            color: iced::Color::from_rgb(0.18, 0.20, 0.24),
        },
        ..Default::default()
    })
    .into()
}

fn tabbed_panel(app: &MpvNe, active: PanelKind) -> Element<'_, Message> {
    // Tab definitions: (label, kind).
    const TABS: &[(&str, PanelKind)] = &[
        ("Playlist", PanelKind::Playlist),
        ("Browser",  PanelKind::Browser),
        ("Recent",   PanelKind::Recent),
        ("Settings", PanelKind::Settings),
    ];

    // Tab bar row.
    let tab_bar = {
        use iced::widget::Row;
        let mut r = Row::new().width(Length::Fill);
        for (label, kind) in TABS {
            let is_active = active == *kind;
            let kind = *kind;
            let tab = button(
                text(*label).size(12).color(if is_active { BG_DEEPEST } else { TEXT_MUTED }),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_t, _status| iced::widget::button::Style {
                background: Some(iced::Background::Color(if is_active {
                    AURORA_TEAL
                } else {
                    BG_SURFACE
                })),
                border: Border {
                    color: BG_DEEPEST,
                    width: 1.0,
                    radius: iced::border::Radius::new(0.0),
                },
                ..Default::default()
            })
            .on_press(Message::TogglePanel(kind));

            let tip_text = match kind {
                PanelKind::Playlist => "Playlist",
                PanelKind::Browser  => "File browser",
                PanelKind::Recent   => "Recently played",
                PanelKind::Settings => "Playback settings",
            };
            let tipped_tab = tooltip(tab, text(tip_text).size(11), tooltip::Position::Bottom)
                .snap_within_viewport(true);
            r = r.push(tipped_tab);
        }

        // Collapse button with tooltip.
        let close_btn = icons::tipped(
            icons::square_btn(icons::panel_close()).on_press(Message::TogglePanel(active)),
            "Close panel",
        );
        r = r.push(close_btn);

        container(r)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG_SURFACE)),
                border: iced::Border {
                    color: BG_DEEPEST,
                    width: 0.0,
                    radius: iced::border::Radius::new(0.0),
                },
                ..Default::default()
            })
    };

    // Content area for the active tab (panels render their own scrollable body
    // without an internal header - the tab bar serves as the header).
    let content: Element<'_, Message> = match active {
        PanelKind::Playlist => playlist_panel::view(app),
        PanelKind::Browser  => browser_panel::view(app),
        PanelKind::Recent   => recent_panel::view(app),
        PanelKind::Settings => settings::view(app),
    };

    column![tab_bar, content]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    // Player column: top bar + video + controls. When the settings panel is
    // docked this column shrinks to give the panel its fixed slice of space.
    let player_col: Element<'_, Message> = if app.chrome_visible() {
        column![top_bar::view(app), video::view(app), controls::view(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        // Focus / hidden mode - chrome as hover overlays.
        let bottom_visible = app.chrome_overlay_visible();

        // Top bar visible only when cursor is in the top 80px (both fullscreen and focus mode).
        let top_visible = app.cursor_pos.map(|(_, y)| y <= 80.0).unwrap_or(false);

        let mut overlay = column![];
        if top_visible {
            overlay = overlay.push(top_bar::view(app));
        }
        // In windowed focus mode: draggable middle area to move the window.
        // NOT added in fullscreen - would capture clicks and block double-click to exit.
        if !app.fullscreen {
            let drag_middle = mouse_area(
                Space::new().width(Length::Fill).height(Length::Fill)
            )
            .on_press(Message::DragWindow);
            overlay = overlay.push(drag_middle);
        } else {
            overlay = overlay.push(Space::new().width(Length::Fill).height(Length::Fill));
        }
        if bottom_visible {
            overlay = overlay.push(controls::view(app));
        }
        stack![
            video::view(app),
            overlay.width(Length::Fill).height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    // When a panel is open: dock a tabbed panel to the right.
    let inner: Element<'_, Message> = if let Some(active_kind) = &app.active_panel {
        let active_kind = *active_kind;
        let panel = container(tabbed_panel(app, active_kind))
            .width(Length::Fixed(SETTINGS_PANEL_W))
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG_DEEPEST)),
                border: iced::Border {
                    color: BG_SURFACE,
                    width: 1.0,
                    radius: iced::border::Radius::new(0.0),
                },
                ..Default::default()
            });

        row![player_col, panel]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        player_col
    };

    let outer = container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_DEEPEST)),
            ..Default::default()
        });

    // Popups are anchored above whichever button was clicked. We saved the
    // cursor X at click time (popup_anchor_x). The popup is 220px wide; we
    // try to center it on the cursor but clamp so it never leaves the video
    // All button popups pinned at full-window level - see below.
    let outer_with_popup: Element<'_, Message> = outer.into();

    // Seekbar thumbnail/time popup - pinned at cursor position in window coords.
    let with_seek_popup: Element<'_, Message> = if let Some((time_str, thumb, win_x)) =
        controls::seek_hover_popup(app)
    {
        use iced::widget::{column as col, image as img_widget};
        let tw = crate::thumbnail::THUMB_W as f32;
        let th = crate::thumbnail::THUMB_H as f32;
        let popup_w = if thumb.is_some() { tw } else { 60.0_f32 };
        let popup_h = if thumb.is_some() { th + 22.0 } else { 26.0_f32 };

        let time_label = text(time_str).size(11).color(TEXT_BRIGHT);
        let popup_body: Element<'_, Message> = if let Some(handle) = thumb {
            col![
                img_widget(handle)
                    .width(Length::Fixed(tw))
                    .height(Length::Fixed(th)),
                container(time_label)
                    .width(Length::Fixed(tw))
                    .align_x(Horizontal::Center)
                    .padding([3, 0]),
            ]
            .spacing(0)
            .width(Length::Shrink)
            .into()
        } else {
            container(time_label).padding([4, 8]).into()
        };

        let popup = container(popup_body)
            .width(Length::Shrink)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG_DEEPEST)),
                border: iced::Border {
                    color: BG_HOVER,
                    width: 1.0,
                    radius: iced::border::Radius::new(4.0),
                },
                ..Default::default()
            });

        // Center popup on cursor X, keep above controls bar.
        let pin_x = (win_x - popup_w / 2.0).clamp(4.0, app.window_w_logical - popup_w - 4.0);
        let pin_y = app.window_h_logical - crate::app::CONTROLS_H as f32 - popup_h - 8.0;

        stack![
            outer_with_popup,
            pin(popup).x(pin_x).y(pin_y),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        outer_with_popup
    };

    // OSD overlay: brief message in the top-left of the video area.
    // Shown over everything else so it reads in both windowed and fullscreen.
    let with_osd: Element<'_, Message> = if app.osd_message.is_empty() {
        with_seek_popup
    } else {
        let osd = container(
            text(&app.osd_message)
                .size(15)
                .color(Color::WHITE),
        )
        .padding([6, 12])
        .style(|_| container::Style {
            background: Some(iced::Background::Color(
                Color::from_rgba(0.0, 0.0, 0.0, 0.65),
            )),
            border: iced::Border {
                radius: iced::border::Radius::new(5.0),
                ..Default::default()
            },
            ..Default::default()
        });

        // Anchor to top-left, below the top bar, with a small margin.
        let osd_layer = container(osd)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Left)
            .align_y(Vertical::Top)
            .padding(Padding { top: 54.0, left: 16.0, right: 0.0, bottom: 0.0 });

        stack![with_seek_popup, osd_layer]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    // Modal dialog overlay - rendered on top of everything.
    // Button popups pinned at full-window level, centered on popup_anchor_x.
    let anchor_x = app.popup_anchor_x;
    let win_w    = app.window_w_logical;
    let _win_h   = app.window_h_logical;
    let bar_h    = crate::app::CONTROLS_H as f32;

    // Popup content + close message (if any popup is open).
    let popup_layer: Option<(Element<'_, Message>, f32, Message)> =
        if app.subs_menu_open {
            let px = (anchor_x - 110.0).clamp(4.0, (win_w - 224.0).max(4.0));
            Some((controls::subs_popup(app), px, Message::CloseSubsMenu))
        } else if app.audio_menu_open {
            let px = (anchor_x - 110.0).clamp(4.0, (win_w - 224.0).max(4.0));
            Some((controls::audio_popup(app), px, Message::CloseAudioMenu))
        } else if app.fit_menu_open {
            let px = (anchor_x - 110.0).clamp(4.0, (win_w - 224.0).max(4.0));
            Some((controls::fit_popup(app), px, Message::CloseFitMenu))
        } else {
            None
        };

    let with_osd: Element<'_, Message> = if let Some((content, px, close)) = popup_layer {
        let bd = mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(close);
        let anchored = column![
            Space::new().height(Length::Fill),
            row![Space::new().width(Length::Fixed(px)), content],
            Space::new().height(Length::Fixed(bar_h + 8.0)),
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        stack![with_osd, bd, anchored].width(Length::Fill).height(Length::Fill).into()
    } else {
        with_osd
    };

    // File context menu - pinned at cursor position in window coords.
    let with_ctx_menu: Element<'_, Message> = if let Some(ctx) = &app.file_context_menu {
        let path = ctx.path.clone();
        let path2 = ctx.path.clone();
        let menu = container(
            column![
                button(text("Open containing folder").size(12).color(TEXT_BRIGHT))
                    .padding([7, 14]).width(Length::Fill)
                    .style(|_, status| {
                        use iced::widget::button::Status;
                        iced::widget::button::Style {
                            background: Some(iced::Background::Color(
                                if matches!(status, Status::Hovered | Status::Pressed) { BG_HOVER } else { BG_DEEPEST }
                            )),
                            ..Default::default()
                        }
                    })
                    .on_press(Message::OpenFileLocation(path)),
                button(text("Copy file path").size(12).color(TEXT_BRIGHT))
                    .padding([7, 14]).width(Length::Fill)
                    .style(|_, status| {
                        use iced::widget::button::Status;
                        iced::widget::button::Style {
                            background: Some(iced::Background::Color(
                                if matches!(status, Status::Hovered | Status::Pressed) { BG_HOVER } else { BG_DEEPEST }
                            )),
                            ..Default::default()
                        }
                    })
                    .on_press(Message::CopyFilePath(path2)),
            ]
            .spacing(1)
            .width(Length::Fixed(200.0)),
        )
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_DEEPEST)),
            border: iced::Border { color: BG_HOVER, width: 1.0, radius: iced::border::Radius::new(6.0) },
            ..Default::default()
        });

        let px = (ctx.x).clamp(4.0, (app.window_w_logical - 208.0).max(4.0));
        let py = (ctx.y).clamp(4.0, (app.window_h_logical - 80.0).max(4.0));

        let backdrop = mouse_area(
            Space::new().width(Length::Fill).height(Length::Fill)
        ).on_press(Message::CloseFileContextMenu);

        stack![
            with_osd,
            backdrop,
            pin(menu).x(px).y(py),
        ]
        .width(Length::Fill).height(Length::Fill)
        .into()
    } else {
        with_osd
    };

    // Panels picker popup - rendered at full window level so position is
    // consistent whether or not a side panel is open.
    let with_panels_popup: Element<'_, Message> = if app.panels_menu_open {
        let px = (app.popup_anchor_x - 90.0)
            .clamp(4.0, (app.window_w_logical - 184.0).max(4.0));
        let backdrop = mouse_area(
            Space::new().width(Length::Fill).height(Length::Fill)
        ).on_press(Message::TogglePanelsMenu);
        let anchored = column![
            Space::new().height(Length::Fill),
            row![Space::new().width(Length::Fixed(px)), panels_popup(app)],
            Space::new().height(Length::Fixed(bar_h + 8.0)),
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        stack![with_ctx_menu, backdrop, anchored]
            .width(Length::Fill).height(Length::Fill).into()
    } else {
        with_ctx_menu
    };

    let with_modal: Element<'_, Message> = if let Some(modal) = &app.modal {
        let dialog = container(
            column![
                text(modal.title).size(14).color(TEXT_BRIGHT),
                text(modal.prompt).size(11).color(TEXT_MUTED),
                iced::widget::text_input("", &modal.input)
                    .on_input(Message::ModalInput)
                    .on_submit(Message::ModalConfirm)
                    .padding([8, 10])
                    .size(13)
                    .style(|_, status| {
                        use iced::widget::text_input::Status;
                        iced::widget::text_input::Style {
                            background: iced::Background::Color(BG_DEEPEST),
                            border: iced::Border {
                                color: match status {
                                    Status::Focused { .. } => AURORA_TEAL,
                                    _ => BG_HOVER,
                                },
                                width: 1.5,
                                radius: iced::border::Radius::new(4.0),
                            },
                            icon: TEXT_MUTED,
                            placeholder: TEXT_MUTED,
                            value: TEXT_BRIGHT,
                            selection: Color { a: 0.3, ..AURORA_TEAL },
                        }
                    }),
                row![
                    Space::new().width(Length::Fill),
                    button(text("Cancel").size(12).color(TEXT_MUTED))
                        .padding([5, 14])
                        .style(|_, status| {
                            use iced::widget::button::Status;
                            iced::widget::button::Style {
                                background: Some(iced::Background::Color(
                                    if matches!(status, Status::Hovered | Status::Pressed) { BG_HOVER } else { BG_BUTTON }
                                )),
                                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                                ..Default::default()
                            }
                        })
                        .on_press(Message::ModalCancel),
                    button(text("OK").size(12).color(BG_DEEPEST))
                        .padding([5, 18])
                        .style(|_, status| {
                            use iced::widget::button::Status;
                            iced::widget::button::Style {
                                background: Some(iced::Background::Color(
                                    if matches!(status, Status::Hovered | Status::Pressed) { AURORA_GREEN } else { AURORA_TEAL }
                                )),
                                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                                ..Default::default()
                            }
                        })
                        .on_press(Message::ModalConfirm),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            ]
            .spacing(12)
            .width(Length::Fixed(340.0)),
        )
        .padding(20)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border {
                color: BG_HOVER,
                width: 1.0,
                radius: iced::border::Radius::new(8.0),
            },
            ..Default::default()
        });

        // Dim backdrop + centered dialog.
        let backdrop = container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55))),
                ..Default::default()
            });

        let centered = container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center);

        stack![
            with_panels_popup,
            mouse_area(backdrop).on_press(Message::ModalCancel),
            centered,
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        with_panels_popup
    };

    // Subtitle search overlay.
    let with_sub_search: Element<'_, Message> = if app.sub_search_open {
        use iced::widget::{scrollable, text_input};

        let tip_style = |_: &iced::Theme, status: iced::widget::text_input::Status| {
            iced::widget::text_input::Style {
                background: iced::Background::Color(BG_DEEPEST),
                border: iced::Border {
                    color: match status {
                        iced::widget::text_input::Status::Focused { .. } => AURORA_TEAL,
                        _ => BG_HOVER,
                    },
                    width: 1.5,
                    radius: iced::border::Radius::new(4.0),
                },
                icon: TEXT_MUTED, placeholder: TEXT_MUTED,
                value: TEXT_BRIGHT,
                selection: iced::Color { a: 0.3, ..AURORA_TEAL },
            }
        };

        let search_bar = row![
            text_input("Search subtitles...", &app.sub_search_query)
                .on_input(Message::SubSearchQuery)
                .on_submit(Message::SubSearch)
                .padding([8, 10])
                .size(13)
                .style(tip_style)
                .width(Length::Fill),
            button(text(if app.sub_search_loading { "…" } else { "Search" }).size(12).color(BG_DEEPEST))
                .padding([8, 14])
                .style(|_, status| {
                    use iced::widget::button::Status;
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(
                            if matches!(status, Status::Hovered | Status::Pressed) { AURORA_GREEN } else { AURORA_TEAL }
                        )),
                        border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                        ..Default::default()
                    }
                })
                .on_press(Message::SubSearch),
        ].spacing(8).align_y(iced::Alignment::Center);

        let results: Element<'_, Message> = if app.sub_search_results.is_empty() {
            if app.sub_search_loading {
                container(text("Searching…").size(12).color(TEXT_MUTED))
                    .padding([12, 0]).into()
            } else {
                container(text("Enter a title and press Search").size(12).color(TEXT_MUTED))
                    .padding([12, 0]).into()
            }
        } else {
            let rows = app.sub_search_results.iter().map(|r| {
                let label = if r.filename.is_empty() { &r.release } else { &r.filename };
                button(
                    row![
                        column![
                            text(label).size(12).color(TEXT_BRIGHT),
                            text(format!("{} • ★{:.1} • {} downloads",
                                r.language.to_uppercase(), r.rating, r.downloads))
                                .size(10).color(TEXT_MUTED),
                        ].spacing(2),
                    ]
                )
                .padding([6, 10])
                .width(Length::Fill)
                .style(|_, status| {
                    use iced::widget::button::Status;
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(
                            if matches!(status, Status::Hovered | Status::Pressed) { BG_HOVER } else { BG_DEEPEST }
                        )),
                        border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                        ..Default::default()
                    }
                })
                .on_press(Message::SubDownload(r.file_id, r.filename.clone()))
                .into()
            });
            scrollable(column(rows).spacing(2).padding([4, 0]))
                .height(Length::Fixed(280.0))
                .into()
        };

        let dialog = container(
            column![
                text("Search OpenSubtitles").size(14).color(TEXT_BRIGHT),
                search_bar,
                results,
                container(text("Click a result to download and load").size(10).color(TEXT_MUTED))
                    .padding([4, 0]),
            ]
            .spacing(10)
            .width(Length::Fixed(440.0)),
        )
        .padding(20)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border { color: BG_HOVER, width: 1.0, radius: iced::border::Radius::new(8.0) },
            ..Default::default()
        });

        let backdrop = mouse_area(
            container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::Fill).height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgba(0.0,0.0,0.0,0.55))),
                    ..Default::default()
                })
        ).on_press(Message::CloseSubSearch);

        let centered = container(dialog)
            .width(Length::Fill).height(Length::Fill)
            .align_x(Horizontal::Center).align_y(Vertical::Center);

        stack![with_modal, backdrop, centered]
            .width(Length::Fill).height(Length::Fill).into()
    } else { with_modal };

    // Keyboard shortcut help overlay.
    let with_help: Element<'_, Message> = if app.show_help {
        const SHORTCUTS: &[(&str, &str)] = &[
            ("Space",          "Play / Pause"),
            ("F",              "Toggle fullscreen"),
            ("H",              "Focus mode (hide chrome)"),
            ("M",              "Mute"),
            ("Left / Right",   "Seek ±5 seconds"),
            ("Ctrl+Left/Right","Previous / next chapter"),
            ("Up / Down",      "Volume ±5%"),
            ("PageUp/Down",    "Previous / next file"),
            ("[ / ]",          "Speed -0.1× / +0.1×"),
            ("\\",             "Reset speed"),
            ("J",              "Cycle subtitles"),
            ("#",              "Cycle audio tracks"),
            ("V",              "Toggle subtitle visibility"),
            ("I",              "Toggle hardware decoding"),
            ("Ctrl+G",         "Jump to time"),
            ("Ctrl+Scroll",    "Seek ±5 seconds"),
            ("End",            "Jump to live edge (growing files)"),
            ("Escape",         "Exit fullscreen / close panel"),
            ("?",              "Show / hide this help"),
        ];

        let rows = SHORTCUTS.iter().map(|(key, desc)| {
            row![
                container(text(*key).size(11).color(AURORA_TEAL))
                    .width(Length::Fixed(150.0)),
                text(*desc).size(11).color(TEXT_BRIGHT),
            ]
            .spacing(8)
            .into()
        });

        let content = container(
            column![
                text("Keyboard shortcuts").size(14).color(TEXT_BRIGHT),
                iced::widget::Space::new().height(Length::Fixed(8.0)),
                column(rows).spacing(6),
                iced::widget::Space::new().height(Length::Fixed(12.0)),
                text("Press ? or Escape to close").size(10).color(TEXT_MUTED),
            ]
            .width(Length::Fixed(360.0)),
        )
        .padding(24)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border {
                color: BG_HOVER,
                width: 1.0,
                radius: iced::border::Radius::new(8.0),
            },
            ..Default::default()
        });

        let backdrop = mouse_area(
            container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::Fill).height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(
                        iced::Color::from_rgba(0.0, 0.0, 0.0, 0.55)
                    )),
                    ..Default::default()
                }),
        ).on_press(Message::ShowHelp);

        let centered = container(content)
            .width(Length::Fill).height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center);

        stack![with_sub_search, backdrop, centered]
            .width(Length::Fill).height(Length::Fill)
            .into()
    } else {
        with_sub_search
    };

    // Resize-cursor feedback only matters when we own the chrome (custom
    // title bar on) and aren't in fullscreen.
    EdgeGrips::new(with_help)
        .enabled(USE_CUSTOM_TITLE_BAR && !app.fullscreen)
        .into()
}
