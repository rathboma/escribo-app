//! The CPU renderer: the exported PNG (fill + shot + annotations +
//! watermark, at the chosen size), and the same annotation and watermark
//! drawing the editor shows on screen.

use crate::doc::{Annotation, Arrow, BRAND, BoxShape, Document, Layout, Rgb, Shape, Shot, rgb};
use crate::geom::{Affine, Path, Rect, V2, v2};
use crate::raster;
use crate::settings::{Settings, UI_FONT, WmMode};
use crate::text::{TextEngine, TextPaint};
use tiny_skia::{
    BlendMode, FillRule, FilterQuality, GradientStop, LinearGradient, Mask, Paint, Pixmap, PixmapPaint, Point,
    SpreadMode, Stroke,
};

pub const WATERMARK_TEXT: &str = "made with escriboapp.com";
/// The pill's usual translucency, so a custom background keeps the same soft
/// look the default white has.
pub const WM_BG_ALPHA: f32 = 0.86;

pub fn paint(c: Rgb, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

/// The fill for backdrop index 1-8 (0 is none) over `w × h`: a solid colour,
/// or a gradient from the top-left corner to the bottom-right.
pub fn backdrop_paint(settings: &Settings, index: u8, w: f32, h: f32) -> Option<Paint<'static>> {
    let kit = settings.kit();
    match index {
        0 => None,
        1..=4 => Some(paint(rgb(&kit.solid_bg[index as usize - 1]), 1.0)),
        _ => {
            let g = &kit.grad_bg[(index as usize - 5).min(3)];
            let (a, b) = (rgb(&g.a), rgb(&g.b));
            let shader = LinearGradient::new(
                Point::from_xy(0.0, 0.0),
                Point::from_xy(w.max(1.0), h.max(1.0)),
                vec![
                    GradientStop::new(0.0, tiny_skia::Color::from_rgba8(a.r, a.g, a.b, 255)),
                    GradientStop::new(1.0, tiny_skia::Color::from_rgba8(b.r, b.g, b.b, 255)),
                ],
                SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            )?;
            Some(Paint { shader, anti_alias: true, ..Default::default() })
        }
    }
}

/// Factor between the natural layout (shot at native resolution) and the
/// PNG. Scales are applied exactly — 1× is lossless and 2× an integer
/// upscale — and only an explicit width makes it fractional.
pub fn export_scale(settings: &Settings, layout: &Layout) -> f64 {
    match settings.style.out_w {
        None => settings.style.out_scale,
        Some(w) => w.clamp(crate::settings::OUT_W_MIN, crate::settings::OUT_W_MAX) as f64 / layout.out_w,
    }
}

pub fn export_size(settings: &Settings, layout: &Layout) -> (u32, u32) {
    let k = export_scale(settings, layout);
    ((layout.out_w * k).round().max(1.0) as u32, (layout.out_h * k).round().max(1.0) as u32)
}

pub fn layout_for(settings: &Settings, shot: &Shot) -> Layout {
    crate::doc::natural_layout(shot.width as f64, shot.height as f64, settings.style.pad, settings.style.ratio().value)
}

