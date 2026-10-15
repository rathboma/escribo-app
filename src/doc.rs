//! The annotated screenshot: the image, the annotations on it, and the undo
//! history. Everything here is in image space — one unit is one pixel of the
//! screenshot, with the origin at its top-left corner. Annotations may run
//! into the margin around the shot, so coordinates can be negative or reach
//! past the image.

use crate::geom::{OrientedBox, Path, Rect, V2, point_in_polygon, v2};
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

pub type Id = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Arrow,
    Rect,
    Mark,
    Redact,
    Text,
    Emoji,
    Crop,
}

impl Tool {
    pub const ALL: [Tool; 8] =
        [Tool::Select, Tool::Arrow, Tool::Rect, Tool::Mark, Tool::Redact, Tool::Text, Tool::Emoji, Tool::Crop];

    pub fn from_key(key: &str) -> Option<Tool> {
        Some(match key {
            "s" => Tool::Select,
            "a" => Tool::Arrow,
            "b" => Tool::Rect,
            "m" => Tool::Mark,
            "r" => Tool::Redact,
            "t" => Tool::Text,
            "e" => Tool::Emoji,
            "c" => Tool::Crop,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Arrow => "Arrow",
            Tool::Rect => "Box",
            Tool::Mark => "Mark",
            Tool::Redact => "Redact",
            Tool::Text => "Text",
            Tool::Emoji => "Emoji",
            Tool::Crop => "Crop",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Select => "S",
            Tool::Arrow => "A",
            Tool::Rect => "B",
            Tool::Mark => "M",
            Tool::Redact => "R",
            Tool::Text => "T",
            Tool::Emoji => "E",
            Tool::Crop => "C",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click an annotation to move or resize it",
            Tool::Arrow => "Drag to draw an arrow",
            Tool::Rect => "Drag to draw a box",
            Tool::Mark => "Drag to highlight a region",
            Tool::Redact => "Drag over anything that should not be readable",
            Tool::Text => "Click anywhere to add a label",
            Tool::Emoji => "Pick an emoji, then click to place it",
            Tool::Crop => "Drag the handles, then apply",
        }
    }
}

pub const EMOJIS: [&str; 16] =
    ["🔥", "👍", "👎", "🎉", "⚠️", "✅", "❌", "💡", "🤔", "😍", "😭", "💀", "🚀", "⭐", "🙈", "🫠"];

/* ==========================================================================
Colour
========================================================================== */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const WHITE: Rgb = Rgb { r: 255, g: 255, b: 255 };
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn parse(hex: &str) -> Option<Rgb> {
        let h = hex.strip_prefix('#')?;
        if h.len() != 6 {
            return None;
        }
        let n = u32::from_str_radix(h, 16).ok()?;
        Some(Rgb::new((n >> 16) as u8, (n >> 8) as u8, n as u8))
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn to_u32(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
}

/// A hex colour from settings, falling back when it does not parse.
pub fn rgb(hex: &str) -> Rgb {
    Rgb::parse(hex).unwrap_or(Rgb::new(0xFF, 0x00, 0x7F))
}

pub const BRAND: Rgb = Rgb::new(0xFF, 0x00, 0x7F);

/* ==========================================================================
Annotations
========================================================================== */

/// A rectangle-like annotation (box or highlight): a frame that may be
/// rotated, a colour, and the style it was drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxShape {
    pub frame: OrientedBox,
    pub color: Rgb,
    /// stroke width for a box; unused for a highlight
    pub stroke: f32,
    pub radius: f32,
    /// fill opacity for a highlight, 0-1
    pub alpha: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    pub tail: V2,
    pub head: V2,
    /// the annotation unit it was drawn at; sets its weight
    pub unit: f32,
    pub color: Rgb,
}

/// A text label. Positioned by its top-left corner, which stays put while the
/// text changes, and rotated about its centre.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub origin: V2,
    pub angle: f32,
    pub scale: f32,
    /// wrap width, unscaled
    pub width: f32,
    pub font_size: f32,
    pub unit: f32,
    pub color: Rgb,
    /// a font id from settings::FONTS or a custom family
    pub font: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Emoji {
    pub center: V2,
    pub angle: f32,
    pub scale: f32,
    pub size: f32,
    pub ch: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Arrow(Arrow),
    Rect(BoxShape),
    Mark(BoxShape),
    Redact(Rect),
    Text(Label),
    Emoji(Emoji),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub id: Id,
    pub shape: Shape,
}

/// Sizes of the things only the text engine can measure: a label's laid-out
/// box and an emoji's glyph box, both unscaled.
pub trait Measure {
    fn label_size(&mut self, label: &Label) -> V2;
    fn emoji_size(&mut self, emoji: &Emoji) -> V2;
}

/// Which handles an annotation offers when it is the only thing selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    pub corners: bool,
    pub sides: bool,
    pub rotate: bool,
    pub arrow_head: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// tl, tr, br, bl — the same order as OrientedBox::corners
    Corner(usize),
    /// ml (0) or mr (1): text width
    Side(usize),
    Rotate,
    ArrowHead,
}

