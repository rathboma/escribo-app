//! The stage: the composed output — fill, shot, annotations, watermark —
//! drawn at the current zoom, and every pointer interaction on it, including
//! editing a label in place.
//!
//! Shapes are drawn with gpui's own paths and quads. Labels, emoji and the
//! watermark are drawn by the same CPU text renderer the export uses and
//! shown as bitmaps, so what is on screen is what lands in the PNG.

use super::{AREA_PAD, Editor, ZOOM_STEP};
use crate::doc::{Annotation, Arrow, BoxShape, DragModifiers, Emoji, Handle, Id, Label, Measure, Rgb, Shape, Tool};
use crate::geom::{Affine, El, OrientedBox, Rect, V2, v2};
use crate::render;
use crate::text::TextEngine;
use crate::theme::{self, Theme};
use gpui::{
    Bounds, ContentMask, Context, CursorStyle, EntityInputHandler, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, PathBuilder, Pixels, Point, RenderImage, ScrollDelta, ScrollWheelEvent, UTF16Selection,
    Window, point, px, size,
};
use parley::editing::{Cursor, Selection};
use parley::layout::Affinity;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

/* ==========================================================================
Geometry
========================================================================== */

/// Where everything is on screen this frame, in window coordinates.
#[derive(Clone, Copy, Debug)]
pub struct StageGeom {
    pub area: Bounds<Pixels>,
    pub stage: Bounds<Pixels>,
    pub shot: Bounds<Pixels>,
    /// screen pixels per image pixel
    pub k: f32,
    pub layout: crate::doc::Layout,
    /// how far the content overflows the area, per axis
    pub overflow: V2,
}

impl StageGeom {
    pub fn to_image(&self, p: Point<Pixels>) -> V2 {
        v2(
            (f32::from(p.x) - f32::from(self.shot.origin.x)) / self.k,
            (f32::from(p.y) - f32::from(self.shot.origin.y)) / self.k,
        )
    }

    pub fn to_screen(&self, p: V2) -> Point<Pixels> {
        point(self.shot.origin.x + px(p.x * self.k), self.shot.origin.y + px(p.y * self.k))
    }

    /// Image space to window space as an affine map.
    pub fn affine(&self) -> Affine {
        Affine::scale(self.k, self.k)
            .then(Affine::translate(v2(f32::from(self.shot.origin.x), f32::from(self.shot.origin.y))))
    }
}

fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

fn rect_of(b: Bounds<Pixels>) -> Rect {
    Rect::new(f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height))
}

/* ==========================================================================
State
========================================================================== */

pub struct TextEdit {
    pub id: Id,
    pub selection: Selection,
    /// IME composition, as a byte range in the label's text
    pub marked: Option<Range<usize>>,
    pub selecting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CropHandle {
    Nw,
    Ne,
    Sw,
    Se,
}

#[derive(Clone, Debug)]
pub enum Drag {
    /// A shape being drawn; `origin` is where the drag started.
    Draw {
        id: Id,
        origin: V2,
        moved: bool,
    },
    Move {
        start: V2,
        originals: Vec<(Id, Shape)>,
        moved: bool,
        reselect_text: Option<Id>,
    },
    Handle {
        id: Id,
        handle: Handle,
        start: Shape,
        frame: OrientedBox,
    },
    Band {
        start: V2,
        now: V2,
        base: Vec<Id>,
    },
    CropNew {
        start: V2,
    },
    CropMove {
        start: V2,
        from: Rect,
    },
    CropResize {
        handle: CropHandle,
        from: Rect,
    },
    TextSelect,
    Slider(super::widgets::SliderId),
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Hover {
    pub cursor: Option<CursorStyle>,
}

struct Bitmap {
    key: u64,
    image: Arc<RenderImage>,
    /// window-space top-left, in device pixels
    origin: (i32, i32),
    size: (u32, u32),
}

/// gpui images uploaded for this window: the screenshot at a few sizes, and
/// the text bitmaps.
#[derive(Default)]
pub struct GpuCache {
    levels: Vec<(u32, u32, Arc<Vec<u8>>)>,
    gpu_levels: Vec<Option<Arc<RenderImage>>>,
    shot_ptr: usize,
    bitmaps: HashMap<Id, Bitmap>,
    watermark: Option<Bitmap>,
    stale: Vec<Arc<RenderImage>>,
}

impl GpuCache {
    pub fn reset(&mut self, window: &mut Window) {
        for img in self.gpu_levels.drain(..).flatten() {
            window.drop_image(img).ok();
        }
        for (_, b) in self.bitmaps.drain() {
            window.drop_image(b.image).ok();
        }
        if let Some(b) = self.watermark.take() {
            window.drop_image(b.image).ok();
        }
        for img in self.stale.drain(..) {
            window.drop_image(img).ok();
        }
        self.levels.clear();
        self.shot_ptr = 0;
    }

    fn flush(&mut self, window: &mut Window) {
        for img in self.stale.drain(..) {
            window.drop_image(img).ok();
        }
    }
}

fn render_image(w: u32, h: u32, bgra: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buf = image::RgbaImage::from_raw(w, h, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buf)])))
}

fn half(rgba: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    let at = |x: u32, y: u32, c: usize| rgba[((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize + c] as u32;
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let v = at(2 * x, 2 * y, c)
                    + at(2 * x + 1, 2 * y, c)
                    + at(2 * x, 2 * y + 1, c)
                    + at(2 * x + 1, 2 * y + 1, c);
                out[((y * nw + x) * 4) as usize + c] = ((v + 2) / 4) as u8;
            }
        }
    }
    (nw, nh, out)
}

const MAX_TEXTURE: u32 = 8192;

/* ==========================================================================
Painting
========================================================================== */

const HANDLE: f32 = 11.0;
const ROTATE_OFFSET: f32 = 30.0;
const SEL_BORDER: u32 = 0xB2CCFF;

impl Editor {
    /// Lays the stage out in `area` for this frame.
    pub(crate) fn compute_geom(&mut self, area: Bounds<Pixels>, cx: &mut Context<Self>) -> Option<StageGeom> {
        let doc = self.doc.as_ref()?;
        let s = self.s(cx);
        let layout = render::layout_for(s, &doc.shot);
        let (aw, ah) = (f32::from(area.size.width), f32::from(area.size.height));
        let avail_w = (aw - AREA_PAD * 2.0).max(1.0);
        let avail_h = (ah - AREA_PAD * 2.0).max(1.0);
        let fit = (avail_w / layout.out_w as f32).min(avail_h / layout.out_h as f32).min(1.0);
        let k = fit * ZOOM_STEP.powi(self.zoom);
        let stage_w = (layout.out_w as f32 * k).round().max(1.0);
        let stage_h = (layout.out_h as f32 * k).round().max(1.0);
        let over = v2((stage_w + AREA_PAD * 2.0 - aw).max(0.0), (stage_h + AREA_PAD * 2.0 - ah).max(0.0));
        self.scroll = v2(self.scroll.x.clamp(0.0, over.x), self.scroll.y.clamp(0.0, over.y));
        let ax = f32::from(area.origin.x);
        let ay = f32::from(area.origin.y);
        let x = if over.x > 0.0 { ax + AREA_PAD - self.scroll.x } else { ax + ((aw - stage_w) / 2.0).round() };
        let y = if over.y > 0.0 { ay + AREA_PAD - self.scroll.y } else { ay + ((ah - stage_h) / 2.0).round() };
        // The shot snaps to whole pixels inside the stage, as the old layout did.
        let k_shot_x = (layout.shot_x as f32 * k).round();
        let k_shot_y = (layout.shot_y as f32 * k).round();
        let shot = bounds(x + k_shot_x, y + k_shot_y, doc.shot.width as f32 * k, doc.shot.height as f32 * k);
        let geom =
            StageGeom { area, stage: bounds(x.round(), y.round(), stage_w, stage_h), shot, k, layout, overflow: over };
        self.geom = Some(geom);
        Some(geom)
    }

