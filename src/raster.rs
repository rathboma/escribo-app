//! Low-level pixel helpers shared by the export renderer and the bitmaps the
//! editor shows on screen.

use tiny_skia::{IntRect, Pixmap};

/// Gaussian-ish blur of a premultiplied pixmap's alpha, in place: three box
/// passes each way, which is within a few percent of a true gaussian and runs
/// in time independent of the radius. Colour channels are blurred alike, so a
/// single-colour pixmap (a shadow) stays correct.
pub fn blur(pixmap: &mut Pixmap, sigma: f32) {
    if sigma < 0.3 {
        return;
    }
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let boxes = box_sizes(sigma, 3);
    let data = pixmap.data_mut();
    let mut tmp = vec![0u8; data.len()];
    for r in boxes {
        box_blur_h(data, &mut tmp, w, h, r);
        box_blur_v(&tmp, data, w, h, r);
    }
}

/// Radii of `n` box filters whose combination approximates a gaussian of
/// standard deviation `sigma` (Ivan Kutskir's derivation).
fn box_sizes(sigma: f32, n: usize) -> Vec<usize> {
    let n_f = n as f32;
    let w_ideal = (12.0 * sigma * sigma / n_f + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal =
        (12.0 * sigma * sigma - n_f * (wl * wl) as f32 - 4.0 * n_f * wl as f32 - 3.0 * n_f) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    (0..n as i32).map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize).collect()
}

fn box_blur_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let norm = 1.0 / (2 * r + 1) as f32;
    for y in 0..h {
        let row = y * w * 4;
        for c in 0..4 {
            let at = |x: isize| -> u32 {
                let x = x.clamp(-1, w as isize);
                if x < 0 || x >= w as isize { 0 } else { src[row + x as usize * 4 + c] as u32 }
            };
            let mut acc: u32 = 0;
            for x in -(r as isize)..=(r as isize) {
                acc += at(x);
            }
            for x in 0..w {
                dst[row + x * 4 + c] = (acc as f32 * norm).round().min(255.0) as u8;
                acc += at(x as isize + r as isize + 1);
                acc -= at(x as isize - r as isize);
            }
        }
    }
}

fn box_blur_v(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let norm = 1.0 / (2 * r + 1) as f32;
    for x in 0..w {
        for c in 0..4 {
            let at = |y: isize| -> u32 {
                if y < 0 || y >= h as isize { 0 } else { src[(y as usize * w + x) * 4 + c] as u32 }
            };
            let mut acc: u32 = 0;
            for y in -(r as isize)..=(r as isize) {
                acc += at(y);
            }
            for y in 0..h {
                dst[(y * w + x) * 4 + c] = (acc as f32 * norm).round().min(255.0) as u8;
                acc += at(y as isize + r as isize + 1);
                acc -= at(y as isize - r as isize);
            }
        }
    }
}

/// Premultiplied RGBA (tiny-skia) to the straight-alpha BGRA gpui uploads.
pub fn to_bgra_straight(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let c = px.demultiply();
        out.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
    }
    out
}

/// Premultiplied RGBA to straight RGBA, for PNG encoding.
pub fn to_rgba_straight(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let c = px.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

/// Straight RGBA into a premultiplied pixmap.
pub fn pixmap_from_rgba(w: u32, h: u32, rgba: &[u8]) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w, h)?;
    for (dst, src) in pm.pixels_mut().iter_mut().zip(rgba.chunks_exact(4)) {
        *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
    }
    Some(pm)
}

/// The integer pixel rect covering `[x0, x1) × [y0, y1)`, clipped to `w × h`.
pub fn clip_rect(x0: f32, y0: f32, x1: f32, y1: f32, w: u32, h: u32) -> Option<IntRect> {
    let l = x0.floor().max(0.0) as i32;
    let t = y0.floor().max(0.0) as i32;
    let r = (x1.ceil() as i32).min(w as i32);
    let b = (y1.ceil() as i32).min(h as i32);
    IntRect::from_ltrb(l, t, r, b)
}

/// Encodes straight RGBA as PNG.
pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        // Fast compression: a 4K export encodes in a fraction of the time and
        // is still far smaller than raw.
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_spreads_and_conserves_roughly() {
        let mut pm = Pixmap::new(41, 41).unwrap();
        pm.fill_rect(
            tiny_skia::Rect::from_xywh(15.0, 15.0, 11.0, 11.0).unwrap(),
            &tiny_skia::Paint { shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::BLACK), ..Default::default() },
            tiny_skia::Transform::identity(),
            None,
        );
        let before: u32 = pm.pixels().iter().map(|p| p.alpha() as u32).sum();
        blur(&mut pm, 3.0);
        let after: u32 = pm.pixels().iter().map(|p| p.alpha() as u32).sum();
        assert!(pm.pixel(10, 20).unwrap().alpha() > 0);
        assert!(pm.pixel(20, 20).unwrap().alpha() < 255);
        assert!((after as f32 / before as f32 - 1.0).abs() < 0.05);
    }
}
