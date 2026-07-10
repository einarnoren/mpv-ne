//! Playback settings panel - docked to the right of the video area.

use iced::{
    Alignment, Element, Length, Radians,
    widget::{button, column, container, row, scrollable, slider, text, text_input, Space},
};

use super::{
    AURORA_GREEN, AURORA_PURPLE, AURORA_TEAL, BG_BUTTON, BG_DEEPEST, BG_HOVER,
    BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED,
};
use crate::app::{AfterPlayback, Message, MpvNe};

/// Docked panel view: includes its own header with a close button.
pub fn view(app: &MpvNe) -> Element<'_, Message> {
    let header = container(
        text("Playback settings").size(13).color(TEXT_BRIGHT),
    )
    .padding([8, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    });

    let content = column![
        // ── Playback ──────────────────────────────────────────────
        category("Playback"),
        section("Speed", speed_row(app)),
        gap(),
        section("Options", options_row(app)),
        gap(),
        section("Seeking", row![
            toggle_btn(
                if app.precise_seek { "Precise (exact)" } else { "Fast (keyframe)" },
                app.precise_seek,
                Message::TogglePreciseSeek,
                AURORA_TEAL,
            ),
        ].into()),
        gap(),
        // ── Audio ─────────────────────────────────────────────────
        category("Audio"),
        section("Track", audio_track_list(app)),
        gap(),
        section_sub("Audio sync", "Shift audio earlier/later, relative to video", delay_row(
            app.player.audio_delay,
            Message::AudioDelayAdjust(-0.5),
            Message::AudioDelayAdjust(-0.1),
            Message::AudioDelayReset,
            Message::AudioDelayAdjust(0.1),
            Message::AudioDelayAdjust(0.5),
            "s",
        )),
        gap(),
        section("Mute", row![
            toggle_btn(
                if app.player.muted { "Muted" } else { "Mute" },
                app.player.muted,
                Message::ToggleMute,
                AURORA_PURPLE,
            ),
        ].into()),
        gap(),
        section_sub(
            "Normalization",
            "Evens out volume swings between quiet and loud passages",
            row![
                toggle_btn(
                    if app.audio_normalize { "On" } else { "Off" },
                    app.audio_normalize,
                    Message::ToggleAudioNormalize,
                    AURORA_GREEN,
                ),
            ].into(),
        ),
        gap(),
        section_sub(
            "Preferred language",
            "ISO code, e.g. \"eng\" - picked automatically when a file loads",
            lang_input(&app.audio_lang, Message::AudioLangInput),
        ),
        gap(),
        // ── Subtitles ─────────────────────────────────────────────
        category("Subtitles"),
        section("Track", sub_track_list(app)),
        gap(),
        section_sub("Subtitle sync", "Shift subtitles earlier/later, relative to audio", delay_row(
            app.player.sub_delay,
            Message::SubDelayAdjust(-0.5),
            Message::SubDelayAdjust(-0.1),
            Message::SubDelayReset,
            Message::SubDelayAdjust(0.1),
            Message::SubDelayAdjust(0.5),
            "s",
        )),
        gap(),
        section("Appearance", sub_appearance_rows(app)),
        gap(),
        section("Visibility", row![
            toggle_btn(
                if app.player.sub_visible { "Shown" } else { "Hidden" },
                app.player.sub_visible,
                Message::ToggleSubVisibility,
                AURORA_TEAL,
            ),
        ].into()),
        gap(),
        section_sub(
            "Preferred language",
            "ISO code, e.g. \"eng\" - picked automatically when a file loads",
            lang_input(&app.sub_lang, Message::SubLangInput),
        ),
        gap(),
        section("Load", column![
            action_btn("Open subtitle file...", Message::LoadSubtitle, AURORA_TEAL),
            action_btn("Search OpenSubtitles…",  Message::OpenSubSearch, AURORA_GREEN),
        ].spacing(4).into()),
        gap(),
        // ── Video ─────────────────────────────────────────────────
        category("Video"),
        section("Frame fit", row![
            button(text(format!("{} (cycle)", app.frame_mode.label())).size(12).color(TEXT_BRIGHT))
                .padding([5, 10])
                .style(|_, status| {
                    use iced::widget::button::Status;
                    let bg = match status {
                        Status::Hovered | Status::Pressed => BG_HOVER,
                        _ => BG_BUTTON,
                    };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(bg)),
                        border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                        ..Default::default()
                    }
                })
                .on_press(Message::CycleFrameMode),
        ].into()),
        gap(),
        section("Equalizer", eq_rows(app)),
        gap(),
        section("Aspect ratio", aspect_row()),
        gap(),
        section("Zoom", zoom_row(app)),
        gap(),
        section("Rotate / flip", transform_row(app)),
        gap(),
        section("Window size", window_size_row(app)),
        gap(),
        // ── Playback control ─────────────────────────────────────
        category("Playback control"),
        section("AB repeat", ab_row(app)),
        gap(),
        section("Navigate", column![
            action_btn("Open URL / stream...", Message::OpenUrl,    AURORA_TEAL),
            action_btn("Jump to time (Ctrl+G)", Message::JumpToTime, AURORA_TEAL),
        ].spacing(4).into()),
        gap(),
        section("After playback", after_playback_row(app)),
        gap(),
        // ── Other ─────────────────────────────────────────────────
        category("Other"),
        section("Screenshot", screenshot_section(app)),
    ]
    .spacing(0)
    .width(Length::Fill);

    // A stable id is required here - without one, iced ties the scrollable's
    // retained scroll offset to its position in the widget tree, and this
    // app rebuilds the view on every periodic tick (stats refresh, file-size
    // poll, etc). Any of those causing so much as one conditional element
    // elsewhere in the tree to appear/disappear is enough to make iced lose
    // track of this scrollable and snap it back to the top mid-scroll.
    let body = scrollable(content)
        .id("settings_scroll")
        .width(Length::Fill)
        .height(Length::Fill);

    container(
        column![header, body].width(Length::Fill).height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_DEEPEST)),
        ..Default::default()
    })
    .into()
}

