//! Media info panel - what the loaded file actually *is*.
//!
//! The stats overlay answers "is playback healthy right now"; this answers
//! "what am I playing". They overlap on codec and resolution, but the overlay
//! sits over the video and has to stay short, so the detail lives here.

use iced::{
    Element, Length,
    widget::{column, container, row, scrollable, text},
};

use super::{accent_teal, bg_surface, text_bright, text_muted};
use crate::app::{Message, MpvNe};

/// Bytes as something a person reads, e.g. "1.4 GB".
fn fmt_size(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// One label/value line. Values wrap rather than truncate - a codec string or
/// a path is worth reading in full, and the panel scrolls anyway.
fn line(label: &str, value: String) -> Element<'static, Message> {
    row![
        text(label.to_string())
            .size(11)
            .color(text_muted())
            .width(Length::Fixed(104.0)),
        text(value).size(11).color(text_bright()).width(Length::Fill),
    ]
    .spacing(8)
    .into()
}

fn heading(label: &str) -> Element<'static, Message> {
    container(text(label.to_string()).size(10).color(accent_teal()))
        .padding(iced::Padding {
            top: 6.0,
            right: 0.0,
            bottom: 2.0,
            left: 0.0,
        })
        .into()
}

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    if app.player.path.is_none() {
        return container(
            text("No file loaded").size(12).color(text_muted()),
        )
        .padding(14)
        .width(Length::Fill)
        .into();
    }

    let p = &app.player;
    let s = &app.stats;
    let info = app.media_info.clone().unwrap_or_default();
    // mpv reports nothing for plenty of these depending on container and
    // codec, so anything absent is skipped rather than shown as blank.
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".to_string());

    let mut items: Vec<Element<'_, Message>> = Vec::new();

    items.push(heading("FILE"));
    if let Some(path) = &p.path {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        items.push(line("Name", name));
    }
    items.push(line("Container", opt(&info.file_format)));
    if p.duration > 0.0 {
        items.push(line("Duration", fmt_duration(p.duration)));
    }
    if info.file_size > 0 {
        items.push(line("Size", fmt_size(info.file_size)));
    }

    items.push(heading("VIDEO"));
    items.push(line("Codec", p.video_codec.clone()));
    if let Some(f) = &info.video_format {
        items.push(line("Format", f.clone()));
    }
    items.push(line("Resolution", format!("{} × {}", p.width, p.height)));
    if s.container_fps > 0.0 {
        items.push(line("Frame rate", format!("{:.3} fps", s.container_fps)));
    }
    if s.video_bitrate > 0.0 {
        items.push(line(
            "Bitrate",
            format!("{:.2} Mb/s", s.video_bitrate / 1_000_000.0),
        ));
    }
    items.push(line("Pixel format", opt(&info.pixel_format)));
    items.push(line("Colour matrix", opt(&info.colormatrix)));
    items.push(line("Primaries", p.primaries.clone()));
    if let Some(g) = &info.gamma {
        items.push(line("Transfer", g.clone()));
    }
    if let Some(l) = &info.color_levels {
        items.push(line("Levels", l.clone()));
    }
    items.push(line("Decoder", p.hwdec.clone()));

    items.push(heading("AUDIO"));
    items.push(line("Codec", p.audio_codec.clone()));
    if let Some(f) = &info.audio_format {
        items.push(line("Format", f.clone()));
    }
    items.push(line("Channels", format!("{}", p.audio_channels)));
    if let Some(l) = &info.audio_channel_layout {
        items.push(line("Layout", l.clone()));
    }
    if info.audio_samplerate > 0 {
        items.push(line(
            "Sample rate",
            format!("{:.1} kHz", info.audio_samplerate as f64 / 1000.0),
        ));
    }
    if s.audio_bitrate > 0.0 {
        items.push(line(
            "Bitrate",
            format!("{:.0} kb/s", s.audio_bitrate / 1_000.0),
        ));
    }

    let body = column(items).spacing(3).width(Length::Fill);

    container(
        scrollable(container(body).padding(12).width(Length::Fill))
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(bg_surface())),
        ..Default::default()
    })
    .into()
}
