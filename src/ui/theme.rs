//! Colour themes.
//!
//! The UI used to read nine `const Color`s directly. Themes need those to
//! vary at runtime, so they're now functions reading from whichever
//! `Palette` is active. The active theme is a single atomic holding an enum
//! discriminant, and every palette is a `const` - so a colour lookup is an
//! atomic load plus a field read, with no locking or allocation. That
//! matters because these are read many times per frame during `view()`.
//!
//! Accent slots keep the names they had under the original Aurora theme
//! (`accent_green`/`teal`/`purple`). They're *slots*, not literal hues -
//! Midnight Sun maps ambers onto them. Renaming them per-theme isn't
//! possible when the call sites are shared, and numbering them
//! (`accent_1`…) would make every call site harder to read.

use iced::Color;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// One theme's full set of colours.
pub struct Palette {
    pub bg_deepest: Color,
    pub bg_surface: Color,
    pub bg_button: Color,
    pub bg_hover: Color,
    pub text_bright: Color,
    pub text_muted: Color,
    pub accent_green: Color,
    pub accent_teal: Color,
    pub accent_purple: Color,
    /// Button/UI glyphs. Its own slot rather than reusing `text_bright`:
    /// icons sit on button fills rather than in running text, so they often
    /// want to be dimmer or brighter than body copy.
    pub icon: Color,
    /// Outlines and dividers. Its own slot rather than reusing `bg_hover`:
    /// a hover fill and a border are different jobs, and sharing one value
    /// meant a rule appeared in the colour that means "you're pointing at
    /// this".
    pub border: Color,
    /// The logo gradient's three stops, in the artwork's order (stop 0 ->
    /// stop 1). Held separately from the accent slots because those were
    /// tuned for text legibility - the adjustments that keep an 11px label
    /// readable pull the brand colours badly off on a large graphic.
    /// These are the design spec's accents, unadjusted.
    pub logo: [Color; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Theme {
    /// The original northern-lights palette: green/teal/purple on a cool
    /// near-black. Default, and unchanged from before themes existed.
    #[default]
    Aurora,
    /// Deep winter - the months the sun doesn't rise. Deep sky and navy,
    /// with the warm tan of the last light.
    PolarNight,
    /// The opposite: high summer, when the sun never sets. A *light* theme -
    /// peach amber and coral on warm paper.
    MidnightSun,
    /// Light, clear teal and sea green.
    SeaBreeze,
    /// True black, phosphor teal and electric blue.
    Void,
    /// User-defined colours - see `CUSTOM`. Seeded from whichever theme was
    /// active when it's first selected, so there's a sane base to edit
    /// rather than nine blank fields.
    Custom,
}

impl Theme {
    pub const ALL: &'static [Theme] = &[
        Theme::Aurora,
        Theme::PolarNight,
        Theme::MidnightSun,
        Theme::SeaBreeze,
        Theme::Void,
        Theme::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Aurora => "Northern Lights",
            Theme::PolarNight => "Polar Night",
            Theme::MidnightSun => "Midnight Sun",
            Theme::SeaBreeze => "Sea Breeze",
            Theme::Void => "Void",
            Theme::Custom => "Custom",
        }
    }

    /// Stable string for settings.toml - the enum's discriminant would
    /// silently remap if the variant order ever changed.
    pub fn id(self) -> &'static str {
        match self {
            Theme::Aurora => "aurora",
            Theme::PolarNight => "polar_night",
            Theme::MidnightSun => "midnight_sun",
            Theme::SeaBreeze => "sea_breeze",
            Theme::Void => "void",
            Theme::Custom => "custom",
        }
    }

    pub fn from_id(s: &str) -> Theme {
        match s {
            "polar_night" => Theme::PolarNight,
            "midnight_sun" => Theme::MidnightSun,
            "sea_breeze" => Theme::SeaBreeze,
            "void" => Theme::Void,
            "custom" => Theme::Custom,
            _ => Theme::Aurora,
        }
    }

    /// The built-in palette. `Custom` has none of its own - its colours
    /// live in `CUSTOM` and change at runtime - so it reports the default
    /// here purely as a seed value.
    pub fn palette(self) -> &'static Palette {
        match self {
            Theme::PolarNight => &POLAR_NIGHT,
            Theme::MidnightSun => &MIDNIGHT_SUN,
            Theme::SeaBreeze => &SEA_BREEZE,
            Theme::Void => &VOID,
            Theme::Aurora | Theme::Custom => &AURORA,
        }
    }
}