// ── section rows ─────────────────────────────────────────────────────────────

fn speed_row(app: &MpvNe) -> Element<'_, Message> {
    let speed = app.player.speed;
    let label = if (speed - 1.0).abs() < 0.005 {
        "1x".to_string()
    } else {
        format!("{:.2}x", speed)
    };

    // Slider covers 0.25 - 4.0 with fine control around 1.0.
    // Fine nudge buttons still available for precise increments.
    column![
        row![
            slider(0.25f64..=4.0, speed, |v| {
                // Round to nearest 0.05 so the slider doesn't feel jittery.
                let snapped = (v * 20.0).round() / 20.0;
                Message::SpeedSet(snapped)
            })
            .step(0.05)
            .width(Length::Fill),
        ]
        .align_y(Alignment::Center),
        row![
            value_label(label),
            Space::new().width(Length::Fill),
            nudge_btn("-0.1", Message::SpeedAdjust(-0.1)),
            nudge_btn("+0.1", Message::SpeedAdjust(0.1)),
            reset_btn(Message::SpeedReset),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn options_row(app: &MpvNe) -> Element<'_, Message> {
    let hw_active = !app.player.hwdec.is_empty() && app.player.hwdec != "no";
    let hw_label = if app.player.hwdec.is_empty() || app.player.hwdec == "no" {
        "HW: off".to_string()
    } else {
        format!("HW: {}", app.player.hwdec)
    };
    column![
        row![
            toggle_btn("Loop file",     app.player.loop_file,     Message::ToggleLoopFile,     AURORA_TEAL),
            toggle_btn("Loop playlist", app.player.loop_playlist, Message::ToggleLoopPlaylist, AURORA_TEAL),
        ]
        .spacing(6),
        row![
            toggle_btn("Deinterlace",  app.player.deinterlace, Message::ToggleDeinterlace, AURORA_PURPLE),
            toggle_btn("Resume",       app.resume_enabled,     Message::ToggleResume,       AURORA_GREEN),
        ]
        .spacing(6),
        // Hardware decode mode selector.
        row![
            toggle_btn(&hw_label, hw_active, Message::ToggleHwDec, AURORA_TEAL),
            Space::new().width(Length::Fixed(4.0)),
            hwdec_btn("auto",    &app.player.hwdec),
            hwdec_btn("nvdec",   &app.player.hwdec),
            hwdec_btn("d3d11va", &app.player.hwdec),
            hwdec_btn("no",      &app.player.hwdec),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn hwdec_btn(mode: &'static str, current: &str) -> iced::widget::Button<'static, Message> {
    let active = current == mode || (mode == "auto" && current.starts_with("auto"));
    let color = if active { AURORA_TEAL } else { TEXT_MUTED };
    button(text(mode).size(10).color(color))
        .padding([3, 6])
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => if active { BG_HOVER } else { BG_BUTTON },
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                border: iced::Border {
                    color: if active { iced::Color { a: 0.4, ..AURORA_TEAL } } else { iced::Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                    radius: iced::border::Radius::new(3.0),
                },
                ..Default::default()
            }
        })
        .on_press(Message::HwDecSet(mode.to_string()))
}

fn delay_row<'a>(
    value: f64,
    big_minus: Message,
    small_minus: Message,
    reset: Message,
    small_plus: Message,
    big_plus: Message,
    unit: &str,
) -> Element<'a, Message> {
    let label = format!("{:+.1}{}", value, unit);
    column![
        row![
            nudge_btn("-0.5", big_minus),
            nudge_btn("-0.1", small_minus),
            Space::new().width(Length::Fill),
            nudge_btn("+0.1", small_plus),
            nudge_btn("+0.5", big_plus),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
        row![
            value_label(label),
            Space::new().width(Length::Fill),
            reset_btn(reset),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .into()
}

fn sub_appearance_rows(app: &MpvNe) -> Element<'_, Message> {
    column![
        row![
            text("Size").size(11).color(TEXT_MUTED).width(Length::Fixed(46.0)),
            slider(10.0f64..=200.0, app.player.sub_font_size as f64,
                |v| Message::SubFontSizeSet(v as i64))
                .step(1.0)
                .width(Length::Fill),
            container(text(format!("{}", app.player.sub_font_size)).size(11).color(TEXT_BRIGHT))
                .padding([2, 6])
                .width(Length::Fixed(36.0))
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(BG_BUTTON)),
                    border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                    ..Default::default()
                }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        row![
            text("Pos").size(11).color(TEXT_MUTED).width(Length::Fixed(46.0)),
            slider(0.0f64..=150.0, app.player.sub_pos as f64,
                |v| Message::SubPosSet(v as i64))
                .step(1.0)
                .width(Length::Fill),
            container(text(format!("{}", app.player.sub_pos)).size(11).color(TEXT_BRIGHT))
                .padding([2, 6])
                .width(Length::Fixed(36.0))
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(BG_BUTTON)),
                    border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                    ..Default::default()
                }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn aspect_row<'a>() -> Element<'a, Message> {
    const PRESETS: &[(&str, &str)] = &[
        ("Auto",  ""),
        ("4:3",   "4:3"),
        ("16:9",  "16:9"),
        ("21:9",  "21:9"),
        ("1:1",   "1:1"),
        ("2.35",  "2.35:1"),
    ];
    // Two rows of 3 buttons so they don't overflow the 280px panel.
    let mut top = iced::widget::Row::new().spacing(4);
    let mut bot = iced::widget::Row::new().spacing(4);
    for (idx, (label, ratio)) in PRESETS.iter().enumerate() {
        let ratio_s = ratio.to_string();
        let btn = button(text(*label).size(11).color(TEXT_BRIGHT))
            .padding([4, 8])
            .style(|_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_BUTTON,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                    ..Default::default()
                }
            })
            .on_press(Message::AspectRatioSet(ratio_s));
        if idx < 3 { top = top.push(btn); } else { bot = bot.push(btn); }
    }
    column![top, bot].spacing(6).into()
}

fn zoom_row(app: &MpvNe) -> Element<'_, Message> {
    let zoom = app.player.video_zoom;
    // Display as percentage: 2^zoom * 100.
    let pct = (2.0_f64.powf(zoom) * 100.0).round() as i32;
    let label = format!("{pct}%");
    column![
        row![
            slider(-2.0f64..=2.0, zoom, |v| {
                let snapped = if v.abs() < 0.04 { 0.0 } else { (v * 20.0).round() / 20.0 };
                Message::VideoZoomSet(snapped)
            })
            .step(0.05)
            .width(Length::Fill)
            .style(settings_slider_style),
        ],
        row![
            value_label(label),
            Space::new().width(Length::Fill),
            reset_btn(Message::VideoZoomReset),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn eq_rows(app: &MpvNe) -> Element<'_, Message> {
    let p = &app.player;
    column![
        eq_row("Brightness", p.brightness, Message::BrightnessSet),
        eq_row("Contrast",   p.contrast,   Message::ContrastSet),
        eq_row("Saturation", p.saturation, Message::SaturationSet),
        eq_row("Hue",        p.hue,        Message::HueSet),
        eq_row("Gamma",      p.gamma,      Message::GammaSet),
        row![
            Space::new().width(Length::Fill),
            reset_btn(Message::VideoEqReset),
        ],
    ]
    .spacing(8)
    .into()
}

fn eq_row<'a>(
    label: &'static str,
    value: i64,
    on_change: impl Fn(i64) -> Message + 'a,
) -> Element<'a, Message> {
    let color = if value == 0 { TEXT_MUTED } else { TEXT_BRIGHT };
    row![
        text(label).size(11).color(TEXT_MUTED).width(Length::Fixed(66.0)),
        slider(-100.0f64..=100.0, value as f64, move |v| on_change(v as i64))
            .step(1.0)
            .width(Length::Fill)
            .style(settings_slider_style),
        container(
            text(format!("{:+}", value)).size(11).color(color),
        )
        .padding([2, 6])
        .width(Length::Fixed(36.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_BUTTON)),
            border: iced::Border {
                radius: iced::border::Radius::new(3.0),
                ..Default::default()
            },
            ..Default::default()
        }),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn transform_row(app: &MpvNe) -> Element<'_, Message> {
    let btn = |label: &'static str, msg: Message| {
        button(text(label).size(11).color(TEXT_BRIGHT))
            .padding([4, 8])
            .style(|_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_BUTTON,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                    ..Default::default()
                }
            })
            .on_press(msg)
    };

    let rot_label = format!("{}°", app.video_rotate);
    column![
        row![
            btn("↻ 90°", Message::VideoRotateCw),
            btn("↺ 90°", Message::VideoRotateCcw),
            Space::new().width(Length::Fixed(4.0)),
            text(rot_label).size(11).color(TEXT_MUTED),
        ].spacing(4).align_y(Alignment::Center),
        row![
            btn("⇔ H-flip", Message::VideoHFlip),
            btn("⇕ V-flip", Message::VideoVFlip),
            Space::new().width(Length::Fixed(4.0)),
            text(format!("{}{}",
                if app.video_hflip { "H " } else { "" },
                if app.video_vflip { "V" } else { "" },
            )).size(11).color(AURORA_TEAL),
        ].spacing(4).align_y(Alignment::Center),
        btn("Reset transform", Message::VideoTransformReset),
    ]
    .spacing(6)
    .into()
}

