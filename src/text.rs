//! Text for labels, emoji and the watermark: laid out with parley and drawn
//! glyph by glyph with tiny-skia, so the editor's on-screen bitmaps and the
//! exported PNG come from the same code and match exactly.
//!
//! A label is drawn the way the old fabric.js Textbox was: a white outline
//! painted first, a soft drop shadow, then the fill. Outline glyphs become
//! paths (so the outline is a real stroke); colour glyphs — emoji — are
//! rasterised by swash at the size they will show and drawn as images.

use crate::doc::{Emoji, Label, Measure, Rgb};
use crate::geom::{Affine, V2, v2};
use crate::raster;
use parley::fontique::Blob;
use parley::layout::PositionedLayoutItem;
use parley::style::{FontFamily, FontWeight, LineHeight, StyleProperty};
use parley::{FontContext, Layout, LayoutContext};
use std::collections::HashMap;
use std::sync::Arc as Rc;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{self, PathData as _};
use tiny_skia::{FillRule, Paint, Pixmap, PixmapPaint, Stroke};

pub const COOKIE_TTF: &[u8] = include_bytes!("../assets/fonts/Cookie-Regular.ttf");

/// fabric's line height (1.16) times its font-size multiplier (1.13).
const LABEL_LINE_HEIGHT: f32 = 1.16 * 1.13;
/// Emoji are laid out as a one-glyph line at fabric's default line height.
const EMOJI_LINE_HEIGHT: f32 = 1.13;

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    text: String,
    family: String,
    size_bits: u32,
    weight: u16,
    width_bits: u32,
    line_height_bits: u32,
}

/// One laid-out run of text and the box it fills, in text units.
pub struct TextLayout {
    pub layout: Layout<()>,
    pub size: V2,
}

/// What `draw_text` paints.
pub struct TextPaint {
    pub fill: Rgb,
    pub fill_alpha: f32,
    /// outline colour and width (text units), painted under the fill
    pub stroke: Option<(Rgb, f32)>,
    /// rgba colour, blur (canvas shadowBlur, text units) and offset (text
    /// units, applied unrotated — canvas shadows ignore rotation)
    pub shadow: Option<([u8; 4], f32, V2)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    index: u32,
    glyph: u32,
    size_bits: u32,
}

pub struct TextEngine {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    scaler: ScaleContext,
    cache: HashMap<LayoutKey, Rc<TextLayout>>,
    outlines: HashMap<GlyphKey, Option<Rc<tiny_skia::Path>>>,
    colors: HashMap<GlyphKey, Option<Rc<ColorGlyph>>>,
}

