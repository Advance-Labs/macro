import { createRoot, createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createChangesNarrow } from './create-changes-narrow';

const environment = vi.hoisted(() => ({
  width: (): number | undefined => undefined,
  touch: false,
}));
vi.mock('@core/mobile/isTouchDevice', () => ({
  isTouchDevice: () => environment.touch,
}));
vi.mock('@solid-primitives/resize-observer', () => ({
  createElementSize: () => ({
    get width() {
      return environment.width();
    },
  }),
}));
afterEach(() => {
  environment.width = () => undefined;
  environment.touch = false;
});

describe('createChangesNarrow', () => {
  it('matches the measured container width and responds to resizing', () => {
    createRoot((dispose) => {
      const [width, setWidth] = createSignal(721);
      environment.width = width;
      const narrow = createChangesNarrow(() => undefined);
      expect(narrow()).toBe(false);
      setWidth(720);
      expect(narrow()).toBe(true);
      setWidth(400);
      expect(narrow()).toBe(true);
      setWidth(1200);
      expect(narrow()).toBe(false);
      dispose();
    });
  });

  it.each([undefined, 0])(
    'does not treat unmeasured width %s as narrow',
    (width) => {
      createRoot((dispose) => {
        environment.width = () => width;
        expect(createChangesNarrow(() => undefined)()).toBe(false);
        dispose();
      });
    }
  );

  it('retains the touch takeover even with a wide measured container', () => {
    createRoot((dispose) => {
      environment.width = () => 1200;
      environment.touch = true;
      expect(createChangesNarrow(() => undefined)()).toBe(true);
      dispose();
    });
  });
});