/// Resize the app window to match the video's resolution - compact version
/// of the fit-to-scale popup on the controls bar (same messages, fewer
/// options to fit a narrow settings panel).
fn window_size_row(app: &MpvNe) -> Element<'_, Message> {
    let btn = |label: String, msg: Message| {
        button(text(label).size(11).color(TEXT_BRIGHT))
            .padding([4, 8])
            .style(|_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_BUTTON,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                    ..Default::default()
                }
            })
            .on_press(msg)
    };

    let has_video = app.player.width > 0 && app.player.height > 0;
    if !has_video {
        return text("No video loaded").size(11).color(TEXT_MUTED).into();
    }

    column![
        row![
            btn("Fit to visible".into(), Message::FitToVisible),
            btn("Native (100%)".into(), Message::FitToScale(1.0)),
        ].spacing(4),
        row![
            btn("50%".into(), Message::FitToScale(0.5)),
            btn("150%".into(), Message::FitToScale(1.5)),
            btn("200%".into(), Message::FitToScale(2.0)),
        ].spacing(4),
    ]
    .spacing(6)
    .into()
}

fn after_playback_row(app: &MpvNe) -> Element<'_, Message> {
    let opt = |label: &'static str, val: AfterPlayback| {
        let active = app.after_playback == val;
        let color = if active { AURORA_GREEN } else { TEXT_MUTED };
        button(text(label).size(11).color(color))
            .padding([4, 8])
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => if active { BG_HOVER } else { BG_BUTTON },
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        color: if active { iced::Color { a: 0.4, ..AURORA_GREEN } } else { iced::Color::TRANSPARENT },
                        width: if active { 1.0 } else { 0.0 },
                        radius: iced::border::Radius::new(4.0),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::SetAfterPlayback(val))
    };

    row![
        opt("Nothing",    AfterPlayback::DoNothing),
        opt("Next file",  AfterPlayback::NextFile),
        opt("Loop",       AfterPlayback::LoopFile),
        opt("Close",      AfterPlayback::ClosePlayer),
    ]
    .spacing(4)
    .into()
}

