import { createArrow, attachArrowControls, setArrowHeadPoint, syncArrowGeometry } from './arrow.js';
import { createRedact, setRedactSource, prepareRedact } from './redact.js';

/* ==========================================================================
   Constants
   ========================================================================== */

const WATERMARK_TEXT = 'made with escriboapp.com';
const BRAND = '#FF007F';
const STORE_KEY = 'escribo.settings';
const AREA_PAD = 22;          // #area padding, keep in sync with styles.css
const ZOOM_STEP = 1.25;
const MAX_ZOOM_STEPS = 8;

const SOLID_NAMES = ['Ink', 'Sand', 'Slate', 'Blush'];
const GRAD_NAMES = ['Dusk', 'Rose', 'Sky', 'Mint'];

// A profile is the global kit — annotation palette, padding fills, label
// font — and owns the presets built on top of it. One profile is active at a
// time; its fields are read through `settings` (see defineProfileAccessors).
const PROFILE_DEFAULTS = {
  palette: ['#FF007F', '#FF3B30', '#FFB300', '#12B76A', '#2E7DFF', '#15151A'],
  defaultIdx: 0,
  solidBg: ['#101014', '#EFE7DC', '#3A4354', '#F7D6E4'],
  gradBg: [
    { a: '#2B3242', b: '#12151D' },
    { a: '#FFD9E8', b: '#FF9CC6' },
    { a: '#C6E2FF', b: '#7FAEFF' },
    { a: '#D9F5E8', b: '#8AD6B8' }
  ],
  font: 'system'
};
const PROFILE_FIELDS = ['palette', 'defaultIdx', 'solidBg', 'gradBg', 'font', 'presets'];

// A preset is a screenshot style: exactly these fields of the current state.
const STYLE_FIELDS = ['ratio', 'outScale', 'outW', 'pad', 'backdrop', 'watermark'];

const DEFAULTS = {
  theme: 'dark',
  // the current style
  watermark: true,
  backdrop: 0,       // fill: 0 none, 1-4 solid, 5-8 gradient (the profile's fills)
  pad: 0,            // margin, % of the output width
  ratio: 'orig',     // output aspect ratio
  outScale: 1,       // export scale when no explicit width is set
  outW: null,        // explicit export width, or null to follow outScale
  // profiles, and what is active
  profiles: [],
  activeProfile: null,
  activePreset: null
};

const EMOJIS = ['🔥', '👍', '👎', '🎉', '⚠️', '✅', '❌', '💡',
                '🤔', '😍', '😭', '💀', '🚀', '⭐', '🙈', '🫠'];

const HINTS = {
  select: 'Click an annotation to move or resize it',
  arrow: 'Drag to draw an arrow',
  rect: 'Drag to draw a box',
  mark: 'Drag to highlight a region',
  redact: 'Drag over anything that should not be readable',
  text: 'Click anywhere to add a label',
  emoji: 'Pick an emoji, then click to place it',
  crop: 'Drag the handles, then apply'
};

const KEYMAP = { s: 'select', a: 'arrow', b: 'rect', m: 'mark', r: 'redact', t: 'text', e: 'emoji', c: 'crop' };

const RATIOS = [
  { id: 'orig', label: 'Orig', v: null, hint: 'Match the screenshot' },
  { id: '16:9', label: '16:9', v: 16 / 9, hint: 'Slides, video, Twitter' },
  { id: '4:3', label: '4:3', v: 4 / 3, hint: 'Docs and older displays' },
  { id: '1:1', label: '1:1', v: 1, hint: 'Square — social posts' },
  { id: '9:16', label: '9:16', v: 9 / 16, hint: 'Portrait — stories' }
];
const SCALES = [
  { label: '½×', k: 0.5 },
  { label: '1×', k: 1 },
  { label: '2×', k: 2 }
];
const NONE_CHIP = 'linear-gradient(135deg,var(--sunken) 44%,var(--muted) 44%,var(--muted) 56%,var(--sunken) 56%)';
const UI_FONT = '-apple-system, "Segoe UI", Cantarell, Ubuntu, system-ui, sans-serif';

// Label fonts. Only families that exist offline everywhere, plus the bundled
// brand script; anything else goes in as a custom family string.
const FONTS = [
  { id: 'system', label: 'System', family: UI_FONT },
  { id: 'serif', label: 'Serif', family: 'Georgia, "Times New Roman", serif' },
  { id: 'mono', label: 'Mono', family: 'Menlo, Consolas, "DejaVu Sans Mono", monospace' },
  { id: 'script', label: 'Script', family: 'Cookie, cursive' }
];

/* ==========================================================================
   Settings (persisted)
   ========================================================================== */

// Settings that were saved to localStorage before the switch to
// electron-store. Read once so nothing is lost when a user upgrades; after
// the first save everything lives in electron-store's JSON file instead.
function migrateLegacySettings() {
  try {
    const raw = JSON.parse(localStorage.getItem(STORE_KEY) || 'null');
    if (raw) return raw;
    // Carry over the standalone watermark flag used before the redesign.
    const legacy = localStorage.getItem('watermark');
    if (legacy !== null) return { watermark: legacy === 'true' };
  } catch (err) {
    console.warn('Could not read legacy settings', err);
  }
  return null;
}

function loadSettings() {
  const s = { ...DEFAULTS };
  try {
    const stored = window.electronAPI.getSettings();
    const raw = stored && Object.keys(stored).length ? stored : migrateLegacySettings();
    if (raw) Object.assign(s, raw);
  } catch (err) {
    console.warn('Could not read saved settings', err);
  }

  // Profiles. Settings saved before profiles existed kept the palette,
  // fills, font and presets at the top level — fold those into one profile.
  const profiles = Array.isArray(s.profiles) ? s.profiles.map(sanitizeProfile).filter(Boolean) : [];
  if (!profiles.length) profiles.push(newProfile('Default', s));
  s.profiles = profiles;
  if (!profiles.some((p) => p.id === s.activeProfile)) s.activeProfile = profiles[0].id;
  PROFILE_FIELDS.forEach((key) => delete s[key]);
  defineProfileAccessors(s);

  // The current style, checked the way an imported preset is.
  const style = sanitizeValues(s);
  STYLE_FIELDS.forEach((key) => { s[key] = key in style ? style[key] : DEFAULTS[key]; });
  return s;
}

/** The active profile's fields read and write through `settings`, so the
 *  rest of the app keeps its flat view of them. They are not enumerable:
 *  the data is stored inside `profiles`, not again at the top level. */
function defineProfileAccessors(s) {
  const current = () => s.profiles.find((p) => p.id === s.activeProfile) || s.profiles[0];
  PROFILE_FIELDS.forEach((key) => {
    Object.defineProperty(s, key, {
      get: () => current()[key],
      set: (value) => { current()[key] = value; },
      enumerable: false
    });
  });
}

const settings = loadSettings();

function saveSettings() {
  try {
    // Serialize to a plain object first: the profile fields are non-enumerable
    // accessors, and this matches what JSON.stringify persisted before.
    window.electronAPI.saveSettings(JSON.parse(JSON.stringify(settings)));
  } catch (err) {
    console.warn('Could not save settings', err);
  }
  syncPresetsUI();
}

/* ==========================================================================
   Session state
   ========================================================================== */

const ui = {
  tool: 'arrow',
  colorIdx: settings.defaultIdx,
  zoom: 0,
  prefsTab: 'annotate',
  pendingEmoji: null,
  editingText: false
};

let img = null;        // HTMLImageElement holding the full-resolution source
let imgW = 0;
let imgH = 0;
let crop = null;       // { x, y, w, h } in image pixels while cropping

const $ = (sel) => document.querySelector(sel);
const el = {
  area: $('#area'),
  stage: $('#stage'),
  backdrop: $('#backdrop'),
  shot: $('#shot'),
  shotImg: $('#shot-img'),
  watermark: $('#watermark'),
  empty: $('#empty'),
  hint: $('#hint'),
  titleName: $('#title-name'),
  titleDims: $('#title-dims'),
  palette: $('#palette'),
  emojiPicker: $('#emoji-picker'),
  cropLayer: $('#crop-layer'),
  cropBox: $('#crop-box'),
  cropBar: $('#crop-bar'),
  cropSize: $('#crop-size'),
  zoomFit: $('#zoom-fit'),
  toast: $('#toast'),
  prefs: $('#prefs'),
  backdropPop: $('#backdrop-pop'),
  backdropBtn: $('#backdrop-btn'),
  backdropChip: $('#backdrop-chip'),
  undo: $('#undo'),
  redo: $('#redo'),
  ratioBtn: $('#ratio-btn'),
  ratioPop: $('#ratio-pop'),
  ratioLabel: $('#ratio-label'),
  ratioNote: $('#ratio-note'),
  ratioSeg: $('#ratio-seg'),
  scaleSeg: $('#scale-seg'),
  outW: $('#out-w'),
  outH: $('#out-h'),
  copyCaret: $('#copy-caret'),
  copyMenu: $('#copy-menu'),
  presetsBtn: $('#presets-btn'),
  presetsPop: $('#presets-pop'),
  presetsList: $('#presets-list'),
  presetsLabel: $('#presets-label'),
  presetForm: $('#preset-form'),
  presetName: $('#preset-name')
};

const isDesktop = !!window.electronAPI;
document.body.classList.toggle('is-desktop', isDesktop);

// Window chrome follows the OS; on the web there is no chrome, so the layout
// falls back to the plain one and only the modifier-key labels adapt.
const platform = (isDesktop && window.electronAPI.platform) || 'linux';
document.documentElement.dataset.platform = platform;
const isMac = platform === 'darwin' || (!isDesktop && /Mac/.test(navigator.platform));
const MOD = isMac ? '⌘' : 'Ctrl+';
const MOD_MENU = isMac ? '⌘' : 'Ctrl ';

/* ==========================================================================
   Canvas
   ========================================================================== */

const canvas = new fabric.Canvas('canvas', {
  selection: true,
  preserveObjectStacking: true,
  uniformScaling: false,
  width: 0,
  height: 0
});

canvas.extraProps = ['selectable', 'editable', 'escTool', 'globalCompositeOperation', 'arrowUnit'];

