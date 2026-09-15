// The redact tool scrambles part of the screenshot instead of covering it up.
// Every block in the region is repainted from the pixels underneath, so what
// was written there is gone from the exported PNG rather than hidden behind an
// overlay someone could peel off.
//
// A block takes the average colour of what it covered, pushed off that average
// by as much as the block's own contrast. Flat background comes out flat; a
// block that held a couple of characters comes out somewhere else entirely, so
// the region reads as deliberately scrambled rather than as a smudge. It is
// also what makes the scramble hard to undo: blocks are coarse — 12 units, so
// wider than a character at any screenshot resolution — and the exact block
// averages that de-pixelation attacks match candidate text against are pushed
// furthest exactly where there was something to read. The push is derived from
// the block's own position rather than from a random number, so every render of
// the same screenshot is identical and undo, redo and export all agree.

const BLOCK_UNITS = 12;          // block size, in the app's 900px-render units
const MIN_BLOCK = 5;             // ...but never finer than this, in image pixels
const SCRAMBLE = 1.2;            // push off the average, as a multiple of the block's contrast
const BASE_JITTER = 5;           // ...and this much regardless, so flat blocks move too
const STRIP_PIXELS = 256;        // rows of the shot to read back at a time, give or take
const UNREADABLE_FILL = '#101014';

/* ==========================================================================
   The plate — block averages for the whole screenshot, one pixel per block
   ========================================================================== */

let source = null;
let sourceUnit = 1;
let plate = null;   // { canvas, cols, rows, block }, false if unreadable, null if unbuilt

/** Points the tool at the screenshot being annotated. Called whenever the
 *  image changes — a new shot, or a crop applied to this one. */
function setRedactSource(image, unit = 1) {
  source = image || null;
  sourceUnit = unit > 0 ? unit : 1;
  plate = null;
}

/** Builds the plate up front, so the first drag doesn't pay for it. */
function prepareRedact() {
  buildPlate();
}

function byte(value) {
  return Math.max(0, Math.min(255, Math.round(value)));
}

/** A stable number in [-1, 1] for one block. Hashing the block's coordinates
 *  instead of drawing a random number is what keeps two renders of the same
 *  screenshot pixel-identical. */
function noise(col, row, channel) {
  let h = Math.imul(col + 1, 73856093) ^ Math.imul(row + 1, 19349663) ^ Math.imul(channel + 1, 83492791);
  h = Math.imul(h ^ (h >>> 15), 2246822519);
  h = Math.imul(h ^ (h >>> 13), 3266489917);
  h = (h ^ (h >>> 16)) >>> 0;
  return (h / 0xffffffff) * 2 - 1;
}

function buildPlate() {
  if (plate !== null) return plate;
  plate = false;
  if (!source) return plate;

  const w = source.naturalWidth;
  const h = source.naturalHeight;
  if (!w || !h) return plate;

  const block = Math.max(MIN_BLOCK, Math.round(BLOCK_UNITS * sourceUnit));
  const cols = Math.ceil(w / block);
  const rows = Math.ceil(h / block);

  const out = document.createElement('canvas');
  out.width = cols;
  out.height = rows;
  const outCtx = out.getContext('2d');
  const blocks = outCtx.createImageData(cols, rows);

  // The shot is read back a band of blocks at a time rather than in one go:
  // a 4K screenshot reads back an order of magnitude quicker that way, and
  // never holds more than a few megabytes of pixels at once.
  const band = Math.max(1, Math.round(STRIP_PIXELS / block));
  const strip = document.createElement('canvas');
  strip.width = w;
  strip.height = Math.min(h, band * block);
  const stripCtx = strip.getContext('2d', { willReadFrequently: true });

  for (let first = 0; first < rows; first += band) {
    const top = first * block;
    const stripH = Math.min(strip.height, h - top);
    const last = Math.min(rows, first + band);

    let pixels;
    try {
      // Cleared first: the shot may have transparency, which would otherwise
      // be drawn over the band before it.
      stripCtx.clearRect(0, 0, w, stripH);
      stripCtx.drawImage(source, 0, top, w, stripH, 0, 0, w, stripH);
      pixels = stripCtx.getImageData(0, 0, w, stripH).data;
    } catch (err) {
      // Reading the shot back can fail — a cross-origin image taints the
      // canvas. Leave the plate unbuilt and a redact box paints solid instead,
      // which hides as much as the blocks do and never less.
      console.warn('Could not read the screenshot back to scramble it', err);
      return plate;
    }

    for (let row = first; row < last; row++) {
      const y0 = (row - first) * block;
      const y1 = Math.min(stripH, y0 + block);
      for (let col = 0; col < cols; col++) {
        const x0 = col * block;
        const x1 = Math.min(w, x0 + block);
        let r = 0;
        let g = 0;
        let b = 0;
        let a = 0;
        let l = 0;
        let l2 = 0;
        for (let y = y0; y < y1; y++) {
          let i = (y * w + x0) * 4;
          for (let x = x0; x < x1; x++, i += 4) {
            const alpha = pixels[i + 3];
            r += pixels[i] * alpha;
            g += pixels[i + 1] * alpha;
            b += pixels[i + 2] * alpha;
            const lum = 0.299 * pixels[i] + 0.587 * pixels[i + 1] + 0.114 * pixels[i + 2];
            l += lum * alpha;
            l2 += lum * lum * alpha;
            a += alpha;
          }
        }

        // Everything is averaged weighted by alpha, so a transparent
        // screenshot scrambles to transparent blocks rather than black ones.
        const o = (row * cols + col) * 4;
        blocks.data[o + 3] = byte(a / ((x1 - x0) * (y1 - y0)));
        if (!a) continue;

        // How much the block's own pixels varied, and so how far it may travel.
        const mean = l / a;
        const contrast = Math.sqrt(Math.max(0, l2 / a - mean * mean));
        const push = noise(col, row, 0) * SCRAMBLE * contrast;
        blocks.data[o] = byte(r / a + push + noise(col, row, 1) * BASE_JITTER);
        blocks.data[o + 1] = byte(g / a + push + noise(col, row, 2) * BASE_JITTER);
        blocks.data[o + 2] = byte(b / a + push + noise(col, row, 3) * BASE_JITTER);
      }
    }
  }
  outCtx.putImageData(blocks, 0, 0);

  plate = { canvas: out, cols, rows, block };
  return plate;
}