// ── slider style ─────────────────────────────────────────────────────────────

fn settings_slider_style(_t: &iced::Theme, _s: iced::widget::slider::Status) -> iced::widget::slider::Style {
    use iced::widget::slider::{Handle, HandleShape, Rail, Style};
    let mut g = iced::gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2));
    g = g.add_stop(0.0, AURORA_TEAL);
    g = g.add_stop(1.0, AURORA_PURPLE);
    Style {
        rail: Rail {
            backgrounds: (
                iced::Background::Gradient(iced::Gradient::Linear(g)),
                iced::Background::Color(iced::Color::from_rgb(0.18, 0.20, 0.25)),
            ),
            width: 3.0,
            border: iced::Border { radius: iced::border::Radius::new(2.0), ..Default::default() },
        },
        handle: Handle {
            shape: HandleShape::Circle { radius: 5.0 },
            background: iced::Background::Color(iced::Color::WHITE),
            border_width: 0.0,
            border_color: iced::Color::TRANSPARENT,
        },
    }
}

// ── widget helpers ────────────────────────────────────────────────────────────

fn ab_row(app: &MpvNe) -> Element<'_, Message> {
    let ab_active  = app.ab_loop_a.is_some() || app.ab_loop_b.is_some();
    let ab_looping = app.ab_loop_a.is_some() && app.ab_loop_b.is_some();

    let fmt = |t: f64| -> String {
        let s = t as u64;
        format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    };

    let a_label = app.ab_loop_a.map(fmt).unwrap_or_else(|| "A: --:--:--".into());
    let b_label = app.ab_loop_b.map(fmt).unwrap_or_else(|| "B: --:--:--".into());

    let btn = |label: String, msg: Message, active: bool, color: iced::Color| {
        button(text(label).size(11).color(if active { color } else { TEXT_MUTED }))
            .padding([4, 10])
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => if active { BG_HOVER } else { BG_BUTTON },
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        color: if active { iced::Color { a: 0.4, ..color } } else { iced::Color::TRANSPARENT },
                        width: if active { 1.0 } else { 0.0 },
                        radius: iced::border::Radius::new(4.0),
                    },
                    ..Default::default()
                }
            })
            .on_press(msg)
    };

    let status_text = if ab_looping {
        "Looping A→B"
    } else if app.ab_loop_a.is_some() {
        "A set — click B to start loop"
    } else {
        "[ = set A,  ] = set B"
    };

    let clear_el: Element<'_, Message> = if ab_active {
        btn("Clear".to_string(), Message::AbLoopClear, true, AURORA_PURPLE).into()
    } else {
        Space::new().into()
    };

    column![
        row![
            btn(a_label, Message::AbLoopSetA, app.ab_loop_a.is_some(), AURORA_GREEN),
            btn(b_label, Message::AbLoopSetB, app.ab_loop_b.is_some(), AURORA_TEAL),
            clear_el,
        ].spacing(6).align_y(iced::Alignment::Center),
        text(status_text).size(10).color(TEXT_MUTED),
    ]
    .spacing(4)
    .into()
}