// Undo rebuilds objects from JSON, so what is drawn must not depend on state
// the JSON cannot carry. The per-object raster cache is one such thing (the
// arrow tool edits points without ever invalidating it), and coordinates
// rounded to two decimals are another — enough to nudge a rotated, ×7-scaled
// arrow by a visible pixel. Direct rendering is cheap at this object count,
// and together these make every undo round trip pixel-exact.
fabric.Object.prototype.objectCaching = false;
fabric.Object.NUM_FRACTION_DIGITS = 6;

fabric.Rect.prototype._controlsVisibility = { mt: false, mb: false, ml: false, mr: false };
fabric.Polygon.prototype._controlsVisibility = {
  tl: false, tr: false, bl: false, br: false, mt: false, mb: false, ml: true, mr: false
};
fabric.Rect.prototype.rx = 2;
fabric.Rect.prototype.ry = 2;

/** One "unit" is a pixel as it would look on a 900px-wide render, so annotation
 *  weights stay proportional whatever the screenshot's resolution is. */
const unit = () => Math.max(1, imgW / 900);
const color = () => settings.palette[ui.colorIdx] || settings.palette[0];
const backdropOn = () => settings.backdrop !== 0;
const hasImage = () => !!img;

/* ==========================================================================
   Layout — the output box (ratio + padding) with the shot fitted inside
   ========================================================================== */

function backdropCss(index = settings.backdrop) {
  if (index === 0) return 'transparent';
  if (index <= 4) return settings.solidBg[index - 1];
  const g = settings.gradBg[index - 5];
  return `linear-gradient(135deg,${g.a} 0%,${g.b} 100%)`;
}

/** Padding is a percentage of the output width on all four sides. */
function padFraction() {
  return settings.pad / 100;
}

function currentRatio() {
  return RATIOS.find((r) => r.id === settings.ratio) || RATIOS[0];
}

/**
 * Geometry of the composed output at 1×, in image pixels: the box, its
 * padding, and where the shot sits in it. "Orig" wraps the screenshot in a
 * uniform margin and adds no canvas; a fixed ratio fits the screenshot inside
 * the padded box and leaves the rest as extra canvas.
 */
function naturalLayout() {
  const f = padFraction();
  const R = currentRatio();

  if (!R.v) {
    const outW = imgW / (1 - 2 * f);
    const pad = f * outW;
    return { outW, outH: imgH + 2 * pad, pad, shotX: pad, shotY: pad };
  }

  // Ratio of the box left once padding is taken off each side.
  const availRatio = (1 - 2 * f) / (1 / R.v - 2 * f);
  const heightBound = availRatio >= imgW / imgH;
  const outW = heightBound ? imgH / (1 / R.v - 2 * f) : imgW / (1 - 2 * f);
  const outH = outW / R.v;
  const pad = f * outW;
  return {
    outW,
    outH,
    pad,
    shotX: pad + (outW - 2 * pad - imgW) / 2,
    shotY: pad + (outH - 2 * pad - imgH) / 2
  };
}

function fitScale(layout = naturalLayout()) {
  const availW = Math.max(1, el.area.clientWidth - AREA_PAD * 2);
  const availH = Math.max(1, el.area.clientHeight - AREA_PAD * 2);
  return Math.min(availW / layout.outW, availH / layout.outH, 1);
}

function currentScale(layout = naturalLayout()) {
  return fitScale(layout) * Math.pow(ZOOM_STEP, ui.zoom);
}

function relayout() {
  if (!hasImage()) return;

  const L = naturalLayout();
  const k = currentScale(L);
  const stageW = Math.max(1, Math.round(L.outW * k));
  const stageH = Math.max(1, Math.round(L.outH * k));
  const shotW = Math.max(1, Math.round(imgW * k));
  const shotH = Math.max(1, Math.round(imgH * k));
  const shotX = Math.round(L.shotX * k);
  const shotY = Math.round(L.shotY * k);

  // The canvas spans the whole output so annotations can run into the margin.
  // Image space is offset to the shot, so they stay anchored to the screenshot
  // however the margin or ratio changes.
  canvas.setDimensions({ width: stageW, height: stageH });
  canvas.setViewportTransform([k, 0, 0, k, shotX, shotY]);

  el.stage.style.width = stageW + 'px';
  el.stage.style.height = stageH + 'px';
  el.backdrop.style.background = backdropCss();

  el.shot.style.left = shotX + 'px';
  el.shot.style.top = shotY + 'px';
  el.shot.style.width = shotW + 'px';
  el.shot.style.height = shotH + 'px';
  el.shot.style.borderRadius = settings.pad > 0 ? shotW * 0.012 + 'px' : '0px';
  el.shot.style.boxShadow = backdropOn() && settings.pad > 0 ? '0 10px 34px rgba(0,0,0,.32)' : 'none';

  // Watermark: bottom-right of the whole output, sized and inset off its width.
  el.watermark.style.setProperty('--wm-size', stageW * 0.0125 + 'px');
  el.watermark.style.setProperty('--wm-inset', stageW * 0.013 + 'px');

  // The crop layer lives in #stage, so give it the shot's exact box.
  el.cropLayer.style.left = shotX + 'px';
  el.cropLayer.style.top = shotY + 'px';
  el.cropLayer.style.width = shotW + 'px';
  el.cropLayer.style.height = shotH + 'px';

  el.zoomFit.textContent = ui.zoom === 0 ? 'Fit' : Math.round(k * 100) + '%';
  if (crop) renderCropBox();
  syncRatioPop(L);
  canvas.requestRenderAll();
}

// Relayout on the next frame: it resizes #stage, which is inside the observed
// area, and doing that during delivery makes Chromium complain about a loop.
let relayoutFrame = 0;
new ResizeObserver(() => {
  cancelAnimationFrame(relayoutFrame);
  relayoutFrame = requestAnimationFrame(relayout);
}).observe(el.area);

/* ==========================================================================
   Loading images
   ========================================================================== */

function setSource(image, name) {
  img = image;
  imgW = image.naturalWidth;
  imgH = image.naturalHeight;
  setRedactSource(image, unit());

  canvas.clear();
  el.shotImg.src = image.src;
  el.stage.classList.add('ready');
  el.empty.classList.add('hidden');

  el.titleName.textContent = name || 'Untitled';
  el.titleDims.textContent = `${imgW} × ${imgH}`;
  document.title = `${name || 'Untitled'} — escribo`;

  ui.zoom = 0;
  relayout();
  updateHint();
  canvas.clearHistory();
  resetHistoryBaseline();
  updateHistoryButtons();
}

function loadDataURL(dataUrl, name) {
  const image = new Image();
  image.onload = () => setSource(image, name);
  image.onerror = () => toast('Could not read that image');
  image.src = dataUrl;
}

function loadBlob(blob, name) {
  const reader = new FileReader();
  reader.onload = (e) => loadDataURL(e.target.result, name);
  reader.readAsDataURL(blob);
}

function loadFromQueryParam() {
  const filePath = new URLSearchParams(window.location.search).get('file');
  if (!filePath || !window.electronAPI) return;

  window.electronAPI.getFileContent(filePath).then((response) => {
    if (!response || !response.success) {
      console.error('Error loading file:', response && response.error);
      return;
    }
    const name = filePath === 'app:clipboard'
      ? 'Clipboard image'
      : filePath.split(/[/\\]/).pop();
    loadDataURL(`data:${response.mimeType};base64,${response.content}`, name);
  });
}

/* ==========================================================================
   Tools
   ========================================================================== */

function setTool(tool) {
  if (ui.tool === 'crop' && tool !== 'crop') exitCrop();
  ui.tool = tool;
  if (tool !== 'emoji') ui.pendingEmoji = null;

  document.querySelectorAll('.tool-btn').forEach((btn) => {
    btn.setAttribute('aria-pressed', String(btn.dataset.tool === tool));
  });

  const selecting = tool === 'select';
  canvas.selection = selecting;
  canvas.forEachObject((o) => { o.selectable = selecting; o.evented = selecting; });
  if (!selecting && !ui.editingText) canvas.discardActiveObject();
  canvas.defaultCursor = selecting ? 'default' : 'crosshair';
  canvas.hoverCursor = selecting ? 'move' : 'crosshair';

  el.emojiPicker.hidden = tool !== 'emoji';
  if (tool === 'crop') enterCrop();
  // Reading the screenshot back costs a few milliseconds; spend them on the
  // click rather than on the first drag.
  if (tool === 'redact') prepareRedact();

  updateHint();
  canvas.requestRenderAll();
}

function updateHint() {
  if (!hasImage()) {
    el.hint.textContent = 'Drop a screenshot here, or paste with Ctrl+V';
    return;
  }
  if (ui.editingText) {
    el.hint.textContent = `Type the label — Esc or ${MOD}Enter when done`;
    return;
  }
  if (ui.tool === 'emoji' && ui.pendingEmoji) {
    el.hint.textContent = `Click to place ${ui.pendingEmoji}`;
    return;
  }
  el.hint.textContent = HINTS[ui.tool] || '';
}

/* --- Drawing -------------------------------------------------------------- */

let drawing = null;
let origin = { x: 0, y: 0 };