struct ColorGlyph {
    pixmap: Pixmap,
    left: f32,
    top: f32,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        fonts.collection.register_fonts(Blob::new(Rc::new(COOKIE_TTF)), None);
        Self {
            fonts,
            layouts: LayoutContext::new(),
            scaler: ScaleContext::new(),
            cache: HashMap::new(),
            outlines: HashMap::new(),
            colors: HashMap::new(),
        }
    }

    /// Lays out `text` in the given CSS family list. `wrap` is the width to
    /// break lines at, if any.
    pub fn layout(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        weight: u16,
        wrap: Option<f32>,
        line_height: f32,
    ) -> Rc<TextLayout> {
        let key = LayoutKey {
            text: text.to_string(),
            family: family.to_string(),
            size_bits: size.to_bits(),
            weight,
            width_bits: wrap.unwrap_or(-1.0).to_bits(),
            line_height_bits: line_height.to_bits(),
        };
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        // Emoji go to the colour emoji font first, as browsers send them:
        // otherwise any text font with a monochrome glyph for the character
        // (FreeMono has a 🔥) wins on fallback. Everything else ends its list
        // in the emoji font too, for symbols the text fonts lack.
        let stack = format!("{family}, emoji");
        let emoji_stack = format!("emoji, {family}");
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(stack.as_str())));
        for range in emoji_ranges(text) {
            builder.push(StyleProperty::FontFamily(FontFamily::from(emoji_stack.as_str())), range);
        }
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight as f32)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(line_height)));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(wrap);
        layout.align(parley::Alignment::Start, parley::AlignmentOptions::default());
        let w = layout.width().max(wrap.unwrap_or(0.0));
        let h = layout.height().max(size * line_height);
        let out = Rc::new(TextLayout { layout, size: v2(w, h) });
        if self.cache.len() > 512 {
            self.cache.clear();
        }
        self.cache.insert(key, out.clone());
        out
    }

    pub fn label_layout(&mut self, l: &Label) -> Rc<TextLayout> {
        let family = crate::settings::font_css(&l.font);
        self.layout(&l.text, &family, l.font_size, 900, Some(l.width), LABEL_LINE_HEIGHT)
    }

    pub fn emoji_layout(&mut self, e: &Emoji) -> Rc<TextLayout> {
        self.layout(&e.ch, "sans-serif", e.size, 400, None, EMOJI_LINE_HEIGHT)
    }

    /// The width of one line of text, for the watermark.
    pub fn measure_line(&mut self, text: &str, family: &str, size: f32, weight: u16) -> f32 {
        self.layout(text, family, size, weight, None, 1.0).layout.width()
    }

    /// The paint a label is drawn with: white outline, soft shadow, fill.
    pub fn label_paint(l: &Label) -> TextPaint {
        let u = l.unit;
        TextPaint {
            fill: l.color,
            fill_alpha: 1.0,
            stroke: Some((Rgb::WHITE, 2.0 * u)),
            shadow: Some(([0, 0, 0, 77], 2.0 * u, v2(2.0 * u, 2.0 * u))),
        }
    }

    /// The transform from a label's own frame (top-left of its box, unscaled)
    /// into image space.
    pub fn label_transform(l: &Label) -> Affine {
        Affine::scale(l.scale, l.scale).then(Affine::rotate(l.angle)).then(Affine::translate(l.origin))
    }

    /// The transform from an emoji's layout frame into image space: the glyph
    /// box is centred on the emoji's centre.
    pub fn emoji_transform(&mut self, e: &Emoji) -> Affine {
        let size = self.emoji_layout(e).size;
        Affine::translate(v2(-size.x / 2.0, -size.y / 2.0))
            .then(Affine::scale(e.scale, e.scale))
            .then(Affine::rotate(e.angle))
            .then(Affine::translate(e.center))
    }

    pub fn draw_label(&mut self, target: &mut Pixmap, l: &Label, to_target: Affine) {
        let layout = self.label_layout(l);
        let t = Self::label_transform(l).then(to_target);
        self.draw_text(target, &layout, t, &Self::label_paint(l));
    }

    pub fn draw_emoji(&mut self, target: &mut Pixmap, e: &Emoji, to_target: Affine) {
        let layout = self.emoji_layout(e);
        let t = self.emoji_transform(e).then(to_target);
        let paint = TextPaint { fill: Rgb::BLACK, fill_alpha: 1.0, stroke: None, shadow: None };
        self.draw_text(target, &layout, t, &paint);
    }

    /// Draws a laid-out text into `target` through `t` (text units to
    /// target pixels). `t` may scale and rotate but must not skew.
    pub fn draw_text(&mut self, target: &mut Pixmap, text: &TextLayout, t: Affine, paint: &TextPaint) {
        let dev_scale = t.scale_factor().max(1e-3);
        let glyphs = self.collect_glyphs(&text.layout, dev_scale);
        if glyphs.is_empty() {
            return;
        }

        // Shadow first: the union of outline, fill and colour glyphs, blurred
        // and offset in target space so it always falls down and to the right.
        if let Some((rgba, blur, offset)) = paint.shadow {
            let sigma = blur * dev_scale / 2.0;
            let off = v2(offset.x * dev_scale, offset.y * dev_scale);
            let stroke_w = paint.stroke.map(|s| s.1).unwrap_or(0.0);
            if let Some(bounds) = glyph_bounds(&glyphs, t, stroke_w * dev_scale / 2.0 + sigma * 3.0 + 2.0) {
                let (bx, by) = (bounds.0.floor(), bounds.1.floor());
                let (bw, bh) = ((bounds.2 - bx).ceil() as u32, (bounds.3 - by).ceil() as u32);
                if let Some(mut mask) = Pixmap::new(bw.max(1), bh.max(1)) {
                    let local = t.then(Affine::translate(v2(-bx, -by)));
                    let black = solid(Rgb::BLACK, 1.0);
                    for g in &glyphs {
                        match g {
                            Placed::Outline(path, at) => {
                                let gt = Affine::translate(*at).then(local).to_skia();
                                mask.fill_path(path, &black, FillRule::Winding, gt, None);
                                if stroke_w > 0.0 {
                                    mask.stroke_path(path, &black, &stroke(stroke_w), gt, None);
                                }
                            }
                            Placed::Color(img, at) => draw_color_glyph(&mut mask, img, *at, local, dev_scale),
                        }
                    }
                    raster::blur(&mut mask, sigma);
                    let shade = tiny_skia::ColorU8::from_rgba(rgba[0], rgba[1], rgba[2], 255).premultiply();
                    for px in mask.pixels_mut() {
                        let a = px.alpha() as u32 * rgba[3] as u32 / 255;
                        *px = tiny_skia::PremultipliedColorU8::from_rgba(
                            (shade.red() as u32 * a / 255) as u8,
                            (shade.green() as u32 * a / 255) as u8,
                            (shade.blue() as u32 * a / 255) as u8,
                            a as u8,
                        )
                        .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
                    }
                    target.draw_pixmap(
                        (bx + off.x).round() as i32,
                        (by + off.y).round() as i32,
                        mask.as_ref(),
                        &PixmapPaint::default(),
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            }
        }

        if let Some((color, width)) = paint.stroke {
            let p = solid(color, 1.0);
            for g in &glyphs {
                if let Placed::Outline(path, at) = g {
                    target.stroke_path(path, &p, &stroke(width), Affine::translate(*at).then(t).to_skia(), None);
                }
            }
        }

        let fill = solid(paint.fill, paint.fill_alpha);
        for g in &glyphs {
            match g {
                Placed::Outline(path, at) => {
                    target.fill_path(path, &fill, FillRule::Winding, Affine::translate(*at).then(t).to_skia(), None);
                }
                Placed::Color(img, at) => draw_color_glyph(target, img, *at, t, dev_scale),
            }
        }
    }

    /// Every glyph in the layout, positioned in text units: outline glyphs as
    /// paths (font units scaled to the run's size, y down), colour glyphs as
    /// images rasterised for `dev_scale`.
    fn collect_glyphs(&mut self, layout: &Layout<()>, dev_scale: f32) -> Vec<Placed> {
        let mut out = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let r = run.run();
                let font = r.font();
                let size = r.font_size();
                let coords: Vec<i16> = r.normalized_coords().to_vec();
                let embolden = r.synthesis().embolden();
                let skew = r.synthesis().skew();
                let Some(font_ref) = swash::FontRef::from_index(font.data.as_ref(), font.index as usize) else {
                    continue;
                };
                let font_id = font.data.id();
                for g in run.positioned_glyphs() {
                    let key = GlyphKey {
                        font: font_id,
                        index: font.index,
                        glyph: g.id,
                        size_bits: (size * dev_scale).to_bits(),
                    };
                    let at = v2(g.x, g.y);
                    if let Some(img) = self.color_glyph(font_ref, &coords, key, size * dev_scale) {
                        out.push(Placed::Color(img, at));
                        continue;
                    }
                    let okey = GlyphKey { size_bits: size.to_bits(), ..key };
                    if let Some(path) = self.outline(font_ref, &coords, okey, size, embolden, skew) {
                        out.push(Placed::Outline(path, at));
                    }
                }
            }
        }
        out
    }

    fn color_glyph(&mut self, font: swash::FontRef, coords: &[i16], key: GlyphKey, px: f32) -> Option<Rc<ColorGlyph>> {
        if let Some(hit) = self.colors.get(&key) {
            return hit.clone();
        }
        let result = (|| {
            let mut scaler = self.scaler.builder(font).size(px).hint(false).normalized_coords(coords).build();
            if !scaler.has_color_outlines() && !scaler.has_color_bitmaps() {
                return None;
            }
            let image = Render::new(&[Source::ColorOutline(0), Source::ColorBitmap(StrikeWith::BestFit)])
                .render(&mut scaler, key.glyph as u16)?;
            if image.content != Content::Color || image.placement.width == 0 || image.placement.height == 0 {
                return None;
            }
            let premultiplied = matches!(image.source, Source::ColorOutline(_));
            let mut pm = Pixmap::new(image.placement.width, image.placement.height)?;
            for (dst, src) in pm.pixels_mut().iter_mut().zip(image.data.chunks_exact(4)) {
                *dst = if premultiplied {
                    tiny_skia::PremultipliedColorU8::from_rgba(src[0], src[1], src[2], src[3])
                        .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT)
                } else {
                    tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply()
                };
            }
            Some(Rc::new(ColorGlyph { pixmap: pm, left: image.placement.left as f32, top: image.placement.top as f32 }))
        })();
        if self.colors.len() > 1024 {
            self.colors.clear();
        }
        self.colors.insert(key, result.clone());
        result
    }

    fn outline(
        &mut self,
        font: swash::FontRef,
        coords: &[i16],
        key: GlyphKey,
        size: f32,
        embolden: bool,
        skew: Option<f32>,
    ) -> Option<Rc<tiny_skia::Path>> {
        if let Some(hit) = self.outlines.get(&key) {
            return hit.clone();
        }
        let result = (|| {
            let mut scaler = self.scaler.builder(font).size(size).hint(false).normalized_coords(coords).build();
            let mut outline = scaler.scale_outline(key.glyph as u16)?;
            if embolden {
                // Synthetic bold for a family without a heavy face, about as
                // strong as browsers make it.
                let s = size / 24.0;
                outline.embolden(s, s);
            }
            if let Some(deg) = skew {
                outline.transform(&zeno::Transform::skew(zeno::Angle::from_degrees(deg), zeno::Angle::ZERO));
            }
            let mut pb = tiny_skia::PathBuilder::new();
            // Font outlines are y-up; layout space is y-down.
            for cmd in outline.path().commands() {
                match cmd {
                    zeno::Command::MoveTo(p) => pb.move_to(p.x, -p.y),
                    zeno::Command::LineTo(p) => pb.line_to(p.x, -p.y),
                    zeno::Command::QuadTo(c, p) => pb.quad_to(c.x, -c.y, p.x, -p.y),
                    zeno::Command::CurveTo(a, b, p) => pb.cubic_to(a.x, -a.y, b.x, -b.y, p.x, -p.y),
                    zeno::Command::Close => pb.close(),
                }
            }
            pb.finish().map(Rc::new)
        })();
        if self.outlines.len() > 4096 {
            self.outlines.clear();
        }
        self.outlines.insert(key, result.clone());
        result
    }
}

