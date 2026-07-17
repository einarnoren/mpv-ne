use iced::{
    Alignment, Element, Length, Radians,
    widget::{column, container, pin, responsive, row, slider, stack, text, Space},
};

use super::{
    AURORA_GREEN, AURORA_PURPLE, AURORA_TEAL, BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED, icons,
};
use crate::app::{Message, MpvNe};

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    let has_media = app.player.path.is_some();

    // Transport row buttons. Each is wrapped with `icons::tipped` so hover
    // shows a short label. Toggles use `square_toggle` so their on-state is
    // visually obvious (nord8 background when active).
    let play_glyph = if app.player.paused { icons::play() } else { icons::pause() };
    let play_pause = {
        let mut b = icons::square_btn(play_glyph);
        if has_media {
            b = b.on_press(Message::TogglePause);
        }
        icons::tipped(b, if app.player.paused { "Play (Space)" } else { "Pause (Space)" })
    };

    let stop_btn = {
        let mut b = icons::square_btn(icons::stop());
        if has_media {
            b = b.on_press(Message::Stop);
        }
        icons::tipped(b, "Stop")
    };

    let skip_back = {
        let mut b = icons::square_btn(icons::rewind());
        if has_media {
            b = b.on_press(Message::SeekRelative(-10.0));
        }
        icons::tipped(b, "Back 10 s (←)")
    };
    let skip_fwd = {
        let mut b = icons::square_btn(icons::fast_forward());
        if has_media {
            b = b.on_press(Message::SeekRelative(10.0));
        }
        icons::tipped(b, "Forward 10 s (→)")
    };

    let prev_btn = {
        let mut b = icons::square_btn(icons::skip_back());
        if app.playlist_idx > 0 {
            b = b.on_press(Message::PrevFile);
        }
        icons::tipped(b, "Previous file (PgUp)")
    };
    let next_btn = {
        let mut b = icons::square_btn(icons::skip_forward());
        if app.playlist_idx + 1 < app.playlist.len() {
            b = b.on_press(Message::NextFile);
        }
        icons::tipped(b, "Next file (PgDown)")
    };

    let open_btn = icons::tipped(
        icons::square_btn(icons::folder_open()).on_press(Message::OpenFile),
        "Open file…",
    );

    // Use the video-column width for breakpoints, not the full window width,
    // so responsive hiding/showing still fires at sensible sizes when the
    // docked side panel is open.
    let panel_w = if app.active_panel.is_some() { super::SETTINGS_PANEL_W } else { 0.0 };
    let w = app.window_w_logical - panel_w;

    // Volume: shrink the slider and drop the "Vol X%" label at narrow widths.
    // Colour the label amber when boosted above 100% so the user knows.
    let vol_color = if app.player.volume > 100.0 { super::AURORA_GREEN } else { TEXT_MUTED };
    let vol_text: Element<'_, Message> = if w >= 750.0 {
        text(format!("Vol {:.0}%", app.player.volume))
            .color(vol_color)
            .size(12)
            .into()
    } else {
        Space::new().into()
    };
    let vol_slider_w = if w >= 650.0 { 90.0 } else { 60.0 };

    // Seek slider with chapter markers and AB loop markers stacked on top.
    // Responsive gives us the real available width so positions match the track.
    let chapters = &app.player.chapters;
    let duration = app.player.duration;
    let position = app.player.position;
    let ab_a      = app.ab_loop_a;
    let ab_b      = app.ab_loop_b;
    let cache_time = app.player.cache_time;
    let bookmarks: &[crate::resume::Bookmark] = app.player.path.as_deref()
        .map(|p| app.resume_db.bookmarks(p))
        .unwrap_or(&[]);
    // Slider default height is ~22px; pin the wrapper to that so it doesn't
    // gobble all vertical space and shove the bottom row down.
    // Seekbar hover time: compute from cursor X relative to the seekbar area.
    // Compute hover position + thumbnail.
    // Hover popup rendered as a window overlay in ui/mod.rs via seek_hover_popup().
    const SEEK_H: f32 = 22.0;
    let seek: Element<'_, Message> = responsive(move |size| {
        let base: Element<'_, Message> =
            slider(0.0..=duration.max(1.0), position, Message::Seek)
                .style(|_t, _status| seek_slider_style())
                .into();
        // cache_time > position means there's buffered content ahead.
        // Only meaningful for network streams; local files cache instantly.
        let has_cache = duration > 0.0 && cache_time > position + 1.0;
        let has_markers = duration > 0.0
            && (!chapters.is_empty() || ab_a.is_some() || ab_b.is_some() || !bookmarks.is_empty());
        if !has_markers && !has_cache {
            return base;
        }
        let mut layers: Vec<Element<'_, Message>> = Vec::with_capacity(chapters.len() + 4);
        layers.push(base);

        // Buffer bar: teal strip showing how far the stream is buffered.
        // Sits on top of the inactive rail, below the slider handle.
        if has_cache {
            let cache_frac = (cache_time as f32 / duration as f32).clamp(0.0, 1.0);
            let play_frac  = (position    as f32 / duration as f32).clamp(0.0, 1.0);
            // Draw only the buffered-but-not-yet-played region.
            let x = play_frac  * size.width;
            let w = (cache_frac * size.width - x).max(0.0);
            let rail_y  = (SEEK_H - 4.0) / 2.0; // vertically center on the 4px rail
            let buf_bar = container(Space::new())
                .width(Length::Fixed(w))
                .height(Length::Fixed(4.0))
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(
                        iced::Color::from_rgba(0.32, 0.78, 0.86, 0.45),
                    )),
                    border: iced::Border { radius: iced::border::Radius::new(2.0), ..Default::default() },
                    ..Default::default()
                });
            layers.push(pin(buf_bar).x(x).y(rail_y).into());
        }

        // AB region highlight: a semi-transparent green bar between A and B.
        if let (Some(a), Some(b)) = (ab_a, ab_b) {
            let x1 = (a as f32 / duration as f32) * size.width;
            let x2 = (b as f32 / duration as f32) * size.width;
            let w = (x2 - x1).max(2.0);
            let region = container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::Fixed(w))
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(
                        iced::Color::from_rgba(0.38, 0.86, 0.66, 0.22),
                    )),
                    ..Default::default()
                });
            layers.push(pin(region).x(x1.max(0.0)).y(0.0).into());
        }

        // AB point markers: A = green, B = teal, 3 px wide.
        const AB_W: f32 = 3.0;
        for (time, color) in [(ab_a, AURORA_GREEN), (ab_b, AURORA_TEAL)] {
            let Some(t) = time else { continue };
            if t < 0.0 || t > duration { continue; }
            let x = (t as f32 / duration as f32) * size.width - AB_W / 2.0;
            let marker = container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::Fixed(AB_W))
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(color)),
                    ..Default::default()
                });
            layers.push(pin(marker).x(x.max(0.0)).y(0.0).into());
        }

        // Chapter ticks: 4 px wide, clickable.
        const TICK_W: f32 = 4.0;
        for chap in chapters {
            if chap.time <= 0.0 || chap.time >= duration {
                continue;
            }
            let x = (chap.time as f32 / duration as f32) * size.width - TICK_W / 2.0;
            let tick = iced::widget::button(
                Space::new()
                    .width(Length::Fixed(TICK_W))
                    .height(Length::Fill),
            )
            .padding(0)
            .style(|_t, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => AURORA_GREEN,
                    _ => AURORA_PURPLE,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: iced::Color::TRANSPARENT,
                    border: iced::Border {
                        radius: iced::border::Radius::new(1.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::Seek(chap.time));
            layers.push(pin(tick).x(x.max(0.0)).y(0.0).into());
        }

        // Bookmark markers: small dots at the top edge, distinct from
        // chapters' full-height bars so the two don't visually blend
        // together when a file has both.
        const BM_W: f32 = 6.0;
        for bm in bookmarks {
            if bm.position <= 0.0 || bm.position >= duration {
                continue;
            }
            let x = (bm.position as f32 / duration as f32) * size.width - BM_W / 2.0;
            let dot = iced::widget::button(
                Space::new().width(Length::Fixed(BM_W)).height(Length::Fixed(BM_W)),
            )
            .padding(0)
            .style(|_t, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => AURORA_TEAL,
                    _ => AURORA_GREEN,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: iced::Color::TRANSPARENT,
                    border: iced::Border {
                        radius: iced::border::Radius::new(3.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::Seek(bm.position));
            layers.push(pin(dot).x(x.max(0.0)).y(0.0).into());
        }
        stack(layers).into()
    })
    .height(Length::Fixed(SEEK_H))
    .into();

    let volume = slider(0.0..=200.0, app.player.volume, Message::VolumeChanged)
        .width(Length::Fixed(vol_slider_w))
        .style(|_t, _status| volume_slider_style(app.player.volume));

    let time = text(format!(
        "{} / {}",
        format_time(app.player.position),
        format_time(app.player.duration),
    ))
    .color(TEXT_BRIGHT)
    .size(13);

    let live_badge: Element<'_, Message> = if app.stream_is_live {
        container(text("LIVE").size(10).color(iced::Color::WHITE))
            .padding([2, 6])
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.820, 0.290, 0.290))),
                border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                ..Default::default()
            })
            .into()
    } else {
        Space::new().into()
    };


    // Codec/audio info line: first thing to go as the window narrows.
    let info_text = build_info_line(app);
    let info: Element<'_, Message> = if w >= 1000.0 && !info_text.is_empty() {
        text(info_text).color(TEXT_MUTED).size(12).into()
    } else {
        Space::new().into()
    };

    // Subtitle picker: icon button opens a popup menu; the label next to it
    // shows the current track. Label is truncated to stop it ever wrapping,
    // and hidden entirely below 950 px (info line is already gone by then).
    let current_label = app
        .player
        .sub_tracks
        .iter()
        .find(|t| t.id == app.player.current_sid)
        .map(|t| truncate(&t.label, 18))
        .unwrap_or_else(|| "Off".to_string());

    let subs_icon = if app.player.sub_visible {
        icons::captions()
    } else {
        icons::captions_off()
    };
    let subs_btn = icons::tipped(
        icons::square_toggle(subs_icon, app.subs_menu_open, AURORA_GREEN)
            .on_press(Message::ToggleSubsMenu),
        "Subtitle track (J to cycle)",
    );
    let subs: Element<'_, Message> = if w >= 950.0 {
        row![subs_btn, text(current_label).color(TEXT_MUTED).size(12)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    } else {
        subs_btn
    };

    // Audio track selector: icon button + truncated current track label.
    let audio_label = app
        .player
        .audio_tracks
        .iter()
        .find(|t| t.id == app.player.current_aid)
        .map(|t| truncate(&t.label, 18))
        .unwrap_or_else(|| "Off".to_string());

    let audio_btn = icons::tipped(
        icons::square_toggle(icons::audio_tracks(), app.audio_menu_open, AURORA_PURPLE)
            .on_press(Message::ToggleAudioMenu),
        "Audio track (# to cycle)",
    );
    let audio: Element<'_, Message> = if w >= 950.0 {
        row![audio_btn, text(audio_label).color(TEXT_MUTED).size(12)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    } else {
        audio_btn
    };

    let mute_glyph = if app.player.muted {
        icons::volume_muted()
    } else {
        icons::volume()
    };
    let mute = icons::tipped(
        icons::square_toggle(mute_glyph, app.player.muted, AURORA_TEAL)
            .on_press(Message::ToggleMute),
        "Mute (M)",
    );

    let full_glyph = if app.fullscreen { icons::minimize() } else { icons::maximize() };
    let fullscreen = icons::tipped(
        icons::square_toggle(full_glyph, app.fullscreen, AURORA_PURPLE)
            .on_press(Message::ToggleFullscreen),
        "Fullscreen (F)",
    );

    // AB repeat button. Cycles: idle -> A set -> A+B set (looping) -> clear.

    // Panels button: toggles the last-used side panel; tabs switch within it.
    let panel_active = app.active_panel.is_some();
    let panels_btn = icons::tipped(
        icons::square_toggle(icons::panels_menu(), panel_active, AURORA_TEAL)
            .on_press(Message::TogglePanelsMenu),
        "Panels  (Playlist / Browser / Recent / Settings)",
    );

    let mut fit_btn = icons::square_toggle(icons::fit_to_native(), app.fit_menu_open, AURORA_TEAL);
    // The fit menu now works without a video too (heights assume 16:9 when
    // no file is loaded), so only fullscreen disables the button.
    if !app.fullscreen {
        fit_btn = fit_btn.on_press(Message::ToggleFitMenu);
    }
    let fit = icons::tipped(fit_btn, "Fit window / scale video");

    // seek stays as-is; the hover popup is added as an overlay on the full controls bar below.

    // Top row: seek fills available space, volume slider always present.
    let top_row = row![seek, vol_text, volume]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    // Bottom row built conditionally so elements drop out as width shrinks.
    // Priority (highest = last to drop):
    //   play / stop / prev / next / open / time / mute / fit / fullscreen
    // >= 580px: skip_back, skip_fwd
    // >= 620px: subs, audio toggles
    {
        use iced::widget::Row;
        let mut r = Row::new().spacing(8).align_y(Alignment::Center).width(Length::Fill);

        r = r.push(play_pause);
        r = r.push(stop_btn);
        r = r.push(prev_btn);
        r = r.push(next_btn);
        if w >= 580.0 {
            r = r.push(skip_back);
            r = r.push(skip_fwd);
        }
        r = r.push(open_btn);
        r = r.push(time);
        r = r.push(live_badge);
        r = r.push(info);
        r = r.push(Space::new().width(Length::Fill));
        if w >= 620.0 {
            r = r.push(subs);
            r = r.push(audio);
        }
        r = r.push(mute);

        r = r.push(fit);
        r = r.push(fullscreen);
        r = r.push(panels_btn);

        let bottom_row = r;

        container(column![top_row, bottom_row].spacing(8))
            .padding(8)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG_SURFACE)),
                ..Default::default()
            })
            .into()
    }
}

