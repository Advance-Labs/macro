import {
  type Accessor,
  createEffect,
  createMemo,
  on,
  onCleanup,
} from 'solid-js';
import type { GameStatus } from '../core/status';
import type { RoundResult } from '../core/turn-match';

/** How long a status must hold before it is published, to absorb quick flips. */
const STATUS_SETTLE_MS = 1_500;
/** Other editors publish later, only in case the leading client could not. */
const STATUS_FALLBACK_MS = 6_000;

/**
 * Publish the room's status whenever it settles on a new value. Every editor
 * derives the same value; the client that caused the change publishes first,
 * and the rest follow up later in case it went offline.
 */
export function createStatusPublisher(options: {
  status: Accessor<GameStatus | undefined>;
  enabled: Accessor<boolean>;
  /** Whether this client caused the latest change. */
  leads: Accessor<boolean>;
  /** Resolves true once the status is stored. */
  publish: (status: GameStatus) => Promise<boolean>;
}) {
  let published: GameStatus | undefined;
  let settled: GameStatus | undefined;
  let queue: Promise<void> = Promise.resolve();

  // Writes run one at a time and each sends the latest settled value, so a
  // slow earlier write can never land after a newer one.
  function publishSettled() {
    queue = queue.then(async () => {
      const status = settled;
      if (!status || status === published) return;
      try {
        if (await options.publish(status)) published = status;
      } catch (cause) {
        // The next change retries; a failure must not stall later writes.
        console.error('Failed to publish game status', cause);
      }
    });
  }

  // Memoized so the settle timer restarts only when the value changes, not
  // on every move or clock tick behind it.
  const current = createMemo(() =>
    options.enabled() ? options.status() : undefined
  );
  createEffect(
    on(current, (status) => {
      if (!status || status === published) return;
      const delay = options.leads() ? STATUS_SETTLE_MS : STATUS_FALLBACK_MS;
      const timer = setTimeout(() => {
        settled = status;
        publishSettled();
      }, delay);
      onCleanup(() => clearTimeout(timer));
    })
  );
}

/**
 * Report each finished round once per visit, from its players' clients. Rounds
 * that finished before this visit were reported by whoever watched them, so
 * only the latest is retried; the server keeps the first report of a round.
 */
export function createRoundReporter(options: {
  results: Accessor<RoundResult[]>;
  userId: Accessor<string | undefined>;
  enabled: Accessor<boolean>;
  report: (result: RoundResult) => Promise<void>;
}) {
  const handled = new Set<number>();
  let primed = false;

  createEffect(
    on(
      () => (options.enabled() ? options.results() : undefined),
      (results) => {
        if (!results) return;
        if (!primed) {
          for (const result of results.slice(0, -1)) handled.add(result.round);
          primed = true;
        }
        const userId = options.userId();
        for (const result of results) {
          if (handled.has(result.round)) continue;
          handled.add(result.round);
          if (userId && result.players.includes(userId))
            void options.report(result);
        }
      }
    )
  );
}
