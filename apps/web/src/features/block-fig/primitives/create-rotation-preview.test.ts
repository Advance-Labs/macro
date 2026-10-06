import type { TileRequest, TileResult } from '@core/fig-engine/client';
import type { LiftPlan } from '@core/fig-engine/types';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createRotationPreview } from './create-rotation-preview';

const plan: LiftPlan = {
  runs: [
    {
      ids: ['1:2'],
      bounds: { x: 400, y: 400, w: 200, h: 100 },
      origin: { x: 400, y: 400 },
      clips: ['M0 0H1000V1000H0Z'],
      above: [],
    },
  ],
};

function fixture() {
  vi.stubGlobal('Path2D', class {});
  const requests: {
    tile: TileRequest;
    resolve: (result: TileResult | null) => void;
    bitmap: ImageBitmap;
  }[] = [];
  const engine = {
    render: vi.fn((tile: TileRequest) => ({
      id: requests.length,
      promise: new Promise<TileResult | null>((resolve) =>
        requests.push({
          tile,
          resolve,
          bitmap: {
            width: tile.width,
            height: tile.height,
            close: vi.fn(),
          } as unknown as ImageBitmap,
        })
      ),
    })),
    cancel: vi.fn(),
  };
  const preview = createRotationPreview({
    engine,
    plan,
    page: 0,
    outline: false,
    center: { x: 500, y: 450 },
    scale: 2,
  })!;
  const ctx = {
    save: vi.fn(),
    restore: vi.fn(),
    setTransform: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    clip: vi.fn(),
    drawImage: vi.fn(),
  };
  const draw = () =>
    preview.draw(ctx as unknown as CanvasRenderingContext2D, {
      camera: { x: 0, y: 0, zoom: 1 },
      dpr: 1,
    });
  const finish = (index: number) => {
    const r = requests[index];
    r.resolve({ bitmap: r.bitmap, millis: 1 });
  };
  return { preview, requests, engine, ctx, draw, finish };
}

afterEach(() => vi.unstubAllGlobals());

describe('rotation snapshots', () => {
  it('publishes complete layers together and turns them without more rasterization', async () => {
    const { preview, requests, engine, ctx, draw, finish } = fixture();
    finish(1);
    finish(0);
    await Promise.resolve();
    draw();
    expect(ctx.drawImage).not.toHaveBeenCalled();
    finish(2);
    expect(await preview.ready).toBe(true);
    for (const angle of [10, 45, 90]) {
      preview.rotate(angle);
      draw();
      expect(ctx.rotate).toHaveBeenLastCalledWith((-angle * Math.PI) / 180);
    }
    expect(engine.render).toHaveBeenCalledTimes(3);
    expect(ctx.drawImage.mock.calls.slice(0, 3).map((args) => args[0])).toEqual(
      requests.map((r) => r.bitmap)
    );
    expect(ctx.clip).toHaveBeenCalledTimes(3);
    preview.dispose();
    for (const r of requests) expect(r.bitmap.close).toHaveBeenCalledOnce();
  });

  it('disposes late results when preparation is cancelled', async () => {
    const { preview, requests, ctx, draw, finish } = fixture();
    finish(0);
    await Promise.resolve();
    preview.dispose();
    finish(1);
    finish(2);
    expect(await preview.ready).toBe(false);
    draw();
    expect(ctx.drawImage).not.toHaveBeenCalled();
    for (const r of requests) expect(r.bitmap.close).toHaveBeenCalledOnce();
  });

  it('leaves unsupported compositing to the normal renderer', () => {
    const { preview, engine } = fixture();
    expect(
      createRotationPreview({
        engine,
        plan: { runs: [], refused: 'blend mode' },
        page: 0,
        outline: false,
        center: { x: 0, y: 0 },
        scale: 1,
      })
    ).toBeUndefined();
    preview.dispose();
  });
});