    pub(crate) fn paint_area(
        &mut self,
        area: Bounds<Pixels>,
        hitbox: &gpui::Hitbox,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = self.theme(cx);
        paint_dots(window, area, &theme);
        let Some(g) = self.compute_geom(area, cx) else {
            self.geom = None;
            return;
        };
        let dpr = window.scale_factor();

        window.with_content_mask(Some(ContentMask { bounds: area, ..Default::default() }), |window| {
            // The stage's own shadow and hairline ring on the desk.
            window.paint_drop_shadows(
                g.stage,
                px(3.0).into(),
                0.0,
                &[
                    gpui::BoxShadow::new(px(0.0), px(8.0), gpui::rgba(0x00000059)).blur_radius(px(30.0)),
                    gpui::BoxShadow::new(px(0.0), px(0.0), gpui::rgba(0x00000040)).spread_radius(px(1.0)),
                ],
            );
            window.with_content_mask(Some(ContentMask { bounds: g.stage, ..Default::default() }), |window| {
                self.paint_backdrop(&g, window, cx);
                self.paint_shot(&g, dpr, window, cx);
                self.paint_annotations(&g, dpr, window, cx);
                self.paint_edit_overlay(&g, window);
                self.paint_selection(&g, window, cx);
                self.paint_band(&g, window);
                if self.s(cx).style.watermark {
                    self.paint_watermark(&g, dpr, window, cx);
                }
                self.paint_crop(&g, window);
            });
            paint_scrollbars(&g, &theme, window);
        });

        self.gpu.flush(window);

        let cursor = self.hover.cursor.unwrap_or(if self.tool == Tool::Select {
            CursorStyle::Arrow
        } else {
            CursorStyle::Crosshair
        });
        window.set_cursor_style(cursor, hitbox);

        if self.edit.is_some() {
            window.handle_input(&self.focus, gpui::ElementInputHandler::new(area, cx.entity()), cx);
        }
    }

    fn paint_backdrop(&mut self, g: &StageGeom, window: &mut Window, cx: &mut Context<Self>) {
        let s = self.s(cx);
        let style = &s.style;
        let shot_alpha = self.doc.as_ref().is_some_and(|d| d.shot.has_alpha);
        // Shows through wherever the export has no fill: margins and extra canvas.
        if style.backdrop == 0 || shot_alpha {
            window.paint_quad(PaintQuad::new(g.stage).background(gpui::rgb(0xECECF1)));
            window.paint_quad(PaintQuad::new(g.stage).background(gpui::checkerboard(gpui::rgb(0xC4C4CE), 9.0)));
        }
        let kit = s.kit();
        match style.backdrop {
            0 => {}
            i @ 1..=4 => {
                let c = crate::doc::rgb(&kit.solid_bg[i as usize - 1]);
                window.paint_quad(PaintQuad::new(g.stage).background(theme::from_rgb(c)));
            }
            i => {
                let gr = &kit.grad_bg[(i as usize - 5).min(3)];
                window.paint_quad(PaintQuad::new(g.stage).background(gradient(
                    crate::doc::rgb(&gr.a),
                    crate::doc::rgb(&gr.b),
                    g.stage,
                )));
            }
        }
    }

    fn shot_radius(&self, g: &StageGeom, cx: &Context<Self>) -> f32 {
        let s = self.s(cx);
        if s.style.pad > 0 {
            f32::from(g.shot.size.width) * (s.kit().advanced.corner_radius as f32 / 100.0)
        } else {
            0.0
        }
    }

    fn paint_shot(&mut self, g: &StageGeom, dpr: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.as_ref() else { return };
        let s = self.s(cx);
        let radius = self.shot_radius(g, cx);
        if s.style.backdrop != 0 && s.style.pad > 0 {
            let a = s.kit().advanced;
            let u = doc.shot.unit() * g.k;
            window.paint_drop_shadows(
                g.shot,
                px(radius).into(),
                0.0,
                &[gpui::BoxShadow::new(
                    px(0.0),
                    px(a.shadow_offset_y as f32 * u),
                    gpui::Rgba::new(0.0, 0.0, 0.0, a.shadow_opacity as f32 / 100.0),
                )
                .blur_radius(px(a.shadow_blur as f32 * u))],
            );
        }
        let shot = doc.shot.clone();
        let want = f32::from(g.shot.size.width) * dpr;
        let Some(image) = self.shot_level(&shot, want) else { return };
        window.paint_image(g.shot, g.shot, px(radius).into(), 0.0, image, 0, false).ok();
    }

    /// The screenshot halved until the next halving would be smaller than it
    /// shows, so the GPU never minifies by more than 2:1 (which aliases text).
    fn shot_level(&mut self, shot: &Arc<crate::doc::Shot>, want_w: f32) -> Option<Arc<RenderImage>> {
        let ptr = Arc::as_ptr(shot) as usize;
        if self.gpu.shot_ptr != ptr {
            self.gpu.levels.clear();
            self.gpu.stale.extend(self.gpu.gpu_levels.drain(..).flatten());
            self.gpu.levels.push((shot.width, shot.height, shot.rgba.clone()));
            self.gpu.gpu_levels.push(None);
            self.gpu.shot_ptr = ptr;
        }
        let mut level = 0;
        loop {
            let (w, h, _) = self.gpu.levels[level];
            let next_ok = (w / 2) as f32 >= want_w && w > 1 && h > 1;
            let too_big = w > MAX_TEXTURE || h > MAX_TEXTURE;
            if !(next_ok || too_big) {
                break;
            }
            if level + 1 >= self.gpu.levels.len() {
                let (w, h, data) = &self.gpu.levels[level];
                let (nw, nh, out) = half(data, *w, *h);
                self.gpu.levels.push((nw, nh, Arc::new(out)));
                self.gpu.gpu_levels.push(None);
            }
            level += 1;
        }
        if self.gpu.gpu_levels[level].is_none() {
            let (w, h, data) = &self.gpu.levels[level];
            let mut bgra = data.as_ref().clone();
            for px in bgra.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            self.gpu.gpu_levels[level] = render_image(*w, *h, bgra);
        }
        self.gpu.gpu_levels[level].clone()
    }

    fn paint_annotations(&mut self, g: &StageGeom, dpr: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.as_ref() else { return };
        let annotations = doc.annotations.clone();
        let shot = doc.shot.clone();
        let t = g.affine();
        let mut live = Vec::with_capacity(annotations.len());
        for a in &annotations {
            match &a.shape {
                Shape::Arrow(arrow) => paint_arrow(arrow, &t, window),
                Shape::Rect(b) => paint_box(b, &t, false, window),
                Shape::Mark(b) => paint_box(b, &t, true, window),
                Shape::Redact(r) => paint_redact(*r, &shot, g, window),
                Shape::Text(_) | Shape::Emoji(_) => {
                    live.push(a.id);
                    if let Some((image, b)) = self.text_bitmap(a, g, dpr) {
                        window.paint_image(b, b, px(0.0).into(), 0.0, image, 0, false).ok();
                    }
                }
            }
        }
        // Labels that are gone free their bitmaps.
        let gone: Vec<Id> = self.gpu.bitmaps.keys().copied().filter(|id| !live.contains(id)).collect();
        for id in gone {
            if let Some(b) = self.gpu.bitmaps.remove(&id) {
                self.gpu.stale.push(b.image);
            }
        }
        let _ = cx;
    }