canvas.on('mouse:down', (opt) => {
  if (!hasImage() || ui.tool === 'crop') return;
  const p = canvas.getPointer(opt.e);
  origin = { x: p.x, y: p.y };
  const u = unit();

  if (ui.tool === 'text') {
    // A click on a label, or while one is being edited, belongs to that label.
    if (ui.editingText || opt.target) return;
    const text = new fabric.Textbox('text', {
      left: p.x,
      top: p.y,
      escTool: 'text',
      fontFamily: fontFamily(),
      fontSize: 22 * u,
      fill: color(),
      stroke: '#ffffff',
      strokeWidth: 2 * u,
      paintFirst: 'stroke',
      shadow: new fabric.Shadow({ color: 'rgba(0,0,0,0.3)', blur: 2 * u, offsetX: 2 * u, offsetY: 2 * u }),
      fontWeight: '900',
      width: 250 * u
    });
    beginHistoryEdit();
    canvas.add(text);
    canvas.setActiveObject(text);
    text.enterEditing();
    text.selectAll();
    // The label is placed; typing continues, but the tool is done.
    setTool('select');
  } else if (ui.tool === 'emoji') {
    if (!ui.pendingEmoji) return;
    const emoji = new fabric.IText(ui.pendingEmoji, {
      left: p.x,
      top: p.y,
      originX: 'center',
      originY: 'center',
      escTool: 'emoji',
      fontFamily: 'sans-serif',
      fontSize: 100 * u,
      editable: false
    });
    canvas.add(emoji);
    ui.pendingEmoji = null;
    setTool('select');
  } else if (ui.tool === 'rect') {
    beginHistoryEdit();
    drawing = new fabric.Rect({
      left: p.x,
      top: p.y,
      originX: 'left',
      originY: 'top',
      width: 0,
      height: 0,
      escTool: 'rect',
      fill: 'rgba(255,255,255,0)',
      stroke: color(),
      strokeWidth: 5 * u,
      hasBorders: false,
      strokeUniform: true
    });
    canvas.add(drawing);
  } else if (ui.tool === 'mark') {
    beginHistoryEdit();
    drawing = new fabric.Rect({
      left: p.x,
      top: p.y,
      originX: 'left',
      originY: 'top',
      width: 0,
      height: 0,
      escTool: 'mark',
      fill: color() + '55',
      strokeWidth: 0,
      rx: 2 * u,
      ry: 2 * u,
      hasBorders: false,
      globalCompositeOperation: 'multiply'
    });
    canvas.add(drawing);
  } else if (ui.tool === 'redact') {
    beginHistoryEdit();
    drawing = createRedact(p.x, p.y);
    canvas.add(drawing);
  } else if (ui.tool === 'arrow') {
    beginHistoryEdit();
    drawing = createArrow(p.x, p.y, { color: color(), unit: u });
    canvas.add(drawing);
  }
  canvas.requestRenderAll();
});

canvas.on('mouse:move', (opt) => {
  if (!drawing) return;
  const p = canvas.getPointer(opt.e);

  if (ui.tool === 'rect' || ui.tool === 'mark' || ui.tool === 'redact') {
    drawing.set({
      left: Math.min(p.x, origin.x),
      top: Math.min(p.y, origin.y),
      width: Math.abs(p.x - origin.x),
      height: Math.abs(p.y - origin.y)
    });
  } else if (ui.tool === 'arrow') {
    setArrowHeadPoint(drawing, p.x, p.y);
  }
  canvas.requestRenderAll();
});

canvas.on('mouse:up', () => {
  if (!drawing) return;
  const made = drawing;
  drawing = null;

  // A click without a drag leaves a zero-size shape behind — drop it, and
  // discard the pending snapshot with it.
  if (made.escTool !== 'arrow' && (made.width < 2 || made.height < 2)) {
    canvas.remove(made);
    abandonHistoryEdit();
    canvas.requestRenderAll();
    return;
  }

  // Measure the finished arrow before the snapshot so undo restores it exactly.
  if (made.escTool === 'arrow') syncArrowGeometry(made);
  commitHistoryEdit();
  setTool('select');
  canvas.requestRenderAll();
});

let textBeforeEdit = '';

canvas.on('text:editing:entered', (opt) => {
  ui.editingText = true;
  textBeforeEdit = (opt && opt.target && opt.target.text) || '';
  updateHint();
});

canvas.on('text:editing:exited', (opt) => {
  ui.editingText = false;
  const target = (opt && opt.target) || canvas.getActiveObject();
  const empty = target && target.type === 'textbox' && !target.text.trim();
  if (empty) canvas.remove(target);

  if (historyPending) {
    // A brand new label: record it, unless it was abandoned blank.
    if (empty) abandonHistoryEdit();
    else commitHistoryEdit();
  } else if (target && target.text !== textBeforeEdit) {
    // Retyping an existing label is a change worth undoing.
    canvas.fire('object:modified');
  }

  // Finishing a label leaves nothing selected, like every other tool — unless
  // this exit was caused by clicking something else, which fabric selects next.
  setTimeout(() => {
    if (canvas.getActiveObject() === target) {
      canvas.discardActiveObject();
      canvas.requestRenderAll();
    }
  }, 0);
  updateHint();
});
canvas.on('object:added', updateHistoryButtons);
canvas.on('object:removed', updateHistoryButtons);
canvas.on('object:modified', (opt) => {
  // Dragging the head control reshapes the arrow too. fabric-history has
  // already snapshotted by now, so re-baseline against the measured object.
  if (opt && opt.target && opt.target.escTool === 'arrow') {
    syncArrowGeometry(opt.target);
    resetHistoryBaseline();
  }
  updateHistoryButtons();
});

function updateHistoryButtons() {
  el.undo.disabled = !canvas.canUndo();
  el.redo.disabled = !canvas.canRedo();
}

/** Objects that come back from JSON are plain fabric types; give arrows their
 *  head-drag control back. */
function restoreArrowControls() {
  canvas.getObjects().forEach((o) => {
    if (o.escTool === 'arrow') attachArrowControls(o);
  });
}

canvas.on('history:undo', restoreArrowControls);
canvas.on('history:redo', restoreArrowControls);

/* --- History snapshots ----------------------------------------------------
 * fabric-history snapshots the canvas when an object is added, but shapes are
 * added empty on mouse:down and sized during the drag — so the snapshot would
 * capture a half-drawn shape and undo would restore it at zero size. These
 * defer the snapshot until the shape is actually finished.
 */
let historyPending = false;

function beginHistoryEdit() {
  if (historyPending) return;
  historyPending = true;
  canvas.historyProcessing = true;
}

function commitHistoryEdit() {
  if (!historyPending) return;
  historyPending = false;
  canvas.historyProcessing = false;
  canvas._historySaveAction();
  updateHistoryButtons();
}

/** Ends a suppressed edit without recording it — for a shape we threw away. */
function abandonHistoryEdit() {
  if (!historyPending) return;
  historyPending = false;
  canvas.historyProcessing = false;
}

/** Re-baselines the snapshot after the canvas changes outside an edit. */
function resetHistoryBaseline() {
  canvas.historyNextState = canvas._historyNext();
}

/* ==========================================================================
   Colour palette
   ========================================================================== */

function fontFamily(id = settings.font) {
  const known = FONTS.find((f) => f.id === id);
  return known ? known.family : (id || FONTS[0].family);
}

function setFont(id) {
  settings.font = id || 'system';
  // Like the swatches: a selected label takes the new font straight away.
  const labels = canvas.getActiveObjects().filter((o) => o.escTool === 'text');
  if (labels.length) {
    labels.forEach((o) => o.set('fontFamily', fontFamily()));
    canvas.requestRenderAll();
    canvas.fire('object:modified');
  }
  saveSettings();
  syncFontUI();
}

function renderFontPicker() {
  const seg = $('#font-seg');
  seg.innerHTML = '';
  FONTS.forEach((f) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.font = f.id;
    btn.textContent = f.label;
    btn.style.fontFamily = f.family;
    btn.addEventListener('click', () => setFont(f.id));
    seg.appendChild(btn);
  });
  $('#font-custom').addEventListener('change', (e) => {
    const value = e.target.value.trim();
    setFont(value || 'system');
    e.target.blur();
  });
}

function syncFontUI() {
  const known = FONTS.some((f) => f.id === settings.font);
  document.querySelectorAll('#font-seg button').forEach((btn) => {
    btn.setAttribute('aria-pressed', String(btn.dataset.font === settings.font));
  });
  const custom = $('#font-custom');
  if (document.activeElement !== custom) custom.value = known ? '' : settings.font;
}

function applyColorToSelection(hex) {
  const objects = canvas.getActiveObjects();
  if (!objects.length) return;
  objects.forEach((o) => {
    if (o.escTool === 'rect') o.set('stroke', hex);
    else if (o.escTool === 'mark') o.set('fill', hex + '55');
    else if (o.escTool === 'emoji') { /* emoji keep their own colours */ }
    else if (o.escTool === 'redact') { /* a scramble takes the shot's own colours */ }
    else o.set('fill', hex);
  });
  canvas.requestRenderAll();
  canvas.fire('object:modified');
}

function renderPalette() {
  el.palette.innerHTML = '';
  settings.palette.forEach((hex, i) => {
    const btn = document.createElement('button');
    btn.className = 'swatch';
    btn.style.background = hex;
    btn.title = hex.toUpperCase();
    btn.setAttribute('aria-label', `Colour ${hex.toUpperCase()}`);
    btn.setAttribute('aria-pressed', String(ui.colorIdx === i));
    btn.addEventListener('click', () => {
      ui.colorIdx = i;
      renderPalette();
      applyColorToSelection(hex);
    });
    el.palette.appendChild(btn);
  });
}

/* ==========================================================================
   Emoji picker
   ========================================================================== */

function renderEmojiPicker() {
  el.emojiPicker.innerHTML = '';
  EMOJIS.forEach((char) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = char;
    btn.addEventListener('click', () => {
      ui.pendingEmoji = char;
      el.emojiPicker.hidden = true;
      updateHint();
    });
    el.emojiPicker.appendChild(btn);
  });
}

/* ==========================================================================
   Crop
   ========================================================================== */

const CROP_INSET = 0.1;

function enterCrop() {
  if (!hasImage()) return;
  // Start inset rather than full-bleed: the handles are then plainly visible and
  // there is margin left to drag out a fresh region.
  crop = {
    x: imgW * CROP_INSET,
    y: imgH * CROP_INSET,
    w: imgW * (1 - CROP_INSET * 2),
    h: imgH * (1 - CROP_INSET * 2)
  };
  el.cropLayer.hidden = false;
  el.cropBar.hidden = false;
  renderCropBox();
}

function exitCrop() {
  crop = null;
  el.cropLayer.hidden = true;
  el.cropBar.hidden = true;
}

function renderCropBox() {
  if (!crop) return;
  const s = canvas.getZoom();
  el.cropBox.style.left = crop.x * s + 'px';
  el.cropBox.style.top = crop.y * s + 'px';
  el.cropBox.style.width = crop.w * s + 'px';
  el.cropBox.style.height = crop.h * s + 'px';
  el.cropSize.textContent = `${Math.round(crop.w)} × ${Math.round(crop.h)}`;
}