/// Track-select pill button, shared shape for both audio and subtitle lists.
fn track_btn<'a>(label: &str, active: bool, msg: Message, color: iced::Color) -> Element<'a, Message> {
    button(text(label.to_string()).size(11).color(if active { color } else { TEXT_MUTED }))
        .padding([4, 8])
        .width(Length::Fill)
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => if active { BG_HOVER } else { BG_BUTTON },
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    color: if active { color } else { iced::Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn audio_track_list(app: &MpvNe) -> Element<'_, Message> {
    if app.player.audio_tracks.is_empty() {
        return text("No audio tracks").size(11).color(TEXT_MUTED).into();
    }
    let rows = app.player.audio_tracks.iter().map(|t| {
        track_btn(&t.label, t.id == app.player.current_aid, Message::AudioTrackSelected(t.clone()), AURORA_PURPLE)
    });
    column(rows).spacing(4).into()
}

fn sub_track_list(app: &MpvNe) -> Element<'_, Message> {
    if app.player.sub_tracks.is_empty() {
        return text("No subtitle tracks").size(11).color(TEXT_MUTED).into();
    }
    let rows = app.player.sub_tracks.iter().map(|t| {
        track_btn(&t.label, t.id == app.player.current_sid, Message::SubTrackSelected(t.clone()), AURORA_TEAL)
    });
    column(rows).spacing(4).into()
}