// Hues come from design/palettes.html, which is the source of truth for
// each theme's identity. Two adjustments were needed to make them work as
// *interface* colours rather than swatches:
//
//   - The spec's per-theme label colour is a caption sitting on a colour
//     chip, not body text. Used directly as `text_muted` it measured as low
//     as 1.6:1 against the surface (readable body text wants ~4.5:1), so
//     these are lifted while keeping their hue.
//   - The spec's accents are fills. The UI mostly paints accents as *text*
//     on a button, where Polar Night's navy hit 1.3:1 against its own
//     button. They're re-tuned for that job, same hue family.
//
// `bg_button`/`bg_hover` aren't in the spec at all: they step lighter than
// the surface on dark themes and slightly darker on light ones, kept close
// enough that accent text still has contrast room.
// Every foreground/background pair below measures >= 4.5:1.

/// Northern Lights - the original dark theme, aurora accents.
const AURORA: Palette = Palette {
    bg_deepest: Color::from_rgb8(0x13, 0x16, 0x20),
    bg_surface: Color::from_rgb8(0x1B, 0x1E, 0x25),
    bg_button: Color::from_rgb8(0x25, 0x29, 0x30),
    bg_hover: Color::from_rgb8(0x32, 0x37, 0x40),
    text_bright: Color::from_rgb8(0xC9, 0xD3, 0xDE),
    text_muted: Color::from_rgb8(0x93, 0xA1, 0xB5),
    accent_green: Color::from_rgb8(0x61, 0xDB, 0xA8),
    accent_teal: Color::from_rgb8(0x52, 0xC7, 0xDC),
    accent_purple: Color::from_rgb8(0xBB, 0x96, 0xEE),
    icon: Color::from_rgb8(0xC9, 0xD3, 0xDE),
    border: Color::from_rgb8(0x2C, 0x31, 0x3B),
    logo: [
        Color::from_rgb8(0xB3,0x8C,0xEB),
        Color::from_rgb8(0x52,0xC7,0xDC),
        Color::from_rgb8(0x61,0xDB,0xA8),
    ],
};

/// Polar Night - deep sky, navy, and the warm tan of the last light.
const POLAR_NIGHT: Palette = Palette {
    bg_deepest: Color::from_rgb8(0x08, 0x08, 0x10),
    bg_surface: Color::from_rgb8(0x11, 0x15, 0x28),
    bg_button: Color::from_rgb8(0x15, 0x1A, 0x2A),
    bg_hover: Color::from_rgb8(0x1F, 0x27, 0x40),
    text_bright: Color::from_rgb8(0xAF, 0xC2, 0xD8),
    text_muted: Color::from_rgb8(0x7C, 0x90, 0xA8),
    accent_green: Color::from_rgb8(0x6E, 0x9F, 0xD4),
    accent_teal: Color::from_rgb8(0x8A, 0xA3, 0xC4),
    accent_purple: Color::from_rgb8(0xD7, 0xA9, 0x8F),
    icon: Color::from_rgb8(0xAF, 0xC2, 0xD8),
    border: Color::from_rgb8(0x26, 0x30, 0x50),
    logo: [
        Color::from_rgb8(0xC4,0x90,0x7A),
        Color::from_rgb8(0x2A,0x3F,0x5C),
        Color::from_rgb8(0x1E,0x3A,0x5F),
    ],
};