impl Measure for TextEngine {
    fn label_size(&mut self, label: &Label) -> V2 {
        self.label_layout(label).size
    }

    fn emoji_size(&mut self, emoji: &Emoji) -> V2 {
        self.emoji_layout(emoji).size
    }
}

enum Placed {
    Outline(Rc<tiny_skia::Path>, V2),
    Color(Rc<ColorGlyph>, V2),
}

fn solid(c: Rgb, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

fn stroke(width: f32) -> Stroke {
    Stroke { width, line_join: tiny_skia::LineJoin::Round, line_cap: tiny_skia::LineCap::Round, ..Default::default() }
}

/// A colour glyph was rasterised `dev_scale` times larger than text units;
/// draw it back down through `t`.
fn draw_color_glyph(target: &mut Pixmap, g: &ColorGlyph, at: V2, t: Affine, dev_scale: f32) {
    let inv = 1.0 / dev_scale;
    let gt = Affine::scale(inv, inv).then(Affine::translate(at + v2(g.left * inv, -g.top * inv))).then(t);
    let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
    target.draw_pixmap(0, 0, g.pixmap.as_ref(), &paint, gt.to_skia(), None);
}

/// Target-space bounds of every glyph, grown by `pad`: (x0, y0, x1, y1).
fn glyph_bounds(glyphs: &[Placed], t: Affine, pad: f32) -> Option<(f32, f32, f32, f32)> {
    let mut lo = v2(f32::INFINITY, f32::INFINITY);
    let mut hi = v2(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut grow = |p: V2| {
        lo = lo.min(p);
        hi = hi.max(p);
    };
    for g in glyphs {
        let (at, b) = match g {
            Placed::Outline(path, at) => {
                let b = path.bounds();
                (*at, (b.left(), b.top(), b.right(), b.bottom()))
            }
            Placed::Color(img, at) => {
                // Colour glyph boxes are in device pixels; any scale is fine
                // for a bound, so take a generous one.
                let s = 1.0 / t.scale_factor().max(1e-3);
                let w = img.pixmap.width() as f32 * s;
                let h = img.pixmap.height() as f32 * s;
                let l = img.left * s;
                let top = -img.top * s;
                (*at, (l, top, l + w, top + h))
            }
        };
        for c in [v2(b.0, b.1), v2(b.2, b.1), v2(b.2, b.3), v2(b.0, b.3)] {
            grow(t.apply(at + c));
        }
    }
    (lo.x.is_finite()).then_some((lo.x - pad, lo.y - pad, hi.x + pad, hi.y + pad))
}

/// Byte ranges of `text` that should render as colour emoji: characters with
/// emoji presentation, anything followed by the emoji variation selector,
/// and the joiners, modifiers and tags that glue a sequence together.
pub fn emoji_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, &(at, c)) in chars.iter().enumerate() {
        let next = chars.get(i + 1).map(|&(_, n)| n);
        let glue = matches!(c as u32, 0x200D | 0xFE0F | 0x20E3 | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F);
        let emoji = is_emoji_presentation(c) || next == Some('\u{FE0F}') || next == Some('\u{20E3}');
        let continues = glue && out.last().is_some_and(|r| r.end == at);
        if emoji || continues {
            let end = at + c.len_utf8();
            match out.last_mut() {
                Some(r) if r.end == at => r.end = end,
                _ => out.push(at..end),
            }
        }
    }
    out
}