/// Returns (time_string, thumbnail_handle, window_x) for the seek hover popup,
/// where window_x is the absolute X position in window coordinates.
/// Returns None when the cursor is not over the seekbar.
pub fn seek_hover_popup(
    app: &MpvNe,
) -> Option<(String, Option<iced::widget::image::Handle>, f32)> {
    let duration = app.player.duration;
    if duration <= 0.0 { return None; }
    let (cx, cy) = app.cursor_pos?;
    let controls_top = app.window_h_logical - crate::app::CONTROLS_H as f32;
    let seek_top    = controls_top + 8.0;
    let seek_bottom = seek_top + 22.0;
    if cy < seek_top || cy > seek_bottom { return None; }

    let panel_w      = if app.active_panel.is_some() { crate::ui::SETTINGS_PANEL_W } else { 0.0 };
    let video_col_w  = app.window_w_logical - panel_w;
    // Top row: [pad=8] [seek=fill] [spacing=8] [vol_text?] [spacing=8] [vol_slider] [pad=8]
    // vol_text only rendered when video_col_w >= 750.
    let vol_slider_w = if video_col_w >= 650.0 { 90.0 } else { 60.0 };
    let vol_text_w   = if video_col_w >= 750.0 { 48.0 } else { 0.0 };
    let bar_left  = 8.0;
    let bar_right = video_col_w
                    - 8.0   // right padding
                    - 8.0 - vol_slider_w  // spacing + slider
                    - (if vol_text_w > 0.0 { 8.0 + vol_text_w } else { 0.0 }); // spacing + text
    let bar_w    = (bar_right - bar_left).max(1.0);
    // Only show the popup when the cursor is actually over the bar's
    // horizontal extent — the vertical check above only confirms the
    // cursor is somewhere in the seek row, which also covers the volume
    // slider and any padding to either side.
    if cx < bar_left || cx > bar_left + bar_w { return None; }
    // The slider handle is a 6px-radius circle (12px wide). iced maps value 0..1
    // across (bar_w - handle_w), with half a handle of padding on each side.
    let handle_w = 12.0_f32;
    let effective_w = (bar_w - handle_w).max(1.0);
    let frac = ((cx - bar_left - handle_w / 2.0) / effective_w).clamp(0.0, 1.0);
    let t        = frac as f64 * duration;

    let time_str = crate::app::fmt_time(t);
    let thumb    = {
        let cache = app.thumb_cache.lock().unwrap();
        cache.get_nearest(t).map(|px| {
            iced::widget::image::Handle::from_rgba(
                crate::thumbnail::THUMB_W,
                crate::thumbnail::THUMB_H,
                px,
            )
        })
    };
    Some((time_str, thumb, cx))
}

