import type { PendingTile, TileRequest } from '@core/fig-engine/client';
import { expect, type Page, test } from '@playwright/test';

declare global {
  interface Window {
    rotationProbe: {
      requests: TileRequest[];
      snapshots: number;
      blockPage: boolean;
      blockSnapshots: boolean;
      release: (() => void)[];
    };
  }
}

async function setup(page: Page) {
  await page.goto('/?new');
  await page.getByTestId('fig-tool-rectangle').click();
  const canvas = page.getByTestId('fig-canvas');
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + 350, box.y + 300);
  await page.mouse.down();
  await page.mouse.move(box.x + 650, box.y + 500, { steps: 5 });
  await page.mouse.up();
  await expect(page.getByTestId('fig-field-w')).toHaveValue('300');
  await expect.poll(() => mismatches(page, 0)).toBe(0);
  // Let the compositor finish its one-time margin/overview prefetch.
  await page.waitForTimeout(400);
  await page.evaluate(() => {
    const engine = window.figFixture.engine()!;
    const render = engine.render.bind(engine);
    const probe = {
      requests: [] as TileRequest[],
      snapshots: 0,
      blockPage: false,
      blockSnapshots: false,
      release: [] as (() => void)[],
    };
    window.rotationProbe = probe;
    engine.render = (tile): PendingTile => {
      probe.requests.push(tile);
      const job = render(tile);
      return {
        id: job.id,
        promise: job.promise.then((result) => {
          if (tile.layers) probe.snapshots++;
          if (
            result &&
            ((probe.blockPage && !tile.layers) ||
              (probe.blockSnapshots && tile.layers))
          )
            return new Promise((resolve) =>
              probe.release.push(() => resolve(result))
            );
          return result;
        }),
      };
    };
  });
  return box;
}

/** Whole-shape pixel check on both sides of the x/y=512 tile boundaries. */
async function mismatches(page: Page, clockwise: number) {
  return page
    .getByTestId('fig-canvas')
    .locator('canvas')
    .first()
    .evaluate((node, degrees) => {
      const canvas = node as HTMLCanvasElement;
      const ctx = canvas.getContext('2d')!;
      const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
      const rad = (degrees * Math.PI) / 180;
      let wrong = 0;
      for (let y = 200; y < 600; y += 10) {
        for (let x = 300; x < 700; x += 10) {
          const dx = (x - 500) * Math.cos(rad) + (y - 400) * Math.sin(rad);
          const dy = -(x - 500) * Math.sin(rad) + (y - 400) * Math.cos(rad);
          if (
            Math.abs(Math.abs(dx) - 150) < 4 ||
            Math.abs(Math.abs(dy) - 100) < 4
          )
            continue;
          const expected = Math.abs(dx) < 150 && Math.abs(dy) < 100 ? 217 : 245;
          const at = (y * canvas.width + x) * 4;
          if (Math.abs(pixels[at] - expected) > 2) wrong++;
        }
      }
      return wrong;
    }, clockwise);
}