impl Arrow {
    pub fn angle(&self) -> f32 {
        let d = self.head - self.tail;
        d.y.atan2(d.x)
    }

    pub fn length(&self) -> f32 {
        (self.head - self.tail).len()
    }

    /// The outline in the arrow's own frame (tail at the origin, pointing
    /// along +x). Base outline is 40 units long; the head widens and the tip
    /// pulls back as the arrow grows, so long arrows keep a bold head.
    pub fn local_outline(&self) -> [V2; 6] {
        let u = self.unit;
        let base_w = 40.0 * u;
        let len = self.length().max(1e-3);
        let sx = len / base_w;
        let sy = sx.min(3.0);
        let k = 1.0 + 0.1 * sx;
        let factor = (sx / 4.0).min(1.0);
        let tip = (26.0 * u) * (1.0 - factor) + (35.0 * u) * factor;
        let pts = [
            v2(0.0, 0.0),
            v2(tip, 2.0 * u * k),
            v2(tip, 5.0 * u * k),
            v2(base_w, 0.0),
            v2(tip, -5.0 * u * k),
            v2(tip, -2.0 * u * k),
        ];
        pts.map(|p| v2(p.x * sx, p.y * sy))
    }

    pub fn outline(&self) -> [V2; 6] {
        let a = self.angle();
        self.local_outline().map(|p| self.tail + p.rotate(a))
    }

    pub fn stroke_width(&self) -> f32 {
        4.0 * self.unit
    }

    pub fn frame(&self) -> OrientedBox {
        let local = self.local_outline();
        let half_h = local.iter().fold(0.0f32, |m, p| m.max(p.y.abs()));
        let len = self.length();
        let a = self.angle();
        OrientedBox { center: self.tail + v2(len / 2.0, 0.0).rotate(a), w: len, h: half_h * 2.0, angle: a }
    }
}

impl BoxShape {
    pub fn path(&self) -> Path {
        Path::round_rect(self.frame.w, self.frame.h, self.radius).transformed(
            &crate::geom::Affine::rotate(self.frame.angle).then(crate::geom::Affine::translate(self.frame.center)),
        )
    }
}

impl Label {
    pub fn frame(&self, size: V2) -> OrientedBox {
        let (w, h) = (size.x * self.scale, size.y * self.scale);
        OrientedBox { center: self.origin + v2(w / 2.0, h / 2.0).rotate(self.angle), w, h, angle: self.angle }
    }

    /// Moves the origin so the frame's centre lands on `center`.
    fn set_center(&mut self, size: V2, center: V2) {
        let (w, h) = (size.x * self.scale, size.y * self.scale);
        self.origin = center - v2(w / 2.0, h / 2.0).rotate(self.angle);
    }
}

impl Emoji {
    pub fn frame(&self, size: V2) -> OrientedBox {
        OrientedBox { center: self.center, w: size.x * self.scale, h: size.y * self.scale, angle: self.angle }
    }
}

impl Shape {
    pub fn tool(&self) -> Tool {
        match self {
            Shape::Arrow(_) => Tool::Arrow,
            Shape::Rect(_) => Tool::Rect,
            Shape::Mark(_) => Tool::Mark,
            Shape::Redact(_) => Tool::Redact,
            Shape::Text(_) => Tool::Text,
            Shape::Emoji(_) => Tool::Emoji,
        }
    }

    /// The box it is selected, hit-tested and transformed by.
    pub fn frame(&self, m: &mut dyn Measure) -> OrientedBox {
        match self {
            Shape::Arrow(a) => a.frame(),
            Shape::Rect(b) | Shape::Mark(b) => b.frame,
            Shape::Redact(r) => OrientedBox { center: r.center(), w: r.w, h: r.h, angle: 0.0 },
            Shape::Text(l) => l.frame(m.label_size(l)),
            Shape::Emoji(e) => e.frame(m.emoji_size(e)),
        }
    }

