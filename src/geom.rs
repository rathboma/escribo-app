//! Small 2D geometry: points, affine transforms and the path description both
//! renderers (gpui on screen, tiny-skia for export) draw from.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    pub const ZERO: V2 = v2(0.0, 0.0);

    pub fn len(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }

    pub fn rotate(self, angle: f32) -> V2 {
        let (s, c) = angle.sin_cos();
        v2(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    pub fn lerp(self, o: V2, t: f32) -> V2 {
        self + (o - self) * t
    }

    pub fn min(self, o: V2) -> V2 {
        v2(self.x.min(o.x), self.y.min(o.y))
    }

    pub fn max(self, o: V2) -> V2 {
        v2(self.x.max(o.x), self.y.max(o.y))
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, k: f32) -> V2 {
        v2(self.x * k, self.y * k)
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        v2(-self.x, -self.y)
    }
}

/// An axis-aligned rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_points(a: V2, b: V2) -> Self {
        let lo = a.min(b);
        let hi = a.max(b);
        Self::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y)
    }

    pub fn bounding(points: &[V2]) -> Self {
        let mut lo = v2(f32::INFINITY, f32::INFINITY);
        let mut hi = v2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        Self::from_points(lo, hi)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> V2 {
        v2(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, p: V2) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x <= o.right() && o.x <= self.right() && self.y <= o.bottom() && o.y <= self.bottom()
    }

    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect::from_points(v2(self.x.min(o.x), self.y.min(o.y)), v2(self.right().max(o.right()), self.bottom().max(o.bottom())))
    }

    pub fn inflate(&self, d: f32) -> Rect {
        Rect::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }

    pub fn corners(&self) -> [V2; 4] {
        [v2(self.x, self.y), v2(self.right(), self.y), v2(self.right(), self.bottom()), v2(self.x, self.bottom())]
    }
}

/// A box that may be rotated about its centre: the frame every annotation is
/// selected, hit-tested and transformed in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrientedBox {
    pub center: V2,
    pub w: f32,
    pub h: f32,
    /// radians, clockwise on screen (y points down)
    pub angle: f32,
}

impl OrientedBox {
    /// A world point in the box's own frame, relative to its centre.
    pub fn to_local(&self, p: V2) -> V2 {
        (p - self.center).rotate(-self.angle)
    }

    pub fn to_world(&self, local: V2) -> V2 {
        self.center + local.rotate(self.angle)
    }

    pub fn contains(&self, p: V2, pad: f32) -> bool {
        let l = self.to_local(p);
        l.x.abs() <= self.w / 2.0 + pad && l.y.abs() <= self.h / 2.0 + pad
    }

    /// tl, tr, br, bl in world space.
    pub fn corners(&self) -> [V2; 4] {
        let (hw, hh) = (self.w / 2.0, self.h / 2.0);
        [v2(-hw, -hh), v2(hw, -hh), v2(hw, hh), v2(-hw, hh)].map(|c| self.to_world(c))
    }

    pub fn aabb(&self) -> Rect {
        Rect::bounding(&self.corners())
    }
}