/// Draws the screenshot's drop shadow: the shot's rounded rect, blurred,
/// under where the shot will go. Only the shadow — a transparent screenshot
/// shows the fill through it rather than a black slab.
fn draw_shadow(target: &mut Pixmap, shot: Rect, radius: f32, blur: f32, offset_y: f32, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    let sigma = blur / 2.0;
    let pad = (sigma * 3.0).ceil() + 2.0;
    let (bx, by) = ((shot.x - pad).floor(), (shot.y - pad).floor());
    let (bw, bh) = ((shot.w + pad * 2.0).ceil() as u32 + 1, (shot.h + pad * 2.0).ceil() as u32 + 1);
    let Some(mut layer) = Pixmap::new(bw, bh) else {
        return;
    };
    let Some(path) = Path::round_rect(shot.w, shot.h, radius)
        .transformed(&Affine::translate(v2(shot.x - bx + shot.w / 2.0, shot.y - by + shot.h / 2.0)))
        .to_skia()
    else {
        return;
    };
    layer.fill_path(&path, &paint(Rgb::BLACK, opacity), FillRule::Winding, tiny_skia::Transform::identity(), None);
    raster::blur(&mut layer, sigma);
    // The shot covers its own rect; the shadow shows only around it.
    target.draw_pixmap(
        bx as i32,
        (by + offset_y).round() as i32,
        layer.as_ref(),
        &PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

/// Composes the full export at the chosen size. Returns straight-alpha RGBA.
pub fn compose(doc: &Document, settings: &Settings, text: &mut TextEngine) -> Option<(u32, u32, Vec<u8>)> {
    let shot = &doc.shot;
    let layout = layout_for(settings, shot);
    let k = export_scale(settings, &layout) as f32;
    let (out_w, out_h) = export_size(settings, &layout);
    let style = &settings.style;
    let adv = settings.kit().advanced;
    let shot_rect =
        Rect::new(layout.shot_x as f32 * k, layout.shot_y as f32 * k, shot.width as f32 * k, shot.height as f32 * k);
    let radius = if style.pad > 0 { shot_rect.w * (adv.corner_radius as f32 / 100.0) } else { 0.0 };

    let mut out = Pixmap::new(out_w, out_h)?;

    // Margins and extra canvas take the fill; without one they stay transparent.
    if let Some(fill) = backdrop_paint(settings, style.backdrop, out_w as f32, out_h as f32) {
        out.fill_rect(
            tiny_skia::Rect::from_xywh(0.0, 0.0, out_w as f32, out_h as f32)?,
            &fill,
            tiny_skia::Transform::identity(),
            None,
        );
        if style.pad > 0 {
            let u = shot.unit() * k;
            draw_shadow(
                &mut out,
                shot_rect,
                radius,
                adv.shadow_blur as f32 * u,
                adv.shadow_offset_y as f32 * u,
                adv.shadow_opacity as f32 / 100.0,
            );
        }
    }

    draw_shot(&mut out, shot, shot_rect, radius);

    // Annotations go over everything, margin included. They are drawn on a
    // layer of their own, as the old canvas did, so a highlighter multiplies
    // with the marks beneath it but not with the screenshot.
    let mut layer = Pixmap::new(out_w, out_h)?;
    let to_out = Affine::scale(k, k).then(Affine::translate(v2(shot_rect.x, shot_rect.y)));
    for a in &doc.annotations {
        draw_annotation(&mut layer, a, shot, to_out, text);
    }
    out.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), tiny_skia::Transform::identity(), None);

    if style.watermark {
        draw_watermark(&mut out, settings, text, out_w as f32, out_h as f32, Affine::IDENTITY);
    }

    Some((out_w, out_h, raster::to_rgba_straight(&out)))
}

/// The screenshot at `rect`, clipped to its rounded corners. Native size is
/// copied pixel for pixel; anything else is resampled.
fn draw_shot(out: &mut Pixmap, shot: &Shot, rect: Rect, radius: f32) {
    let Some(src) = raster::pixmap_from_rgba(shot.width, shot.height, &shot.rgba) else {
        return;
    };
    let mask = if radius > 0.0 {
        let mut m = Mask::new(out.width(), out.height());
        let path = Path::round_rect(rect.w, rect.h, radius).transformed(&Affine::translate(rect.center())).to_skia();
        if let (Some(m), Some(path)) = (m.as_mut(), path) {
            m.fill_path(&path, FillRule::Winding, true, tiny_skia::Transform::identity());
        }
        m
    } else {
        None
    };
    let sx = rect.w / shot.width as f32;
    let sy = rect.h / shot.height as f32;
    let exact = (sx - 1.0).abs() < 1e-6 && (sy - 1.0).abs() < 1e-6;
    let quality = if exact { FilterQuality::Nearest } else { FilterQuality::Bicubic };
    // Bicubic alone aliases text badly past 2:1, so a big downscale steps
    // down in halves first, as a mipmap would.
    let (src, sx, sy) = if sx < 0.5 { halve_until(src, sx, sy) } else { (src, sx, sy) };
    let t = tiny_skia::Transform::from_row(sx, 0.0, 0.0, sy, rect.x, rect.y);
    let pp = PixmapPaint { quality, blend_mode: BlendMode::SourceOver, opacity: 1.0 };
    if exact {
        out.draw_pixmap(
            rect.x.round() as i32,
            rect.y.round() as i32,
            src.as_ref(),
            &pp,
            tiny_skia::Transform::identity(),
            mask.as_ref(),
        );
    } else {
        out.draw_pixmap(0, 0, src.as_ref(), &pp, t, mask.as_ref());
    }
}