    pub fn controls(&self) -> Controls {
        let none = Controls { corners: false, sides: false, rotate: false, arrow_head: false };
        match self {
            Shape::Arrow(_) => Controls { arrow_head: true, ..none },
            Shape::Rect(_) | Shape::Mark(_) | Shape::Emoji(_) => Controls { corners: true, rotate: true, ..none },
            Shape::Text(_) => Controls { corners: true, sides: true, rotate: true, ..none },
            // Blocks line up with the screenshot's own grid, so a rotated box
            // would scramble one region and paint the result over another.
            Shape::Redact(_) => Controls { corners: true, ..none },
        }
    }

    /// Hit test in image space; `tol` widens the target (a few screen pixels).
    pub fn hit(&self, p: V2, tol: f32, m: &mut dyn Measure) -> bool {
        match self {
            Shape::Arrow(a) => {
                let outline = a.outline();
                point_in_polygon(p, &outline)
                    || outline
                        .iter()
                        .zip(outline.iter().cycle().skip(1))
                        .any(|(s, e)| crate::geom::dist_to_segment(p, *s, *e) <= tol + a.stroke_width() / 2.0)
            }
            Shape::Rect(b) => b.frame.contains(p, tol + b.stroke / 2.0),
            _ => self.frame(m).contains(p, tol),
        }
    }

    pub fn translate(&mut self, d: V2) {
        match self {
            Shape::Arrow(a) => {
                a.tail += d;
                a.head += d;
            }
            Shape::Rect(b) | Shape::Mark(b) => b.frame.center += d,
            Shape::Redact(r) => {
                r.x += d.x;
                r.y += d.y;
            }
            Shape::Text(l) => l.origin += d,
            Shape::Emoji(e) => e.center += d,
        }
    }

    /// Recolours, as a palette swatch does for the selection. Emoji keep their
    /// own colours and a scramble takes the shot's.
    pub fn set_color(&mut self, color: Rgb, highlight_alpha: f32) {
        match self {
            Shape::Arrow(a) => a.color = color,
            Shape::Rect(b) => b.color = color,
            Shape::Mark(b) => {
                b.color = color;
                b.alpha = highlight_alpha;
            }
            Shape::Text(l) => l.color = color,
            Shape::Emoji(_) | Shape::Redact(_) => {}
        }
    }

    /// A drag on one of the handles. `start` is the shape when the drag
    /// began and `start_frame` its frame; `p` is the pointer in image space.
    pub fn drag_handle(
        &mut self,
        start: &Shape,
        start_frame: OrientedBox,
        handle: Handle,
        p: V2,
        modifiers: DragModifiers,
        m: &mut dyn Measure,
    ) {
        match handle {
            Handle::ArrowHead => {
                if let Shape::Arrow(a) = self {
                    a.head = p;
                }
            }
            Handle::Rotate => {
                let d = p - start_frame.center;
                let mut angle = d.y.atan2(d.x) + FRAC_PI_2;
                if modifiers.shift {
                    let step = 15f32.to_radians();
                    angle = (angle / step).round() * step;
                }
                self.set_angle(angle, start_frame.center, m);
            }
            Handle::Corner(i) => self.drag_corner(start, start_frame, i, p, modifiers, m),
            Handle::Side(i) => {
                if let (Shape::Text(l), Shape::Text(s)) = (&mut *self, start) {
                    let fixed_x = if i == 0 { start_frame.w / 2.0 } else { -start_frame.w / 2.0 };
                    let local = start_frame.to_local(p);
                    let new_w = ((local.x - fixed_x).abs() / s.scale).max(s.font_size);
                    l.width = new_w;
                    l.origin = s.origin;
                    if i == 0 {
                        // The right edge stays put: move the origin along the
                        // label's own x axis by the change in width.
                        let dw = (m.label_size(l).x - m.label_size(s).x) * s.scale;
                        l.origin = s.origin - v2(dw, 0.0).rotate(s.angle);
                    }
                }
            }
        }
    }

