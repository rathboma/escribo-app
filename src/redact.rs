//! The redact tool scrambles part of the screenshot instead of covering it up.
//! Every block in the region is repainted from the pixels underneath, so what
//! was written there is gone from the exported PNG rather than hidden behind
//! an overlay someone could peel off.
//!
//! A block takes the average colour of what it covered, pushed off that
//! average by as much as the block's own contrast. Flat background comes out
//! flat; a block that held a couple of characters comes out somewhere else
//! entirely, so the region reads as deliberately scrambled rather than as a
//! smudge. It is also what makes the scramble hard to undo: blocks are coarse
//! — 12 units, so wider than a character at any screenshot resolution — and
//! the exact block averages that de-pixelation attacks match candidate text
//! against are pushed furthest exactly where there was something to read. The
//! push is derived from the block's own position rather than from a random
//! number, so every render of the same screenshot is identical and undo, redo
//! and export all agree.

/// block size, in the app's 900px-render units
const BLOCK_UNITS: f32 = 12.0;
/// ...but never finer than this, in image pixels
const MIN_BLOCK: u32 = 5;
/// push off the average, as a multiple of the block's contrast
const SCRAMBLE: f32 = 1.2;
/// ...and this much regardless, so flat blocks move too
const BASE_JITTER: f32 = 5.0;

/// Block averages for the whole screenshot, one straight-alpha RGBA pixel per
/// block.
pub struct Plate {
    pub cols: u32,
    pub rows: u32,
    /// block size in image pixels
    pub block: u32,
    pub rgba: Vec<u8>,
}

/// A stable number in [-1, 1] for one block. Hashing the block's coordinates
/// instead of drawing a random number is what keeps two renders of the same
/// screenshot pixel-identical.
fn noise(col: u32, row: u32, channel: u32) -> f32 {
    let mut h = (col.wrapping_add(1).wrapping_mul(73_856_093))
        ^ (row.wrapping_add(1).wrapping_mul(19_349_663))
        ^ (channel.wrapping_add(1).wrapping_mul(83_492_791));
    h = (h ^ (h >> 15)).wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(3_266_489_917);
    h ^= h >> 16;
    (h as f64 / 0xffff_ffffu32 as f64 * 2.0 - 1.0) as f32
}

fn byte(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

impl Plate {
    pub fn build(rgba: &[u8], w: u32, h: u32, unit: f32) -> Option<Plate> {
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return None;
        }
        let block = MIN_BLOCK.max((BLOCK_UNITS * unit).round() as u32);
        let cols = w.div_ceil(block);
        let rows = h.div_ceil(block);
        let mut out = vec![0u8; (cols * rows * 4) as usize];

        for row in 0..rows {
            let y0 = row * block;
            let y1 = (y0 + block).min(h);
            for col in 0..cols {
                let x0 = col * block;
                let x1 = (x0 + block).min(w);
                let (mut r, mut g, mut b, mut a, mut l, mut l2) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
                for y in y0..y1 {
                    let mut i = ((y * w + x0) * 4) as usize;
                    for _ in x0..x1 {
                        let alpha = rgba[i + 3] as f64;
                        let (pr, pg, pb) = (rgba[i] as f64, rgba[i + 1] as f64, rgba[i + 2] as f64);
                        r += pr * alpha;
                        g += pg * alpha;
                        b += pb * alpha;
                        let lum = 0.299 * pr + 0.587 * pg + 0.114 * pb;
                        l += lum * alpha;
                        l2 += lum * lum * alpha;
                        a += alpha;
                        i += 4;
                    }
                }

                // Everything is averaged weighted by alpha, so a transparent
                // screenshot scrambles to transparent blocks, not black ones.
                let o = ((row * cols + col) * 4) as usize;
                out[o + 3] = byte((a / ((x1 - x0) * (y1 - y0)) as f64) as f32);
                if a == 0.0 {
                    continue;
                }
                // How much the block's own pixels varied, and so how far it may travel.
                let mean = l / a;
                let contrast = (l2 / a - mean * mean).max(0.0).sqrt() as f32;
                let push = noise(col, row, 0) * SCRAMBLE * contrast;
                out[o] = byte((r / a) as f32 + push + noise(col, row, 1) * BASE_JITTER);
                out[o + 1] = byte((g / a) as f32 + push + noise(col, row, 2) * BASE_JITTER);
                out[o + 2] = byte((b / a) as f32 + push + noise(col, row, 3) * BASE_JITTER);
            }
        }
        Some(Plate { cols, rows, block, rgba: out })
    }

    /// The blocks a box (image pixels) covers, clamped to the shot:
    /// `(col0, row0, col1, row1)`, exclusive at the end. None when the box
    /// hangs entirely in the margin, where there is nothing to hide.
    pub fn span(&self, x: f32, y: f32, w: f32, h: f32) -> Option<(u32, u32, u32, u32)> {
        let b = self.block as f32;
        let col0 = (x / b).floor().max(0.0) as u32;
        let row0 = (y / b).floor().max(0.0) as u32;
        let col1 = ((x + w) / b).ceil().clamp(0.0, self.cols as f32) as u32;
        let row1 = ((y + h) / b).ceil().clamp(0.0, self.rows as f32) as u32;
        (col1 > col0 && row1 > row0).then_some((col0, row0, col1, row1))
    }

    pub fn pixel(&self, col: u32, row: u32) -> [u8; 4] {
        let o = ((row * self.cols + col) * 4) as usize;
        [self.rgba[o], self.rgba[o + 1], self.rgba[o + 2], self.rgba[o + 3]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_matches_the_javascript_hash() {
        // Values produced by the old redact.js noise() for the same inputs, so
        // a shot scrambles exactly as it did in the Electron app.
        for (args, want) in
            [((0, 0, 0), 0.503371878877136), ((3, 7, 2), 0.20248907646222247), ((120, 45, 3), 0.8761379634673097)]
        {
            let got = noise(args.0, args.1, args.2);
            assert!((got as f64 - want).abs() < 1e-6, "{args:?}: {got} != {want}");
        }
    }

    #[test]
    fn flat_image_stays_nearly_flat_and_transparent_stays_transparent() {
        let (w, h) = (40u32, 20u32);
        let mut px = Vec::new();
        for _ in 0..w * h {
            px.extend_from_slice(&[100, 150, 200, 255]);
        }
        let p = Plate::build(&px, w, h, 1.0).unwrap();
        assert_eq!(p.block, 12);
        assert_eq!((p.cols, p.rows), (4, 2));
        for c in 0..p.cols {
            let [r, g, b, a] = p.pixel(c, 0);
            assert_eq!(a, 255);
            assert!((r as i32 - 100).abs() <= 5 && (g as i32 - 150).abs() <= 5 && (b as i32 - 200).abs() <= 5);
        }
        let clear = vec![0u8; (w * h * 4) as usize];
        let p = Plate::build(&clear, w, h, 1.0).unwrap();
        assert_eq!(p.pixel(1, 1)[3], 0);
    }

    #[test]
    fn span_clamps_to_the_shot() {
        let p = Plate { cols: 10, rows: 10, block: 10, rgba: vec![0; 400] };
        assert_eq!(p.span(15.0, 15.0, 10.0, 10.0), Some((1, 1, 3, 3)));
        assert_eq!(p.span(-50.0, -50.0, 20.0, 20.0), None);
        assert_eq!(p.span(95.0, 95.0, 50.0, 50.0), Some((9, 9, 10, 10)));
    }
}