/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f` — the canvas / tiny-skia layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(t: V2) -> Affine {
        Affine { e: t.x, f: t.y, ..Self::IDENTITY }
    }

    pub fn scale(sx: f32, sy: f32) -> Affine {
        Affine { a: sx, d: sy, ..Self::IDENTITY }
    }

    pub fn rotate(angle: f32) -> Affine {
        let (s, c) = angle.sin_cos();
        Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }

    /// Applies `self` first, then `after`.
    pub fn then(self, after: Affine) -> Affine {
        let m = self;
        let n = after;
        Affine {
            a: n.a * m.a + n.c * m.b,
            b: n.b * m.a + n.d * m.b,
            c: n.a * m.c + n.c * m.d,
            d: n.b * m.c + n.d * m.d,
            e: n.a * m.e + n.c * m.f + n.e,
            f: n.b * m.e + n.d * m.f + n.f,
        }
    }

    pub fn apply(&self, p: V2) -> V2 {
        v2(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    /// How much the transform scales lengths (geometric mean of the axes).
    pub fn scale_factor(&self) -> f32 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }

    pub fn to_skia(self) -> tiny_skia::Transform {
        tiny_skia::Transform::from_row(self.a, self.b, self.c, self.d, self.e, self.f)
    }
}

/// A path both renderers can draw. Arcs are already reduced to cubics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum El {
    Move(V2),
    Line(V2),
    Cubic(V2, V2, V2),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub els: Vec<El>,
}

impl Path {
    pub fn polygon(points: &[V2]) -> Path {
        let mut els = Vec::with_capacity(points.len() + 1);
        for (i, p) in points.iter().enumerate() {
            els.push(if i == 0 { El::Move(*p) } else { El::Line(*p) });
        }
        els.push(El::Close);
        Path { els }
    }

    /// A rounded rectangle centred on the origin, in its own frame.
    pub fn round_rect(w: f32, h: f32, r: f32) -> Path {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (x0, y0, x1, y1) = (-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
        if r <= 0.0 {
            return Path::polygon(&[v2(x0, y0), v2(x1, y0), v2(x1, y1), v2(x0, y1)]);
        }
        // Quarter circles as cubics (the usual 0.5523 handle length).
        let k = r * 0.552_284_8;
        let mut p = Path::default();
        p.els.push(El::Move(v2(x0 + r, y0)));
        p.els.push(El::Line(v2(x1 - r, y0)));
        p.els.push(El::Cubic(v2(x1 - r + k, y0), v2(x1, y0 + r - k), v2(x1, y0 + r)));
        p.els.push(El::Line(v2(x1, y1 - r)));
        p.els.push(El::Cubic(v2(x1, y1 - r + k), v2(x1 - r + k, y1), v2(x1 - r, y1)));
        p.els.push(El::Line(v2(x0 + r, y1)));
        p.els.push(El::Cubic(v2(x0 + r - k, y1), v2(x0, y1 - r + k), v2(x0, y1 - r)));
        p.els.push(El::Line(v2(x0, y0 + r)));
        p.els.push(El::Cubic(v2(x0, y0 + r - k), v2(x0 + r - k, y0), v2(x0 + r, y0)));
        p.els.push(El::Close);
        p
    }

    pub fn transformed(&self, t: &Affine) -> Path {
        Path {
            els: self
                .els
                .iter()
                .map(|e| match *e {
                    El::Move(p) => El::Move(t.apply(p)),
                    El::Line(p) => El::Line(t.apply(p)),
                    El::Cubic(a, b, c) => El::Cubic(t.apply(a), t.apply(b), t.apply(c)),
                    El::Close => El::Close,
                })
                .collect(),
        }
    }

    pub fn to_skia(&self) -> Option<tiny_skia::Path> {
        let mut pb = tiny_skia::PathBuilder::new();
        for e in &self.els {
            match *e {
                El::Move(p) => pb.move_to(p.x, p.y),
                El::Line(p) => pb.line_to(p.x, p.y),
                El::Cubic(a, b, c) => pb.cubic_to(a.x, a.y, b.x, b.y, c.x, c.y),
                El::Close => pb.close(),
            }
        }
        pb.finish()
    }
}

/// Even-odd point in polygon.
pub fn point_in_polygon(p: V2, poly: &[V2]) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn dist_to_segment(p: V2, a: V2, b: V2) -> f32 {
    let ab = b - a;
    let t = if ab.dot(ab) > 0.0 { ((p - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0) } else { 0.0 };
    (p - (a + ab * t)).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_composes_in_order() {
        let t = Affine::scale(2.0, 2.0).then(Affine::translate(v2(10.0, 0.0)));
        assert_eq!(t.apply(v2(1.0, 1.0)), v2(12.0, 2.0));
        let r = Affine::rotate(std::f32::consts::FRAC_PI_2);
        let p = r.apply(v2(1.0, 0.0));
        assert!((p.x).abs() < 1e-6 && (p.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn oriented_box_round_trips() {
        let b = OrientedBox { center: v2(5.0, 5.0), w: 4.0, h: 2.0, angle: 0.7 };
        let p = v2(6.0, 4.5);
        let back = b.to_world(b.to_local(p));
        assert!((back - p).len() < 1e-5);
        assert!(b.contains(b.center, 0.0));
        assert!(!b.contains(v2(50.0, 5.0), 0.0));
    }

    #[test]
    fn polygon_hit() {
        let sq = [v2(0.0, 0.0), v2(2.0, 0.0), v2(2.0, 2.0), v2(0.0, 2.0)];
        assert!(point_in_polygon(v2(1.0, 1.0), &sq));
        assert!(!point_in_polygon(v2(3.0, 1.0), &sq));
    }
}