/** Paints the blocks over `box`, given in screenshot pixels, inside the clip
 *  the caller has set up. Whole blocks go down and the clip trims them, which
 *  keeps the grid anchored to the screenshot: a box can be moved or resized
 *  without the blocks under it churning, and two boxes side by side line up. */
function paintBlocks(ctx, box) {
  if (!source) return;

  // Annotations are free to run into the margin around the shot, but a
  // scramble is not: there is nothing out there to hide, and the last block in
  // a row or column can reach past the edge.
  ctx.beginPath();
  ctx.rect(0, 0, source.naturalWidth, source.naturalHeight);
  ctx.clip();

  const p = buildPlate();
  if (!p) {
    ctx.fillStyle = UNREADABLE_FILL;
    ctx.fillRect(box.left, box.top, box.width, box.height);
    return;
  }

  const b = p.block;
  const col0 = Math.max(0, Math.floor(box.left / b));
  const row0 = Math.max(0, Math.floor(box.top / b));
  const col1 = Math.min(p.cols, Math.ceil((box.left + box.width) / b));
  const row1 = Math.min(p.rows, Math.ceil((box.top + box.height) / b));
  // Nothing of the shot under this box — it hangs in the margin, where there
  // is nothing to hide.
  if (col1 <= col0 || row1 <= row0) return;

  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(
    p.canvas,
    col0, row0, col1 - col0, row1 - row0,
    col0 * b, row0 * b, (col1 - col0) * b, (row1 - row0) * b
  );
}

/* ==========================================================================
   The object
   ========================================================================== */

const Redact = fabric.util.createClass(fabric.Object, {
  type: 'redact',

  initialize: function (options) {
    this.callSuper('initialize', Object.assign({
      originX: 'left',
      originY: 'top',
      escTool: 'redact',
      strokeWidth: 0,
      hasBorders: false,
      // Blocks line up with the screenshot's own grid, so a rotated box would
      // scramble one region and paint the result over another.
      lockRotation: true
    }, options));
    this.setControlsVisibility({ mt: false, mb: false, ml: false, mr: false, mtr: false });
  },

  /** fabric has already put the object's transform on the context, so local
   *  (0,0) is the centre of the box and one local unit is one unit of an
   *  unscaled box. The blocks have to be square in screenshot pixels instead,
   *  so this clips to the box in the object's own space — that is its exact
   *  footprint, whatever it has been scaled to — and then steps back into
   *  screenshot space to paint. */
  _render: function (ctx) {
    if (this.width < 1 || this.height < 1) return;

    const m = this.calcTransformMatrix();
    const halfW = this.width / 2;
    const halfH = this.height / 2;
    const corners = [
      { x: -halfW, y: -halfH },
      { x: halfW, y: -halfH },
      { x: halfW, y: halfH },
      { x: -halfW, y: halfH }
    ].map((p) => fabric.util.transformPoint(new fabric.Point(p.x, p.y), m));
    const box = fabric.util.makeBoundingBoxFromPoints(corners);

    ctx.save();
    ctx.beginPath();
    ctx.rect(-halfW, -halfH, this.width, this.height);
    ctx.clip();
    const inv = fabric.util.invertTransform(m);
    ctx.transform(inv[0], inv[1], inv[2], inv[3], inv[4], inv[5]);
    paintBlocks(ctx, box);
    ctx.restore();
  }
});

/** Only geometry is stored — the blocks are read off the screenshot when the
 *  object is drawn — so a box rebuilt by undo, redo or duplicate scrambles
 *  whatever it now sits over, and the JSON stays small. */
Redact.fromObject = function (object, callback) {
  callback(new Redact(object));
};

// fabric finds the class by the object's `type` when it rebuilds the canvas
// from JSON, which is how undo, redo and duplicate get their boxes back.
fabric.Redact = Redact;

function createRedact(left, top) {
  return new Redact({ left, top, width: 0, height: 0 });
}

export { createRedact, setRedactSource, prepareRedact };
