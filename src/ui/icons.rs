//! Glow Icons (https://github.com/glow-ui/glow-icons, MIT) embedded as inline
//! SVG path data. Glow's Outline set uses filled paths with evenodd rule so
//! the look is consistent: outlined glyphs that pop on a dark background.
//!
//! `square_btn`     - plain icon button (36×36).
//! `square_toggle`  - same, but with an active state lit by an aurora colour.
//! `tipped`         - wraps a widget in a Nord-styled tooltip.

use iced::widget::{button, container, svg, text, tooltip, Button, Svg};
use iced::{Border, Color, Element, Length, Padding};

use super::{BG_BUTTON, BG_HOVER, BG_DEEPEST, TEXT_BRIGHT};

const ICON_SIZE: u16 = 18;
const BTN_SIZE: f32 = 30.0;
/// Default monochrome icon fill - slightly cool light grey.
const ICON_HEX: &str = "#C5CDD9";

/// Standard idle 36×36 icon button.
pub fn square_btn<'a, Message: Clone + 'a>(icon: Svg<'a>) -> Button<'a, Message> {
    let bg = move |_t: &iced::Theme, status: iced::widget::button::Status| {
        use iced::widget::button::Status;
        let bg = match status {
            Status::Hovered | Status::Pressed => BG_HOVER,
            _ => BG_BUTTON,
        };
        iced::widget::button::Style {
            background: Some(iced::Background::Color(bg)),
            text_color: TEXT_BRIGHT,
            border: Border {
                radius: iced::border::Radius::new(4.0),
                ..Default::default()
            },
            ..Default::default()
        }
    };
    button(icon)
        .width(Length::Fixed(BTN_SIZE))
        .height(Length::Fixed(BTN_SIZE))
        .padding(Padding::new(6.0))
        .style(bg)
}

/// Toggle button. When `active`, the background lights up in `active_color`
/// (an aurora hue) so on/off state is unmistakable. The text colour is dark
/// when active for contrast against the bright background.
pub fn square_toggle<'a, Message: Clone + 'a>(
    icon: Svg<'a>,
    active: bool,
    active_color: Color,
) -> Button<'a, Message> {
    let style = move |_t: &iced::Theme, status: iced::widget::button::Status| {
        use iced::widget::button::Status;
        let bg = if active {
            active_color
        } else {
            match status {
                Status::Hovered | Status::Pressed => BG_HOVER,
                _ => BG_BUTTON,
            }
        };
        let fg = if active { BG_DEEPEST } else { TEXT_BRIGHT };
        iced::widget::button::Style {
            background: Some(iced::Background::Color(bg)),
            text_color: fg,
            border: Border {
                radius: iced::border::Radius::new(4.0),
                ..Default::default()
            },
            ..Default::default()
        }
    };
    button(icon)
        .width(Length::Fixed(BTN_SIZE))
        .height(Length::Fixed(BTN_SIZE))
        .padding(Padding::new(6.0))
        .style(style)
}

/// Wrap any widget with a small dark tooltip above it.
pub fn tipped<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    label: &'static str,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(text(label).size(11).color(TEXT_BRIGHT))
            .padding([4, 8])
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(BG_DEEPEST)),
                border: Border {
                    radius: iced::border::Radius::new(4.0),
                    ..Default::default()
                },
                ..Default::default()
            }),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

// ── SVG helpers ─────────────────────────────────────────────────────────────

fn glow<'a>(body: &str) -> Svg<'a> {
    // Glow's source SVGs use `fill="black"`. Swap that to our icon colour.
    let body = body.replace("fill=\"black\"", &format!("fill=\"{ICON_HEX}\""));
    let xml = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>"
    );
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// ── Transport ───────────────────────────────────────────────────────────────

pub fn play<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M4 4.83164C4 3.24931 5.75049 2.29363 7.08152 3.14928L18.2323 10.3176C19.4569 11.1049 19.4569 12.8951 18.2323 13.6823L7.08152 20.8507C5.75049 21.7063 4 20.7506 4 19.1683V4.83164ZM17.1507 12L6 4.83164V19.1683L17.1507 12Z" fill="black"/>"##)
}