    /// A label or emoji, rendered at the size it shows and cached until its
    /// look (not its position) changes.
    fn text_bitmap(&mut self, a: &Annotation, g: &StageGeom, dpr: f32) -> Option<(Arc<RenderImage>, Bounds<Pixels>)> {
        let mut text = self.text.borrow_mut();
        let (to_image, frame, pad, mut hasher) = match &a.shape {
            Shape::Text(l) => {
                let size = text.label_size(l);
                let mut h = std::collections::hash_map::DefaultHasher::new();
                0u8.hash(&mut h);
                label_key(l, &mut h);
                let u = l.unit * l.scale;
                (TextEngine::label_transform(l), l.frame(size), 8.0 * u, h)
            }
            Shape::Emoji(e) => {
                let size = text.emoji_size(e);
                let mut h = std::collections::hash_map::DefaultHasher::new();
                1u8.hash(&mut h);
                e.ch.hash(&mut h);
                e.size.to_bits().hash(&mut h);
                e.scale.to_bits().hash(&mut h);
                e.angle.to_bits().hash(&mut h);
                (text.emoji_transform(e), e.frame(size), 0.2 * e.size * e.scale, h)
            }
            _ => return None,
        };
        // Window space in device pixels, translation rounded so the bitmap
        // only changes when the label's look does.
        let dev = g.affine().then(Affine::scale(dpr, dpr));
        let full = to_image.then(dev);
        let anchor = full.apply(V2::ZERO);
        let snapped =
            Affine { e: full.e - anchor.x + anchor.x.round(), f: full.f - anchor.y + anchor.y.round(), ..full };
        let aabb = Rect::bounding(&frame.corners().map(|c| dev.apply(c))).inflate(pad * g.k * dpr);
        // Only the visible part is rendered: a label at 8× zoom can be huge.
        let visible = rect_of(g.stage);
        let visible = Rect::new(visible.x * dpr, visible.y * dpr, visible.w * dpr, visible.h * dpr);
        let clip = aabb.intersect(&visible)?;
        let (x0, y0) = (clip.x.floor() as i32, clip.y.floor() as i32);
        let (w, h) =
            ((clip.right().ceil() as i32 - x0).max(1) as u32, (clip.bottom().ceil() as i32 - y0).max(1) as u32);
        // The key holds everything but the whole-pixel position.
        let rel = (x0 - anchor.x.round() as i32, y0 - anchor.y.round() as i32);
        for v in [full.a, full.b, full.c, full.d] {
            v.to_bits().hash(&mut hasher);
        }
        rel.hash(&mut hasher);
        (w, h).hash(&mut hasher);
        let key = hasher.finish();

        let fresh = match self.gpu.bitmaps.get(&a.id) {
            Some(b) if b.key == key => None,
            _ => {
                let mut pm = tiny_skia::Pixmap::new(w, h)?;
                let to_pm = snapped.then(Affine::translate(v2(-x0 as f32, -y0 as f32)));
                let inv = to_image_inverse(&to_image);
                let local_to_pm = inv.then(to_pm);
                match &a.shape {
                    Shape::Text(l) => text.draw_label(&mut pm, l, local_to_pm),
                    Shape::Emoji(e) => text.draw_emoji(&mut pm, e, local_to_pm),
                    _ => {}
                }
                let image = render_image(w, h, crate::raster::to_bgra_straight(&pm))?;
                Some(image)
            }
        };
        if let Some(image) = fresh {
            if let Some(old) = self.gpu.bitmaps.insert(a.id, Bitmap { key, image, origin: (0, 0), size: (w, h) }) {
                self.gpu.stale.push(old.image);
            }
        }
        let b = self.gpu.bitmaps.get_mut(&a.id)?;
        b.origin = (anchor.x.round() as i32 + rel.0, anchor.y.round() as i32 + rel.1);
        let bb = bounds(b.origin.0 as f32 / dpr, b.origin.1 as f32 / dpr, b.size.0 as f32 / dpr, b.size.1 as f32 / dpr);
        Some((b.image.clone(), bb))
    }

    fn paint_watermark(&mut self, g: &StageGeom, dpr: f32, window: &mut Window, cx: &mut Context<Self>) {
        let s = self.s(cx).clone();
        let (ow, oh) = (g.layout.out_w as f32, g.layout.out_h as f32);
        let kd = g.k * dpr;
        let mut text = self.text.borrow_mut();
        let wb = render::watermark_box(&s, &mut text, ow, oh);
        let pad = wb.size * 0.8;
        let stage_dev = v2(f32::from(g.stage.origin.x) * dpr, f32::from(g.stage.origin.y) * dpr);
        let r = wb.rect.inflate(pad);
        let x0 = (stage_dev.x + r.x * kd).floor() as i32;
        let y0 = (stage_dev.y + r.y * kd).floor() as i32;
        let w = (r.w * kd).ceil() as u32 + 2;
        let h = (r.h * kd).ceil() as u32 + 2;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let kit = s.kit();
        (render::watermark_text(&s), &kit.wm_color, &kit.wm_bg, render::watermark_has_icon(&s)).hash(&mut hasher);
        (ow.to_bits(), oh.to_bits(), kd.to_bits(), w, h).hash(&mut hasher);
        let frac = (stage_dev.x + r.x * kd - x0 as f32, stage_dev.y + r.y * kd - y0 as f32);
        ((frac.0 * 4.0) as i32, (frac.1 * 4.0) as i32).hash(&mut hasher);
        let key = hasher.finish();
        if self.gpu.watermark.as_ref().map(|b| b.key) != Some(key) {
            let Some(mut pm) = tiny_skia::Pixmap::new(w, h) else { return };
            let t = Affine::scale(kd, kd).then(Affine::translate(v2(stage_dev.x - x0 as f32, stage_dev.y - y0 as f32)));
            render::draw_watermark(&mut pm, &s, &mut text, ow, oh, t);
            let Some(image) = render_image(w, h, crate::raster::to_bgra_straight(&pm)) else { return };
            if let Some(old) = self.gpu.watermark.replace(Bitmap { key, image, origin: (x0, y0), size: (w, h) }) {
                self.gpu.stale.push(old.image);
            }
        }
        if let Some(b) = &mut self.gpu.watermark {
            b.origin = (x0, y0);
            let bb = bounds(x0 as f32 / dpr, y0 as f32 / dpr, w as f32 / dpr, h as f32 / dpr);
            window.paint_image(bb, bb, px(0.0).into(), 0.0, b.image.clone(), 0, false).ok();
        }
    }

    fn paint_selection(&mut self, g: &StageGeom, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.as_ref() else { return };
        let mut text = self.text.borrow_mut();
        let border = gpui::rgb(SEL_BORDER);
        let single = self.selection.len() == 1;
        for id in &self.selection {
            let Some(a) = doc.get(*id) else { continue };
            if self.edit.as_ref().is_some_and(|e| e.id == *id) {
                // A label being edited shows a dashed frame, as fabric's did.
                let f = a.shape.frame(&mut *text);
                stroke_polygon(&f.corners().map(|c| g.to_screen(c)), 1.0, border, window);
                continue;
            }
            let f = a.shape.frame(&mut *text);
            let corners = f.corners().map(|c| g.to_screen(c));
            let c = a.shape.controls();
            stroke_polygon(&corners, 1.0, border, window);
            if !single {
                continue;
            }
            if c.corners {
                for p in corners {
                    paint_handle_square(p, window);
                }
            }
            if c.sides {
                for i in 0..2 {
                    paint_handle_square(self.side_handle_pos(&f, i, g), window);
                }
            }
            if c.rotate {
                let (top, knob) = self.rotate_handle_pos(&f, g);
                stroke_polyline(&[top, knob], 1.0, border, window);
                paint_handle_circle(knob, HANDLE / 2.0, gpui::rgb(0xffffff), border, window);
            }
            if c.arrow_head {
                if let Shape::Arrow(arrow) = &a.shape {
                    let tip = arrow.head + (arrow.head - arrow.tail) * (1.0 / arrow.length().max(1.0)) * (6.0 / g.k);
                    paint_handle_circle(g.to_screen(tip), 6.0, gpui::rgb(0x2E7DFF), gpui::rgb(0xffffff), window);
                }
            }
        }
        let _ = cx;
    }

    fn side_handle_pos(&self, f: &OrientedBox, i: usize, g: &StageGeom) -> Point<Pixels> {
        let x = if i == 0 { -f.w / 2.0 } else { f.w / 2.0 };
        g.to_screen(f.to_world(v2(x, 0.0)))
    }

    fn rotate_handle_pos(&self, f: &OrientedBox, g: &StageGeom) -> (Point<Pixels>, Point<Pixels>) {
        let top = f.to_world(v2(0.0, -f.h / 2.0));
        let knob = f.to_world(v2(0.0, -f.h / 2.0 - ROTATE_OFFSET / g.k));
        (g.to_screen(top), g.to_screen(knob))
    }

    fn paint_band(&self, g: &StageGeom, window: &mut Window) {
        if let Some(Drag::Band { start, now, .. }) = &self.drag {
            let a = g.to_screen(*start);
            let b = g.to_screen(*now);
            let r = Bounds::from_corners(point(a.x.min(b.x), a.y.min(b.y)), point(a.x.max(b.x), a.y.max(b.y)));
            window.paint_quad(
                PaintQuad::new(r)
                    .background(gpui::rgba(0x6496ff26))
                    .border_widths(px(1.0))
                    .border_color(gpui::rgba(0x6496ffcc)),
            );
        }
    }