fn is_emoji_presentation(c: char) -> bool {
    matches!(c as u32,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD..=0x25FE
        | 0x2614..=0x2615 | 0x2648..=0x2653 | 0x267F | 0x2693 | 0x26A1 | 0x26AA..=0x26AB
        | 0x26BD..=0x26BE | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3
        | 0x26F5 | 0x26FA | 0x26FD | 0x2705 | 0x270A..=0x270B | 0x2728 | 0x274C | 0x274E
        | 0x2753..=0x2755 | 0x2757 | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C
        | 0x2B50 | 0x2B55 | 0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A
        | 0x1F1E6..=0x1F1FF | 0x1F201 | 0x1F21A | 0x1F22F | 0x1F232..=0x1F236
        | 0x1F238..=0x1F23A | 0x1F250..=0x1F251 | 0x1F300..=0x1F320 | 0x1F32D..=0x1F335
        | 0x1F337..=0x1F37C | 0x1F37E..=0x1F393 | 0x1F3A0..=0x1F3CA | 0x1F3CF..=0x1F3D3
        | 0x1F3E0..=0x1F3F0 | 0x1F3F4 | 0x1F3F8..=0x1F43E | 0x1F440 | 0x1F442..=0x1F4FC
        | 0x1F4FF..=0x1F53D | 0x1F54B..=0x1F54E | 0x1F550..=0x1F567 | 0x1F57A
        | 0x1F595..=0x1F596 | 0x1F5A4 | 0x1F5FB..=0x1F64F | 0x1F680..=0x1F6C5 | 0x1F6CC
        | 0x1F6D0..=0x1F6D2 | 0x1F6D5..=0x1F6D7 | 0x1F6DC..=0x1F6DF | 0x1F6EB..=0x1F6EC
        | 0x1F6F4..=0x1F6FC | 0x1F7E0..=0x1F7EB | 0x1F7F0 | 0x1F90C..=0x1F93A
        | 0x1F93C..=0x1F945 | 0x1F947..=0x1F9FF | 0x1FA70..=0x1FAFF)
}