(function wireCrop() {
  let drag = null;

  const toImage = (e) => {
    const rect = el.cropLayer.getBoundingClientRect();
    const s = canvas.getZoom();
    return {
      x: clamp((e.clientX - rect.left) / s, 0, imgW),
      y: clamp((e.clientY - rect.top) / s, 0, imgH)
    };
  };

  el.cropLayer.addEventListener('pointerdown', (e) => {
    if (!crop) return;
    const handle = e.target.dataset.handle;
    const start = toImage(e);
    el.cropLayer.setPointerCapture(e.pointerId);

    if (handle) drag = { kind: 'resize', handle, box: { ...crop } };
    else if (e.target === el.cropBox) drag = { kind: 'move', start, box: { ...crop } };
    else drag = { kind: 'new', start };
    e.preventDefault();
  });

  el.cropLayer.addEventListener('pointermove', (e) => {
    if (!drag || !crop) return;
    const p = toImage(e);

    if (drag.kind === 'new') {
      crop = {
        x: Math.min(p.x, drag.start.x),
        y: Math.min(p.y, drag.start.y),
        w: Math.abs(p.x - drag.start.x),
        h: Math.abs(p.y - drag.start.y)
      };
    } else if (drag.kind === 'move') {
      const dx = p.x - drag.start.x;
      const dy = p.y - drag.start.y;
      crop.x = clamp(drag.box.x + dx, 0, imgW - drag.box.w);
      crop.y = clamp(drag.box.y + dy, 0, imgH - drag.box.h);
    } else {
      const b = drag.box;
      const left = drag.handle.includes('w') ? p.x : b.x;
      const top = drag.handle.includes('n') ? p.y : b.y;
      const right = drag.handle.includes('e') ? p.x : b.x + b.w;
      const bottom = drag.handle.includes('s') ? p.y : b.y + b.h;
      crop = {
        x: Math.min(left, right),
        y: Math.min(top, bottom),
        w: Math.abs(right - left),
        h: Math.abs(bottom - top)
      };
    }
    renderCropBox();
  });

  const endDrag = () => {
    if (!drag) return;
    drag = null;
    // A stray click shouldn't wipe the selection out.
    if (crop && (crop.w < 8 || crop.h < 8)) {
      enterCrop();
    }
  };

  el.cropLayer.addEventListener('pointerup', endDrag);
  el.cropLayer.addEventListener('pointercancel', endDrag);
})();

function applyCrop() {
  if (!crop || !hasImage()) return;
  const x = Math.round(crop.x);
  const y = Math.round(crop.y);
  const w = Math.max(1, Math.round(crop.w));
  const h = Math.max(1, Math.round(crop.h));

  if (w === imgW && h === imgH && x === 0 && y === 0) {
    setTool('select');
    return;
  }

  const cut = document.createElement('canvas');
  cut.width = w;
  cut.height = h;
  cut.getContext('2d').drawImage(img, x, y, w, h, 0, 0, w, h);

  canvas.getObjects().forEach((o) => {
    o.set({ left: o.left - x, top: o.top - y });
    o.setCoords();
  });

  const cropped = new Image();
  cropped.onload = () => {
    img = cropped;
    imgW = w;
    imgH = h;
    setRedactSource(cropped, unit());
    el.shotImg.src = cropped.src;
    el.titleDims.textContent = `${w} × ${h}`;
    ui.zoom = 0;
    exitCrop();
    setTool('select');
    relayout();
    // Coordinates moved with the crop, so earlier history entries no longer
    // apply — drop them and re-baseline against the shifted objects.
    canvas.clearHistory();
    resetHistoryBaseline();
    updateHistoryButtons();
    toast(`Cropped to ${w} × ${h}`);
  };
  cropped.onerror = () => toast('Could not apply the crop');
  cropped.src = cut.toDataURL('image/png');
}

/* ==========================================================================
   Export — fill + shot + annotations + watermark, at the chosen size
   ========================================================================== */

function roundRectPath(ctx, x, y, w, h, r) {
  const radius = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + w, y, x + w, y + h, radius);
  ctx.arcTo(x + w, y + h, x, y + h, radius);
  ctx.arcTo(x, y + h, x, y, radius);
  ctx.arcTo(x, y, x + w, y, radius);
  ctx.closePath();
}

/** Factor between the natural layout (shot at native resolution) and the PNG.
 *  Scales are applied exactly — 1× is lossless and 2× an integer upscale —
 *  and only an explicit width makes it fractional. */
function exportScale(layout = naturalLayout()) {
  if (settings.outW == null) return settings.outScale;
  return clamp(Math.round(settings.outW), 240, 8000) / layout.outW;
}

function exportWidth(layout = naturalLayout()) {
  return Math.max(1, Math.round(layout.outW * exportScale(layout)));
}

/** Bottom-right of the output, sized and inset off its width, so it lands in
 *  the same corner whether or not there is padding or extra canvas. */
function drawWatermark(ctx, outW, outH) {
  const size = outW * 0.0125;
  const inset = outW * 0.013;
  const padX = size * 0.65;
  const padY = size * 0.3;
  const gap = size * 0.4;
  const icon = size * 0.95;
  const label = `600 ${size}px ${UI_FONT}`;

  ctx.save();
  ctx.font = label;
  const textW = ctx.measureText(WATERMARK_TEXT).width;
  const boxW = padX * 2 + icon + gap + textW;
  const boxH = padY * 2 + Math.max(icon, size);
  const bx = outW - inset - boxW;
  const by = outH - inset - boxH;

  ctx.shadowColor = 'rgba(0,0,0,.12)';
  ctx.shadowBlur = size * 0.3;
  ctx.shadowOffsetY = size * 0.1;
  ctx.fillStyle = 'rgba(255,255,255,.86)';
  roundRectPath(ctx, bx, by, boxW, boxH, size * 0.45);
  ctx.fill();
  ctx.shadowColor = 'transparent';

  const cx = bx + padX + icon / 2;
  const cy = by + boxH / 2;
  ctx.fillStyle = BRAND;
  ctx.beginPath();
  ctx.arc(cx, cy, icon / 2, 0, Math.PI * 2);
  ctx.fill();

  // Same glyph maths as the .mark rule in styles.css: 1.3245 makes the "e"'s
  // ink half the circle's diameter, and the offsets recentre it.
  const glyph = icon * 1.3245;
  ctx.fillStyle = '#ffffff';
  ctx.font = `400 ${glyph}px Cookie, cursive`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText('e', cx - glyph * 0.03375, cy - glyph * 0.06375);

  ctx.fillStyle = '#7c7c88';
  ctx.font = label;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  ctx.fillText(WATERMARK_TEXT, bx + padX + icon + gap, cy);
  ctx.restore();
}

/** Renders the annotations at `k` times native resolution, independent of the
 *  on-screen zoom. */
/** Renders the annotation layer for the whole output at `k` times its natural
 *  size, independent of the on-screen zoom. Image space is offset to the shot,
 *  so marks that run into the margin come out where they were drawn. */
function renderAnnotations(k, layout) {
  const vpt = canvas.viewportTransform.slice();
  const width = canvas.getWidth();
  const height = canvas.getHeight();
  // An active selection re-parents its objects, which would throw the offscreen
  // render's geometry off — drop it before measuring.
  canvas.discardActiveObject();
  canvas.setDimensions({ width: Math.round(layout.outW), height: Math.round(layout.outH) });
  canvas.setViewportTransform([1, 0, 0, 1, layout.shotX, layout.shotY]);
  const out = canvas.toCanvasElement(k);
  canvas.setDimensions({ width, height });
  canvas.setViewportTransform(vpt);
  canvas.requestRenderAll();
  return out;
}

async function compose() {
  if (!hasImage()) return null;
  if (settings.watermark && document.fonts) {
    try {
      await document.fonts.load(`400 ${Math.round(imgW * 0.01)}px Cookie`);
    } catch (err) {
      console.warn('Cookie font unavailable for the watermark', err);
    }
  }

  const L = naturalLayout();
  const k = exportScale(L);
  const outW = exportWidth(L);
  const outH = Math.max(1, Math.round(L.outH * k));
  const shot = { x: L.shotX * k, y: L.shotY * k, w: imgW * k, h: imgH * k };
  const radius = settings.pad > 0 ? shot.w * 0.012 : 0;

  const out = document.createElement('canvas');
  out.width = outW;
  out.height = outH;
  const ctx = out.getContext('2d');

  // Margins and extra canvas take the fill; without one they stay transparent.
  if (backdropOn()) {
    const index = settings.backdrop;
    if (index <= 4) {
      ctx.fillStyle = settings.solidBg[index - 1];
    } else {
      const g = settings.gradBg[index - 5];
      const grad = ctx.createLinearGradient(0, 0, outW, outH);
      grad.addColorStop(0, g.a);
      grad.addColorStop(1, g.b);
      ctx.fillStyle = grad;
    }
    ctx.fillRect(0, 0, outW, outH);

    if (settings.pad > 0) {
      const u = unit() * k;
      ctx.save();
      ctx.shadowColor = 'rgba(0,0,0,.32)';
      ctx.shadowBlur = 34 * u;
      ctx.shadowOffsetY = 10 * u;
      ctx.fillStyle = '#000';
      roundRectPath(ctx, shot.x, shot.y, shot.w, shot.h, radius);
      ctx.fill();
      ctx.restore();
    }
  }

  ctx.save();
  roundRectPath(ctx, shot.x, shot.y, shot.w, shot.h, radius);
  ctx.clip();
  ctx.drawImage(img, shot.x, shot.y, shot.w, shot.h);
  ctx.restore();

  // Annotations go over everything, margin included.
  ctx.drawImage(renderAnnotations(k, L), 0, 0, outW, outH);

  if (settings.watermark) drawWatermark(ctx, outW, outH);

  return out.toDataURL('image/png');
}

function exportName() {
  const base = (el.titleName.textContent || 'escribo').replace(/\.[^.]+$/, '');
  return `${base || 'escribo'}-annotated.png`;
}

function flashLabel(button, text) {
  const label = button.querySelector('.label');
  if (!label || label.dataset.busy) return;
  const original = label.textContent;
  label.dataset.busy = '1';
  label.textContent = text;
  setTimeout(() => {
    label.textContent = original;
    delete label.dataset.busy;
  }, 1400);
}

