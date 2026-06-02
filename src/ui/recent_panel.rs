//! Recent files panel - shows the last N opened files, most-recent-first.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, text, Space},
};

use super::{AURORA_TEAL, BG_DEEPEST, BG_HOVER, BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED};
use crate::app::{Message, MpvNe};

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
    let entries: Element<'_, Message> = if app.recent_files.paths.is_empty() {
        container(text("No recent files").size(12).color(TEXT_MUTED))
            .padding([16, 14])
            .width(Length::Fill)
            .into()
    } else {
        let is_current_path = app.player.path.clone();

        let rows = app.recent_files.paths.iter().map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let display_name = trunc(&name, 32);

            let dir = path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir_display = trunc(&dir, 30);

            let is_current = is_current_path.as_deref() == Some(&path.to_string_lossy().into_owned());
            let name_color = if is_current { AURORA_TEAL } else { TEXT_BRIGHT };

            let meta = super::fmt_meta(path, &app.size_cache, &app.resume_db);
            let age  = app.resume_db
                .last_played(&path.to_string_lossy())
                .map(|t| super::fmt_age(t))
                .unwrap_or_default();

            let item = button(
                column![
                    text(display_name).size(12).color(name_color),
                    text(dir_display).size(10).color(TEXT_MUTED),
                    text(format!("{}{}", meta,
                        if !age.is_empty() && !meta.is_empty() { format!("  |  {age}") }
                        else if !age.is_empty() { age }
                        else { String::new() }
                    )).size(10).color(TEXT_MUTED),
                ]
                .spacing(2),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_DEEPEST,
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
            .on_press(Message::BrowserOpen(path.clone()));

            let p = path.clone();
            iced::widget::mouse_area(item)
                .on_right_press(Message::FileContextMenu(p))
                .into()
        });

        scrollable(
            column(rows).width(Length::Fill).spacing(2).padding([4, 4]),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    let clear_btn = button(text("Clear").size(11).color(super::AURORA_PURPLE))
        .padding([3, 8])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => super::BG_BUTTON,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: super::AURORA_PURPLE,
                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                ..Default::default()
            }
        })
        .on_press(Message::ClearRecent);

    let footer = container(
        row![
            text(format!("{} recent file{}", app.recent_files.paths.len(),
                if app.recent_files.paths.len() == 1 { "" } else { "s" }))
                .size(11).color(TEXT_MUTED),
            Space::new().width(Length::Fill),
            clear_btn,
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    });

    container(
        column![entries, footer]
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_DEEPEST)),
        ..Default::default()
    })
    .into()
}