pub fn pause<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M4 5C4 3.89543 4.89543 3 6 3H9C10.1046 3 11 3.89543 11 5V19C11 20.1046 10.1046 21 9 21H6C4.89543 21 4 20.1046 4 19V5ZM9 5H6V19H9V5Z" fill="black"/><path fill-rule="evenodd" clip-rule="evenodd" d="M13 5C13 3.89543 13.8954 3 15 3H18C19.1046 3 20 3.89543 20 5V19C20 20.1046 19.1046 21 18 21H15C13.8954 21 13 20.1046 13 19V5ZM18 5H15V19H18V5Z" fill="black"/>"##)
}

pub fn stop<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M3 5.33333C3 4.04467 4.04467 3 5.33333 3H18.6667C19.9553 3 21 4.04467 21 5.33333V18.6667C21 19.9553 19.9553 21 18.6667 21H5.33333C4.04467 21 3 19.9553 3 18.6667V5.33333ZM5.33333 5C5.14924 5 5 5.14924 5 5.33333V18.6667C5 18.8508 5.14924 19 5.33333 19H18.6667C18.8508 19 19 18.8508 19 18.6667V5.33333C19 5.14924 18.8508 5 18.6667 5H5.33333Z" fill="black"/>"##)
}

pub fn skip_back<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M17.8906 4.20444C19.2197 3.31836 21 4.27115 21 5.86854V18.1315C21 19.7289 19.2197 20.6817 17.8906 19.7956L8.69337 13.6641C7.50591 12.8725 7.50591 11.1276 8.69337 10.3359L17.8906 4.20444ZM19 5.86854L9.80277 12L19 18.1315V5.86854Z" fill="black"/><path d="M5 5C5 4.44772 4.55228 4 4 4C3.44772 4 3 4.44772 3 5V19C3 19.5523 3.44772 20 4 20C4.55228 20 5 19.5523 5 19V5Z" fill="black"/>"##)
}

pub fn skip_forward<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M6.1094 4.20444C4.78029 3.31836 3 4.27115 3 5.86854V18.1315C3 19.7289 4.78029 20.6817 6.1094 19.7956L15.3066 13.6641C16.4941 12.8725 16.4941 11.1276 15.3066 10.3359L6.1094 4.20444ZM5 5.86854L14.1972 12L5 18.1315V5.86854Z" fill="black"/><path d="M21 5C21 4.44772 20.5523 4 20 4C19.4477 4 19 4.44772 19 5V19C19 19.5523 19.4477 20 20 20C20.5523 20 21 19.5523 21 19V5Z" fill="black"/>"##)
}

pub fn rewind<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M9.57909 4.83828C10.5644 4.07194 12 4.77409 12 6.02231V12V17.9777C12 19.2259 10.5644 19.928 9.57909 19.1617L1.38606 12.7893C1.14247 12.5999 1 12.3086 1 12C1 11.6914 1.14247 11.4001 1.38606 11.2106L9.57909 4.83828ZM3.62882 12L10 16.9553V7.04463L3.62882 12Z" fill="black"/><path fill-rule="evenodd" clip-rule="evenodd" d="M12 12C12 12.3086 12.1425 12.5999 12.3861 12.7893L20.5791 19.1617C21.5644 19.928 23 19.2259 23 17.9777V6.02231C23 4.77409 21.5644 4.07194 20.5791 4.83828L12.3861 11.2106C12.1425 11.4001 12 11.6914 12 12ZM14.6288 12L21 16.9553V7.04463L14.6288 12Z" fill="black"/>"##)
}

pub fn fast_forward<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M3.42091 4.83828C2.43562 4.07194 1 4.77409 1 6.02231V17.9777C1 19.2259 2.43562 19.928 3.42091 19.1617L11.6139 12.7893C11.8575 12.5999 12 12.3086 12 12V17.9777C12 19.2259 13.4356 19.928 14.4209 19.1617L22.6139 12.7893C22.8575 12.5999 23 12.3086 23 12C23 11.6914 22.8575 11.4001 22.6139 11.2106L14.4209 4.83828C13.4356 4.07194 12 4.77409 12 6.02231V12C12 11.6914 11.8575 11.4001 11.6139 11.2106L3.42091 4.83828ZM9.37118 12L3 16.9553V7.04463L9.37118 12ZM20.3712 12L14 16.9553V7.04463L20.3712 12Z" fill="black"/>"##)
}