async function copyToClipboard() {
  const dataUrl = await compose();
  if (!dataUrl) {
    toast('Nothing to copy yet');
    return false;
  }
  try {
    const blob = await (await fetch(dataUrl)).blob();
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
    flashLabel($('#copy'), 'Copied');
    toast('Copied to clipboard');
    return true;
  } catch (err) {
    console.error('Clipboard write failed', err);
    toast('Could not copy the image');
    return false;
  }
}

/** Copy, then dismiss the window — the quick way to hand a shot on. */
async function copyAndClose() {
  closeCopyMenu();
  const copied = await copyToClipboard();
  if (!copied || !isDesktop || !window.electronAPI.window) return;
  flashLabel($('#copy'), 'Closing…');
  setTimeout(() => window.electronAPI.window('close'), 350);
}

async function saveToDisk() {
  const dataUrl = await compose();
  if (!dataUrl) return toast('Nothing to save yet');

  if (window.electronAPI && window.electronAPI.saveImage) {
    const result = await window.electronAPI.saveImage(dataUrl, exportName());
    if (result && result.success) {
      flashLabel($('#save'), 'Saved');
      toast('Saved');
    } else if (result && result.error) {
      toast('Could not save the image');
    }
    return;
  }

  const link = document.createElement('a');
  link.href = dataUrl;
  link.download = exportName();
  link.click();
  flashLabel($('#save'), 'Saved');
}

/* ==========================================================================
   Status bar — padding & fill, aspect ratio & export size, theme, zoom
   ========================================================================== */

function padNote() {
  if (settings.pad === 0) return 'No margin around the screenshot.';
  return backdropOn() ? 'Margin uses the fill below.' : 'Margin stays transparent in the PNG.';
}

function ratioNote(R) {
  if (!R.v) return 'Matches the screenshot — no extra canvas.';
  return backdropOn() ? 'Extra canvas is filled with the chosen fill.' : 'Extra canvas stays transparent in the PNG.';
}

/** Builds the fill chips once. Selecting one must not replace these nodes: the
 *  click target would detach mid-dispatch and the popover would read it as a
 *  click outside itself and close. syncPaddingUI() updates them in place. */
function renderBackdropChips() {
  const solids = $('#solid-chips');
  const grads = $('#grad-chips');
  solids.innerHTML = '';
  grads.innerHTML = '';

  const addChip = (index, parent) => {
    const btn = document.createElement('button');
    btn.className = 'chip';
    btn.dataset.backdrop = String(index);
    btn.title = index === 0 ? 'None' : index <= 4 ? SOLID_NAMES[index - 1] : GRAD_NAMES[index - 5];
    btn.setAttribute('aria-label', btn.title);
    btn.addEventListener('click', () => pickFill(index));
    parent.appendChild(btn);
  };

  [0, 1, 2, 3, 4].forEach((i) => addChip(i, solids));
  [5, 6, 7, 8].forEach((i) => addChip(i, grads));
  syncPaddingUI();
}

function pickFill(index) {
  settings.backdrop = index;
  // A fill with no margin has nowhere to show on "Orig" — give it some.
  if (index !== 0 && settings.pad === 0) settings.pad = 6;
  saveSettings();
  syncPaddingUI();
  relayout();
}

function setPad(value) {
  settings.pad = clamp(Math.round(Number(value) || 0), 0, 18);
  saveSettings();
  syncPaddingUI();
  relayout();
}

function syncPaddingUI() {
  document.querySelectorAll('#backdrop-pop .chip').forEach((btn) => {
    const index = Number(btn.dataset.backdrop);
    btn.style.background = index === 0 ? NONE_CHIP : backdropCss(index);
    btn.setAttribute('aria-pressed', String(settings.backdrop === index));
  });
  document.querySelectorAll('#pad-range, #pad-range-prefs').forEach((input) => { input.value = settings.pad; });
  document.querySelectorAll('.pad-value').forEach((span) => { span.textContent = settings.pad + '%'; });
  document.querySelectorAll('.pad-note').forEach((note) => { note.textContent = padNote(); });

  el.backdropChip.style.background = backdropOn() ? backdropCss() : NONE_CHIP;
  el.backdropBtn.classList.toggle('on', backdropOn() || settings.pad > 0);
  if (hasImage()) syncRatioPop();
}

/** Builds the ratio and scale pickers once; syncRatioPop() keeps them current. */
function renderRatioPop() {
  el.ratioSeg.innerHTML = '';
  RATIOS.forEach((r) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.ratio = r.id;
    btn.textContent = r.label;
    btn.title = r.hint;
    btn.addEventListener('click', () => {
      settings.ratio = r.id;
      saveSettings();
      relayout();
    });
    el.ratioSeg.appendChild(btn);
  });

  el.scaleSeg.innerHTML = '';
  SCALES.forEach((sc) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.scale = String(sc.k);
    btn.textContent = sc.label;
    btn.addEventListener('click', () => {
      settings.outScale = sc.k;
      settings.outW = null;
      saveSettings();
      syncRatioPop();
    });
    el.scaleSeg.appendChild(btn);
  });
}

function syncRatioPop(layout) {
  const R = currentRatio();
  el.ratioLabel.textContent = R.label;
  el.ratioBtn.classList.toggle('on', !!R.v);
  el.ratioNote.textContent = ratioNote(R);
  el.ratioSeg.querySelectorAll('button').forEach((btn) => {
    btn.setAttribute('aria-pressed', String(btn.dataset.ratio === R.id));
  });

  if (!hasImage()) {
    el.outW.value = '';
    el.outH.textContent = '—';
    return;
  }

  const L = layout || naturalLayout();
  const outW = exportWidth(L);
  el.scaleSeg.querySelectorAll('button').forEach((btn) => {
    const k = Number(btn.dataset.scale);
    btn.title = Math.round(L.outW * k) + 'px wide';
    btn.setAttribute('aria-pressed', String(settings.outW == null && settings.outScale === k));
  });
  if (document.activeElement !== el.outW) el.outW.value = outW;
  el.outH.textContent = Math.max(1, Math.round((L.outH * outW) / L.outW));
  syncPresetsUI();
}

function setTheme(theme) {
  settings.theme = theme;
  document.documentElement.dataset.theme = theme;
  $('#theme-label').textContent = theme === 'light' ? 'Light' : 'Dark';
  saveSettings();
}

function setZoom(step) {
  ui.zoom = clamp(step, 0, MAX_ZOOM_STEPS);
  relayout();
}

/* ==========================================================================
   Profiles & presets
   A profile is the global kit (palette, fills, font) and owns its presets.
   A preset is a screenshot style: ratio, export size, padding, fill and
   watermark. Both travel as JSON so a team can share one file.
   ========================================================================== */

const PRESET_FILE_KIND = 'escribo-presets';
const PROFILE_FILE_KIND = 'escribo-profile';

function esc(text) {
  return String(text).replace(/[&<>"']/g, (c) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
  ));
}

function slug(text) {
  return String(text).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'untitled';
}