/// Small text field for a 3-letter-ish language code (e.g. "eng"), used for
/// both the audio and subtitle preferred-language settings.
fn lang_input<'a>(value: &str, on_input: impl Fn(String) -> Message + 'a) -> Element<'a, Message> {
    text_input("e.g. eng", value)
        .on_input(on_input)
        .padding([5, 8])
        .size(12)
        .width(Length::Fixed(90.0))
        .style(|_, status| {
            use iced::widget::text_input::Status;
            iced::widget::text_input::Style {
                background: iced::Background::Color(BG_BUTTON),
                border: iced::Border {
                    color: match status {
                        Status::Focused { .. } => AURORA_TEAL,
                        _ => BG_HOVER,
                    },
                    width: 1.0,
                    radius: iced::border::Radius::new(4.0),
                },
                icon: TEXT_MUTED,
                placeholder: TEXT_MUTED,
                value: TEXT_BRIGHT,
                selection: iced::Color { a: 0.3, ..AURORA_TEAL },
            }
        })
        .into()
}

fn screenshot_section(app: &MpvNe) -> Element<'_, Message> {
    let dir_label = if app.screenshot_dir.is_empty() {
        "Default folder".to_string()
    } else {
        // Show just the last component to keep it compact.
        std::path::Path::new(&app.screenshot_dir)
            .file_name()
            .map(|n| format!("…/{}", n.to_string_lossy()))
            .unwrap_or_else(|| app.screenshot_dir.clone())
    };
    column![
        action_btn("Take screenshot", Message::TakeScreenshot, AURORA_GREEN),
        row![
            text("Folder:").size(11).color(TEXT_MUTED),
            Space::new().width(Length::Fixed(6.0)),
            text(dir_label).size(11).color(TEXT_BRIGHT),
            Space::new().width(Length::Fill),
            action_btn("Change…", Message::ChooseScreenshotDir, AURORA_TEAL),
        ]
        .align_y(iced::Alignment::Center),
    ]
    .spacing(6)
    .into()
}

fn section<'a>(label: &'static str, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(label).size(13).color(TEXT_BRIGHT),
            content,
        ]
        .spacing(8),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    })
    .into()
}

/// Same as `section`, plus a small muted line clarifying what the setting
/// actually does - used where the label alone is easy to mix up with a
/// similarly-named neighbor (e.g. "Audio sync" vs. "Subtitle sync").
fn section_sub<'a>(label: &'static str, subtext: &'static str, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(label).size(13).color(TEXT_BRIGHT),
            text(subtext).size(10).color(TEXT_MUTED),
            content,
        ]
        .spacing(6),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    })
    .into()
}

/// Category divider: a bold label above a group of related sections
/// (Playback / Audio / Subtitles / Video / Playback control / Other).
fn category<'a>(label: &'static str) -> Element<'a, Message> {
    container(
        text(label.to_uppercase()).size(12).color(AURORA_TEAL),
    )
    .padding(iced::Padding { top: 16.0, bottom: 6.0, left: 14.0, right: 14.0 })
    .width(Length::Fill)
    .into()
}

fn gap<'a>() -> Element<'a, Message> {
    Space::new().height(Length::Fixed(2.0)).width(Length::Fill).into()
}

fn nudge_btn<'a>(label: &'static str, msg: Message) -> Element<'a, Message> {
    button(text(label).size(11).color(TEXT_BRIGHT))
        .padding([4, 7])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_BUTTON,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: TEXT_BRIGHT,
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

/// Lit toggle button: highlights in `color` when `active`, plain when not.
fn toggle_btn<'a>(
    label: impl ToString,
    active: bool,
    msg: Message,
    color: iced::Color,
) -> Element<'a, Message> {
    let text_color = if active { color } else { TEXT_MUTED };
    button(text(label.to_string()).size(12).color(text_color))
        .padding([5, 10])
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
                    color: if active { color } else { iced::Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn value_label(s: String) -> Element<'static, Message> {
    container(text(s).size(13).color(TEXT_BRIGHT))
        .padding([4, 10])
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_BUTTON)),
            border: iced::Border {
                radius: iced::border::Radius::new(4.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}


fn reset_btn<'a>(msg: Message) -> Element<'a, Message> {
    button(text("Reset").size(11).color(AURORA_TEAL))
        .padding([4, 8])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_BUTTON,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: AURORA_TEAL,
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn action_btn<'a>(label: &'static str, msg: Message, color: iced::Color) -> Element<'a, Message> {
    button(text(label).size(13).color(color))
        .padding([6, 14])
        .style(move |_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_BUTTON,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: color,
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}