// ── Audio ──────────────────────────────────────────────────────────────────

pub fn volume<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M12.1657 2.14424C12.8728 2.50021 13 3.27314 13 3.7446V20.2561C13 20.7286 12.8717 21.4998 12.1656 21.8554C11.416 22.2331 10.7175 21.8081 10.3623 21.4891L4.95001 16.6248H3.00001C1.89544 16.6248 1.00001 15.7293 1.00001 14.6248L1 9.43717C1 8.3326 1.89543 7.43717 3 7.43717H4.94661L10.3623 2.51158C10.7163 2.19354 11.4151 1.76635 12.1657 2.14424ZM11 4.63507L6.00618 9.17696C5.82209 9.34439 5.58219 9.43717 5.33334 9.43717H3L3.00001 14.6248H5.33334C5.58015 14.6248 5.81823 14.716 6.00179 14.881L11 19.3731V4.63507Z" fill="black"/><path d="M16.0368 4.73124C16.1852 4.19927 16.7368 3.88837 17.2688 4.03681C20.6116 4.9696 23 8.22106 23 12C23 15.779 20.6116 19.0304 17.2688 19.9632C16.7368 20.1117 16.1852 19.8007 16.0368 19.2688C15.8884 18.7368 16.1993 18.1852 16.7312 18.0368C19.1391 17.3649 21 14.9567 21 12C21 9.04332 19.1391 6.63512 16.7312 5.96321C16.1993 5.81477 15.8884 5.2632 16.0368 4.73124Z" fill="black"/><path d="M16.2865 8.04192C15.7573 7.88372 15.2001 8.18443 15.0419 8.71357C14.8837 9.24271 15.1844 9.79992 15.7136 9.95812C16.3702 10.1544 17 10.9209 17 12C17 13.0791 16.3702 13.8456 15.7136 14.0419C15.1844 14.2001 14.8837 14.7573 15.0419 15.2865C15.2001 15.8156 15.7573 16.1163 16.2865 15.9581C17.9301 15.4667 19 13.8076 19 12C19 10.1924 17.9301 8.53333 16.2865 8.04192Z" fill="black"/>"##)
}

pub fn volume_muted<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M13 3.7446C13 3.27314 12.8728 2.50021 12.1657 2.14424C11.4151 1.76635 10.7163 2.19354 10.3623 2.51158L4.94661 7.43717H3C1.89543 7.43717 1 8.3326 1 9.43717L1.00001 14.6248C1.00001 15.7293 1.89544 16.6248 3.00001 16.6248H4.95001L10.3623 21.4891C10.7175 21.8081 11.416 22.2331 12.1656 21.8554C12.8717 21.4998 13 20.7286 13 20.2561V3.7446ZM6.00618 9.17696L11 4.63507V19.3731L6.00179 14.881C5.81823 14.716 5.58015 14.6248 5.33334 14.6248H3.00001L3 9.43717H5.33334C5.58219 9.43717 5.82209 9.34439 6.00618 9.17696Z" fill="black"/><path d="M15.2929 8.29289C15.6834 7.90237 16.3166 7.90237 16.7071 8.29289L19 10.5858L21.2929 8.29289C21.6834 7.90237 22.3166 7.90237 22.7071 8.29289C23.0976 8.68342 23.0976 9.31658 22.7071 9.70711L20.4142 12L22.7071 14.2929C23.0976 14.6834 23.0976 15.3166 22.7071 15.7071C22.3166 16.0976 21.6834 16.0976 21.2929 15.7071L19 13.4142L16.7071 15.7071C16.3166 16.0976 15.6834 16.0976 15.2929 15.7071C14.9024 15.3166 14.9024 14.6834 15.2929 14.2929L17.5858 12L15.2929 9.70711C14.9024 9.31658 14.9024 8.68342 15.2929 8.29289Z" fill="black"/>"##)
}

// ── Window / view ──────────────────────────────────────────────────────────

