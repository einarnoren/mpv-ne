//! Standalone Log Viewer window - shows the in-memory debug ring buffer
//! (see `debug_log`) live, auto-scrolling as new lines arrive. Always its
//! own OS window with native decorations (see `Message::OpenLogViewer`),
//! independent of the main window and the custom-chrome windows - closing
//! either one has no effect on the other.

use iced::{
    Alignment, Element, Font, Length,
    widget::{button, column, container, row, scrollable, text, Space},
};

use super::{accent_purple, accent_teal, bg_deepest, bg_hover, bg_surface, text_bright, text_muted};
use crate::app::{Message, MpvNe};

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    let lines: Element<'_, Message> = if app.log_viewer_lines.is_empty() {
        container(text("No log output yet").size(12).color(text_muted()))
            .padding(14)
            .width(Length::Fill)
            .into()
    } else {
        let rows = app.log_viewer_lines.iter().map(|line| {
            text(line.clone())
                .size(12)
                .font(Font::MONOSPACE)
                .color(text_bright())
                .into()
        });
        scrollable(
            column(rows).width(Length::Fill).spacing(1).padding([6, 10]),
        )
        .id("log_viewer_scroll")
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(|viewport| Message::LogViewerScrolled(viewport.relative_offset()))
        .into()
    };

    let clear_btn = button(text("Clear").size(11).color(crate::ui::theme::legible_on_chrome(accent_purple())))
        .padding([3, 8])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => bg_hover(),
                _ => super::bg_button(),
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: accent_purple(),
                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                ..Default::default()
            }
        })
        .on_press(Message::LogViewerClear);

    let copy_btn = button(text("Copy all").size(11).color(crate::ui::theme::legible_on_chrome(accent_teal())))
        .padding([3, 8])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => bg_hover(),
                _ => super::bg_button(),
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: accent_teal(),
                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                ..Default::default()
            }
        })
        .on_press(Message::LogViewerCopyAll);

    let header = container(
        row![
            text(format!("{} line{}", app.log_viewer_lines.len(), if app.log_viewer_lines.len() == 1 { "" } else { "s" }))
                .size(11)
                .color(text_muted()),
            Space::new().width(Length::Fill),
            copy_btn,
            clear_btn,
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(bg_surface())),
        ..Default::default()
    });

    container(
        column![lines, header]
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(bg_deepest())),
        ..Default::default()
    })
    .into()
}