    fn paint_crop(&self, g: &StageGeom, window: &mut Window) {
        let Some(c) = self.crop else { return };
        let shot = g.shot;
        let tl = g.to_screen(v2(c.x, c.y));
        let br = g.to_screen(v2(c.right(), c.bottom()));
        let dim = gpui::rgba(0x08080c8c);
        window.with_content_mask(Some(ContentMask { bounds: shot, ..Default::default() }), |window| {
            let (sx0, sy0) = (shot.origin.x, shot.origin.y);
            let (sx1, sy1) = (shot.origin.x + shot.size.width, shot.origin.y + shot.size.height);
            for r in [
                Bounds::from_corners(point(sx0, sy0), point(sx1, tl.y)),
                Bounds::from_corners(point(sx0, br.y), point(sx1, sy1)),
                Bounds::from_corners(point(sx0, tl.y), point(tl.x, br.y)),
                Bounds::from_corners(point(br.x, tl.y), point(sx1, br.y)),
            ] {
                if r.size.width > px(0.0) && r.size.height > px(0.0) {
                    window.paint_quad(PaintQuad::new(r).background(dim));
                }
            }
            let bx = Bounds::from_corners(tl, br);
            window.paint_quad(PaintQuad::new(bx).border_widths(px(1.0)).border_color(gpui::rgba(0xffffffe6)));
            // Rule-of-thirds guides.
            for i in 1..3 {
                let x = tl.x + (br.x - tl.x) * (i as f32 / 3.0);
                let y = tl.y + (br.y - tl.y) * (i as f32 / 3.0);
                window.paint_quad(
                    PaintQuad::new(Bounds::new(point(x, tl.y), size(px(1.0), br.y - tl.y)))
                        .background(gpui::rgba(0xffffff40)),
                );
                window.paint_quad(
                    PaintQuad::new(Bounds::new(point(tl.x, y), size(br.x - tl.x, px(1.0))))
                        .background(gpui::rgba(0xffffff40)),
                );
            }
            // Inset rather than outset: a crop covering the whole image would
            // otherwise push its handles outside the shot and clip them away.
            for (hx, hy) in
                [(tl.x, tl.y), (br.x - px(12.0), tl.y), (tl.x, br.y - px(12.0)), (br.x - px(12.0), br.y - px(12.0))]
            {
                let hb = Bounds::new(point(hx, hy), size(px(12.0), px(12.0)));
                window.paint_drop_shadows(
                    hb,
                    px(2.0).into(),
                    0.0,
                    &[gpui::BoxShadow::new(px(0.0), px(1.0), gpui::rgba(0x00000066)).blur_radius(px(3.0))],
                );
                window.paint_quad(PaintQuad::new(hb).corner_radii(px(2.0)).background(gpui::rgb(0xffffff)));
            }
        });
    }

    fn paint_edit_overlay(&mut self, g: &StageGeom, window: &mut Window) {
        let Some(edit) = &self.edit else { return };
        let Some(Shape::Text(label)) = self.doc.as_ref().and_then(|d| d.get(edit.id)).map(|a| a.shape.clone()) else {
            return;
        };
        let layout = self.text.borrow_mut().label_layout(&label);
        let t = TextEngine::label_transform(&label).then(g.affine());
        for (bb, _) in edit.selection.geometry(&layout.layout) {
            let quad = [
                v2(bb.x0 as f32, bb.y0 as f32),
                v2(bb.x1 as f32, bb.y0 as f32),
                v2(bb.x1 as f32, bb.y1 as f32),
                v2(bb.x0 as f32, bb.y1 as f32),
            ];
            fill_polygon(&quad.map(|p| to_point(t.apply(p))), gpui::rgba(0x1177ff4d), window);
        }
        if edit.selection.is_collapsed() {
            let w = (2.0 / t.scale_factor()).max(0.5);
            let bb = edit.selection.focus().geometry(&layout.layout, w);
            let quad = [
                v2(bb.x0 as f32, bb.y0 as f32),
                v2(bb.x1 as f32, bb.y0 as f32),
                v2(bb.x1 as f32, bb.y1 as f32),
                v2(bb.x0 as f32, bb.y1 as f32),
            ];
            fill_polygon(&quad.map(|p| to_point(t.apply(p))), theme::from_rgb(label.color), window);
        }
    }

    /* ======================================================================
    Pointer
    ====================================================================== */

    pub(crate) fn on_area_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.emoji_picker_click_away(cx);
        let Some(g) = self.geom else { return };
        if self.doc.is_none() {
            return;
        }
        let p = g.to_image(ev.position);
        let tol = 4.0 / g.k;

        if self.tool == Tool::Crop {
            self.crop_down(ev.position, p, &g);
            cx.notify();
            return;
        }

        // A click inside the label being edited moves the caret.
        if let Some(edit_id) = self.edit.as_ref().map(|e| e.id) {
            let inside = self
                .doc
                .as_ref()
                .and_then(|d| d.get(edit_id))
                .is_some_and(|a| a.shape.frame(&mut *self.text.borrow_mut()).contains(p, tol));
            if inside {
                self.edit_pointer_down(p, ev.click_count, ev.modifiers.shift, cx);
                return;
            }
            self.finish_edit(cx);
        }