    fn set_angle(&mut self, angle: f32, center: V2, m: &mut dyn Measure) {
        match self {
            Shape::Rect(b) | Shape::Mark(b) => {
                b.frame.angle = angle;
                b.frame.center = center;
            }
            Shape::Text(l) => {
                let size = m.label_size(l);
                l.angle = angle;
                l.set_center(size, center);
            }
            Shape::Emoji(e) => e.angle = angle,
            _ => {}
        }
    }

    fn drag_corner(
        &mut self,
        start: &Shape,
        f: OrientedBox,
        corner: usize,
        p: V2,
        modifiers: DragModifiers,
        m: &mut dyn Measure,
    ) {
        let corners = f.corners();
        let fixed = corners[(corner + 2) % 4];
        let fixed_local = f.to_local(fixed);
        let moving_local = f.to_local(p);
        match (&mut *self, start) {
            (Shape::Rect(b), _) | (Shape::Mark(b), _) => {
                let mut w = (moving_local.x - fixed_local.x).abs();
                let mut h = (moving_local.y - fixed_local.y).abs();
                if modifiers.shift && f.w > 0.0 && f.h > 0.0 {
                    let k = (w / f.w).max(h / f.h);
                    w = f.w * k;
                    h = f.h * k;
                }
                let sx = if moving_local.x >= fixed_local.x { 1.0 } else { -1.0 };
                let sy = if moving_local.y >= fixed_local.y { 1.0 } else { -1.0 };
                b.frame.w = w.max(1.0);
                b.frame.h = h.max(1.0);
                b.frame.center = f.to_world(fixed_local + v2(sx * w / 2.0, sy * h / 2.0));
            }
            (Shape::Redact(r), _) => {
                let rect = Rect::from_points(fixed, p);
                *r = Rect::new(rect.x, rect.y, rect.w.max(1.0), rect.h.max(1.0));
            }
            (Shape::Text(l), Shape::Text(s)) => {
                let ratio = uniform_ratio(fixed, corners[corner], p);
                l.scale = (s.scale * ratio).max(0.05);
                let size = m.label_size(l);
                let center = fixed + (f.center - fixed) * (l.scale / s.scale);
                l.set_center(size, center);
            }
            (Shape::Emoji(e), Shape::Emoji(s)) => {
                let ratio = uniform_ratio(fixed, corners[corner], p);
                e.scale = (s.scale * ratio).max(0.05);
                e.center = fixed + (f.center - fixed) * (e.scale / s.scale);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragModifiers {
    pub shift: bool,
}

/// How far the pointer has moved a corner along the box's diagonal, as a
/// ratio of the diagonal's original length.
fn uniform_ratio(fixed: V2, corner: V2, p: V2) -> f32 {
    let diag = corner - fixed;
    let len2 = diag.dot(diag);
    if len2 <= 0.0 {
        return 1.0;
    }
    ((p - fixed).dot(diag) / len2).max(0.01)
}

/* ==========================================================================
Layout — the output box (ratio + padding) with the shot fitted inside
========================================================================== */

/// Geometry of the composed output at 1×, in image pixels: the box, its
/// padding, and where the shot sits in it. "Orig" wraps the screenshot in a
/// uniform margin and adds no canvas; a fixed ratio fits the screenshot inside
/// the padded box and leaves the rest as extra canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub out_w: f64,
    pub out_h: f64,
    pub pad: f64,
    pub shot_x: f64,
    pub shot_y: f64,
}

pub fn natural_layout(img_w: f64, img_h: f64, pad_pct: u32, ratio: Option<f64>) -> Layout {
    let f = pad_pct as f64 / 100.0;
    let Some(r) = ratio else {
        let out_w = img_w / (1.0 - 2.0 * f);
        let pad = f * out_w;
        return Layout { out_w, out_h: img_h + 2.0 * pad, pad, shot_x: pad, shot_y: pad };
    };
    // Ratio of the box left once padding is taken off each side.
    let avail_ratio = (1.0 - 2.0 * f) / (1.0 / r - 2.0 * f);
    let height_bound = avail_ratio >= img_w / img_h;
    let out_w = if height_bound { img_h / (1.0 / r - 2.0 * f) } else { img_w / (1.0 - 2.0 * f) };
    let out_h = out_w / r;
    let pad = f * out_w;
    Layout {
        out_w,
        out_h,
        pad,
        shot_x: pad + (out_w - 2.0 * pad - img_w) / 2.0,
        shot_y: pad + (out_h - 2.0 * pad - img_h) / 2.0,
    }
}

/* ==========================================================================
The screenshot
========================================================================== */

/// The decoded screenshot, straight-alpha RGBA.
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    pub has_alpha: bool,
    plate: std::sync::OnceLock<Option<crate::redact::Plate>>,
}

impl Shot {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        let has_alpha = rgba.chunks_exact(4).any(|p| p[3] != 255);
        Self { width, height, rgba: Arc::new(rgba), has_alpha, plate: std::sync::OnceLock::new() }
    }

    /// One "unit" is a pixel as it would look on a 900px-wide render, so
    /// annotation weights stay proportional whatever the resolution is.
    pub fn unit(&self) -> f32 {
        (self.width as f32 / 900.0).max(1.0)
    }

    /// Block averages for the redact tool, built the first time they are
    /// needed and kept for as long as the image is.
    pub fn plate(&self) -> Option<&crate::redact::Plate> {
        self.plate
            .get_or_init(|| crate::redact::Plate::build(&self.rgba, self.width, self.height, self.unit()))
            .as_ref()
    }

    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Shot {
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let start = ((row * self.width + x) * 4) as usize;
            out.extend_from_slice(&self.rgba[start..start + (w * 4) as usize]);
        }
        Shot::new(w, h, out)
    }
}

