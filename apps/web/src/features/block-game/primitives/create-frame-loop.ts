import { type Accessor, createEffect, on, onCleanup } from 'solid-js';

/** Longest step a frame may simulate; a backgrounded tab resumes without a jump. */
const MAX_FRAME_MS = 50;

/**
 * Call `onFrame` with the elapsed milliseconds on every animation frame while
 * `running` is true. The frame clock is an external system, so it lives in an
 * effect and stops with its owner.
 */
export function createFrameLoop(options: {
  running: Accessor<boolean>;
  onFrame: (dtMs: number, now: number) => void;
}) {
  createEffect(
    on(options.running, (running) => {
      if (!running) return;
      const schedule =
        typeof requestAnimationFrame === 'function'
          ? (callback: FrameRequestCallback) => requestAnimationFrame(callback)
          : (callback: FrameRequestCallback) =>
              setTimeout(
                () => callback(performance.now()),
                16
              ) as unknown as number;
      const cancel =
        typeof cancelAnimationFrame === 'function'
          ? (handle: number) => cancelAnimationFrame(handle)
          : (handle: number) => clearTimeout(handle);
      let last = performance.now();
      let handle = schedule(function frame(now) {
        const dt = Math.min(MAX_FRAME_MS, Math.max(0, now - last));
        last = now;
        options.onFrame(dt, now);
        handle = schedule(frame);
      });
      onCleanup(() => cancel(handle));
    })
  );
}