function newId(prefix) {
  return prefix + '_' + Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

function validId(id) {
  return typeof id === 'string' && /^[\w-]{1,40}$/.test(id) ? id : null;
}

function isHex(v) {
  return typeof v === 'string' && /^#[0-9a-f]{6}$/i.test(v);
}

function cleanName(name, fallback = 'Untitled') {
  return String(name == null ? '' : name).trim().slice(0, 60) || fallback;
}

/* --- Presets: a screenshot style ------------------------------------------ */

/** The current style — what a preset captures. */
function presetValues() {
  const out = {};
  STYLE_FIELDS.forEach((key) => { out[key] = settings[key]; });
  return out;
}

/** Keeps only fields a preset may set, each checked — imports are untrusted. */
function sanitizeValues(v) {
  const out = {};
  if (!v || typeof v !== 'object') return out;
  if (RATIOS.some((r) => r.id === v.ratio)) out.ratio = v.ratio;
  if (SCALES.some((sc) => sc.k === v.outScale)) out.outScale = v.outScale;
  out.outW = Number.isFinite(v.outW) ? clamp(Math.round(v.outW), 240, 8000) : null;
  if (typeof v.pad === 'number' && Number.isFinite(v.pad)) out.pad = clamp(Math.round(v.pad), 0, 18);
  if (Number.isInteger(v.backdrop) && v.backdrop >= 0 && v.backdrop <= 8) out.backdrop = v.backdrop;
  if (typeof v.watermark === 'boolean') out.watermark = v.watermark;
  return out;
}

function sanitizePreset(p) {
  if (!p || typeof p !== 'object' || typeof p.name !== 'string' || !p.values || typeof p.values !== 'object') return null;
  const clean = {
    id: validId(p.id) || newId('p'),
    name: cleanName(p.name),
    createdAt: typeof p.createdAt === 'string' ? p.createdAt : new Date().toISOString(),
    values: sanitizeValues(p.values)
  };
  if (typeof p.updatedAt === 'string') clean.updatedAt = p.updatedAt;
  return clean;
}

function activePreset() {
  return settings.presets.find((p) => p.id === settings.activePreset) || null;
}

/** True when the current style differs from the preset in any field the
 *  preset defines (an imported preset may leave some out). */
function presetIsModified(preset) {
  const want = sanitizeValues(preset.values);
  const have = presetValues();
  return Object.keys(want).some((key) => JSON.stringify(want[key]) !== JSON.stringify(have[key]));
}

/** Re-renders everything that shows the current style. */
function refreshStyleUI() {
  syncPaddingUI();
  wmToggle.checked = settings.watermark;
  document.body.classList.toggle('no-watermark', !settings.watermark);
  relayout();
  syncPresetsUI();
}

function applyPreset(preset) {
  const v = sanitizeValues(preset.values);
  STYLE_FIELDS.forEach((key) => { if (key in v) settings[key] = v[key]; });
  settings.activePreset = preset.id;
  saveSettings();
  refreshStyleUI();
  toast(`Applied “${preset.name}”`);
}

function savePreset(name) {
  const preset = { id: newId('p'), name: cleanName(name, ''), createdAt: new Date().toISOString(), values: presetValues() };
  if (!preset.name) return;
  settings.presets.push(preset);
  settings.activePreset = preset.id;
  saveSettings();
  syncPresetsUI(true);
  toast(`Saved “${preset.name}”`);
}

function updatePreset(preset) {
  preset.values = presetValues();
  preset.updatedAt = new Date().toISOString();
  settings.activePreset = preset.id;
  saveSettings();
  syncPresetsUI(true);
  toast(`Updated “${preset.name}”`);
}

function deletePreset(preset) {
  settings.presets = settings.presets.filter((p) => p.id !== preset.id);
  if (settings.activePreset === preset.id) settings.activePreset = null;
  saveSettings();
  syncPresetsUI(true);
}

/* --- Profiles: the global kit --------------------------------------------- */

/** A profile's options, each checked, with defaults filled in. */
function sanitizeProfileValues(v) {
  const d = PROFILE_DEFAULTS;
  const src = v && typeof v === 'object' ? v : {};
  return {
    palette: Array.isArray(src.palette) && src.palette.length === 6 && src.palette.every(isHex)
      ? src.palette.slice() : d.palette.slice(),
    defaultIdx: Number.isInteger(src.defaultIdx) && src.defaultIdx >= 0 && src.defaultIdx < 6 ? src.defaultIdx : d.defaultIdx,
    solidBg: Array.isArray(src.solidBg) && src.solidBg.length === 4 && src.solidBg.every(isHex)
      ? src.solidBg.slice() : d.solidBg.slice(),
    gradBg: Array.isArray(src.gradBg) && src.gradBg.length === 4 && src.gradBg.every((g) => g && isHex(g.a) && isHex(g.b))
      ? src.gradBg.map((g) => ({ a: g.a, b: g.b })) : d.gradBg.map((g) => ({ ...g })),
    font: typeof src.font === 'string' && src.font.trim() && src.font.length <= 120 ? src.font : d.font,
    presets: Array.isArray(src.presets) ? src.presets.map(sanitizePreset).filter(Boolean) : []
  };
}

function newProfile(name, values) {
  return { id: newId('prof'), name: cleanName(name), createdAt: new Date().toISOString(), ...sanitizeProfileValues(values) };
}

function sanitizeProfile(p) {
  if (!p || typeof p !== 'object' || typeof p.name !== 'string') return null;
  const clean = {
    id: validId(p.id) || newId('prof'),
    name: cleanName(p.name),
    createdAt: typeof p.createdAt === 'string' ? p.createdAt : new Date().toISOString(),
    ...sanitizeProfileValues(p)
  };
  if (typeof p.updatedAt === 'string') clean.updatedAt = p.updatedAt;
  return clean;
}

function activeProfile() {
  return settings.profiles.find((p) => p.id === settings.activeProfile) || settings.profiles[0];
}

/** Re-renders everything that reads the active profile. The current style
 *  stays as it is — only the kit around it changes. */
function refreshProfileUI() {
  ui.colorIdx = settings.defaultIdx;
  renderPalette();
  syncFontUI();
  syncPaddingUI();
  if (!el.prefs.hidden) renderPrefs();
  relayout();
  syncPresetsUI(true);
}

function setActiveProfile(id) {
  const profile = settings.profiles.find((p) => p.id === id);
  if (!profile || profile.id === settings.activeProfile) return;
  settings.activeProfile = profile.id;
  saveSettings();
  refreshProfileUI();
  toast(`Switched to “${profile.name}”`);
}

function createProfile(name, from = null) {
  const profile = from ? newProfile(name, from) : newProfile(name, PROFILE_DEFAULTS);
  // A copy gets its own presets, not shared ids with the original.
  profile.presets = profile.presets.map((p) => ({ ...p, id: newId('p'), values: { ...p.values } }));
  settings.profiles.push(profile);
  settings.activeProfile = profile.id;
  saveSettings();
  refreshProfileUI();
  toast(from ? `Duplicated as “${profile.name}”` : `Created “${profile.name}”`);
}

function renameProfile(profile, name) {
  const next = cleanName(name, '');
  if (!next) return;
  profile.name = next;
  profile.updatedAt = new Date().toISOString();
  saveSettings();
  refreshProfileUI();
}

function deleteProfile(profile) {
  if (settings.profiles.length <= 1) return toast('Keep at least one profile');
  settings.profiles = settings.profiles.filter((p) => p.id !== profile.id);
  if (settings.activeProfile === profile.id) settings.activeProfile = settings.profiles[0].id;
  saveSettings();
  refreshProfileUI();
  toast(`Deleted “${profile.name}”`);
}

/* --- Files: export & import ----------------------------------------------- */

function presetsFile(list) {
  return JSON.stringify({
    app: 'escribo',
    kind: PRESET_FILE_KIND,
    version: 1,
    exportedAt: new Date().toISOString(),
    presets: list.map((p) => ({ id: p.id, name: p.name, createdAt: p.createdAt, updatedAt: p.updatedAt, values: p.values }))
  }, null, 2);
}

function profileFile(profile) {
  return JSON.stringify({
    app: 'escribo',
    kind: PROFILE_FILE_KIND,
    version: 1,
    exportedAt: new Date().toISOString(),
    profile: sanitizeProfile(profile)
  }, null, 2);
}

async function exportJson(text, suggestedName, label) {
  if (isDesktop && window.electronAPI.saveText) {
    const result = await window.electronAPI.saveText(text, suggestedName);
    if (result && result.success) toast(`${label} exported`);
    else if (result && result.error) toast(`Could not export ${label.toLowerCase()}`);
    return;
  }

  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = suggestedName;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  toast(`${label} exported`);
}

function exportPresets(list, suggestedName) {
  if (!list.length) return toast('No presets to export');
  return exportJson(presetsFile(list), suggestedName, list.length === 1 ? 'Preset' : 'Presets');
}

function exportProfile(profile) {
  return exportJson(profileFile(profile), `${slug(profile.name)}.escribo-profile.json`, 'Profile');
}

/** Reads any escribo file: a profile (with its presets) or a list of presets.
 *  A profile is added and switched to; presets join the active profile. */
function importText(text) {
  let data;
  try {
    data = JSON.parse(text);
  } catch (err) {
    return toast('That file is not valid JSON');
  }
  if (data && typeof data === 'object' && (data.kind === PROFILE_FILE_KIND || (data.profile && typeof data.profile === 'object'))) {
    return importProfile(data.profile);
  }
  const list = Array.isArray(data) ? data : (data && Array.isArray(data.presets) ? data.presets : null);
  if (!list) return toast('No presets or profile found in that file');
  importPresetList(list);
}

function importProfile(raw) {
  const clean = sanitizeProfile(raw);
  if (!clean) return toast('No usable profile in that file');
  const existing = settings.profiles.findIndex((p) => p.id === clean.id);
  if (existing >= 0) {
    settings.profiles[existing] = clean;
  } else {
    if (settings.profiles.some((p) => p.name === clean.name)) clean.name += ' (imported)';
    settings.profiles.push(clean);
  }
  settings.activeProfile = clean.id;
  saveSettings();
  refreshProfileUI();
  const n = clean.presets.length;
  toast(`Imported profile “${clean.name}” with ${n} preset${n === 1 ? '' : 's'}`);
}

function importPresetList(list) {
  let count = 0;
  list.forEach((raw) => {
    const clean = sanitizePreset(raw);
    if (!clean) return;
    const existing = settings.presets.findIndex((x) => x.id === clean.id);
    if (existing >= 0) {
      settings.presets[existing] = clean;
    } else {
      if (settings.presets.some((x) => x.name === clean.name)) clean.name += ' (imported)';
      settings.presets.push(clean);
    }
    count++;
  });

  saveSettings();
  syncPresetsUI(true);
  toast(count ? `Imported ${count} preset${count === 1 ? '' : 's'} into “${activeProfile().name}”` : 'No usable presets in that file');
}

async function importFile() {
  if (isDesktop && window.electronAPI.openText) {
    const result = await window.electronAPI.openText();
    if (result && result.success) importText(result.text);
    else if (result && result.error) toast('Could not read that file');
    return;
  }
  $('#preset-file').click();
}

/* --- UI: the presets menu and the Profiles tab ---------------------------- */

const ICON_UPDATE = '<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path fill-rule="evenodd" d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"></path><path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"></path></svg>';
const ICON_EXPORT = '<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"></path><path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"></path></svg>';
const ICON_DELETE = '<svg width="11" height="11" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7"></path></svg>';
const ICON_RENAME = '<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"></path></svg>';
const ICON_DUPLICATE = '<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M4 2a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V2zm2-1a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V2a1 1 0 0 0-1-1H6zM2 5a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-1h1v1a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h1v1H2z"></path></svg>';

/** The status-bar label and profile name always; the rows only while the
 *  menu is open (or when asked), since the list is rebuilt from scratch. */
function syncPresetsUI(rows = false) {
  if (!el.presetsList) return;
  const profile = activeProfile();
  const active = activePreset();
  const modified = active ? presetIsModified(active) : false;
  el.presetsLabel.textContent = active ? active.name + (modified ? ' •' : '') : 'Presets';
  el.presetsBtn.title = active ? `Preset: ${active.name}${modified ? ' (modified)' : ''}` : 'Presets';
  el.presetsBtn.classList.toggle('on', !!active);
  $('#presets-profile').textContent = profile.name;
  $('#presets-profile').title = `Profile “${profile.name}” — manage profiles`;
  $('#prefs-profile').textContent = profile.name;

  if (!rows && el.presetsPop.hidden) return;

  el.presetsList.innerHTML = '';
  settings.presets.forEach((p) => {
    const row = document.createElement('div');
    row.className = 'preset-row';
    row.setAttribute('role', 'option');
    const isActive = p.id === settings.activePreset;
    row.setAttribute('aria-selected', String(isActive));
    row.innerHTML = `
      <span class="dot"></span>
      <button class="name" type="button" title="Apply “${esc(p.name)}”">${esc(p.name)}</button>
      ${isActive && modified ? '<span class="tag">modified</span>' : ''}
      <button class="row-btn" type="button" data-act="update" title="Update with the current style">${ICON_UPDATE}</button>
      <button class="row-btn" type="button" data-act="export" title="Export this preset">${ICON_EXPORT}</button>
      <button class="row-btn danger" type="button" data-act="delete" title="Delete">${ICON_DELETE}</button>`;
    row.querySelector('.name').addEventListener('click', () => applyPreset(p));
    row.querySelector('[data-act="update"]').addEventListener('click', () => updatePreset(p));
    row.querySelector('[data-act="export"]').addEventListener('click', () => exportPresets([p], `${slug(p.name)}.escribo-preset.json`));
    row.querySelector('[data-act="delete"]').addEventListener('click', () => {
      if (window.confirm(`Delete preset “${p.name}”?`)) deletePreset(p);
    });
    el.presetsList.appendChild(row);
  });

  $('#presets-empty').hidden = settings.presets.length > 0;
  $('#presets-export').disabled = settings.presets.length === 0;
}

function openPresetForm() {
  el.presetForm.hidden = false;
  el.presetName.value = '';
  el.presetName.focus();
}

function closePresetForm() {
  el.presetForm.hidden = true;
  el.presetName.value = '';
}

function renderProfileRows() {
  const list = $('#profile-list');
  list.innerHTML = '';
  const last = settings.profiles.length <= 1;
  settings.profiles.forEach((p) => {
    const row = document.createElement('div');
    row.className = 'preset-row profile-row';
    row.setAttribute('role', 'option');
    row.setAttribute('aria-selected', String(p.id === settings.activeProfile));
    const n = p.presets.length;
    row.innerHTML = `
      <span class="dot"></span>
      <button class="name" type="button" title="Switch to “${esc(p.name)}”">${esc(p.name)}</button>
      <span class="tag">${n} preset${n === 1 ? '' : 's'}</span>
      <button class="row-btn" type="button" data-act="rename" title="Rename">${ICON_RENAME}</button>
      <button class="row-btn" type="button" data-act="duplicate" title="Duplicate">${ICON_DUPLICATE}</button>
      <button class="row-btn" type="button" data-act="export" title="Export this profile, presets included">${ICON_EXPORT}</button>
      <button class="row-btn danger" type="button" data-act="delete" title="${last ? 'Keep at least one profile' : 'Delete'}"${last ? ' disabled' : ''}>${ICON_DELETE}</button>`;
    row.querySelector('.name').addEventListener('click', () => setActiveProfile(p.id));
    row.querySelector('[data-act="rename"]').addEventListener('click', () => openProfileForm('rename', p));
    row.querySelector('[data-act="duplicate"]').addEventListener('click', () => openProfileForm('duplicate', p));
    row.querySelector('[data-act="export"]').addEventListener('click', () => exportProfile(p));
    row.querySelector('[data-act="delete"]').addEventListener('click', () => {
      if (window.confirm(`Delete profile “${p.name}” and its ${n} preset${n === 1 ? '' : 's'}?`)) deleteProfile(p);
    });
    list.appendChild(row);
  });
}

let profileForm = null;   // { mode: 'new' | 'rename' | 'duplicate', target }

function openProfileForm(mode, target = null) {
  profileForm = { mode, target };
  const input = $('#profile-name');
  $('#profile-form').hidden = false;
  input.value = mode === 'rename' ? target.name : mode === 'duplicate' ? `${target.name} copy` : '';
  $('#profile-form-save').textContent = mode === 'rename' ? 'Rename' : mode === 'duplicate' ? 'Duplicate' : 'Create';
  input.focus();
  input.select();
}

function closeProfileForm() {
  profileForm = null;
  $('#profile-form').hidden = true;
  $('#profile-name').value = '';
}

function submitProfileForm() {
  if (!profileForm) return;
  const input = $('#profile-name');
  const name = input.value.trim();
  if (!name) return input.focus();
  const { mode, target } = profileForm;
  if (mode === 'rename') renameProfile(target, name);
  else createProfile(name, mode === 'duplicate' ? target : null);
  closeProfileForm();
}

/* ==========================================================================
   Preferences
   ========================================================================== */

function renderPrefs() {
  const paletteRows = $('#palette-rows');
  paletteRows.innerHTML = '';
  settings.palette.forEach((hex, i) => {
    const row = document.createElement('div');
    row.className = 'pref-row';
    row.innerHTML = `
      <input type="color" value="${hex}" aria-label="Palette colour ${i + 1}">
      <span class="hex">${hex.toUpperCase()}</span>
      <button class="star-btn" title="Arm by default" aria-label="Make default"
              aria-pressed="${settings.defaultIdx === i}">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M3.612 15.443c-.386.198-.824-.149-.746-.592l.83-4.73L.173 6.765c-.329-.314-.158-.888.283-.95l4.898-.696L7.538.792c.197-.39.73-.39.927 0l2.184 4.327 4.898.696c.441.062.612.636.283.95l-3.523 3.356.83 4.73c.078.443-.36.79-.746.592L8 13.187l-4.389 2.256z"></path></svg>
      </button>`;
    row.querySelector('input').addEventListener('input', (e) => {
      settings.palette[i] = e.target.value;
      row.querySelector('.hex').textContent = e.target.value.toUpperCase();
      saveSettings();
      renderPalette();
    });
    row.querySelector('.star-btn').addEventListener('click', () => {
      settings.defaultIdx = i;
      ui.colorIdx = i;
      saveSettings();
      renderPrefs();
      renderPalette();
    });
    paletteRows.appendChild(row);
  });

  const solidRows = $('#solid-rows');
  solidRows.innerHTML = '';
  settings.solidBg.forEach((hex, i) => {
    const row = document.createElement('div');
    row.className = 'pref-row';
    row.innerHTML = `
      <input type="color" value="${hex}" aria-label="${SOLID_NAMES[i]} backdrop">
      <span class="hex">${SOLID_NAMES[i]}</span>`;
    row.querySelector('input').addEventListener('input', (e) => {
      settings.solidBg[i] = e.target.value;
      saveSettings();
      syncPaddingUI();
      relayout();
    });
    solidRows.appendChild(row);
  });

  const gradRows = $('#grad-rows');
  gradRows.innerHTML = '';
  settings.gradBg.forEach((g, i) => {
    const row = document.createElement('div');
    row.className = 'pref-row grad';
    row.innerHTML = `
      <input type="color" value="${g.a}" aria-label="${GRAD_NAMES[i]} start">
      <span class="grad-name">${GRAD_NAMES[i]}</span>
      <span class="grad-preview"></span>
      <input type="color" value="${g.b}" aria-label="${GRAD_NAMES[i]} end">`;
    const preview = row.querySelector('.grad-preview');
    const paint = () => { preview.style.background = backdropCss(5 + i); };
    paint();
    const [start, end] = row.querySelectorAll('input');
    start.addEventListener('input', (e) => {
      settings.gradBg[i].a = e.target.value;
      paint();
      saveSettings();
      syncPaddingUI();
      relayout();
    });
    end.addEventListener('input', (e) => {
      settings.gradBg[i].b = e.target.value;
      paint();
      saveSettings();
      syncPaddingUI();
      relayout();
    });
    gradRows.appendChild(row);
  });

  renderProfileRows();
}

function setPrefsTab(tab) {
  ui.prefsTab = tab;
  document.querySelectorAll('.prefs-tab').forEach((btn) => {
    btn.setAttribute('aria-selected', String(btn.dataset.tab === tab));
  });
  $('#tab-annotate').hidden = tab !== 'annotate';
  $('#tab-backdrop').hidden = tab !== 'backdrop';
  $('#tab-profiles').hidden = tab !== 'profiles';
  if (tab !== 'profiles') closeProfileForm();
}

function openPrefs(tab) {
  closeBackdropPop();
  renderPrefs();
  if (typeof tab === 'string') setPrefsTab(tab);   // also used directly as a click handler
  el.prefs.hidden = false;
}

function closePrefs() {
  el.prefs.hidden = true;
  closeProfileForm();
}

/* ==========================================================================
   Helpers
   ========================================================================== */

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

let toastTimer = null;
function toast(message) {
  el.toast.textContent = message;
  el.toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.toast.hidden = true; }, 1800);
}

