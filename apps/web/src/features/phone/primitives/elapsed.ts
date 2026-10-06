import { type Accessor, createSignal, onCleanup } from 'solid-js';

/**
 * Milliseconds since `since`, refreshed every `intervalMs` while the owner
 * lives; `null` while `since` is `null`.
 */
export function createElapsed(
  since: Accessor<number | null>,
  options: { now?: () => number; intervalMs?: number } = {}
): Accessor<number | null> {
  const now = options.now ?? Date.now;
  const [tick, setTick] = createSignal(now());
  const handle = setInterval(() => setTick(now()), options.intervalMs ?? 1000);
  onCleanup(() => clearInterval(handle));
  return () => {
    const start = since();
    return start === null ? null : Math.max(0, tick() - start);
  };
}