        let u = self.annotation_unit(cx);
        let color = self.color(cx);
        let adv = self.s(cx).kit().advanced;
        let font = self.s(cx).kit().font.clone();
        if self.tool == Tool::Select {
            if let Some((id, handle, start, frame)) = self.handle_at(ev.position, &g) {
                self.drag = Some(Drag::Handle { id, handle, start, frame });
                cx.notify();
                return;
            }
        }
        let Some(doc) = self.doc.as_mut() else { return };
        match self.tool {
            Tool::Select => {
                let hit = doc.hit(p, tol, &mut *self.text.borrow_mut());
                match hit {
                    Some(id) => {
                        let was_selected = self.selection == [id];
                        if ev.modifiers.shift {
                            if let Some(i) = self.selection.iter().position(|s| *s == id) {
                                self.selection.remove(i);
                            } else {
                                self.selection.push(id);
                            }
                        } else if !self.selection.contains(&id) {
                            self.selection = vec![id];
                        }
                        let is_text = matches!(doc.get(id).map(|a| &a.shape), Some(Shape::Text(_)));
                        if is_text && ev.click_count >= 2 {
                            self.selection = vec![id];
                            self.begin_edit(id, false, cx);
                            self.edit_pointer_down(p, 2, false, cx);
                            return;
                        }
                        let originals = self
                            .selection
                            .iter()
                            .filter_map(|sid| doc.get(*sid).map(|a| (*sid, a.shape.clone())))
                            .collect();
                        self.drag = Some(Drag::Move {
                            start: p,
                            originals,
                            moved: false,
                            reselect_text: (was_selected && is_text).then_some(id),
                        });
                    }
                    None => {
                        let base = if ev.modifiers.shift { self.selection.clone() } else { Vec::new() };
                        self.selection = base.clone();
                        self.drag = Some(Drag::Band { start: p, now: p, base });
                    }
                }
            }
            Tool::Text => {
                let id = doc.add(Shape::Text(Label {
                    origin: p,
                    angle: 0.0,
                    scale: 1.0,
                    width: 250.0 * u,
                    font_size: 22.0 * u,
                    unit: u,
                    color,
                    font,
                    text: "text".into(),
                }));
                self.selection = vec![id];
                // The label is placed; typing continues, but the tool is done.
                self.set_tool(Tool::Select, cx);
                self.begin_edit(id, true, cx);
            }
            Tool::Emoji => {
                let Some(ch) = self.pending_emoji else { return };
                doc.add(Shape::Emoji(Emoji { center: p, angle: 0.0, scale: 1.0, size: 100.0 * u, ch: ch.into() }));
                doc.commit();
                self.pending_emoji = None;
                self.set_tool(Tool::Select, cx);
            }
            Tool::Rect | Tool::Mark => {
                let frame = OrientedBox { center: p, w: 0.0, h: 0.0, angle: 0.0 };
                let b = BoxShape {
                    frame,
                    color,
                    stroke: 5.0 * u,
                    radius: adv.box_radius as f32 * u,
                    alpha: adv.highlight_opacity as f32 / 100.0,
                };
                let id = doc.add(if self.tool == Tool::Rect { Shape::Rect(b) } else { Shape::Mark(b) });
                self.drag = Some(Drag::Draw { id, origin: p, moved: false });
            }
            Tool::Redact => {
                let id = doc.add(Shape::Redact(Rect::new(p.x, p.y, 0.0, 0.0)));
                self.drag = Some(Drag::Draw { id, origin: p, moved: false });
            }
            Tool::Arrow => {
                // A click without a drag places a default arrow pointing at
                // the click; a drag draws from the press to the pointer.
                let id = doc.add(Shape::Arrow(Arrow { tail: p - v2(40.0 * u, 0.0), head: p, unit: u, color }));
                self.drag = Some(Drag::Draw { id, origin: p, moved: false });
            }
            Tool::Crop => {}
        }
        cx.notify();
    }

    /// The handle under `pos` on the single selected annotation.
    fn handle_at(&self, pos: Point<Pixels>, g: &StageGeom) -> Option<(Id, Handle, Shape, OrientedBox)> {
        if self.selection.len() != 1 {
            return None;
        }
        let a = self.doc.as_ref()?.get(self.selection[0])?;
        let f = a.shape.frame(&mut *self.text.borrow_mut());
        let c = a.shape.controls();
        let near = |q: Point<Pixels>, r: f32| {
            let d = v2(f32::from(pos.x - q.x), f32::from(pos.y - q.y));
            d.len() <= r
        };
        let hit = HANDLE * 0.75;
        if c.arrow_head {
            if let Shape::Arrow(arrow) = &a.shape {
                let tip = arrow.head + (arrow.head - arrow.tail) * (1.0 / arrow.length().max(1.0)) * (6.0 / g.k);
                if near(g.to_screen(tip), 9.0) {
                    return Some((a.id, Handle::ArrowHead, a.shape.clone(), f));
                }
            }
        }
        if c.rotate && near(self.rotate_handle_pos(&f, g).1, hit) {
            return Some((a.id, Handle::Rotate, a.shape.clone(), f));
        }
        if c.corners {
            for (i, corner) in f.corners().iter().enumerate() {
                if near(g.to_screen(*corner), hit) {
                    return Some((a.id, Handle::Corner(i), a.shape.clone(), f));
                }
            }
        }
        if c.sides {
            for i in 0..2 {
                if near(self.side_handle_pos(&f, i, g), hit) {
                    return Some((a.id, Handle::Side(i), a.shape.clone(), f));
                }
            }
        }
        None
    }

    pub(crate) fn on_pointer_move(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Drag::Slider(id)) = self.drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                self.slider_to(id, ev.position, cx);
            }
            return;
        }
        let Some(g) = self.geom else { return };
        let p = g.to_image(ev.position);
        if self.drag.is_none() {
            let cursor = self.cursor_at(ev.position, &g);
            if cursor != self.hover.cursor {
                self.hover.cursor = cursor;
                cx.notify();
            }
            return;
        }
        let modifiers = DragModifiers { shift: ev.modifiers.shift };
        let threshold = 3.0 / g.k;
        let mut text = self.text.borrow_mut();
        let Some(doc) = self.doc.as_mut() else { return };
        match self.drag.as_mut() {
            Some(Drag::Draw { id, origin, moved }) => {
                if (p - *origin).len() > threshold {
                    *moved = true;
                }
                let origin = *origin;
                let moved = *moved;
                if let Some(a) = doc.get_mut(*id) {
                    match &mut a.shape {
                        Shape::Rect(b) | Shape::Mark(b) => {
                            let r = Rect::from_points(origin, p);
                            b.frame = OrientedBox { center: r.center(), w: r.w, h: r.h, angle: 0.0 };
                        }
                        Shape::Redact(r) => *r = Rect::from_points(origin, p),
                        Shape::Arrow(arrow) if moved => {
                            arrow.tail = origin;
                            arrow.head = p;
                        }
                        _ => {}
                    }
                }
            }
            Some(Drag::Move { start, originals, moved, .. }) => {
                let d = p - *start;
                if d.len() > threshold {
                    *moved = true;
                }
                if *moved {
                    for (id, shape) in originals.iter() {
                        if let Some(a) = doc.get_mut(*id) {
                            let mut s = shape.clone();
                            s.translate(d);
                            a.shape = s;
                        }
                    }
                }
            }
            Some(Drag::Handle { id, handle, start, frame }) => {
                if let Some(a) = doc.get_mut(*id) {
                    a.shape.drag_handle(start, *frame, *handle, p, modifiers, &mut *text);
                }
            }
            Some(Drag::Band { start, now, base }) => {
                *now = p;
                let band = Rect::from_points(*start, p);
                let mut sel = base.clone();
                for a in &doc.annotations {
                    if !sel.contains(&a.id) && a.shape.frame(&mut *text).aabb().intersects(&band) {
                        sel.push(a.id);
                    }
                }
                self.selection = sel;
            }
            Some(Drag::CropNew { start }) => {
                let (w, h) = (doc.shot.width as f32, doc.shot.height as f32);
                let q = v2(p.x.clamp(0.0, w), p.y.clamp(0.0, h));
                self.crop = Some(Rect::from_points(*start, q));
            }
            Some(Drag::CropMove { start, from }) => {
                let (w, h) = (doc.shot.width as f32, doc.shot.height as f32);
                let d = p - *start;
                self.crop = Some(Rect::new(
                    (from.x + d.x).clamp(0.0, (w - from.w).max(0.0)),
                    (from.y + d.y).clamp(0.0, (h - from.h).max(0.0)),
                    from.w,
                    from.h,
                ));
            }
            Some(Drag::CropResize { handle, from }) => {
                let (w, h) = (doc.shot.width as f32, doc.shot.height as f32);
                let q = v2(p.x.clamp(0.0, w), p.y.clamp(0.0, h));
                let left = if matches!(handle, CropHandle::Nw | CropHandle::Sw) { q.x } else { from.x };
                let top = if matches!(handle, CropHandle::Nw | CropHandle::Ne) { q.y } else { from.y };
                let right = if matches!(handle, CropHandle::Ne | CropHandle::Se) { q.x } else { from.right() };
                let bottom = if matches!(handle, CropHandle::Sw | CropHandle::Se) { q.y } else { from.bottom() };
                self.crop = Some(Rect::from_points(v2(left, top), v2(right, bottom)));
            }
            Some(Drag::TextSelect) => {
                drop(text);
                self.edit_pointer_drag(p, cx);
            }
            Some(Drag::Slider(_)) | None => {}
        }
        let _ = window;
        cx.notify();
    }

    pub(crate) fn on_pointer_up(&mut self, ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if ev.button != MouseButton::Left {
            return;
        }
        let Some(drag) = self.drag.take() else { return };
        match drag {
            Drag::Draw { id, .. } => {
                let Some(doc) = self.doc.as_mut() else { return };
                let keep = doc.get(id).is_some_and(|a| match &a.shape {
                    Shape::Arrow(_) => true,
                    Shape::Rect(b) | Shape::Mark(b) => b.frame.w >= 2.0 && b.frame.h >= 2.0,
                    Shape::Redact(r) => r.w >= 2.0 && r.h >= 2.0,
                    _ => true,
                });
                // A click without a drag leaves a zero-size shape behind —
                // drop it, and the pending history step with it.
                if keep {
                    doc.commit();
                    self.set_tool(Tool::Select, cx);
                } else {
                    doc.remove(&[id]);
                }
            }
            Drag::Move { moved, reselect_text, .. } => {
                if moved {
                    if let Some(doc) = self.doc.as_mut() {
                        doc.commit();
                    }
                } else if let Some(id) = reselect_text {
                    // A click on a label that was already selected edits it.
                    if let (Some(g), Some(doc)) = (self.geom, self.doc.as_ref()) {
                        if doc.get(id).is_some() {
                            self.begin_edit(id, false, cx);
                            let p = g.to_image(ev.position);
                            self.edit_pointer_down(p, 1, false, cx);
                            if let Some(e) = &mut self.edit {
                                e.selecting = false;
                            }
                        }
                    }
                }
            }
            Drag::Handle { .. } => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.commit();
                }
            }
            Drag::Band { .. } => {}
            Drag::CropNew { .. } | Drag::CropMove { .. } | Drag::CropResize { .. } => {
                // A stray click shouldn't wipe the selection out.
                if self.crop.is_some_and(|c| c.w < 8.0 || c.h < 8.0) {
                    self.enter_crop();
                }
            }
            Drag::TextSelect => {
                if let Some(e) = &mut self.edit {
                    e.selecting = false;
                }
            }
            Drag::Slider(_) => {}
        }
        cx.notify();
    }

    pub(crate) fn on_area_scroll(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(g) = self.geom else { return };
        if g.overflow.x <= 0.0 && g.overflow.y <= 0.0 {
            return;
        }
        let d = match ev.delta {
            ScrollDelta::Pixels(p) => v2(f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => v2(l.x * 40.0, l.y * 40.0),
        };
        let d = if ev.modifiers.shift && d.x == 0.0 { v2(d.y, 0.0) } else { d };
        self.scroll =
            v2((self.scroll.x - d.x).clamp(0.0, g.overflow.x), (self.scroll.y - d.y).clamp(0.0, g.overflow.y));
        cx.notify();
    }

    fn cursor_at(&self, pos: Point<Pixels>, g: &StageGeom) -> Option<CursorStyle> {
        if !g.area.contains(&pos) {
            return None;
        }
        let p = g.to_image(pos);
        match self.tool {
            Tool::Crop => {
                let c = self.crop?;
                if let Some(h) = self.crop_handle_at(pos, g) {
                    return Some(match h {
                        CropHandle::Nw | CropHandle::Se => CursorStyle::ResizeUpLeftDownRight,
                        CropHandle::Ne | CropHandle::Sw => CursorStyle::ResizeUpRightDownLeft,
                    });
                }
                return Some(if c.contains(p) { CursorStyle::OpenHand } else { CursorStyle::Crosshair });
            }
            Tool::Select => {}
            _ => return Some(CursorStyle::Crosshair),
        }
        if let Some(edit) = &self.edit {
            let inside = self.doc.as_ref()?.get(edit.id)?.shape.frame(&mut *self.text.borrow_mut()).contains(p, 0.0);
            if inside {
                return Some(CursorStyle::IBeam);
            }
        }
        if let Some((_, handle, _, f)) = self.handle_at(pos, g) {
            return Some(match handle {
                Handle::Rotate | Handle::ArrowHead => CursorStyle::PointingHand,
                Handle::Side(_) => CursorStyle::ResizeLeftRight,
                Handle::Corner(i) => {
                    // Pick the diagonal cursor closest to the rotated corner.
                    let diag = f.corners()[i] - f.center;
                    if diag.x * diag.y > 0.0 {
                        CursorStyle::ResizeUpLeftDownRight
                    } else {
                        CursorStyle::ResizeUpRightDownLeft
                    }
                }
            });
        }
        let hit = self.doc.as_ref()?.hit(p, 4.0 / g.k, &mut *self.text.borrow_mut());
        hit.map(|_| CursorStyle::OpenHand)
    }

    /* ======================================================================
    Crop
    ====================================================================== */

    pub(crate) fn enter_crop(&mut self) {
        // Start inset rather than full-bleed: the handles are then plainly
        // visible and there is margin left to drag out a fresh region.
        if let Some(doc) = &self.doc {
            let (w, h) = (doc.shot.width as f32, doc.shot.height as f32);
            self.crop = Some(Rect::new(w * 0.1, h * 0.1, w * 0.8, h * 0.8));
        }
        self.selection.clear();
    }

    fn crop_handle_at(&self, pos: Point<Pixels>, g: &StageGeom) -> Option<CropHandle> {
        let c = self.crop?;
        let tl = g.to_screen(v2(c.x, c.y));
        let br = g.to_screen(v2(c.right(), c.bottom()));
        let inside = |x: Pixels, y: Pixels| pos.x >= x && pos.x <= x + px(12.0) && pos.y >= y && pos.y <= y + px(12.0);
        if inside(tl.x, tl.y) {
            Some(CropHandle::Nw)
        } else if inside(br.x - px(12.0), tl.y) {
            Some(CropHandle::Ne)
        } else if inside(tl.x, br.y - px(12.0)) {
            Some(CropHandle::Sw)
        } else if inside(br.x - px(12.0), br.y - px(12.0)) {
            Some(CropHandle::Se)
        } else {
            None
        }
    }

    fn crop_down(&mut self, pos: Point<Pixels>, p: V2, g: &StageGeom) {
        let Some(c) = self.crop else { return };
        let Some(doc) = &self.doc else { return };
        let q = v2(p.x.clamp(0.0, doc.shot.width as f32), p.y.clamp(0.0, doc.shot.height as f32));
        self.drag = Some(if let Some(handle) = self.crop_handle_at(pos, g) {
            Drag::CropResize { handle, from: c }
        } else if c.contains(p) {
            Drag::CropMove { start: p, from: c }
        } else {
            Drag::CropNew { start: q }
        });
    }

    pub(crate) fn apply_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(c), Some(doc)) = (self.crop, self.doc.as_mut()) else { return };
        let x = c.x.round().max(0.0) as u32;
        let y = c.y.round().max(0.0) as u32;
        let w = (c.w.round() as u32).max(1).min(doc.shot.width - x.min(doc.shot.width - 1));
        let h = (c.h.round() as u32).max(1).min(doc.shot.height - y.min(doc.shot.height - 1));
        if x == 0 && y == 0 && w == doc.shot.width && h == doc.shot.height {
            self.set_tool(Tool::Select, cx);
            return;
        }
        doc.crop(x, y, w, h);
        self.gpu.reset(window);
        self.crop = None;
        self.zoom = 0;
        self.set_tool(Tool::Select, cx);
        self.show_toast(format!("Cropped to {w} × {h}"), cx);
    }

    /* ======================================================================
    Text editing
    ====================================================================== */

    fn label(&self, id: Id) -> Option<Label> {
        match &self.doc.as_ref()?.get(id)?.shape {
            Shape::Text(l) => Some(l.clone()),
            _ => None,
        }
    }

    pub(crate) fn begin_edit(&mut self, id: Id, select_all: bool, cx: &mut Context<Self>) {
        let Some(label) = self.label(id) else { return };
        let layout = self.text.borrow_mut().label_layout(&label);
        let len = label.text.len();
        let selection = if select_all {
            Selection::new(
                Cursor::from_byte_index(&layout.layout, 0, Affinity::Downstream),
                Cursor::from_byte_index(&layout.layout, len, Affinity::Upstream),
            )
        } else {
            Selection::from_byte_index(&layout.layout, len, Affinity::Upstream)
        };
        self.edit = Some(TextEdit { id, selection, marked: None, selecting: false });
        cx.notify();
    }

    /// Ends editing. A label left blank is removed; a changed one is one undo
    /// step. Finishing leaves nothing selected, like every other tool.
    pub(crate) fn finish_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.take() else { return };
        if let Some(doc) = self.doc.as_mut() {
            let blank = matches!(doc.get(edit.id).map(|a| &a.shape), Some(Shape::Text(l)) if l.text.trim().is_empty());
            if blank {
                doc.remove(&[edit.id]);
            }
            doc.commit();
        }
        self.selection.retain(|id| *id != edit.id);
        if matches!(self.drag, Some(Drag::TextSelect)) {
            self.drag = None;
        }
        cx.notify();
    }

    fn label_local(&self, label: &Label, p: V2) -> V2 {
        let d = (p - label.origin).rotate(-label.angle);
        v2(d.x / label.scale, d.y / label.scale)
    }

    fn edit_pointer_down(&mut self, p: V2, clicks: usize, shift: bool, cx: &mut Context<Self>) {
        let Some(edit) = &self.edit else { return };
        let Some(label) = self.label(edit.id) else { return };
        let layout = self.text.borrow_mut().label_layout(&label);
        let l = self.label_local(&label, p);
        let sel = match clicks {
            2 => Selection::word_from_point(&layout.layout, l.x, l.y),
            n if n >= 3 => Selection::line_from_point(&layout.layout, l.x, l.y),
            _ if shift => edit.selection.extend_to_point(&layout.layout, l.x, l.y),
            _ => Selection::from_point(&layout.layout, l.x, l.y),
        };
        if let Some(e) = &mut self.edit {
            e.selection = sel;
            e.marked = None;
            e.selecting = true;
        }
        self.drag = Some(Drag::TextSelect);
        cx.notify();
    }

    fn edit_pointer_drag(&mut self, p: V2, cx: &mut Context<Self>) {
        let Some(edit) = &self.edit else { return };
        let Some(label) = self.label(edit.id) else { return };
        let layout = self.text.borrow_mut().label_layout(&label);
        let l = self.label_local(&label, p);
        let sel = edit.selection.extend_to_point(&layout.layout, l.x, l.y);
        if let Some(e) = &mut self.edit {
            e.selection = sel;
        }
        cx.notify();
    }

    fn edit_text(&self) -> Option<String> {
        self.label(self.edit.as_ref()?.id).map(|l| l.text)
    }

    /// Replaces `range` (bytes) with `text` and puts the caret after it.
    fn edit_replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let id = self.edit.as_ref()?.id;
        let doc = self.doc.as_mut()?;
        let Shape::Text(l) = &mut doc.get_mut(id)?.shape else { return None };
        let start = range.start.min(l.text.len());
        let end = range.end.clamp(start, l.text.len());
        l.text.replace_range(start..end, text);
        let label = l.clone();
        let caret = start + text.len();
        let layout = self.text.borrow_mut().label_layout(&label);
        if let Some(e) = &mut self.edit {
            e.selection = Selection::from_byte_index(&layout.layout, caret, Affinity::Downstream);
        }
        cx.notify();
        Some(start..caret)
    }

    fn edit_selection_range(&self) -> Range<usize> {
        self.edit.as_ref().map(|e| e.selection.text_range()).unwrap_or(0..0)
    }

    /// Keys while a label is being edited. Returns whether it was handled;
    /// unhandled printable keys fall through to the input handler.
    pub(crate) fn edit_key(&mut self, ev: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let ctrl = m.control || m.platform;
        let Some(edit) = &self.edit else { return false };
        let Some(label) = self.label(edit.id) else { return false };
        let layout = self.text.borrow_mut().label_layout(&label);
        let lay = &layout.layout;
        let sel = edit.selection;
        let text = label.text.clone();
        let set = |this: &mut Self, s: Selection, cx: &mut Context<Self>| {
            if let Some(e) = &mut this.edit {
                e.selection = s;
                e.marked = None;
            }
            cx.notify();
        };
        match ks.key.as_str() {
            "escape" => {
                self.finish_edit(cx);
                true
            }
            "enter" if ctrl => {
                self.finish_edit(cx);
                self.set_tool(Tool::Select, cx);
                true
            }
            "enter" => {
                let r = self.edit_selection_range();
                self.edit_replace(r, "\n", cx);
                true
            }
            "left" if ctrl => {
                set(self, sel.previous_visual_word(lay, m.shift), cx);
                true
            }
            "right" if ctrl => {
                set(self, sel.next_visual_word(lay, m.shift), cx);
                true
            }
            "left" => {
                let s = if !m.shift && !sel.is_collapsed() {
                    Selection::from_byte_index(lay, sel.text_range().start, Affinity::Downstream)
                } else {
                    sel.previous_visual(lay, m.shift)
                };
                set(self, s, cx);
                true
            }
            "right" => {
                let s = if !m.shift && !sel.is_collapsed() {
                    Selection::from_byte_index(lay, sel.text_range().end, Affinity::Upstream)
                } else {
                    sel.next_visual(lay, m.shift)
                };
                set(self, s, cx);
                true
            }
            "up" => {
                set(self, sel.previous_line(lay, m.shift), cx);
                true
            }
            "down" => {
                set(self, sel.next_line(lay, m.shift), cx);
                true
            }
            "home" if ctrl => {
                let c = Cursor::from_byte_index(lay, 0, Affinity::Downstream);
                set(self, if m.shift { sel.extend(c) } else { Selection::from(c) }, cx);
                true
            }
            "end" if ctrl => {
                let c = Cursor::from_byte_index(lay, text.len(), Affinity::Upstream);
                set(self, if m.shift { sel.extend(c) } else { Selection::from(c) }, cx);
                true
            }
            "home" => {
                set(self, sel.line_start(lay, m.shift), cx);
                true
            }
            "end" => {
                set(self, sel.line_end(lay, m.shift), cx);
                true
            }
            "backspace" | "delete" => {
                let forward = ks.key == "delete";
                let range = if !sel.is_collapsed() {
                    sel.text_range()
                } else if ctrl {
                    let moved =
                        if forward { sel.next_visual_word(lay, true) } else { sel.previous_visual_word(lay, true) };
                    moved.text_range()
                } else {
                    let at = sel.focus().index();
                    if forward { at..next_grapheme(&text, at) } else { prev_grapheme(&text, at)..at }
                };
                if !range.is_empty() {
                    self.edit_replace(range, "", cx);
                }
                true
            }
            "a" if ctrl => {
                let s = Selection::new(
                    Cursor::from_byte_index(lay, 0, Affinity::Downstream),
                    Cursor::from_byte_index(lay, text.len(), Affinity::Upstream),
                );
                set(self, s, cx);
                true
            }
            "c" | "x" if ctrl => {
                let r = sel.text_range();
                if !r.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text[r.clone()].to_string()));
                    if ks.key == "x" {
                        self.edit_replace(r, "", cx);
                    }
                }
                true
            }
            "v" if ctrl => {
                if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    let r = self.edit_selection_range();
                    self.edit_replace(r, &t, cx);
                }
                true
            }
            "z" | "y" if ctrl => true,
            "tab" => true,
            _ => {
                let _ = window;
                false
            }
        }
    }
}

