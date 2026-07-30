//! Chapter list panel - the chapters mpv read from the container, as a
//! clickable list. The seek bar already draws chapter tick marks; this is
//! the same data in a form you can actually read titles from and jump
//! through, rather than guessing from a 4px mark.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, text, Space},
};

use super::{AURORA_TEAL, BG_DEEPEST, BG_HOVER, BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED};
use crate::app::{Message, MpvNe};

fn fmt_time(t: f64) -> String {
    let s = t.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn trunc(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        let cut: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        format!("{cut}\u{2026}")
    }
}

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    let chapters = &app.player.chapters;

    let body: Element<'_, Message> = if chapters.is_empty() {
        container(
            text("No chapters in this file")
                .size(12)
                .color(TEXT_MUTED),
        )
        .padding([16, 14])
        .width(Length::Fill)
        .into()
    } else {
        // The chapter we're currently inside: the last one starting at or
        // before the playhead. Chapters come from mpv in time order.
        let pos = app.player.position;
        let current = chapters
            .iter()
            .rposition(|c| c.time <= pos + 0.05);

        let rows = chapters.iter().enumerate().map(|(i, chap)| {
            let is_current = current == Some(i);
            let name_color = if is_current { AURORA_TEAL } else { TEXT_BRIGHT };

            // Containers often omit chapter titles - fall back to a number
            // rather than showing an empty row.
            let title = chap
                .title
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .map(|t| trunc(t, 28))
                .unwrap_or_else(|| format!("Chapter {}", i + 1));

            let seek_to = chap.time;
            button(
                row![
                    text(fmt_time(chap.time))
                        .size(11)
                        .color(TEXT_MUTED)
                        .width(Length::Fixed(56.0)),
                    text(title).size(12).color(name_color),
                    Space::new().width(Length::Fill),
                ]
                .align_y(Alignment::Center)
                .spacing(4),
            )
            .padding([6, 12])
            .width(Length::Fill)
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => if is_current { BG_SURFACE } else { BG_DEEPEST },
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    ..Default::default()
                }
            })
            .on_press(Message::Seek(seek_to))
            .into()
        });

        scrollable(column(rows).spacing(1).width(Length::Fill))
            .height(Length::Fill)
            .into()
    };

    // Prev/next chapter buttons mirror the seek-bar's tick marks being
    // clickable, but work without aiming at a 4px target. Uses the same SVG
    // icon set as the transport controls so it matches the rest of the UI -
    // unicode triangles render as colour emoji in this font and looked
    // completely out of place.
    let nav_btn = |icon: iced::widget::Svg<'static>, label: &'static str, msg: Message| {
        button(
            row![icon, text(label).size(11).color(TEXT_BRIGHT)]
                .spacing(5)
                .align_y(Alignment::Center),
        )
        .padding([4, 10])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_SURFACE,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                border: iced::Border {
                    radius: iced::border::Radius::new(4.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(msg)
    };

    // Nothing to navigate in a file without chapters - showing dead
    // Prev/Next buttons and "0 chapters" there just looks broken.
    let footer: Element<'_, Message> = if chapters.is_empty() {
        Space::new().into()
    } else {
        container(
            row![
                nav_btn(super::icons::skip_back(), "Prev", Message::PrevChapter),
                nav_btn(super::icons::skip_forward(), "Next", Message::NextChapter),
                Space::new().width(Length::Fill),
                text(format!("{} chapters", chapters.len()))
                    .size(10)
                    .color(TEXT_MUTED),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .into()
    };

    column![body, footer]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