/// Box-filters `src` down by 2 until the remaining scale is at least ½, so the
/// final resample never skips source pixels.
fn halve_until(mut src: Pixmap, mut sx: f32, mut sy: f32) -> (Pixmap, f32, f32) {
    while sx < 0.5 && sy < 0.5 && src.width() > 1 && src.height() > 1 {
        let (w, h) = (src.width() / 2, src.height() / 2);
        let Some(mut half) = Pixmap::new(w, h) else { break };
        let sw = src.width() as usize;
        let s = src.data();
        let d = half.data_mut();
        for y in 0..h as usize {
            for x in 0..w as usize {
                for c in 0..4 {
                    let i = |xx: usize, yy: usize| s[(yy * sw + xx) * 4 + c] as u32;
                    let v = i(2 * x, 2 * y) + i(2 * x + 1, 2 * y) + i(2 * x, 2 * y + 1) + i(2 * x + 1, 2 * y + 1);
                    d[(y * w as usize + x) * 4 + c] = ((v + 2) / 4) as u8;
                }
            }
        }
        sx *= src.width() as f32 / w as f32;
        sy *= src.height() as f32 / h as f32;
        src = half;
    }
    (src, sx, sy)
}

/// Draws one annotation into `target`; `t` maps image space to target pixels.
pub fn draw_annotation(target: &mut Pixmap, a: &Annotation, shot: &Shot, t: Affine, text: &mut TextEngine) {
    match &a.shape {
        Shape::Arrow(arrow) => draw_arrow(target, arrow, t),
        Shape::Rect(b) => draw_box(target, b, t, false),
        Shape::Mark(b) => draw_box(target, b, t, true),
        Shape::Redact(r) => draw_redact(target, *r, shot, t),
        Shape::Text(l) => text.draw_label(target, l, t),
        Shape::Emoji(e) => text.draw_emoji(target, e, t),
    }
}