pub fn maximize<'a>() -> Svg<'a> {
    glow(r##"<path d="M21.7092 2.29502C21.8041 2.3904 21.8757 2.50014 21.9241 2.61722C21.9727 2.73425 21.9996 2.8625 22 2.997L22 3V9C22 9.55228 21.5523 10 21 10C20.4477 10 20 9.55228 20 9V5.41421L14.7071 10.7071C14.3166 11.0976 13.6834 11.0976 13.2929 10.7071C12.9024 10.3166 12.9024 9.68342 13.2929 9.29289L18.5858 4H15C14.4477 4 14 3.55228 14 3C14 2.44772 14.4477 2 15 2H20.9998C21.2749 2 21.5242 2.11106 21.705 2.29078L21.7092 2.29502Z" fill="black"/><path d="M10.7071 14.7071L5.41421 20H9C9.55228 20 10 20.4477 10 21C10 21.5523 9.55228 22 9 22H3.00069L2.997 22C2.74301 21.9992 2.48924 21.9023 2.29502 21.7092L2.29078 21.705C2.19595 21.6096 2.12432 21.4999 2.07588 21.3828C2.02699 21.2649 2 21.1356 2 21V15C2 14.4477 2.44772 14 3 14C3.55228 14 4 14.4477 4 15V18.5858L9.29289 13.2929C9.68342 12.9024 10.3166 12.9024 10.7071 13.2929C11.0976 13.6834 11.0976 14.3166 10.7071 14.7071Z" fill="black"/>"##)
}

pub fn minimize<'a>() -> Svg<'a> {
    glow(r##"<path d="M21.7071 3.70711L16.4142 9H20C20.5523 9 21 9.44772 21 10C21 10.5523 20.5523 11 20 11H14.0007L13.997 11C13.743 10.9992 13.4892 10.9023 13.295 10.7092L13.2908 10.705C13.196 10.6096 13.1243 10.4999 13.0759 10.3828C13.0273 10.2657 13.0004 10.1375 13 10.003L13 10V4C13 3.44772 13.4477 3 14 3C14.5523 3 15 3.44772 15 4V7.58579L20.2929 2.29289C20.6834 1.90237 21.3166 1.90237 21.7071 2.29289C22.0976 2.68342 22.0976 3.31658 21.7071 3.70711Z" fill="black"/><path d="M9 20C9 20.5523 9.44772 21 10 21C10.5523 21 11 20.5523 11 20V14.0007L11 13.997C10.9992 13.7231 10.8883 13.4752 10.7092 13.295L10.705 13.2908C10.6096 13.196 10.4999 13.1243 10.3828 13.0759C10.2657 13.0273 10.1375 13.0004 10.003 13L10 13H4C3.44772 13 3 13.4477 3 14C3 14.5523 3.44772 15 4 15H7.58579L2.29289 20.2929C1.90237 20.6834 1.90237 21.3166 2.29289 21.7071C2.68342 22.0976 3.31658 22.0976 3.70711 21.7071L9 16.4142V20Z" fill="black"/>"##)
}

pub fn folder_open<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M4 4C3.73478 4 3.48043 4.10536 3.29289 4.29289C3.10536 4.48043 3 4.73478 3 5V19C3 19.2652 3.10536 19.5196 3.29289 19.7071C3.48043 19.8946 3.73478 20 4 20H20C20.2652 20 20.5196 19.8946 20.7071 19.7071C20.8946 19.5196 21 19.2652 21 19V8C21 7.73478 20.8946 7.48043 20.7071 7.29289C20.5196 7.10536 20.2652 7 20 7H11.5352C10.8665 7 10.242 6.6658 9.87108 6.1094L8.46482 4H4ZM1.87868 2.87868C2.44129 2.31607 3.20435 2 4 2H8.46482C9.13352 2 9.75799 2.3342 10.1289 2.8906L11.5352 5H20C20.7957 5 21.5587 5.31607 22.1213 5.87868C22.6839 6.44129 23 7.20435 23 8V19C23 19.7957 22.6839 20.5587 22.1213 21.1213C21.5587 21.6839 20.7957 22 20 22H4C3.20435 22 2.44129 21.6839 1.87868 21.1213C1.31607 20.5587 1 19.7957 1 19V5C1 4.20435 1.31607 3.44129 1.87868 2.87868Z" fill="black"/>"##)
}

pub fn eye<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M12 7.5C9.51472 7.5 7.5 9.51472 7.5 12C7.5 14.4853 9.51472 16.5 12 16.5C14.4853 16.5 16.5 14.4853 16.5 12C16.5 9.51472 14.4853 7.5 12 7.5ZM9.5 12C9.5 10.6193 10.6193 9.5 12 9.5C13.3807 9.5 14.5 10.6193 14.5 12C14.5 13.3807 13.3807 14.5 12 14.5C10.6193 14.5 9.5 13.3807 9.5 12Z" fill="black"/><path fill-rule="evenodd" clip-rule="evenodd" d="M12 2.5C7.80929 2.5 4.80639 4.84327 2.90291 7.0685C1.94666 8.18638 1.24437 9.29981 0.780976 10.1325C0.548666 10.55 0.374765 10.8998 0.257664 11.1484C0.199077 11.2727 0.154596 11.372 0.124031 11.4419C0.108745 11.4769 0.0969291 11.5046 0.0885598 11.5245L0.0785838 11.5483L0.0755265 11.5557L0.0744793 11.5583L0.0740759 11.5593L-0.0740967 11.9233L0.0646933 12.291L0.0650804 12.292L0.0660813 12.2947L0.0689924 12.3023L0.0784664 12.3266C0.0864105 12.3469 0.0976229 12.3751 0.112146 12.4107C0.141186 12.482 0.183503 12.583 0.239442 12.7095C0.351245 12.9623 0.517901 13.318 0.742247 13.7424C1.18972 14.5889 1.87318 15.7209 2.81783 16.8577C4.70146 19.1243 7.70693 21.5 12 21.5C16.293 21.5 19.2985 19.1243 21.1821 16.8577C22.1267 15.7209 22.8102 14.5889 23.2577 13.7424C23.482 13.318 23.6487 12.9623 23.7605 12.7095C23.8164 12.583 23.8587 12.482 23.8878 12.4107C23.9023 12.3751 23.9135 12.3469 23.9215 12.3266L23.9309 12.3023L23.9338 12.2947L23.9348 12.292L23.9352 12.291L24.074 11.9233L23.9258 11.5593L23.9244 11.5557L23.9213 11.5483L23.9114 11.5245C23.903 11.5046 23.8912 11.4769 23.8759 11.4419C23.8453 11.372 23.8008 11.2727 23.7423 11.1484C23.6252 10.8998 23.4513 10.55 23.2189 10.1325C22.7556 9.29981 22.0533 8.18638 21.097 7.0685C19.1935 4.84327 16.1906 2.5 12 2.5ZM2.5104 12.8077C2.32531 12.4576 2.18603 12.1632 2.09077 11.9504C2.1909 11.7404 2.33654 11.4501 2.52859 11.105C2.94555 10.3558 3.57468 9.35995 4.42272 8.36857C6.12781 6.37527 8.62491 4.5 12 4.5C15.375 4.5 17.8721 6.37527 19.5772 8.36857C20.4252 9.35995 21.0544 10.3558 21.4713 11.105C21.6634 11.4501 21.809 11.7404 21.9092 11.9504C21.8139 12.1632 21.6746 12.4576 21.4895 12.8077C21.0883 13.5667 20.4782 14.5754 19.6439 15.5794C17.9696 17.5942 15.475 19.5 12 19.5C8.52489 19.5 6.03035 17.5942 4.35602 15.5794C3.52168 14.5754 2.91166 13.5667 2.5104 12.8077Z" fill="black"/>"##)
}