/* ==========================================================================
Document & history
========================================================================== */

pub struct Document {
    pub shot: Arc<Shot>,
    pub name: String,
    pub annotations: Vec<Annotation>,
    next_id: Id,
    undo: Vec<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
    /// The committed state the next change is measured against.
    baseline: Vec<Annotation>,
}

impl Document {
    pub fn new(shot: Shot, name: String) -> Self {
        Self {
            shot: Arc::new(shot),
            name,
            annotations: Vec::new(),
            next_id: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            baseline: Vec::new(),
        }
    }

    /// A copy of the annotations over the same shot, for the export thread.
    pub fn snapshot(&self) -> Document {
        Document {
            shot: self.shot.clone(),
            name: self.name.clone(),
            annotations: self.annotations.clone(),
            next_id: self.next_id,
            undo: Vec::new(),
            redo: Vec::new(),
            baseline: Vec::new(),
        }
    }

    pub fn add(&mut self, shape: Shape) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        self.annotations.push(Annotation { id, shape });
        id
    }

    pub fn get(&self, id: Id) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Annotation> {
        self.annotations.iter_mut().find(|a| a.id == id)
    }

    pub fn remove(&mut self, ids: &[Id]) {
        self.annotations.retain(|a| !ids.contains(&a.id));
    }

    /// Topmost annotation under `p`.
    pub fn hit(&self, p: V2, tol: f32, m: &mut dyn Measure) -> Option<Id> {
        self.annotations.iter().rev().find(|a| a.shape.hit(p, tol, m)).map(|a| a.id)
    }

    /// Records the current annotations as one undoable step, if they changed.
    pub fn commit(&mut self) -> bool {
        if self.annotations == self.baseline {
            return false;
        }
        self.undo.push(std::mem::replace(&mut self.baseline, self.annotations.clone()));
        self.redo.clear();
        true
    }

    /// Drops uncommitted changes, going back to the last committed state.
    pub fn revert(&mut self) {
        self.annotations = self.baseline.clone();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.baseline, prev.clone()));
        self.annotations = prev;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.baseline, next.clone()));
        self.annotations = next;
        true
    }

    /// Cuts the shot to `rect` (image pixels) and moves the annotations with
    /// it. Earlier history no longer lines up, so it is dropped.
    pub fn crop(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.shot = Arc::new(self.shot.crop(x, y, w, h));
        let d = v2(-(x as f32), -(y as f32));
        for a in &mut self.annotations {
            a.shape.translate(d);
        }
        self.undo.clear();
        self.redo.clear();
        self.baseline = self.annotations.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;
    impl Measure for Fixed {
        fn label_size(&mut self, l: &Label) -> V2 {
            v2(l.width, l.font_size * 1.3)
        }
        fn emoji_size(&mut self, e: &Emoji) -> V2 {
            v2(e.size, e.size * 1.13)
        }
    }

    fn shot(w: u32, h: u32) -> Shot {
        Shot::new(w, h, vec![255; (w * h * 4) as usize])
    }

    #[test]
    fn orig_layout_wraps_in_uniform_margin() {
        let l = natural_layout(800.0, 600.0, 10, None);
        assert!((l.out_w - 1000.0).abs() < 1e-9);
        assert!((l.pad - 100.0).abs() < 1e-9);
        assert!((l.out_h - 800.0).abs() < 1e-9);
        assert_eq!((l.shot_x, l.shot_y), (100.0, 100.0));
    }

    #[test]
    fn ratio_layout_centres_the_shot() {
        let l = natural_layout(1000.0, 1000.0, 0, Some(16.0 / 9.0));
        assert!((l.out_h - 1000.0).abs() < 1e-6);
        assert!((l.out_w / l.out_h - 16.0 / 9.0).abs() < 1e-9);
        assert!((l.shot_x - (l.out_w - 1000.0) / 2.0).abs() < 1e-6);
        assert!(l.shot_y.abs() < 1e-9);
    }

    #[test]
    fn arrow_head_lands_on_the_pointer() {
        let a = Arrow { tail: v2(10.0, 10.0), head: v2(110.0, 60.0), unit: 1.0, color: BRAND };
        let tip = a.outline()[3];
        assert!((tip - a.head).len() < 1e-3);
        assert!(a.outline()[0] == a.tail);
        assert!(Shape::Arrow(a.clone()).hit(v2(60.0, 35.0), 0.0, &mut Fixed));
        assert!(!Shape::Arrow(a).hit(v2(10.0, 100.0), 2.0, &mut Fixed));
    }

    #[test]
    fn history_round_trips() {
        let mut d = Document::new(shot(10, 10), "t".into());
        assert!(!d.commit());
        let id = d.add(Shape::Redact(Rect::new(0.0, 0.0, 4.0, 4.0)));
        assert!(d.commit());
        d.get_mut(id).unwrap().shape.translate(v2(1.0, 1.0));
        assert!(d.commit());
        assert!(d.undo());
        assert_eq!(d.get(id).unwrap().shape, Shape::Redact(Rect::new(0.0, 0.0, 4.0, 4.0)));
        assert!(d.undo());
        assert!(d.annotations.is_empty());
        assert!(!d.undo());
        assert!(d.redo() && d.redo());
        assert_eq!(d.get(id).unwrap().shape, Shape::Redact(Rect::new(1.0, 1.0, 4.0, 4.0)));
        assert!(!d.can_redo());
    }

    #[test]
    fn crop_moves_annotations_and_clears_history() {
        let mut d = Document::new(shot(100, 100), "t".into());
        d.add(Shape::Redact(Rect::new(50.0, 50.0, 10.0, 10.0)));
        d.commit();
        d.crop(20, 30, 50, 40);
        assert_eq!((d.shot.width, d.shot.height), (50, 40));
        assert_eq!(d.annotations[0].shape, Shape::Redact(Rect::new(30.0, 20.0, 10.0, 10.0)));
        assert!(!d.can_undo());
    }

    #[test]
    fn corner_drag_keeps_the_opposite_corner() {
        let start = Shape::Rect(BoxShape {
            frame: OrientedBox { center: v2(50.0, 50.0), w: 20.0, h: 10.0, angle: 0.3 },
            color: BRAND,
            stroke: 5.0,
            radius: 2.0,
            alpha: 1.0,
        });
        let f = start.frame(&mut Fixed);
        let fixed = f.corners()[0];
        let mut s = start.clone();
        s.drag_handle(
            &start,
            f,
            Handle::Corner(2),
            f.corners()[2] + v2(10.0, 5.0),
            DragModifiers::default(),
            &mut Fixed,
        );
        let nf = s.frame(&mut Fixed);
        assert!((nf.corners()[0] - fixed).len() < 1e-3);
    }

    #[test]
    fn label_corner_scales_uniformly() {
        let l = Label {
            origin: v2(0.0, 0.0),
            angle: 0.0,
            scale: 1.0,
            width: 100.0,
            font_size: 20.0,
            unit: 1.0,
            color: BRAND,
            font: "system".into(),
            text: "hi".into(),
        };
        let start = Shape::Text(l);
        let f = start.frame(&mut Fixed);
        let mut s = start.clone();
        s.drag_handle(
            &start,
            f,
            Handle::Corner(2),
            f.corners()[2] + (f.corners()[2] - f.corners()[0]),
            DragModifiers::default(),
            &mut Fixed,
        );
        let Shape::Text(l) = &s else { unreachable!() };
        assert!((l.scale - 2.0).abs() < 1e-4);
        assert!(l.origin.len() < 1e-3);
    }
}