fn prev_grapheme(text: &str, at: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text[..at].grapheme_indices(true).next_back().map(|(i, _)| i).unwrap_or(0)
}

fn next_grapheme(text: &str, at: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text[at..].grapheme_indices(true).nth(1).map(|(i, _)| at + i).unwrap_or(text.len())
}

fn utf16_to_byte(text: &str, u16i: usize) -> usize {
    let mut n = 0;
    for (i, c) in text.char_indices() {
        if n >= u16i {
            return i;
        }
        n += c.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, b: usize) -> usize {
    text[..b.min(text.len())].encode_utf16().count()
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.edit_text()?;
        let r = utf16_to_byte(&text, range.start)..utf16_to_byte(&text, range.end);
        adjusted.replace(byte_to_utf16(&text, r.start)..byte_to_utf16(&text, r.end));
        Some(text[r].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let text = self.edit_text()?;
        let sel = self.edit.as_ref()?.selection;
        let r = sel.text_range();
        Some(UTF16Selection {
            range: byte_to_utf16(&text, r.start)..byte_to_utf16(&text, r.end),
            reversed: sel.focus().index() < sel.anchor().index(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let text = self.edit_text()?;
        let m = self.edit.as_ref()?.marked.clone()?;
        Some(byte_to_utf16(&text, m.start)..byte_to_utf16(&text, m.end))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        if let Some(e) = &mut self.edit {
            e.marked = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.edit_text() else { return };
        let r = match range {
            Some(r) => utf16_to_byte(&text, r.start)..utf16_to_byte(&text, r.end),
            None => self.edit.as_ref().and_then(|e| e.marked.clone()).unwrap_or_else(|| self.edit_selection_range()),
        };
        self.edit_replace(r, new, cx);
        if let Some(e) = &mut self.edit {
            e.marked = None;
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.edit_text() else { return };
        let r = match range {
            Some(r) => utf16_to_byte(&text, r.start)..utf16_to_byte(&text, r.end),
            None => self.edit.as_ref().and_then(|e| e.marked.clone()).unwrap_or_else(|| self.edit_selection_range()),
        };
        let Some(inserted) = self.edit_replace(r, new, cx) else { return };
        let marked = (!new.is_empty()).then(|| inserted.clone());
        if let (Some(sel), Some(label)) = (new_selected, self.edit.as_ref().and_then(|e| self.label(e.id))) {
            let layout = self.text.borrow_mut().label_layout(&label);
            let a = inserted.start + utf16_to_byte(new, sel.start);
            let b = inserted.start + utf16_to_byte(new, sel.end);
            if let Some(e) = &mut self.edit {
                e.selection = Selection::new(
                    Cursor::from_byte_index(&layout.layout, a, Affinity::Downstream),
                    Cursor::from_byte_index(&layout.layout, b, Affinity::Downstream),
                );
            }
        }
        if let Some(e) = &mut self.edit {
            e.marked = marked;
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let g = self.geom?;
        let label = self.label(self.edit.as_ref()?.id)?;
        let layout = self.text.borrow_mut().label_layout(&label);
        let b = utf16_to_byte(&label.text, range.start);
        let bb = Cursor::from_byte_index(&layout.layout, b, Affinity::Downstream).geometry(&layout.layout, 1.0);
        let t = TextEngine::label_transform(&label).then(g.affine());
        let p0 = t.apply(v2(bb.x0 as f32, bb.y0 as f32));
        let p1 = t.apply(v2(bb.x1 as f32, bb.y1 as f32));
        Some(Bounds::from_corners(
            point(px(p0.x.min(p1.x)), px(p0.y.min(p1.y))),
            point(px(p0.x.max(p1.x)), px(p0.y.max(p1.y))),
        ))
    }

    fn character_index_for_point(&mut self, pt: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let g = self.geom?;
        let label = self.label(self.edit.as_ref()?.id)?;
        let layout = self.text.borrow_mut().label_layout(&label);
        let l = self.label_local(&label, g.to_image(pt));
        let c = Cursor::from_point(&layout.layout, l.x, l.y);
        Some(byte_to_utf16(&label.text, c.index()))
    }
}

/* ==========================================================================
Drawing helpers
========================================================================== */

fn to_point(p: V2) -> Point<Pixels> {
    point(px(p.x), px(p.y))
}

fn to_image_inverse(t: &Affine) -> Affine {
    let det = t.a * t.d - t.b * t.c;
    if det.abs() < 1e-12 {
        return Affine::IDENTITY;
    }
    let inv = 1.0 / det;
    let a = t.d * inv;
    let b = -t.b * inv;
    let c = -t.c * inv;
    let d = t.a * inv;
    Affine { a, b, c, d, e: -(a * t.e + c * t.f), f: -(b * t.e + d * t.f) }
}

fn label_key(l: &Label, h: &mut impl Hasher) {
    l.text.hash(h);
    l.font.hash(h);
    l.color.hash(h);
    for v in [l.width, l.font_size, l.unit, l.scale, l.angle] {
        v.to_bits().hash(h);
    }
}

pub(crate) fn gradient(a: Rgb, b: Rgb, _area: Bounds<Pixels>) -> gpui::Background {
    gpui::linear_gradient(
        135.0,
        gpui::linear_color_stop(theme::from_rgb(a), 0.0),
        gpui::linear_color_stop(theme::from_rgb(b), 1.0),
    )
    .color_space(gpui::ColorSpace::Srgb)
}

fn build_path(path: &crate::geom::Path, t: &Affine, mut pb: PathBuilder) -> Option<gpui::Path<Pixels>> {
    for el in &path.els {
        match *el {
            El::Move(p) => pb.move_to(to_point(t.apply(p))),
            El::Line(p) => pb.line_to(to_point(t.apply(p))),
            El::Cubic(a, b, c) => pb.cubic_bezier_to(to_point(t.apply(c)), to_point(t.apply(a)), to_point(t.apply(b))),
            El::Close => pb.close(),
        }
    }
    pb.build().ok()
}

fn stroke_builder(width: f32, round: bool) -> PathBuilder {
    let mut opts = gpui::StrokeOptions::default().with_line_width(width);
    if round {
        opts = opts.with_line_join(lyon::tessellation::LineJoin::Round);
    }
    PathBuilder::stroke(px(width)).with_style(gpui::PathStyle::Stroke(opts))
}

fn paint_arrow(arrow: &Arrow, t: &Affine, window: &mut Window) {
    let poly = crate::geom::Path::polygon(&arrow.outline());
    if let Some(p) = build_path(&poly, t, PathBuilder::fill()) {
        window.paint_path(p, theme::from_rgb(arrow.color));
    }
    let w = arrow.stroke_width() * t.scale_factor();
    if let Some(p) = build_path(&poly, t, stroke_builder(w, true)) {
        window.paint_path(p, gpui::rgb(0xffffff));
    }
}

fn paint_box(b: &BoxShape, t: &Affine, highlight: bool, window: &mut Window) {
    if b.frame.w <= 0.0 || b.frame.h <= 0.0 {
        return;
    }
    let path = b.path();
    if highlight {
        if let Some(p) = build_path(&path, t, PathBuilder::fill()) {
            window.paint_path(p, theme::with_alpha(theme::from_rgb(b.color), b.alpha));
        }
    } else if let Some(p) = build_path(&path, t, stroke_builder(b.stroke * t.scale_factor(), false)) {
        window.paint_path(p, theme::from_rgb(b.color));
    }
}

fn paint_redact(r: Rect, shot: &crate::doc::Shot, g: &StageGeom, window: &mut Window) {
    let Some(clip) = r.intersect(&Rect::new(0.0, 0.0, shot.width as f32, shot.height as f32)) else { return };
    let tl = g.to_screen(v2(clip.x, clip.y));
    let br = g.to_screen(v2(clip.right(), clip.bottom()));
    let mask = Bounds::from_corners(tl, br);
    let Some(plate) = shot.plate() else {
        window.paint_quad(PaintQuad::new(mask).background(gpui::rgb(0x101014)));
        return;
    };
    let Some((c0, r0, c1, r1)) = plate.span(clip.x, clip.y, clip.w, clip.h) else { return };
    let b = plate.block as f32;
    window.with_content_mask(Some(ContentMask { bounds: mask, ..Default::default() }), |window| {
        for row in r0..r1 {
            for col in c0..c1 {
                let [r, gg, bl, a] = plate.pixel(col, row);
                if a == 0 {
                    continue;
                }
                let p0 = g.to_screen(v2(col as f32 * b, row as f32 * b));
                let p1 = g.to_screen(v2((col + 1) as f32 * b, (row + 1) as f32 * b));
                // Whole pixels, so neighbouring blocks meet with no seam.
                let q = Bounds::from_corners(
                    point(px(f32::from(p0.x).round()), px(f32::from(p0.y).round())),
                    point(px(f32::from(p1.x).round()), px(f32::from(p1.y).round())),
                );
                window.paint_quad(PaintQuad::new(q).background(gpui::Rgba::new(
                    r as f32 / 255.0,
                    gg as f32 / 255.0,
                    bl as f32 / 255.0,
                    a as f32 / 255.0,
                )));
            }
        }
    });
}

fn fill_polygon(points: &[Point<Pixels>], color: impl Into<gpui::Background>, window: &mut Window) {
    let mut pb = PathBuilder::fill();
    pb.add_polygon(points, true);
    if let Ok(p) = pb.build() {
        window.paint_path(p, color);
    }
}

fn stroke_polygon(points: &[Point<Pixels>], width: f32, color: impl Into<gpui::Background>, window: &mut Window) {
    let mut pb = PathBuilder::stroke(px(width));
    pb.add_polygon(points, true);
    if let Ok(p) = pb.build() {
        window.paint_path(p, color);
    }
}

fn stroke_polyline(points: &[Point<Pixels>], width: f32, color: impl Into<gpui::Background>, window: &mut Window) {
    let mut pb = PathBuilder::stroke(px(width));
    pb.add_polygon(points, false);
    if let Ok(p) = pb.build() {
        window.paint_path(p, color);
    }
}

fn paint_handle_square(p: Point<Pixels>, window: &mut Window) {
    let b = Bounds::new(point(p.x - px(HANDLE / 2.0), p.y - px(HANDLE / 2.0)), size(px(HANDLE), px(HANDLE)));
    window.paint_quad(
        PaintQuad::new(b)
            .corner_radii(px(2.0))
            .background(gpui::rgb(0xffffff))
            .border_widths(px(1.0))
            .border_color(gpui::rgb(SEL_BORDER)),
    );
}

fn paint_handle_circle(
    p: Point<Pixels>,
    r: f32,
    fill: impl Into<gpui::Background>,
    border: impl Into<gpui::Background>,
    window: &mut Window,
) {
    let b = Bounds::new(point(p.x - px(r), p.y - px(r)), size(px(r * 2.0), px(r * 2.0)));
    window
        .paint_quad(PaintQuad::new(b).corner_radii(px(r)).background(fill).border_widths(px(1.5)).border_color(border));
}

fn paint_dots(window: &mut Window, area: Bounds<Pixels>, theme: &Theme) {
    let step = 22.0;
    let (x0, y0) = (f32::from(area.origin.x), f32::from(area.origin.y));
    let (w, h) = (f32::from(area.size.width), f32::from(area.size.height));
    let mut y = 0.0;
    while y < h {
        let mut x = 0.0;
        while x < w {
            window.paint_quad(
                PaintQuad::new(bounds(x0 + x, y0 + y, 2.0, 2.0)).corner_radii(px(1.0)).background(theme.dot),
            );
            x += step;
        }
        y += step;
    }
}

fn paint_scrollbars(g: &StageGeom, theme: &Theme, window: &mut Window) {
    let a = rect_of(g.area);
    let content_w = a.w + g.overflow.x;
    let content_h = a.h + g.overflow.y;
    if g.overflow.x > 0.0 {
        let len = (a.w * a.w / content_w).max(24.0);
        let scroll_x = f32::from(g.area.origin.x) + AREA_PAD - f32::from(g.stage.origin.x);
        let x = a.x + (a.w - len) * (scroll_x / g.overflow.x).clamp(0.0, 1.0);
        window.paint_quad(
            PaintQuad::new(bounds(x + 3.0, a.bottom() - 7.0, len - 6.0, 4.0))
                .corner_radii(px(2.0))
                .background(theme.border),
        );
    }
    if g.overflow.y > 0.0 {
        let len = (a.h * a.h / content_h).max(24.0);
        let scroll_y = f32::from(g.area.origin.y) + AREA_PAD - f32::from(g.stage.origin.y);
        let y = a.y + (a.h - len) * (scroll_y / g.overflow.y).clamp(0.0, 1.0);
        window.paint_quad(
            PaintQuad::new(bounds(a.right() - 7.0, y + 3.0, 4.0, len - 6.0))
                .corner_radii(px(2.0))
                .background(theme.border),
        );
    }
}
