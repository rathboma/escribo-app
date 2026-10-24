//! Design tokens, dark and light — the same values the old stylesheet used.

use gpui::{Hsla, rgb, rgb_to_hsla, rgba};

fn h(c: u32) -> Hsla {
    rgb_to_hsla(rgb(c))
}

fn ha(c: u32) -> Hsla {
    rgb_to_hsla(rgba(c))
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub accent: Hsla,
    pub brand: Hsla,
    pub bg: Hsla,
    pub surface: Hsla,
    pub sunken: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub canvas: Hsla,
    pub hover: Hsla,
    pub dot: Hsla,
    pub scrim: Hsla,
    pub pop_shadow: Hsla,
    pub danger: Hsla,
    pub close_hover: Hsla,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            dark: true,
            accent: h(0xA8365E),
            brand: h(0xFF007F),
            bg: h(0x1f1f24),
            surface: h(0x2a2a31),
            sunken: h(0x17171b),
            border: h(0x37373f),
            text: h(0xececf1),
            muted: h(0x9494a1),
            canvas: h(0x101013),
            hover: ha(0xffffff14),
            dot: ha(0x80808c38),
            scrim: ha(0x06060a99),
            pop_shadow: ha(0x00000080),
            danger: h(0xE5484D),
            close_hover: h(0xC42B1C),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            accent: h(0x9C2E55),
            brand: h(0xFF007F),
            bg: h(0xfbfbfc),
            surface: h(0xeeeef2),
            sunken: h(0xf3f3f6),
            border: h(0xdedee5),
            text: h(0x1b1b20),
            muted: h(0x71717e),
            canvas: h(0xe4e4e8),
            hover: ha(0x0000000e),
            dot: ha(0x7878874d),
            scrim: ha(0x14141c61),
            pop_shadow: ha(0x14141e2e),
            danger: h(0xE5484D),
            close_hover: h(0xC42B1C),
        }
    }

    pub fn for_settings(theme: crate::settings::Theme) -> Self {
        match theme {
            crate::settings::Theme::Dark => Self::dark(),
            crate::settings::Theme::Light => Self::light(),
        }
    }
}

pub const WHITE: Hsla = gpui::hsla(0.0, 0.0, 1.0, 1.0);

pub fn from_rgb(c: crate::doc::Rgb) -> Hsla {
    h(c.to_u32())
}

pub fn with_alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { alpha: a, ..c }
}