pub fn draw_arrow(target: &mut Pixmap, arrow: &Arrow, t: Affine) {
    let Some(path) = Path::polygon(&arrow.outline()).to_skia() else {
        return;
    };
    let ts = t.to_skia();
    target.fill_path(&path, &paint(arrow.color, 1.0), FillRule::Winding, ts, None);
    let stroke = Stroke { width: arrow.stroke_width(), line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    target.stroke_path(&path, &paint(Rgb::WHITE, 1.0), &stroke, ts, None);
}

pub fn draw_box(target: &mut Pixmap, b: &BoxShape, t: Affine, highlight: bool) {
    let Some(path) = b.path().to_skia() else {
        return;
    };
    let ts = t.to_skia();
    if highlight {
        let mut p = paint(b.color, b.alpha);
        p.blend_mode = BlendMode::Multiply;
        target.fill_path(&path, &p, FillRule::Winding, ts, None);
    } else {
        let stroke = Stroke { width: b.stroke, ..Default::default() };
        target.stroke_path(&path, &paint(b.color, 1.0), &stroke, ts, None);
    }
}

/// Paints the scrambled blocks over `box_`, clipped to the box and to the
/// shot: whole blocks go down and the clip trims them, which keeps the grid
/// anchored to the screenshot, so a box can move without the blocks churning.
pub fn draw_redact(target: &mut Pixmap, box_: Rect, shot: &Shot, t: Affine) {
    let Some(clip) = box_.intersect(&Rect::new(0.0, 0.0, shot.width as f32, shot.height as f32)) else {
        return;
    };
    let tl = t.apply(v2(clip.x, clip.y));
    let br = t.apply(v2(clip.right(), clip.bottom()));
    let Some(clip_px) = raster::clip_rect(
        tl.x.min(br.x),
        tl.y.min(br.y),
        tl.x.max(br.x),
        tl.y.max(br.y),
        target.width(),
        target.height(),
    ) else {
        return;
    };
    let Some(plate) = shot.plate() else {
        // Unreadable shot: a solid fill hides as much as the blocks do.
        if let Some(r) = tiny_skia::Rect::from_xywh(
            clip_px.x() as f32,
            clip_px.y() as f32,
            clip_px.width() as f32,
            clip_px.height() as f32,
        ) {
            target.fill_rect(r, &paint(Rgb::new(0x10, 0x10, 0x14), 1.0), tiny_skia::Transform::identity(), None);
        }
        return;
    };
    let Some((c0, r0, c1, r1)) = plate.span(clip.x, clip.y, clip.w, clip.h) else {
        return;
    };
    let b = plate.block as f32;
    // Block edges snap to whole target pixels so neighbours meet with no seam.
    for row in r0..r1 {
        for col in c0..c1 {
            let [r, g, bl, a] = plate.pixel(col, row);
            if a == 0 {
                continue;
            }
            let p0 = t.apply(v2(col as f32 * b, row as f32 * b));
            let p1 = t.apply(v2((col + 1) as f32 * b, (row + 1) as f32 * b));
            let x0 = (p0.x.round() as i32).max(clip_px.left());
            let y0 = (p0.y.round() as i32).max(clip_px.top());
            let x1 = (p1.x.round() as i32).min(clip_px.right());
            let y1 = (p1.y.round() as i32).min(clip_px.bottom());
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let mut p = paint(Rgb::new(r, g, bl), a as f32 / 255.0);
            p.anti_alias = false;
            if let Some(rect) = tiny_skia::Rect::from_ltrb(x0 as f32, y0 as f32, x1 as f32, y1 as f32) {
                target.fill_rect(rect, &p, tiny_skia::Transform::identity(), None);
            }
        }
    }
}

/// The watermark's text: the escribo wordmark, or the profile's custom text
/// when it has any.
pub fn watermark_text(settings: &Settings) -> String {
    let kit = settings.kit();
    let custom = kit.wm_text.trim();
    if kit.wm_mode == WmMode::Custom && !custom.is_empty() { custom.to_string() } else { WATERMARK_TEXT.to_string() }
}

/// The "e" mark rides along only with the default branding.
pub fn watermark_has_icon(settings: &Settings) -> bool {
    settings.kit().wm_mode != WmMode::Custom
}

/// Geometry of the watermark pill for an output `out_w × out_h`.
pub struct WatermarkBox {
    pub rect: Rect,
    pub size: f32,
}

pub fn watermark_box(settings: &Settings, text: &mut TextEngine, out_w: f32, out_h: f32) -> WatermarkBox {
    let size = out_w * 0.0125;
    let inset = out_w * 0.013;
    let pad_x = size * 0.65;
    let pad_y = size * 0.3;
    let gap = size * 0.4;
    let icon = size * 0.95;
    let lead = if watermark_has_icon(settings) { icon + gap } else { 0.0 };
    let text_w = text.measure_line(&watermark_text(settings), UI_FONT, size, 600);
    let w = pad_x * 2.0 + lead + text_w;
    let h = pad_y * 2.0 + icon.max(size);
    WatermarkBox { rect: Rect::new(out_w - inset - w, out_h - inset - h, w, h), size }
}

/// Bottom-right of the output, sized and inset off its width, so it lands in
/// the same corner whether or not there is padding or extra canvas. `t` maps
/// output pixels to target pixels.
pub fn draw_watermark(
    target: &mut Pixmap,
    settings: &Settings,
    text: &mut TextEngine,
    out_w: f32,
    out_h: f32,
    t: Affine,
) {
    let kit = settings.kit();
    let WatermarkBox { rect, size } = watermark_box(settings, text, out_w, out_h);
    let icon = size * 0.95;
    let pad_x = size * 0.65;
    let gap = size * 0.4;
    let has_icon = watermark_has_icon(settings);
    let lead = if has_icon { icon + gap } else { 0.0 };
    let s = t.scale_factor();

    // The pill and its soft shadow.
    if let Some(path) =
        Path::round_rect(rect.w, rect.h, size * 0.45).transformed(&Affine::translate(rect.center()).then(t)).to_skia()
    {
        let blur = size * 0.3 * s;
        let pad = (blur * 1.5).ceil() + 2.0;
        let b = path.bounds();
        if let Some(mut layer) =
            Pixmap::new((b.width() + pad * 2.0).ceil() as u32, (b.height() + pad * 2.0).ceil() as u32)
        {
            let shift = tiny_skia::Transform::from_translate(pad - b.left(), pad - b.top());
            layer.fill_path(&path, &paint(Rgb::BLACK, 0.12), FillRule::Winding, shift, None);
            raster::blur(&mut layer, blur / 2.0);
            target.draw_pixmap(
                (b.left() - pad).round() as i32,
                (b.top() - pad + size * 0.1 * s).round() as i32,
                layer.as_ref(),
                &PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }
        let bg = Rgb::parse(&kit.wm_bg).unwrap_or(Rgb::WHITE);
        target.fill_path(&path, &paint(bg, WM_BG_ALPHA), FillRule::Winding, tiny_skia::Transform::identity(), None);
    }

    let cy = rect.y + rect.h / 2.0;
    if has_icon {
        let c = v2(rect.x + pad_x + icon / 2.0, cy);
        draw_mark(target, text, c, icon, t);
    }

    let label = watermark_text(settings);
    let layout = text.layout(&label, UI_FONT, size, 600, None, 1.0);
    // Vertically centred on the pill, like textBaseline 'middle'.
    let origin = v2(rect.x + pad_x + lead, cy - layout.size.y / 2.0);
    let color = Rgb::parse(&kit.wm_color).unwrap_or(Rgb::new(0x7c, 0x7c, 0x88));
    text.draw_text(
        target,
        &layout,
        Affine::translate(origin).then(t),
        &TextPaint { fill: color, fill_alpha: 1.0, stroke: None, shadow: None },
    );
}

/// The brand mark: a white Cookie "e" centred in a pink circle of diameter
/// `d`, centred on `c` (output pixels, mapped through `t`).
pub fn draw_mark(target: &mut Pixmap, text: &mut TextEngine, c: V2, d: f32, t: Affine) {
    let mut pb = tiny_skia::PathBuilder::new();
    let tc = t.apply(c);
    pb.push_circle(tc.x, tc.y, d / 2.0 * t.scale_factor());
    if let Some(circle) = pb.finish() {
        target.fill_path(&circle, &paint(BRAND, 1.0), FillRule::Winding, tiny_skia::Transform::identity(), None);
    }
    // Same glyph maths as the old .mark rule: 1.3245 makes the "e"'s ink half
    // the circle's diameter, and the offsets recentre it.
    let glyph = d * 1.3245;
    let layout = text.layout("e", "Cookie", glyph, 400, None, 1.0);
    let w = layout.size.x;
    let h = layout.size.y;
    let origin = c + v2(-w / 2.0 - glyph * 0.03375, -h / 2.0 - glyph * 0.06375);
    text.draw_text(
        target,
        &layout,
        Affine::translate(origin).then(t),
        &TextPaint { fill: Rgb::WHITE, fill_alpha: 1.0, stroke: None, shadow: None },
    );
}

/// The PNG's file name: the screenshot's own name, marked as annotated.
pub fn export_name(name: &str) -> String {
    let base = match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    };
    let base = if base.is_empty() { "escribo" } else { base };
    format!("{base}-annotated.png")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Document, Label, Shot};
    use crate::geom::OrientedBox;
    use serde_json::Value;

    fn doc(w: u32, h: u32) -> Document {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255]);
            }
        }
        Document::new(Shot::new(w, h, px), "shot.png".into())
    }

    fn at(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn plain_export_is_the_shot_pixel_for_pixel() {
        let d = doc(64, 48);
        let mut s = Settings::from_value(&Value::Null);
        s.style.watermark = false;
        let (w, h, px) = compose(&d, &s, &mut TextEngine::new()).unwrap();
        assert_eq!((w, h), (64, 48));
        assert_eq!(px, *d.shot.rgba);
    }

    #[test]
    fn padding_fill_and_scale() {
        let d = doc(100, 50);
        let mut s = Settings::from_value(&Value::Null);
        s.style.watermark = false;
        s.style.pad = 10;
        s.style.backdrop = 1;
        s.style.out_scale = 2.0;
        let (w, h, px) = compose(&d, &s, &mut TextEngine::new()).unwrap();
        assert_eq!((w, h), (250, 150));
        // A corner of the margin carries the Ink fill.
        assert_eq!(at(&px, w, 1, 1), [0x10, 0x10, 0x14, 255]);
    }

    #[test]
    fn explicit_width_and_ratio() {
        let d = doc(400, 400);
        let mut s = Settings::from_value(&Value::Null);
        s.style.watermark = false;
        s.style.ratio = "16:9".into();
        s.style.out_w = Some(800);
        let (w, h, px) = compose(&d, &s, &mut TextEngine::new()).unwrap();
        assert_eq!((w, h), (800, 450));
        // Extra canvas without a fill stays transparent.
        assert_eq!(at(&px, w, 2, 225)[3], 0);
    }

    #[test]
    fn annotations_and_watermark_land_in_the_png() {
        let mut d = doc(900, 500);
        d.add(Shape::Rect(BoxShape {
            frame: OrientedBox { center: v2(450.0, 250.0), w: 200.0, h: 100.0, angle: 0.0 },
            color: BRAND,
            stroke: 5.0,
            radius: 2.0,
            alpha: 1.0,
        }));
        d.add(Shape::Redact(Rect::new(10.0, 10.0, 100.0, 40.0)));
        d.add(Shape::Text(Label {
            origin: v2(20.0, 300.0),
            angle: 0.3,
            scale: 1.0,
            width: 250.0,
            font_size: 22.0,
            unit: 1.0,
            color: BRAND,
            font: "system".into(),
            text: "label".into(),
        }));
        let s = Settings::from_value(&Value::Null);
        let (w, _h, px) = compose(&d, &s, &mut TextEngine::new()).unwrap();
        // Box stroke on the left edge.
        assert_eq!(at(&px, w, 350, 250), [0xFF, 0x00, 0x7F, 255]);
        // The redacted region is flat blocks, where the shot was a gradient.
        assert_eq!(at(&px, w, 13, 13), at(&px, w, 22, 22));
        assert_ne!(at(&d.shot.rgba, 900, 13, 13), at(&d.shot.rgba, 900, 22, 22));
        // Watermark pill bottom-right is near-white.
        let wb = watermark_box(&s, &mut TextEngine::new(), 900.0, 500.0);
        let p = at(&px, w, (wb.rect.x + 2.0) as u32, (wb.rect.y + wb.rect.h / 2.0) as u32);
        assert!(p[0] > 200 && p[1] > 200, "{p:?}");
    }

    #[test]
    fn export_names() {
        assert_eq!(export_name("Screenshot 2026.png"), "Screenshot 2026-annotated.png");
        assert_eq!(export_name("Clipboard image"), "Clipboard image-annotated.png");
        assert_eq!(export_name(""), "escribo-annotated.png");
    }
}