test('rotates across tile seams without rasterizing each frame or exposing partial landing tiles', async ({
  page,
}, testInfo) => {
  const box = await setup(page);
  await page.mouse.move(box.x + 660, box.y + 290);
  await page.mouse.down();
  await expect
    .poll(() => page.evaluate(() => window.rotationProbe.snapshots))
    .toBe(3);
  const requests = await page.evaluate(
    () => window.rotationProbe.requests.length
  );
  for (const angle of [15, 30, 45, 60, 75, 90]) {
    const rad = (angle * Math.PI) / 180;
    await page.mouse.move(
      box.x + 500 + 160 * Math.cos(rad) + 110 * Math.sin(rad),
      box.y + 400 + 160 * Math.sin(rad) - 110 * Math.cos(rad)
    );
    await expect(page.getByTestId('fig-field-rotation')).toHaveValue(
      String(-angle)
    );
    await expect.poll(() => mismatches(page, angle)).toBe(0);
  }
  expect(await page.evaluate(() => window.rotationProbe.requests.length)).toBe(
    requests
  );
  await page.screenshot({
    path: testInfo.outputPath('rotation-in-flight.png'),
  });
  await page.evaluate(() => {
    window.rotationProbe.blockPage = true;
  });
  await page.mouse.up();
  await expect
    .poll(() => page.evaluate(() => window.rotationProbe.release.length))
    .toBeGreaterThan(1);
  await page.evaluate(() => {
    window.rotationProbe.release.shift()?.();
  });
  await expect.poll(() => mismatches(page, 90)).toBe(0);
  await page.evaluate(() => {
    window.rotationProbe.blockPage = false;
    for (const release of window.rotationProbe.release.splice(0)) release();
  });
  await expect.poll(() => mismatches(page, 90)).toBe(0);
  await page.getByTestId('fig-canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(page.getByTestId('fig-field-rotation')).toHaveValue('0');
  await expect.poll(() => mismatches(page, 0)).toBe(0);
});

test('commits the last angle when released before the snapshots arrive', async ({
  page,
}) => {
  const box = await setup(page);
  await page.evaluate(() => {
    window.rotationProbe.blockSnapshots = true;
  });
  await page.mouse.move(box.x + 660, box.y + 290);
  await page.mouse.down();
  await page.mouse.move(
    box.x + 500 + 270 / Math.sqrt(2),
    box.y + 400 + 50 / Math.sqrt(2)
  );
  await page.mouse.up();
  await expect
    .poll(() => page.evaluate(() => window.rotationProbe.release.length))
    .toBe(3);
  await page.evaluate(() => {
    window.rotationProbe.blockSnapshots = false;
    for (const release of window.rotationProbe.release.splice(0)) release();
  });
  await expect(page.getByTestId('fig-field-rotation')).toHaveValue('-45');
  await expect.poll(() => mismatches(page, 45)).toBe(0);
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const engine = window.figFixture.engine()!;
        const [row] = await engine.layers(0);
        return (await engine.nodeInfo(0, row.id)).rotation;
      })
    )
    .toBeCloseTo(-45, 1);
});

test('keeps the rotating layer below foreground paint', async ({ page }) => {
  const box = await setup(page);
  await page.getByTestId('fig-tool-rectangle').click();
  await page.mouse.move(box.x + 540, box.y + 360);
  await page.mouse.down();
  await page.mouse.move(box.x + 620, box.y + 450);
  await page.mouse.up();
  await page.getByTestId('fig-fill-0-hex').fill('0000FF');
  await page.getByTestId('fig-fill-0-hex').press('Enter');
  await page
    .getByTestId('fig-layer-row')
    .filter({ hasText: 'Rectangle 1' })
    .click();
  await page.getByTestId('fig-canvas').focus();
  const colors = () =>
    page
      .getByTestId('fig-canvas')
      .locator('canvas')
      .first()
      .evaluate((node) => {
        const ctx = (node as HTMLCanvasElement).getContext('2d')!;
        return [
          [580, 400],
          [500, 540],
          [640, 400],
        ].map(([x, y]) =>
          Array.from(ctx.getImageData(x, y, 1, 1).data).slice(0, 3)
        );
      });
  await expect.poll(async () => (await colors())[0]).toEqual([0, 0, 255]);
  await page.mouse.move(box.x + 660, box.y + 290);
  await page.mouse.down();
  await page.mouse.move(box.x + 610, box.y + 560);
  await expect(page.getByTestId('fig-field-rotation')).toHaveValue('-90');
  await expect.poll(colors).toEqual([
    [0, 0, 255],
    [217, 217, 217],
    [245, 245, 245],
  ]);
  await page.mouse.up();
  await expect.poll(colors).toEqual([
    [0, 0, 255],
    [217, 217, 217],
    [245, 245, 245],
  ]);
});