// ── Gradient slider styles ────────────────────────────────────────────────────

fn gradient_h(stops: &[(f32, iced::Color)]) -> iced::Background {
    let mut g = iced::gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2));
    for &(offset, color) in stops {
        g = g.add_stop(offset, color);
    }
    iced::Background::Gradient(iced::Gradient::Linear(g))
}

fn seek_slider_style() -> iced::widget::slider::Style {
    use iced::widget::slider::{Handle, HandleShape, Rail, Style};
    Style {
        rail: Rail {
            backgrounds: (
                // Active (played) portion: teal -> purple gradient
                gradient_h(&[
                    (0.0, super::AURORA_TEAL),
                    (1.0, super::AURORA_PURPLE),
                ]),
                // Inactive (remaining) portion: dim
                iced::Background::Color(iced::Color::from_rgb(0.18, 0.20, 0.25)),
            ),
            width: 4.0,
            border: iced::Border { radius: iced::border::Radius::new(2.0), ..Default::default() },
        },
        handle: Handle {
            shape: HandleShape::Circle { radius: 6.0 },
            background: iced::Background::Color(iced::Color::WHITE),
            border_width: 0.0,
            border_color: iced::Color::TRANSPARENT,
        },
    }
}

fn volume_slider_style(_volume: f64) -> iced::widget::slider::Style {
    use iced::widget::slider::{Handle, HandleShape, Rail, Style};
    // Gradient: green -> teal -> purple across the full 0-200% range.
    let active = gradient_h(&[
        (0.0, super::AURORA_GREEN),
        (0.5, super::AURORA_TEAL),
        (1.0, super::AURORA_PURPLE),
    ]);
    Style {
        rail: Rail {
            backgrounds: (
                active,
                iced::Background::Color(iced::Color::from_rgb(0.18, 0.20, 0.25)),
            ),
            width: 4.0,
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

fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

fn build_info_line(app: &MpvNe) -> String {
    let mut parts: Vec<String> = Vec::new();

    if app.player.width > 0 && app.player.height > 0 {
        parts.push(format!("{}×{}", app.player.width, app.player.height));
    }

    if !app.player.video_codec.is_empty() {
        parts.push(short_codec(&app.player.video_codec));
    }

    if is_hdr(&app.player.primaries) {
        parts.push("HDR".to_string());
    }

    if !app.player.audio_codec.is_empty() {
        let codec = short_codec(&app.player.audio_codec);
        let ch = channels_label(app.player.audio_channels);
        if ch.is_empty() {
            parts.push(codec);
        } else {
            parts.push(format!("{} {}", codec, ch));
        }
    }

    if !app.player.hwdec.is_empty() && app.player.hwdec != "no" {
        parts.push(format!("HW {}", app.player.hwdec));
    } else if !app.player.hwdec.is_empty() {
        parts.push("SW decode".to_string());
    }

    parts.join("  ·  ")
}

fn short_codec(codec: &str) -> String {
    codec
        .split(|c: char| c == '(' || c == '/')
        .next()
        .unwrap_or(codec)
        .trim()
        .to_uppercase()
}

fn is_hdr(primaries: &str) -> bool {
    let p = primaries.to_ascii_lowercase();
    p.contains("bt.2020") || p.contains("bt2020")
}

/// Standalone view of the subtitle picker popup. Mod-level so `ui::mod` can
/// stack it on top of the rest of the chrome when `subs_menu_open` is true.
pub fn subs_popup(app: &super::super::app::MpvNe) -> Element<'_, Message> {
    use iced::widget::Column;
    let items: Vec<Element<'_, Message>> = app
        .player
        .sub_tracks
        .iter()
        .map(|track| {
            let active = track.id == app.player.current_sid;
            let fg = if active { AURORA_GREEN } else { TEXT_BRIGHT };
            iced::widget::button(text(&track.label).size(13).color(fg))
                .width(Length::Fill)
                .padding([6, 10])
                .style(move |_t, status| {
                    use iced::widget::button::Status;
                    let bg = match status {
                        Status::Hovered | Status::Pressed => {
                            iced::Color::from_rgb(0.180, 0.200, 0.235)
                        }
                        _ => iced::Color::from_rgb(0.105, 0.115, 0.145),
                    };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: fg,
                        border: iced::Border {
                            radius: iced::border::Radius::new(3.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::SubTrackSelected(track.clone()))
                .into()
        })
        .collect();

    container(Column::with_children(items).spacing(2))
        .padding(6)
        .width(Length::Fixed(220.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border {
                radius: iced::border::Radius::new(6.0),
                width: 1.0,
                color: iced::Color::from_rgb(0.180, 0.200, 0.235),
            },
            ..Default::default()
        })
        .into()
}

/// Window-fit popup. First entry trims the current letterbox; the rest scale
/// the video to a percentage of its native pixel size.
pub fn fit_popup(app: &super::super::app::MpvNe) -> Element<'_, Message> {
    use iced::widget::Column;

    fn entry<'a>(label: String, message: Message) -> Element<'a, Message> {
        iced::widget::button(text(label).size(13).color(TEXT_BRIGHT))
            .width(Length::Fill)
            .padding([6, 10])
            .style(|_t, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => iced::Color::from_rgb(0.180, 0.200, 0.235),
                    _ => iced::Color::from_rgb(0.105, 0.115, 0.145),
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: TEXT_BRIGHT,
                    border: iced::Border {
                        radius: iced::border::Radius::new(3.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(message)
            .into()
    }

    let native_w = app.player.width;
    let native_h = app.player.height;
    let has_video = native_w > 0 && native_h > 0;
    let aspect = if has_video {
        native_w as f32 / native_h as f32
    } else {
        16.0_f32 / 9.0
    };

    let mut items: Vec<Element<'_, Message>> = Vec::new();
    if has_video {
        items.push(entry("Fit to visible video".to_string(), Message::FitToVisible));
    }

    // Target heights. Skip any >= native (those duplicate "Native" or upscale).
    let width_for_h = |h: u32| -> i64 { ((h as f32) * aspect).round() as i64 };
    for h in [240_u32, 360, 480, 720, 1080, 1440, 2160] {
        if has_video && (h as i64) >= native_h {
            break;
        }
        items.push(entry(
            format!("{}p  ({}×{})", h, width_for_h(h), h),
            Message::FitToHeight(h),
        ));
    }

    if has_video {
        items.push(entry(
            format!("Native ({}p)  ({}×{})", native_h, native_w, native_h),
            Message::FitToScale(1.0),
        ));

        // Percentages relative to native, for when you want a quick zoom up
        // or down without thinking about a specific height.
        for scale in [0.25_f32, 0.5, 0.75, 1.5, 2.0] {
            let pct = (scale * 100.0).round() as i32;
            let w = (native_w as f32 * scale).round() as i64;
            let h = (native_h as f32 * scale).round() as i64;
            items.push(entry(
                format!("{}%  ({}×{})", pct, w, h),
                Message::FitToScale(scale),
            ));
        }
    }

    container(Column::with_children(items).spacing(2))
        .padding(6)
        .width(Length::Fixed(220.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border {
                radius: iced::border::Radius::new(6.0),
                width: 1.0,
                color: iced::Color::from_rgb(0.180, 0.200, 0.235),
            },
            ..Default::default()
        })
        .into()
}

/// Audio track picker popup. Mirrors `subs_popup` exactly, using `audio_tracks`
/// and emitting `Message::AudioTrackSelected`.
pub fn audio_popup(app: &super::super::app::MpvNe) -> Element<'_, Message> {
    use iced::widget::Column;
    let items: Vec<Element<'_, Message>> = app
        .player
        .audio_tracks
        .iter()
        .map(|track| {
            let active = track.id == app.player.current_aid;
            let fg = if active { AURORA_PURPLE } else { TEXT_BRIGHT };
            iced::widget::button(text(&track.label).size(13).color(fg))
                .width(Length::Fill)
                .padding([6, 10])
                .style(move |_t, status| {
                    use iced::widget::button::Status;
                    let bg = match status {
                        Status::Hovered | Status::Pressed => {
                            iced::Color::from_rgb(0.180, 0.200, 0.235)
                        }
                        _ => iced::Color::from_rgb(0.105, 0.115, 0.145),
                    };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: fg,
                        border: iced::Border {
                            radius: iced::border::Radius::new(3.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::AudioTrackSelected(track.clone()))
                .into()
        })
        .collect();

    container(Column::with_children(items).spacing(2))
        .padding(6)
        .width(Length::Fixed(220.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border {
                radius: iced::border::Radius::new(6.0),
                width: 1.0,
                color: iced::Color::from_rgb(0.180, 0.200, 0.235),
            },
            ..Default::default()
        })
        .into()
}

/// Truncate a label to `max` Unicode characters, appending "..." if cut.
/// Prevents long track names from wrapping and pushing the bar taller.
fn truncate(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        collected + "..."
    } else {
        collected
    }
}

fn channels_label(c: i64) -> String {
    match c {
        0 => String::new(),
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{}ch", n),
    }
}
