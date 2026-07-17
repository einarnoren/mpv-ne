//! Playlist panel - shows the current folder playlist with the active file
//! highlighted. Click any entry to jump to it.

use iced::{
    Alignment, Color, Element, Length,
    widget::{button, column, container, mouse_area, row, scrollable, stack, text, Space},
};

use super::{AURORA_GREEN, AURORA_PURPLE, AURORA_TEAL, BG_BUTTON, BG_DEEPEST, BG_HOVER, BG_SURFACE, TEXT_BRIGHT, TEXT_MUTED};
use crate::app::{Message, MpvNe, PlaylistSort};

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
    let entries: Element<'_, Message> = if app.playlist.is_empty() {
        container(
            text("No files in playlist").size(12).color(TEXT_MUTED),
        )
        .padding([16, 14])
        .width(Length::Fill)
        .into()
    } else {
        let rows = app.playlist.iter().enumerate().map(|(i, path)| {
            let is_current = i == app.playlist_idx;
            let path_str = path.to_string_lossy();
            let is_url = path_str.starts_with("http://") || path_str.starts_with("https://");
            let url_meta = if is_url { app.playlist_url_meta.get(path_str.as_ref()) } else { None };
            // A URL's file_name()/parent() split reads as garbled nonsense
            // (same issue fixed in the Recent panel) - show the probed
            // title if we have one, otherwise the whole URL.
            let name = if let Some(m) = url_meta {
                m.title.clone()
            } else if is_url {
                path_str.clone().into_owned()
            } else {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path_str.clone().into_owned())
            };
            let display_name = trunc(&name, 30);

            let label_color = if is_current { AURORA_TEAL } else { TEXT_BRIGHT };
            let bg_color = if is_current { BG_BUTTON } else { BG_DEEPEST };

            let meta = if let Some(m) = url_meta {
                let dur = m.duration.map(|d| super::fmt_duration(d)).unwrap_or_default();
                let parts: Vec<&str> = [dur.as_str(), m.uploader.as_deref().unwrap_or("")]
                    .into_iter().filter(|s| !s.is_empty()).collect();
                parts.join("  ")
            } else if is_url {
                String::new()
            } else {
                super::fmt_meta(path, &app.size_cache, &app.resume_db)
            };

            // Once we have a real title, the raw URL moves to its own
            // muted line under it instead of a badge next to the title -
            // it's still visible, just not competing with the title itself.
            let url_line: Option<Element<'_, Message>> = if url_meta.is_some() {
                Some(text(trunc(&path_str, 40)).size(9).color(TEXT_MUTED).into())
            } else {
                None
            };

            let mut title_col = column![
                text(display_name).size(12).color(label_color),
            ];
            if let Some(url_line) = url_line {
                title_col = title_col.push(url_line);
            }
            title_col = title_col.push(text(meta).size(10).color(TEXT_MUTED));

            let jump_btn = button(
                row![
                    container(
                        text(format!("{:>2}.", i + 1)).size(11).color(TEXT_MUTED),
                    )
                    .width(Length::Fixed(28.0)),
                    title_col.spacing(1),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([5, 10])
            .width(Length::Fill)
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => bg_color,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        color: if is_current {
                            Color { a: 0.3, ..AURORA_TEAL }
                        } else {
                            Color::TRANSPARENT
                        },
                        width: if is_current { 1.0 } else { 0.0 },
                        radius: iced::border::Radius::new(4.0),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::PlaylistJump(i));

            let path_c = path.clone();
            let jump_btn = mouse_area(jump_btn)
                .on_right_press(Message::FileContextMenu(path_c));

            let remove_btn = button(
                text("×").size(13).color(TEXT_MUTED),
            )
            .padding([4, 7])
            .style(|_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => iced::Color::TRANSPARENT,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                    ..Default::default()
                }
            })
            .on_press(Message::PlaylistRemove(i));

            row![jump_btn, remove_btn]
                .spacing(2)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .into()
        });

        scrollable(
            column(rows).width(Length::Fill).spacing(2).padding([4, 4]),
        )
        .id("playlist_scroll")
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    // ── Chapter list (shown when a file with chapters is loaded) ─────────────
    let chapters_section: Option<Element<'_, Message>> = if app.player.chapters.len() > 1 {
        let chapter_rows = app.player.chapters.iter().enumerate().map(|(i, ch)| {
            let is_current = app.player.chapters.get(i + 1)
                .map(|next| app.player.position < next.time)
                .unwrap_or(true)
                && app.player.position >= ch.time;

            let time_str = {
                let s = ch.time as u64;
                format!("{:02}:{:02}", s / 60, s % 60)
            };
            let label = ch.title.as_deref().unwrap_or("Chapter");
            let label_color = if is_current { AURORA_GREEN } else { TEXT_BRIGHT };

            button(
                row![
                    text(time_str).size(10).color(TEXT_MUTED).width(Length::Fixed(36.0)),
                    text(format!("{:>2}. {}", i + 1, label))
                        .size(11)
                        .color(label_color),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([4, 10])
            .width(Length::Fill)
            .style(move |_, status| {
                use iced::widget::button::Status;
                let bg = match status {
                    Status::Hovered | Status::Pressed => BG_HOVER,
                    _ => BG_DEEPEST,
                };
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                    ..Default::default()
                }
            })
            .on_press(Message::Seek(ch.time))
            .into()
        });

        Some(
            column![
                container(
                    text("Chapters").size(11).color(TEXT_MUTED),
                )
                .padding([6, 10])
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(BG_SURFACE)),
                    ..Default::default()
                }),
                column(chapter_rows).width(Length::Fill).spacing(1).padding([2, 4]),
            ]
            .width(Length::Fill)
            .into()
        )
    } else {
        None
    };

    // ── Bookmark list ─────────────────────────────────────────────────────────
    let bookmarks_section: Option<Element<'_, Message>> = {
        let bmarks = app.player.path.as_deref()
            .map(|p| app.resume_db.bookmarks(p))
            .unwrap_or(&[]);
        if bmarks.is_empty() {
            None
        } else {
            let rows: Vec<Element<'_, Message>> = bmarks.iter().enumerate().map(|(i, b)| {
                let jump_btn = button(
                    row![
                        text(&b.label).size(11).color(AURORA_TEAL).width(Length::Fixed(52.0)),
                        text("⚑").size(10).color(TEXT_MUTED),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([4, 10])
                .width(Length::Fill)
                .style(|_, status| {
                    use iced::widget::button::Status;
                    let bg = match status {
                        Status::Hovered | Status::Pressed => BG_HOVER,
                        _ => BG_DEEPEST,
                    };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(bg)),
                        border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                        ..Default::default()
                    }
                })
                .on_press(Message::JumpToBookmark(b.position));

                let del_btn = button(text("×").size(13).color(TEXT_MUTED))
                    .padding([4, 7])
                    .style(|_, status| {
                        use iced::widget::button::Status;
                        let bg = match status {
                            Status::Hovered | Status::Pressed => BG_HOVER,
                            _ => iced::Color::TRANSPARENT,
                        };
                        iced::widget::button::Style {
                            background: Some(iced::Background::Color(bg)),
                            border: iced::Border { radius: iced::border::Radius::new(3.0), ..Default::default() },
                            ..Default::default()
                        }
                    })
                    .on_press(Message::RemoveBookmark(i));

                row![jump_btn, del_btn]
                    .spacing(2)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into()
            }).collect();

            Some(column![
                container(text("Bookmarks").size(11).color(TEXT_MUTED))
                    .padding([6, 10])
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(BG_SURFACE)),
                        ..Default::default()
                    }),
                column(rows).width(Length::Fill).spacing(1).padding([2, 4]),
            ]
            .width(Length::Fill)
            .into())
        }
    };

    let shuffle_btn = button(text("Shuffle").size(11).color(AURORA_PURPLE))
        .padding([3, 8])
        .style(|_, status| {
            use iced::widget::button::Status;
            let bg = match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_BUTTON,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: AURORA_PURPLE,
                border: iced::Border { radius: iced::border::Radius::new(4.0), ..Default::default() },
                ..Default::default()
            }
        })
        .on_press(Message::ShufflePlaylist);

    let sort_btn = button(text("Sort").size(11).color(TEXT_MUTED))
        .padding([3, 8])
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
        .on_press(Message::TogglePlaylistSort);

    let small_btn = |label: &'static str, msg: Message, color: iced::Color| {
        button(text(label).size(11).color(color))
            .padding([3, 6])
            .style(move |_, status| {
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

    let footer = container(
        row![
            text(format!("{} file{}", app.playlist.len(),
                if app.playlist.len() == 1 { "" } else { "s" }))
                .size(11).color(TEXT_MUTED),
            Space::new().width(Length::Fill),
            small_btn("+URL", Message::OpenModal(crate::app::ModalKind::AddPlaylistUrl), AURORA_TEAL),
            small_btn("Load", Message::LoadPlaylist, AURORA_TEAL),
            small_btn("Save", Message::SavePlaylist, AURORA_GREEN),
            sort_btn,
            Space::new().width(Length::Fixed(4.0)),
            shuffle_btn,
            Space::new().width(Length::Fixed(8.0)),
            text(format!("{} / {}", app.playlist_idx + 1, app.playlist.len().max(1)))
                .size(11).color(TEXT_MUTED),
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_SURFACE)),
        ..Default::default()
    });

    let mut body = column![entries].width(Length::Fill).height(Length::Fill);
    if let Some(chapters) = chapters_section {
        body = body.push(chapters);
    }
    if let Some(bookmarks) = bookmarks_section {
        body = body.push(bookmarks);
    }

    let panel = container(
        column![body, footer]
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(BG_DEEPEST)),
        ..Default::default()
    });

    // Sort dropdown overlay - floats above the footer when open.
    if app.playlist_sort_open {
        fn sort_item(label: &str, msg: Message) -> Element<'_, Message> {
            button(text(label).size(12).color(super::TEXT_BRIGHT))
                .padding([6, 14])
                .width(Length::Fill)
                .style(|_, status| {
                    use iced::widget::button::Status;
                    let bg = match status {
                        Status::Hovered | Status::Pressed => BG_HOVER,
                        _ => BG_SURFACE,
                    };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(bg)),
                        ..Default::default()
                    }
                })
                .on_press(msg)
                .into()
        }

        let dropdown = container(
            column![
                sort_item("Name  A - Z",   Message::SortPlaylist(PlaylistSort::Name)),
                sort_item("Name  Z - A",   Message::SortPlaylist(PlaylistSort::NameDesc)),
                sort_item("Size  small first", Message::SortPlaylist(PlaylistSort::Size)),
                sort_item("Size  large first", Message::SortPlaylist(PlaylistSort::SizeDesc)),
                sort_item("Date modified",  Message::SortPlaylist(PlaylistSort::Modified)),
            ]
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG_SURFACE)),
            border: iced::Border {
                color: Color::from_rgb(0.18, 0.20, 0.24),
                width: 1.0,
                radius: iced::border::Radius::new(6.0),
            },
            ..Default::default()
        });

        // Anchor dropdown to the bottom of the panel, above the footer.
        let overlay = container(dropdown)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding(iced::Padding { bottom: 38.0, left: 4.0, right: 4.0, top: 0.0 });

        let dismiss = mouse_area(
            Space::new().width(Length::Fill).height(Length::Fill),
        )
        .on_press(Message::TogglePlaylistSort);

        stack![panel, dismiss, overlay]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        panel.into()
    }
}