/// Midnight Sun - a *light* theme: peach amber, coral, sky teal on warm
/// paper. The sun that doesn't set, rather than a dark theme with warm
/// accents.
const MIDNIGHT_SUN: Palette = Palette {
    bg_deepest: Color::from_rgb8(0xFE, 0xF4, 0xE8),
    bg_surface: Color::from_rgb8(0xF7, 0xE8, 0xCC),
    bg_button: Color::from_rgb8(0xF2, 0xE2, 0xC2),
    bg_hover: Color::from_rgb8(0xED, 0xDC, 0xB8),
    text_bright: Color::from_rgb8(0x2E, 0x1E, 0x0A),
    text_muted: Color::from_rgb8(0x6E, 0x52, 0x28),
    accent_green: Color::from_rgb8(0x8F, 0x44, 0x12),
    accent_teal: Color::from_rgb8(0x8A, 0x3A, 0x18),
    accent_purple: Color::from_rgb8(0x24, 0x5C, 0x58),
    icon: Color::from_rgb8(0x2E, 0x1E, 0x0A),
    border: Color::from_rgb8(0xDC, 0xC7, 0x9A),
    logo: [
        Color::from_rgb8(0x7B,0xBC,0xB8),
        Color::from_rgb8(0xD4,0x77,0x4A),
        Color::from_rgb8(0xE8,0x90,0x50),
    ],
};

/// Sea Breeze - light, clear teal and sea green over deep water.
const SEA_BREEZE: Palette = Palette {
    bg_deepest: Color::from_rgb8(0xF0, 0xF8, 0xFA),
    bg_surface: Color::from_rgb8(0xE0, 0xF2, 0xF5),
    bg_button: Color::from_rgb8(0xD6, 0xEE, 0xF2),
    bg_hover: Color::from_rgb8(0xCD, 0xE9, 0xEF),
    text_bright: Color::from_rgb8(0x1A, 0x3A, 0x42),
    text_muted: Color::from_rgb8(0x3A, 0x66, 0x70),
    accent_green: Color::from_rgb8(0x0F, 0x5A, 0x6E),
    accent_teal: Color::from_rgb8(0x10, 0x60, 0x44),
    accent_purple: Color::from_rgb8(0x12, 0x4F, 0x60),
    icon: Color::from_rgb8(0x1A, 0x3A, 0x42),
    border: Color::from_rgb8(0xB4, 0xDC, 0xE4),
    logo: [
        Color::from_rgb8(0x2A,0x9B,0xB5),
        Color::from_rgb8(0x61,0xDB,0xA8),
        Color::from_rgb8(0x52,0xC7,0xDC),
    ],
};

/// Void - true black, phosphor teal and electric blue. The spec's third
/// accent is #111111 ("nothing"), which is invisible as text, so that role
/// takes a neutral that can still be read. The video area keeps true black;
/// the chrome above it has to step away from that or the whole UI vanishes.
const VOID: Palette = Palette {
    bg_deepest: Color::from_rgb8(0x00, 0x00, 0x00),
    bg_surface: Color::from_rgb8(0x14, 0x14, 0x19),
    bg_button: Color::from_rgb8(0x20, 0x20, 0x28),
    bg_hover: Color::from_rgb8(0x30, 0x30, 0x3A),
    text_bright: Color::from_rgb8(0xC8, 0xC8, 0xD8),
    text_muted: Color::from_rgb8(0x9A, 0x9A, 0xB0),
    accent_green: Color::from_rgb8(0x4A, 0xDF, 0xB8),
    accent_teal: Color::from_rgb8(0x6B, 0xA5, 0xFF),
    accent_purple: Color::from_rgb8(0xA8, 0xA8, 0xC0),
    icon: Color::from_rgb8(0xC8, 0xC8, 0xD8),
    border: Color::from_rgb8(0x2A, 0x2A, 0x33),
    logo: [
        Color::from_rgb8(0x11,0x11,0x11),
        Color::from_rgb8(0x2A,0x6E,0xE0),
        Color::from_rgb8(0x4A,0xDF,0xB8),
    ],
};

