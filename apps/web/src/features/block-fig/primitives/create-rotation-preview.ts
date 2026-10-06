/**
 * A rotation gesture composites complete snapshots, never independently
 * arriving page tiles. The layer is rasterized once, then the canvas turns
 * that bitmap at pointer pace. The surrounding paint keeps its stacking
 * order and the clipping ancestors stay fixed.
 */
import type { FigEngine, PendingTile } from '@core/fig-engine/client';
import type { LiftPlan, Rect } from '@core/fig-engine/types';
import type { Camera, Point } from '../core/camera';
import { liftParts } from '../core/lift';

/** Bound each snapshot's allocation, including very large/offscreen layers. */
const MAX_SIDE = 4096;
const MAX_PIXELS = 4 * 1024 * 1024;
const PRIORITY = -4_000_000;

export function createRotationPreview(options: {
  engine: Pick<FigEngine, 'render' | 'cancel'>;
  plan: LiftPlan;
  page: number;
  outline: boolean;
  center: Point;
  scale: number;
}) {
  const { engine, plan, center } = options;
  if (plan.refused || plan.runs.length !== 1) return undefined;
  const run = plan.runs[0];
  let clips: Path2D[];
  try {
    clips = run.clips.map((path) => new Path2D(path));
  } catch {
    return undefined;
  }
  const b = run.bounds;
  const radius = Math.hypot(
    Math.max(Math.abs(b.x - center.x), Math.abs(b.x + b.w - center.x)),
    Math.max(Math.abs(b.y - center.y), Math.abs(b.y + b.h - center.y))
  );
  if (!(radius > 0) || !Number.isFinite(radius)) return undefined;
  // Leave pixels for antialiasing without letting the allocation grow past
  // the cap when padding is added. The sweep includes effects and strokes.
  const scale = Math.min(
    options.scale,
    (MAX_SIDE - 8) / (radius * 2),
    (Math.sqrt(MAX_PIXELS) - 8) / (radius * 2)
  );
  if (!(scale > 0)) return undefined;
  const pad = 2 / scale;
  const bounds: Rect = {
    x: center.x - radius - pad,
    y: center.y - radius - pad,
    w: 2 * (radius + pad),
    h: 2 * (radius + pad),
  };
  const source: Rect = {
    x: b.x - pad,
    y: b.y - pad,
    w: b.w + 2 * pad,
    h: b.h + 2 * pad,
  };
  const parts = liftParts(plan);
  const pending: PendingTile[] = [];
  const bitmaps: ImageBitmap[] = [];
  let disposed = false;
  let complete = false;
  let degrees = 0;
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    engine.cancel(pending.map((p) => p.id));
    for (const bitmap of bitmaps) bitmap.close();
  };
  const render = async (rect: Rect, layers: unknown) => {
    const request = engine.render({
      page: options.page,
      outline: options.outline,
      x: rect.x,
      y: rect.y,
      scale,
      width: Math.ceil(rect.w * scale),
      height: Math.ceil(rect.h * scale),
      priority: PRIORITY,
      layers: JSON.stringify(layers),
    });
    pending.push(request);
    const result = await request.promise;
    if (!result) throw new Error('Rotation snapshot cancelled');
    if (disposed) {
      result.bitmap.close();
      throw new Error('Rotation snapshot disposed');
    }
    bitmaps.push(result.bitmap);
    return result.bitmap;
  };
  let snapshots: ImageBitmap[] = [];
  const ready = (async () => {
    try {
      snapshots = await Promise.all([
        render(bounds, parts.below),
        render(source, parts.runs[0]),
        render(bounds, parts.above[0]),
      ]);
      complete = !disposed;
      return complete;
    } catch {
      dispose();
      return false;
    }
  })();
  return {
    bounds,
    ready,
    dispose,
    rotate(angle: number) {
      degrees = angle;
    },
    draw(ctx: CanvasRenderingContext2D, view: { camera: Camera; dpr: number }) {
      if (!complete || disposed) return;
      const unit = view.camera.zoom * view.dpr;
      ctx.save();
      ctx.setTransform(
        unit,
        0,
        0,
        unit,
        -view.camera.x * unit,
        -view.camera.y * unit
      );
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = 'low';
      const blit = (bitmap: ImageBitmap, rect: Rect) =>
        ctx.drawImage(
          bitmap,
          rect.x,
          rect.y,
          bitmap.width / scale,
          bitmap.height / scale
        );
      blit(snapshots[0], bounds);
      ctx.save();
      for (const clip of clips) ctx.clip(clip);
      ctx.translate(center.x, center.y);
      ctx.rotate((-degrees * Math.PI) / 180);
      ctx.translate(-center.x, -center.y);
      blit(snapshots[1], source);
      ctx.restore();
      blit(snapshots[2], bounds);
      ctx.restore();
    },
  };
}

export type RotationPreview = NonNullable<
  ReturnType<typeof createRotationPreview>
>;