/// Exposed for the caret: the families a label is laid out in.
pub fn label_family(l: &Label) -> String {
    crate::settings::font_css(&l.font)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::BRAND;

    fn label(text: &str) -> Label {
        Label {
            origin: v2(10.0, 10.0),
            angle: 0.0,
            scale: 1.0,
            width: 250.0,
            font_size: 22.0,
            unit: 1.0,
            color: BRAND,
            font: "system".into(),
            text: text.into(),
        }
    }

    #[test]
    fn labels_wrap_at_their_width() {
        let mut t = TextEngine::new();
        let one = t.label_size(&label("text"));
        assert_eq!(one.x, 250.0);
        let many = t.label_size(&label("a long label that has to wrap onto more than one line here"));
        assert!(many.y > one.y * 1.5, "{many:?} vs {one:?}");
    }

    #[test]
    fn draws_ink_with_outline_and_shadow() {
        let mut t = TextEngine::new();
        let mut pm = Pixmap::new(300, 80).unwrap();
        t.draw_label(&mut pm, &label("Hello"), Affine::IDENTITY);
        let pink = pm.pixels().iter().filter(|p| p.alpha() == 255 && p.red() > 200 && p.green() < 60).count();
        let white = pm.pixels().iter().filter(|p| p.alpha() == 255 && p.red() > 240 && p.green() > 240).count();
        let soft = pm.pixels().iter().filter(|p| p.alpha() > 0 && p.alpha() < 120).count();
        assert!(pink > 50, "fill drawn: {pink}");
        assert!(white > 20, "outline drawn: {white}");
        assert!(soft > 20, "shadow drawn: {soft}");
    }

    #[test]
    fn emoji_render_in_colour() {
        let mut t = TextEngine::new();
        let e = Emoji { center: v2(60.0, 60.0), angle: 0.0, scale: 1.0, size: 100.0, ch: "🔥".into() };
        let size = t.emoji_size(&e);
        assert!(size.x > 50.0 && size.y > 50.0, "{size:?}");
        let mut pm = Pixmap::new(120, 120).unwrap();
        t.draw_emoji(&mut pm, &e, Affine::IDENTITY);
        let colourful =
            pm.pixels().iter().filter(|p| p.alpha() > 200 && (p.red() as i32 - p.green() as i32).abs() > 60).count();
        assert!(colourful > 200, "emoji drawn in colour: {colourful}");
    }

    #[test]
    fn finds_emoji_runs() {
        assert_eq!(emoji_ranges("a🔥b"), vec![1..5]);
        assert_eq!(emoji_ranges("⚠️ ok"), vec![0..6]);
        assert_eq!(emoji_ranges("👍🏽"), vec![0..8]);
        assert!(emoji_ranges("plain text → arrows").is_empty());
    }

    #[test]
    fn cookie_is_available() {
        let mut t = TextEngine::new();
        let w = t.measure_line("e", "Cookie", 100.0, 400);
        assert!(w > 10.0 && w < 80.0, "{w}");
    }
}