static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// The custom theme's nine colours, packed as 0x00RRGGBB. Atomics rather
/// than a lock: these are read many times per frame from `view()`, while
/// writes only happen when the user edits a colour.
static CUSTOM: [AtomicU32; 11] = [
    AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0),
    AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0),
    AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0),
    AtomicU32::new(0), AtomicU32::new(0),
];

/// Label and slot index for each editable colour, in the order the editor
/// shows them. Kept in one place so the two can't drift apart.
pub const CUSTOM_SLOTS: &[(&str, usize)] = &[
    ("Background", 0),
    ("Surface", 1),
    ("Button", 2),
    ("Hover", 3),
    ("Text", 4),
    ("Text (muted)", 5),
    ("Accent 1", 6),
    ("Accent 2", 7),
    ("Accent 3", 8),
    ("Icons", 9),
    ("Border", 10),
];

/// The slots gathered into the groups the editor shows, so eleven rows read
/// as three short lists rather than one long one. Every slot appears exactly
/// once - `debug_assert`ed in `custom_group_slots`.
pub const CUSTOM_GROUPS: &[(&str, &[usize])] = &[
    ("Backgrounds", &[0, 1, 2, 3, 10]),
    ("Text & icons", &[4, 5, 9]),
    ("Accents", &[6, 7, 8]),
];

/// Label for one slot, as shown in the editor.
pub fn slot_label(idx: usize) -> &'static str {
    CUSTOM_SLOTS
        .iter()
        .find(|(_, i)| *i == idx)
        .map_or("", |(l, _)| l)
}

