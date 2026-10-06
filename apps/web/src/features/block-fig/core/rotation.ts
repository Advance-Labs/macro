/** Rotating a layer by dragging beyond a corner of its selection box. */

import type { Rect } from '@core/fig-engine/types';
import type { Point } from './camera';

/** Rotate a page point by a change in the inspector's counter-clockwise degrees. */
export function rotatePoint(p: Point, center: Point, degrees: number): Point {
  const angle = (-degrees * Math.PI) / 180;
  const x = p.x - center.x;
  const y = p.y - center.y;
  return {
    x: center.x + x * Math.cos(angle) - y * Math.sin(angle),
    y: center.y + x * Math.sin(angle) + y * Math.cos(angle),
  };
}

/** Bounds of points in page coordinates. */
export function pointBounds(points: Point[]): Rect {
  const x = Math.min(...points.map((p) => p.x));
  const y = Math.min(...points.map((p) => p.y));
  return {
    x,
    y,
    w: Math.max(...points.map((p) => p.x)) - x,
    h: Math.max(...points.map((p) => p.y)) - y,
  };
}

/** Degrees as Figma shows them (counter-clockwise), snapped with ⇧. */
export function rotationFor(
  startRotation: number,
  startAngle: number,
  angle: number,
  snap: boolean
): number {
  // Screen angles grow clockwise (y points down).
  let r = startRotation - ((angle - startAngle) * 180) / Math.PI;
  r = ((((r + 180) % 360) + 360) % 360) - 180;
  if (snap) r = Math.round(r / 15) * 15;
  return Math.round(r * 100) / 100;
}

const isHalfTurn = (rotation: number) =>
  Math.abs(Math.abs(rotation) - 180) < 0.01;

/**
 * Whether a layer at `rotation` (as the panel shows it) lies along the
 * page's axes, so its selection box is the layer itself: unrotated, or
 * turned half way (which is also how a vertically flipped layer reads).
 */
export function isAxisAligned(rotation: number): boolean {
  return Math.abs(rotation) < 0.01 || isHalfTurn(rotation);
}

type Box = { x: number; y: number; w: number; h: number };

/**
 * The panel X and Y of an axis-aligned layer whose page bounds `from`
 * become `to`: the panel point is the top left corner, or the bottom right
 * one when the layer is turned half way.
 */
export function resizedOrigin(
  info: { x: number; y: number; rotation: number },
  from: Box,
  to: Box
): { x: number; y: number } {
  if (isHalfTurn(info.rotation))
    return {
      x: info.x + (to.x + to.w - (from.x + from.w)),
      y: info.y + (to.y + to.h - (from.y + from.h)),
    };
  return { x: info.x + (to.x - from.x), y: info.y + (to.y - from.y) };
}
