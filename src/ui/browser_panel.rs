//! Directory browser panel - navigate the filesystem and open media files.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, scrollable, text},
};

use super::{AURORA_GREEN, AURORA_TEAL, BG_DEEPEST, BG_HOVER, BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED};
use crate::app::{Message, MpvNe};

/// Truncate a string to `max_chars`, appending "..." if needed.
fn trunc(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        let cut: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        format!("{cut}\u{2026}") // horizontal ellipsis
    }
}

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    // ── Location bar ─────────────────────────────────────────────────────────
    let location_text = match &app.browser_path {
        None => "This PC".to_string(),
        Some(p) => p.to_string_lossy().into_owned(),
    };
    let display_loc = if location_text.len() > 26 {
        format!("\u{2026}{}", &location_text[location_text.len().saturating_sub(23)..])
    } else {
        location_text.clone()
    };

    let nav_btn = |label: &'static str, msg: Message| {
        button(text(label).size(12).color(TEXT_MUTED))
            .padding([3, 8])
            .style(|_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_SURFACE,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        radius: iced::border::Radius::new(3.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(msg)
    };

    let mut nav_row = iced::widget::Row::new().spacing(6).align_y(Alignment::Center);
    if !app.browser_back_stack.is_empty() {
        nav_row = nav_row.push(nav_btn("<", Message::BrowserBack));
    }
    nav_row = nav_row
        .push(nav_btn("..", Message::BrowserNavigateUp))
        .push(nav_btn("PC", Message::BrowserGoToDrives))
        .push(text(display_loc).size(11).color(TEXT_MUTED));

    let location_bar = container(nav_row)
    .padding([7, 10])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    });

    // ── Entry list ───────────────────────────────────────────────────────────
    let entries: Element<'_, Message> = if app.browser_entries.is_empty() {
        container(text("Empty").size(12).color(TEXT_MUTED))
            .padding([16, 14])
            .width(Length::Fill)
            .into()
    } else {
        let rows = app.browser_entries.iter().map(|entry| {
            let label_color = if entry.is_dir { AURORA_TEAL } else { AURORA_GREEN };
            let display_name = trunc(&entry.name, 32);
            let msg = if entry.is_dir {
                Message::BrowserNavigate(entry.path.clone())
            } else {
                Message::BrowserOpen(entry.path.clone())
            };

            // Thin left accent bar instead of [D]/[F] text.
            let accent = container(iced::widget::Space::new().width(3.0).height(Length::Fill))
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(label_color)),
                    ..Default::default()
                });

            // Size + duration for files (dirs show nothing).
            let meta_str = if !entry.is_dir {
                super::fmt_meta(&entry.path, &app.size_cache, &app.resume_db)
            } else {
                String::new()
            };

            let meta_el: Element<'_, Message> = if !meta_str.is_empty() {
                text(meta_str).size(10).color(TEXT_MUTED).into()
            } else {
                iced::widget::Space::new().into()
            };

            let item = button(
                row![
                    accent,
                    column![
                        text(display_name).size(12).color(TEXT_BRIGHT),
                        meta_el,
                    ]
                    .spacing(1),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([5, 8])
            .width(Length::Fill)
            .style(|_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_DEEPEST,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        radius: iced::border::Radius::new(3.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(msg);

            // Right-click context menu for files (not dirs).
            let item: Element<'_, Message> = if !entry.is_dir {
                let p = entry.path.clone();
                iced::widget::mouse_area(item)
                    .on_right_press(Message::FileContextMenu(p))
                    .into()
            } else {
                item.into()
            };

            item
        });

        scrollable(
            column(rows).width(Length::Fill).spacing(1).padding([4, 4]),
        )
        .id("browser_scroll")
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    // ── Footer ───────────────────────────────────────────────────────────────
    let n_dirs  = app.browser_entries.iter().filter(|e|  e.is_dir).count();
    let n_files = app.browser_entries.iter().filter(|e| !e.is_dir).count();
    let footer_text = match (n_dirs, n_files) {
        (0, 0) => "Empty".to_string(),
        (d, 0) => format!("{d} folder{}", if d == 1 { "" } else { "s" }),
        (0, f) => format!("{f} file{}", if f == 1 { "" } else { "s" }),
        (d, f) => format!("{d} folder{}, {f} file{}",
            if d == 1 { "" } else { "s" },
            if f == 1 { "" } else { "s" }),
    };

    let footer = container(text(footer_text).size(11).color(TEXT_MUTED))
        .padding([6, 12])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            ..Default::default()
        });

    container(
        column![location_bar, entries, footer]
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