/* ==========================================================================
   Wiring
   ========================================================================== */

document.querySelectorAll('.tool-btn').forEach((btn) => {
  btn.addEventListener('click', () => {
    const tool = btn.dataset.tool;
    // Emoji opens a picker, so its button also closes it again.
    setTool(tool === 'emoji' && ui.tool === 'emoji' ? 'select' : tool);
  });
});

// Buttons must not keep keyboard focus after a click: the next hotkey pressed
// would light the button up with a focus ring, and a focused slider or field
// would swallow the key instead.
['#toolbar', '#statusbar', '#titlebar'].forEach((sel) => {
  $(sel).addEventListener('click', (e) => {
    const btn = e.target.closest('button');
    if (btn) btn.blur();
  });
});
document.querySelectorAll('input[type="range"]').forEach((input) => {
  input.addEventListener('change', () => input.blur());
});

const emojiToolBtn = document.querySelector('.tool-btn[data-tool="emoji"]');

el.undo.addEventListener('click', () => { canvas.undo(() => updateHistoryButtons()); });
el.redo.addEventListener('click', () => { canvas.redo(() => updateHistoryButtons()); });
$('#copy').addEventListener('click', copyToClipboard);
$('#save').addEventListener('click', saveToDisk);
$('#prefs-open').addEventListener('click', openPrefs);
$('#colours-btn').addEventListener('click', openPrefs);
$('#prefs-close').addEventListener('click', closePrefs);
$('#prefs-done').addEventListener('click', closePrefs);
el.prefs.addEventListener('click', (e) => { if (e.target === el.prefs) closePrefs(); });
document.querySelectorAll('.prefs-tab').forEach((btn) => {
  btn.addEventListener('click', () => setPrefsTab(btn.dataset.tab));
});

$('#prefs-reset').addEventListener('click', () => {
  // The active profile's kit goes back to stock; its presets stay.
  const fresh = sanitizeProfileValues(PROFILE_DEFAULTS);
  ['palette', 'defaultIdx', 'solidBg', 'gradBg', 'font'].forEach((key) => { settings[key] = fresh[key]; });
  saveSettings();
  refreshProfileUI();
  toast(`Restored the default colours and font for “${activeProfile().name}”`);
});

