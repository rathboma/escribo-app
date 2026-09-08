// Base outline of the arrow, in "units" — one unit is a pixel on a 900px-wide
// render. createArrow multiplies these by the caller's unit so an arrow keeps
// the same proportions whatever the screenshot's resolution is.
const basePoints = [
  { x: 0, y: 0 },
  { x: 26, y: 2 },
  { x: 25, y: 5 },
  { x: 40, y: 0 },
  { x: 25, y: -5 },
  { x: 26, y: -2 },
  { x: 0, y: 0 }
];

function lerp(start, end, amt) {
  return (1 - amt) * start + amt * end;
}

function scalePoints(unit) {
  return basePoints.map((p) => ({ x: p.x * unit, y: p.y * unit }));
}

function createArrow(left, top, options = {}) {
  const unit = options.unit || 1;
  const fill = options.color || '#FF007F';
  const points = scalePoints(unit);

  const polygon = new fabric.Polygon(points, {
    left: left - 40 * unit,
    top: top - 5 * unit,
    fill,
    escTool: 'arrow',
    originX: 'left',
    strokeWidth: 4 * unit,
    stroke: 'white',
    scaleX: 1,
    scaleY: 1,
    strokeUniform: true,
    transparentCorners: false,
    cornerColor: 'blue'
  });

  polygon.strokeLineJoin = 'round';
  // Serialised with the object (see extraProps) so a reloaded arrow keeps its
  // proportions when its head is dragged again.
  polygon.arrowUnit = unit;

  return attachArrowControls(polygon);
}

/**
 * Replaces the standard scale handles with the single head-drag control.
 * Controls are not part of fabric's JSON, so this has to run again on every
 * arrow that comes back from undo, redo or duplicate.
 */
function attachArrowControls(polygon) {
  polygon.controls = {
    right_arrow_control: new fabric.Control({
      x: 0.51,
      y: 0,
      cursorStyle: 'pointer',
      cornerStyle: 'circle',
      actionHandler: (eventData, transform, x, y) => {
        setArrowHeadPoint(transform.target, x, y);
        return true;
      },
      actionName: 'right_polygon_modify'
    })
  };
  return polygon;
}

function updatePoints(scaleX, target) {
  const unit = target.arrowUnit || 1;
  const original = scalePoints(unit);

  for (let i = 1; i < target.points.length - 1; i++) {
    target.points[i].y = original[i].y * (1 + 0.1 * scaleX);
  }

  const factor = Math.min(scaleX / 4, 1);
  const tip = lerp(original[1].x, 35 * unit, factor);
  target.points[1].x = tip;
  target.points[2].x = tip;
  target.points[4].x = tip;
  target.points[5].x = tip;
}

function setArrowHeadPoint(target, x, y) {
  const anchor = new fabric.Point(target.width * -0.5, 0);
  const absoluteAnchorPoint = fabric.util.transformPoint(anchor, target.calcTransformMatrix());

  const angleRadians = Math.atan2(y - absoluteAnchorPoint.y, x - absoluteAnchorPoint.x);
  const distance = Math.sqrt(
    Math.pow(absoluteAnchorPoint.x - x, 2) + Math.pow(absoluteAnchorPoint.y - y, 2)
  );

  target.set({
    angle: fabric.util.radiansToDegrees(angleRadians),
    scaleX: distance / target.width,
    scaleY: Math.min(3, distance / target.width),
    top: target.top,
    left: target.left
  });
  updatePoints(target.scaleX, target);

  target.setCoords();
}

/**
 * updatePoints reshapes the head by rewriting the polygon's points, but fabric
 * only measures a polygon when it is constructed — so the live object keeps
 * the width/height/pathOffset of its original outline. A copy rebuilt from
 * JSON (undo, redo, duplicate) measures the current points instead, and with
 * originY 'top' the taller box moves the centre, so the arrow lands a few
 * pixels off. Re-measure once an edit is over, holding the rendered geometry
 * still: every point draws at centre + R·S·(p − pathOffset), so the centre
 * must move by R·S·(newOffset − oldOffset), which is zero for this symmetric
 * outline but is compensated for regardless.
 */
function syncArrowGeometry(target) {
  if (!target || typeof target._calcDimensions !== 'function') return;
  const dim = target._calcDimensions();
  const nextOffset = { x: dim.left + dim.width / 2, y: dim.top + dim.height / 2 };
  const same = Math.abs(dim.width - target.width) < 1e-6 &&
    Math.abs(dim.height - target.height) < 1e-6 &&
    Math.abs(nextOffset.x - target.pathOffset.x) < 1e-6 &&
    Math.abs(nextOffset.y - target.pathOffset.y) < 1e-6;
  if (same) return;

  const center = target.getCenterPoint();
  const shift = fabric.util.rotateVector(
    new fabric.Point(
      (nextOffset.x - target.pathOffset.x) * target.scaleX,
      (nextOffset.y - target.pathOffset.y) * target.scaleY
    ),
    fabric.util.degreesToRadians(target.angle)
  );

  target.set({ width: dim.width, height: dim.height });
  target.pathOffset = nextOffset;
  target.setPositionByOrigin(center.add(shift), 'center', 'center');
  target.setCoords();
}

export { createArrow, attachArrowControls, setArrowHeadPoint, syncArrowGeometry };