pub fn eye_off<'a>() -> Svg<'a> {
    glow(r##"<path fill-rule="evenodd" clip-rule="evenodd" d="M23.7071 0.292893C24.0976 0.683417 24.0976 1.31658 23.7071 1.70711L18.989 6.4252C18.9805 6.43399 18.9719 6.4426 18.9632 6.45102L6.42853 18.9857C6.42006 18.9945 6.41146 19.0031 6.40273 19.0115L1.70711 23.7071C1.31658 24.0976 0.683417 24.0976 0.292893 23.7071C-0.0976311 23.3166 -0.0976311 22.6834 0.292893 22.2929L4.23434 18.3514C2.9164 17.1406 1.94176 15.7855 1.2732 14.6799C0.867646 14.0092 0.5691 13.421 0.370612 12.9971C0.271255 12.785 0.196646 12.6133 0.145961 12.4922C0.12061 12.4317 0.101217 12.3838 0.0876805 12.3497L0.0717465 12.3091L0.067044 12.2969L0.0655043 12.2928L0.0649367 12.2913L0.074198 11.5593L0.0746014 11.5583L0.0756485 11.5557L0.0787059 11.5483L0.0886819 11.5245C0.0970512 11.5046 0.108867 11.4769 0.124153 11.4419C0.154718 11.372 0.199199 11.2727 0.257786 11.1484C0.374887 10.8998 0.548788 10.55 0.781098 10.1325C1.24449 9.29981 1.94679 8.18638 2.90303 7.0685C4.80651 4.84327 7.80941 2.5 12.0001 2.5C14.4526 2.5 16.5053 3.30553 18.165 4.42081L22.2929 0.292893C22.6834 -0.0976311 23.3166 -0.0976311 23.7071 0.292893ZM9.67199 12.9138L12.9138 9.67199C12.6308 9.56087 12.3227 9.5 12 9.5C10.6193 9.5 9.5 10.6193 9.5 12C9.5 12.3227 9.56087 12.6308 9.67199 12.9138ZM14.3956 8.19018C13.702 7.75332 12.8801 7.5 12 7.5C9.51472 7.5 7.5 9.51472 7.5 12C7.5 12.8801 7.75332 13.702 8.19018 14.3956L5.65003 16.9358C4.48386 15.8793 3.60174 14.6655 2.98463 13.645C2.62146 13.0444 2.35559 12.5199 2.18184 12.1489C2.14799 12.0766 2.11769 12.0103 2.09089 11.9504C2.19102 11.7404 2.33667 11.4501 2.52871 11.105C2.94567 10.3558 3.57481 9.35995 4.42285 8.36856C6.12793 6.37527 8.62503 4.5 12.0001 4.5C13.8295 4.5 15.395 5.04849 16.7186 5.8672L14.3956 8.19018Z" fill="black"/><path d="M20.6071 7.99677C21.0547 7.6733 21.6798 7.77395 22.0033 8.22159C22.6476 9.11312 23.1239 9.93792 23.4403 10.542C23.5987 10.8445 23.7178 11.0931 23.7984 11.2688C23.8387 11.3567 23.8694 11.4264 23.8907 11.4757C23.9013 11.5004 23.9096 11.5199 23.9155 11.5341L23.9227 11.5512L23.9249 11.5567L23.9257 11.5586L23.9262 11.5598L23.9354 12.291L23.935 12.292L23.934 12.2947L23.9311 12.3023L23.9216 12.3266C23.9136 12.3469 23.9024 12.3751 23.8879 12.4107C23.8589 12.482 23.8165 12.583 23.7606 12.7095C23.6488 12.9623 23.4821 13.318 23.2578 13.7424C22.8103 14.5889 22.1269 15.7209 21.1822 16.8577C19.2986 19.1243 16.2931 21.5 12.0001 21.5C11.1273 21.5 10.3032 21.4013 9.5276 21.2228C8.98937 21.099 8.65343 20.5623 8.77726 20.0241C8.90108 19.4859 9.43778 19.1499 9.97601 19.2737C10.6058 19.4186 11.2795 19.5 12.0001 19.5C15.4752 19.5 17.9697 17.5942 19.644 15.5794C20.4784 14.5754 21.0884 13.5667 21.4896 12.8077C21.6747 12.4576 21.814 12.1632 21.9093 11.9504C21.8469 11.8195 21.7667 11.6573 21.6686 11.4699C21.3866 10.9316 20.9593 10.1915 20.3823 9.393C20.0588 8.94536 20.1594 8.32025 20.6071 7.99677Z" fill="black"/>"##)
}

// Thumbtack-style pin (Lucide-derived). Glow only ships a map-pin marker,
// which means "location"; we want the desk-tack metaphor for "keep on top".
pub fn pin<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="m9 10.76-1.78.9A2 2 0 0 0 5 15.24V17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

/// Same shape; active state is communicated by the button background colour.
pub fn pin_active<'a>() -> Svg<'a> {
    pin()
}

// History/clock icon for the recent files panel (Lucide history).
pub fn history<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l4 2"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// List-music icon for the playlist panel toggle (Lucide list-music).
pub fn list_music<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15V6"/><path d="M18.5 18a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z"/><path d="M12 12H3"/><path d="M16 6H3"/><path d="M12 18H3"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// Folder-tree icon for the browser panel (Lucide folder-tree).
pub fn folder_tree<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 10a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1h-2.5a1 1 0 0 1-.8-.4l-.9-1.2A1 1 0 0 0 15 3h-2a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1z"/><path d="M20 21a1 1 0 0 0 1-1v-3a1 1 0 0 0-1-1h-2.9a1 1 0 0 1-.88-.55l-.42-.85a1 1 0 0 0-.88-.6H13a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1z"/><path d="M3 5v14"/><path d="M3 7h5"/><path d="M3 17h5"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// Sliders icon for the playback settings window toggle.
pub fn sliders<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" x2="4" y1="21" y2="14"/><line x1="4" x2="4" y1="10" y2="3"/><line x1="12" x2="12" y1="21" y2="12"/><line x1="12" x2="12" y1="8" y2="3"/><line x1="20" x2="20" y1="21" y2="16"/><line x1="20" x2="20" y1="12" y2="3"/><line x1="2" x2="6" y1="14" y2="14"/><line x1="10" x2="14" y1="8" y2="8"/><line x1="18" x2="22" y1="16" y2="16"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// Audio track picker - Lucide "music" note icon, consistent with captions style.
pub fn audio_tracks<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// Subtitle / captions - Glow has no captions-specific icon, so we keep the
// Lucide-derived one inline. Outline style still reads consistent.
pub fn captions<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="14" x="3" y="5" rx="2" ry="2"/><path d="M7 15h4"/><path d="M15 15h2"/><path d="M7 11h2"/><path d="M13 11h4"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

pub fn captions_off<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="14" x="3" y="5" rx="2" ry="2"/><path d="M7 15h4"/><path d="M15 15h2"/><path d="M7 11h2"/><path d="M13 11h4"/><line x1="2" x2="22" y1="2" y2="22"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

// Window control glyphs (minimize bar / square / X) for the custom title bar.
pub fn window_minimize<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="5" y1="12" x2="19" y2="12"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

pub fn window_maximize<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="5" y="5" rx="1"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

/// "Fit window to video native size" icon: a small filled rectangle inside
/// a larger outlined one, representing the video snug inside the window.
pub fn fit_to_native<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="1.5"/><rect x="7" y="9" width="10" height="6" rx="1" fill="#C5CDD9"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

/// AB repeat: two bracket-arrows enclosing "AB" text.
#[allow(dead_code)]
pub fn ab_loop<'a>() -> Svg<'a> {
    // Left arrow-bracket + A + B + right arrow-bracket
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="7,8 4,12 7,16"/><polyline points="17,8 20,12 17,16"/><text x="7.5" y="16" font-size="9" font-family="monospace" font-weight="bold" fill="#C5CDD9" stroke="none">AB</text></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

/// Panel collapse: vertical bar on the left + bold chevron arrow pointing left.
pub fn help<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

/// Grid / apps icon for the panels menu button.
pub fn panels_menu<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

pub fn panel_close<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="3" x2="4" y2="21"/><polyline points="19 5 9 12 19 19"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}

pub fn window_close<'a>() -> Svg<'a> {
    let body = r##"<g fill="none" stroke="#C5CDD9" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></g>"##;
    let xml = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">{body}</svg>");
    svg(svg::Handle::from_memory(xml.into_bytes()))
        .width(Length::Fixed(ICON_SIZE as f32))
        .height(Length::Fixed(ICON_SIZE as f32))
}