/* --- Popovers ------------------------------------------------------------
 * Each stays open while you work in it — picking fills, dragging padding,
 * typing a width. Only a click outside, its own button again, or Escape
 * closes it, and opening one closes the others. composedPath() is fixed at
 * dispatch, so it reports the true origin even if a handler replaced the
 * clicked node.
 */
const POPOVERS = [
  { name: 'backdrop', pop: el.backdropPop, btn: el.backdropBtn },
  { name: 'ratio', pop: el.ratioPop, btn: el.ratioBtn },
  { name: 'copy', pop: el.copyMenu, btn: el.copyCaret },
  { name: 'presets', pop: el.presetsPop, btn: el.presetsBtn }
];

function closePopovers(except) {
  POPOVERS.forEach((p) => {
    if (p.name === except) return;
    p.pop.hidden = true;
    p.btn.setAttribute('aria-expanded', 'false');
  });
}

function togglePopover(name) {
  const p = POPOVERS.find((x) => x.name === name);
  const open = p.pop.hidden;
  closePopovers(name);
  p.pop.hidden = !open;
  p.btn.setAttribute('aria-expanded', String(open));
}

function closeBackdropPop() { closePopovers('__none__'); }
function closeCopyMenu() { closePopovers('__none__'); }

el.backdropBtn.addEventListener('click', () => togglePopover('backdrop'));
el.presetsBtn.addEventListener('click', () => {
  togglePopover('presets');
  closePresetForm();
  if (!el.presetsPop.hidden) syncPresetsUI(true);
});

$('#preset-new').addEventListener('click', openPresetForm);
$('#preset-cancel').addEventListener('click', closePresetForm);
$('#preset-save').addEventListener('click', () => {
  if (!el.presetName.value.trim()) return el.presetName.focus();
  savePreset(el.presetName.value);
  closePresetForm();
});
el.presetName.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); $('#preset-save').click(); }
  if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closePresetForm(); }
});
$('#presets-import').addEventListener('click', importFile);
$('#presets-export').addEventListener('click', () => exportPresets(settings.presets, `${slug(activeProfile().name)}.escribo-presets.json`));
$('#preset-file').addEventListener('change', function () {
  const file = this.files && this.files[0];
  if (file) file.text().then(importText);
  this.value = null;
});
$('#presets-profile').addEventListener('click', () => {
  closePopovers();
  openPrefs('profiles');
});

$('#profile-new').addEventListener('click', () => openProfileForm('new'));
$('#profile-import').addEventListener('click', importFile);
$('#profile-form-save').addEventListener('click', submitProfileForm);
$('#profile-form-cancel').addEventListener('click', closeProfileForm);
$('#profile-name').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') { e.preventDefault(); submitProfileForm(); }
  if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeProfileForm(); }
});
el.ratioBtn.addEventListener('click', () => togglePopover('ratio'));
el.copyCaret.addEventListener('click', () => togglePopover('copy'));

document.addEventListener('click', (e) => {
  const path = e.composedPath();
  POPOVERS.forEach((p) => {
    if (p.pop.hidden || path.includes(p.pop) || path.includes(p.btn)) return;
    p.pop.hidden = true;
    p.btn.setAttribute('aria-expanded', 'false');
  });
  // The emoji picker closes on a click away too; with nothing picked, that
  // also means the tool is no longer wanted.
  if (!el.emojiPicker.hidden && !path.includes(el.emojiPicker) && !path.includes(emojiToolBtn)) {
    setTool('select');
  }
});

document.querySelectorAll('#pad-range, #pad-range-prefs').forEach((input) => {
  input.addEventListener('input', (e) => setPad(e.target.value));
});

el.outW.addEventListener('change', (e) => {
  settings.outW = clamp(Math.round(Number(e.target.value) || 0), 240, 8000);
  saveSettings();
  syncRatioPop();
  e.target.blur();
});

$('#copy-menu-copy').addEventListener('click', () => {
  closeCopyMenu();
  copyToClipboard();
});
$('#copy-menu-close').addEventListener('click', copyAndClose);

const wmToggle = $('#wm-toggle');
wmToggle.addEventListener('change', () => {
  settings.watermark = wmToggle.checked;
  document.body.classList.toggle('no-watermark', !settings.watermark);
  saveSettings();
});

$('#zoom-in').addEventListener('click', () => setZoom(ui.zoom + 1));
$('#zoom-out').addEventListener('click', () => setZoom(ui.zoom - 1));
el.zoomFit.addEventListener('click', () => setZoom(0));
$('#theme-btn').addEventListener('click', () => setTheme(settings.theme === 'light' ? 'dark' : 'light'));

$('#crop-cancel').addEventListener('click', () => setTool('select'));
$('#crop-apply').addEventListener('click', applyCrop);

$('#file-open').addEventListener('click', () => $('#file-input').click());
$('#file-input').addEventListener('change', function () {
  if (this.files && this.files[0]) loadBlob(this.files[0], this.files[0].name);
  this.value = null;
});

if (isDesktop && window.electronAPI.window) {
  $('#win-min').addEventListener('click', () => window.electronAPI.window('minimize'));
  $('#win-max').addEventListener('click', () => window.electronAPI.window('maximize'));
  $('#win-close').addEventListener('click', () => window.electronAPI.window('close'));
}

/* --- Drag and drop -------------------------------------------------------- */

['dragenter', 'dragover', 'dragleave', 'drop'].forEach((name) => {
  el.area.addEventListener(name, (e) => { e.preventDefault(); e.stopPropagation(); });
});
el.area.addEventListener('dragenter', () => el.area.classList.add('dragging'));
el.area.addEventListener('dragover', () => el.area.classList.add('dragging'));
el.area.addEventListener('dragleave', () => el.area.classList.remove('dragging'));
el.area.addEventListener('drop', (e) => {
  el.area.classList.remove('dragging');
  const file = e.dataTransfer.files && e.dataTransfer.files[0];
  if (file) loadBlob(file, file.name);
});

/* --- Keyboard ------------------------------------------------------------- */

function typingInField(target) {
  return target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
}

document.addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;

  if (e.key === 'Escape') {
    if (!el.prefs.hidden) return closePrefs();
    if (POPOVERS.some((p) => !p.pop.hidden)) return closePopovers('__none__');
    if (ui.tool === 'crop') return setTool('select');
    if (ui.tool === 'emoji') return setTool('select');
    return;
  }

  if (ui.editingText) {
    if (mod && e.key === 'Enter') {
      const textbox = canvas.getActiveObject();
      if (textbox && textbox.exitEditing) textbox.exitEditing();
      setTool('select');
    }
    return;
  }

  if (mod && e.key === ',') { e.preventDefault(); return openPrefs(); }

  if (typingInField(e.target) || !el.prefs.hidden) return;

  if (mod) {
    const key = e.key.toLowerCase();
    if (key === 'c') {
      e.preventDefault();
      if (e.shiftKey) copyAndClose();
      else copyToClipboard();
      return;
    }
    if (key === 's') { e.preventDefault(); saveToDisk(); return; }
    if (key === 'd') { e.preventDefault(); duplicateSelection(); return; }
    if (key === 'z') {
      e.preventDefault();
      if (e.shiftKey) canvas.redo(() => updateHistoryButtons());
      else canvas.undo(() => updateHistoryButtons());
      return;
    }
    if (key === 'a') {
      e.preventDefault();
      setTool('select');
      const all = canvas.getObjects();
      if (all.length) {
        canvas.setActiveObject(new fabric.ActiveSelection(all, { canvas }));
        canvas.requestRenderAll();
      }
      return;
    }
    if (key === 'v') { pasteFromClipboard(); return; }
    if (key === '0') { e.preventDefault(); setZoom(0); return; }
    if (key === '=' || key === '+') { e.preventDefault(); setZoom(ui.zoom + 1); return; }
    if (key === '-') { e.preventDefault(); setZoom(ui.zoom - 1); return; }
    return;
  }

  if (e.key === 'Delete' || e.key === 'Backspace') {
    const active = canvas.getActiveObjects();
    if (active.length) {
      canvas.remove(...active);
      canvas.discardActiveObject();
      canvas.requestRenderAll();
    }
    return;
  }

  const tool = KEYMAP[e.key.toLowerCase()];
  if (tool && hasImage()) setTool(tool);
});

function duplicateSelection() {
  const active = canvas.getActiveObject();
  if (!active) return;
  active.clone((clone) => {
    clone.set({ left: clone.left + 12 * unit(), top: clone.top + 12 * unit(), evented: true });
    if (clone.escTool === 'arrow') attachArrowControls(clone);
    canvas.add(clone);
    canvas.setActiveObject(clone);
    canvas.requestRenderAll();
  }, canvas.extraProps);
}

function pasteFromClipboard() {
  if (!navigator.clipboard || !navigator.clipboard.read) return;
  navigator.clipboard.read().then((items) => {
    for (const item of items) {
      const type = item.types.find((t) => t.startsWith('image/'));
      if (!type) continue;
      item.getType(type).then((blob) => loadBlob(blob, 'Clipboard image'));
      return;
    }
  }).catch((err) => {
    console.warn('Nothing readable in the clipboard', err);
  });
}

/* ==========================================================================
   Boot
   ========================================================================== */

setTheme(settings.theme);
wmToggle.checked = settings.watermark;
document.body.classList.toggle('no-watermark', !settings.watermark);
renderPalette();
renderEmojiPicker();
renderRatioPop();
renderBackdropChips();
renderFontPicker();
syncFontUI();
syncPresetsUI(true);
setPrefsTab('annotate');

// Shortcut labels follow the platform's modifier key.
$('#prefs-open').title = `Preferences — ${MOD},`;
$('#copy').title = `Copy to clipboard — ${MOD}C`;
$('#save').title = `Save to disk — ${MOD}S`;
el.undo.title = `Undo — ${MOD}Z`;
el.redo.title = `Redo — ${isMac ? '⇧⌘Z' : 'Ctrl+Shift+Z'}`;
$('#copy-key').textContent = `${MOD_MENU}C`;
$('#copy-close-key').textContent = `⇧${MOD_MENU}C`;
setTool('arrow');
updateHistoryButtons();
loadFromQueryParam();