/// Every custom colour as `#RRGGBB`, space separated - the form the editor
/// exports and accepts back.
pub fn export_custom() -> String {
    CUSTOM_SLOTS
        .iter()
        .map(|(_, i)| to_hex(custom_slot(*i)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse an exported palette. Requires every slot to be present and valid -
/// a partial line is far more likely to be half-typed than intentional, and
/// applying it would silently mangle the rest of the theme.
pub fn parse_custom_palette(s: &str) -> Option<Vec<Color>> {
    let parts: Vec<&str> = s.split([' ', ',', '\n', '\t']).filter(|p| !p.is_empty()).collect();
    if parts.len() != CUSTOM_SLOTS.len() {
        return None;
    }
    parts.iter().map(|p| parse_hex(p)).collect()
}

#[inline]
fn pack(c: Color) -> u32 {
    let q = |v: f32| ((v.clamp(0.0, 1.0) * 255.0).round() as u32) & 0xFF;
    (q(c.r) << 16) | (q(c.g) << 8) | q(c.b)
}

#[inline]
fn unpack(v: u32) -> Color {
    Color::from_rgb8(((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8)
}

/// Parse `#RRGGBB` or `RRGGBB`. `None` for anything else, so a half-typed
/// value leaves the current colour alone instead of flashing to black.
pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(unpack)
}

/// Slots drawn *on* something, and the ones drawn *under*. Used to decide
/// which pairings a contrast reading should look at.
const FG_SLOTS: [usize; 6] = [4, 5, 6, 7, 8, 9];
const BG_SLOTS: [usize; 4] = [0, 1, 2, 3];

fn relative_luminance(c: Color) -> f32 {
    let ch = |v: f32| {
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c.r) + 0.7152 * ch(c.g) + 0.0722 * ch(c.b)
}

/// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
pub fn contrast(a: Color, b: Color) -> f32 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// The worst contrast this custom slot achieves against the slots it will
/// actually be drawn with. Reported for information - nothing enforces it.
pub fn custom_contrast_worst(idx: usize) -> Option<f32> {
    let pairs: &[usize] = if BG_SLOTS.contains(&idx) {
        &FG_SLOTS
    } else if FG_SLOTS.contains(&idx) {
        &BG_SLOTS
    } else {
        return None;
    };
    Some(
        pairs
            .iter()
            .map(|&o| contrast(custom_slot(idx), custom_slot(o)))
            .fold(f32::INFINITY, f32::min),
    )
}

/// Shift `fg`'s lightness until it clears `min` contrast against `bg`,
/// keeping its hue. Used for the custom-theme editor's own text: the editor
/// is drawn in the very colours being edited, so it has to stay legible even
/// while those are set to something unreadable - otherwise you can't see
/// well enough to fix them.
pub fn ensure_contrast(fg: Color, bg: Color, min: f32) -> Color {
    if contrast(fg, bg) >= min {
        return fg;
    }
    let (h, sat, _) = to_hsl(fg);
    let (mut best, mut best_ratio) = (fg, contrast(fg, bg));
    // Walk dark to light and take the first pass: on a light background that
    // lands on a dark variant, on a dark one a light variant.
    for i in 0..=100 {
        let c = from_hsl(h, sat, i as f32 / 100.0);
        let r = contrast(c, bg);
        if r >= min {
            return c;
        }
        if r > best_ratio {
            (best, best_ratio) = (c, r);
        }
    }
    best
}

/// A foreground colour guaranteed to read on every chrome fill a button can
/// take - idle, active and hover all use `bg_button` or `bg_hover`.
///
/// Checking against one fill is not enough: a label passes at rest and
/// disappears under the cursor, or the reverse. Under a custom theme any
/// accent can be set to exactly a fill's value, which is how a "Save" button
/// and a selected subtitle track both ended up invisible.
pub fn legible_on_chrome(fg: Color) -> Color {
    let score = |c: Color| contrast(c, bg_button()).min(contrast(c, bg_hover()));
    if score(fg) >= 4.5 {
        return fg;
    }
    let (h, sat, _) = to_hsl(fg);
    let (mut best, mut best_score) = (fg, score(fg));
    for i in 0..=100 {
        let c = from_hsl(h, sat, i as f32 / 100.0);
        let sc = score(c);
        if sc >= 4.5 {
            return c;
        }
        if sc > best_score {
            (best, best_score) = (c, sc);
        }
    }
    best
}

/// RGB to hue (0-360), saturation and lightness (both 0-1). Hue is
/// meaningless for a pure grey; the caller keeps the last one it had rather
/// than letting it snap to red.
pub fn to_hsl(c: Color) -> (f32, f32, f32) {
    let (r, g, b) = (c.r, c.g, c.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    ((h + 360.0) % 360.0, s.clamp(0.0, 1.0), l)
}

pub fn from_hsl(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color::from_rgb(
        (r + m).clamp(0.0, 1.0),
        (g + m).clamp(0.0, 1.0),
        (b + m).clamp(0.0, 1.0),
    )
}

pub fn to_hex(c: Color) -> String {
    format!("#{:06X}", pack(c))
}

/// Marks a slot as written. Pure black is a legitimate colour, so a zero
/// word can't be used to mean "unset" - hence a bit above the packed RGB.
const SET: u32 = 0x0100_0000;

pub fn set_custom_slot(idx: usize, c: Color) {
    if let Some(slot) = CUSTOM.get(idx) {
        slot.store(pack(c) | SET, Ordering::Relaxed);
    }
}

/// A custom colour, falling back to the default palette for any slot that
/// was never set. That fallback is what keeps a config naming the custom
/// theme without any colours from rendering the UI black on black.
pub fn custom_slot(idx: usize) -> Color {
    let raw = CUSTOM.get(idx).map_or(0, |s| s.load(Ordering::Relaxed));
    if raw & SET == 0 {
        palette_slot(&AURORA, idx)
    } else {
        unpack(raw)
    }
}

/// One colour out of a palette by slot index, matching `CUSTOM_SLOTS`.
pub fn palette_slot(p: &Palette, idx: usize) -> Color {
    match idx {
        0 => p.bg_deepest,
        1 => p.bg_surface,
        2 => p.bg_button,
        3 => p.bg_hover,
        4 => p.text_bright,
        5 => p.text_muted,
        6 => p.accent_green,
        7 => p.accent_teal,
        8 => p.accent_purple,
        9 => p.icon,
        _ => p.border,
    }
}

/// General-purpose swatches, packed 0x00RRGGBB: a neutral ramp then warm,
/// green and cool families. Four rows of eight, which is what the picker
/// lays out.
pub const SWATCHES: &[u32] = &[
    0x000000, 0x1A1A1A, 0x333333, 0x4D4D4D, 0x808080, 0xB3B3B3, 0xD9D9D9, 0xFFFFFF,
    0x7F1D1D, 0xC0392B, 0xE74C3C, 0xF1948A, 0x7C3A00, 0xD35400, 0xE67E22, 0xF5B041,
    0x14532D, 0x1E8449, 0x2ECC71, 0x82E0AA, 0x0E4C4C, 0x148F77, 0x1ABC9C, 0x76D7C4,
    0x1A3A6B, 0x2874A6, 0x3498DB, 0x85C1E9, 0x4A235A, 0x7D3C98, 0x9B59B6, 0xC39BD3,
];

/// The same slot taken from each built-in theme. Offered first in the
/// picker because mixing themes - say Void's background with Aurora's
/// accents - is the most likely thing someone wants.
pub fn theme_swatches(idx: usize) -> Vec<Color> {
    Theme::ALL
        .iter()
        .filter(|t| **t != Theme::Custom)
        .map(|t| palette_slot(t.palette(), idx))
        .collect()
}

pub fn swatch_color(packed: u32) -> Color {
    unpack(packed)
}

/// Copy a built-in palette into the custom slots - used to seed editing.
pub fn set_custom_palette(p: &Palette) {
    let vals = [
        p.bg_deepest, p.bg_surface, p.bg_button, p.bg_hover,
        p.text_bright, p.text_muted,
        p.accent_green, p.accent_teal, p.accent_purple,
        p.icon,
        p.border,
    ];
    for (i, c) in vals.iter().enumerate() {
        set_custom_slot(i, *c);
    }
}

pub fn set_theme(t: Theme) {
    ACTIVE.store(t as u8, Ordering::Relaxed);
}

pub fn active_theme() -> Theme {
    match ACTIVE.load(Ordering::Relaxed) {
        1 => Theme::PolarNight,
        2 => Theme::MidnightSun,
        3 => Theme::SeaBreeze,
        4 => Theme::Void,
        5 => Theme::Custom,
        _ => Theme::Aurora,
    }
}

// Accessors matching the names the old constants had, so call sites read
// the same aside from the parentheses. Each is an atomic load plus a field
// read - or an atomic load and unpack when a custom theme is active.
#[inline]
fn slot(idx: usize, pick: impl Fn(&'static Palette) -> Color) -> Color {
    let t = active_theme();
    if t == Theme::Custom { custom_slot(idx) } else { pick(t.palette()) }
}

#[inline] pub fn bg_deepest() -> Color { slot(0, |p| p.bg_deepest) }
#[inline] pub fn bg_surface() -> Color { slot(1, |p| p.bg_surface) }
#[inline] pub fn bg_button() -> Color { slot(2, |p| p.bg_button) }
#[inline] pub fn bg_hover() -> Color { slot(3, |p| p.bg_hover) }
#[inline] pub fn text_bright() -> Color { slot(4, |p| p.text_bright) }
#[inline] pub fn text_muted() -> Color { slot(5, |p| p.text_muted) }
#[inline] pub fn accent_green() -> Color { slot(6, |p| p.accent_green) }
#[inline] pub fn accent_teal() -> Color { slot(7, |p| p.accent_teal) }
#[inline] pub fn accent_purple() -> Color { slot(8, |p| p.accent_purple) }
#[inline] pub fn icon() -> Color { slot(9, |p| p.icon) }
#[inline] pub fn border() -> Color { slot(10, |p| p.border) }

/// The logo gradient's stops for the active theme. A custom theme has no
/// artwork of its own, so it uses its accent slots - there you're picking the
/// colours directly, so there's nothing to correct for.
pub fn logo_stops() -> [Color; 3] {
    let t = active_theme();
    if t == Theme::Custom {
        [custom_slot(8), custom_slot(7), custom_slot(6)]
    } else {
        t.palette().logo
    }
}
