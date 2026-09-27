import { createRoot, createSignal } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GameStatus } from '../core/status';
import type { RoundResult } from '../core/turn-match';
import {
  createRoundReporter,
  createStatusPublisher,
} from './create-room-reporting';

const ANN = 'macro|ann@macro.com';
const BOB = 'macro|bob@macro.com';
const CAT = 'macro|cat@macro.com';

const result = (round: number, winners: string[] = [ANN]): RoundResult => ({
  round,
  players: [ANN, BOB],
  winners,
});

let dispose: (() => void) | undefined;
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  dispose?.();
  dispose = undefined;
  vi.useRealTimers();
});

describe('createStatusPublisher', () => {
  it('publishes a status once it settles, and retries after a failure', async () => {
    const published: GameStatus[] = [];
    let succeed = false;
    const [status, setStatus] = createSignal<GameStatus>('waiting');
    createRoot((disposeRoot) => {
      dispose = disposeRoot;
      createStatusPublisher({
        status,
        enabled: () => true,
        leads: () => true,
        publish: async (next) => {
          published.push(next);
          return succeed;
        },
      });
    });

    setStatus('in_progress');
    setStatus('waiting');
    await vi.advanceTimersByTimeAsync(2_000);
    // Quick flips collapse into the value that held.
    expect(published).toEqual(['waiting']);

    succeed = true;
    setStatus('in_progress');
    await vi.advanceTimersByTimeAsync(2_000);
    setStatus('waiting');
    await vi.advanceTimersByTimeAsync(2_000);
    expect(published).toEqual(['waiting', 'in_progress', 'waiting']);
  });

  it('lets the client that caused a change publish it first', async () => {
    const publish = vi.fn(async (_status: GameStatus) => true);
    createRoot((disposeRoot) => {
      dispose = disposeRoot;
      createStatusPublisher({
        status: () => 'finished',
        enabled: () => true,
        leads: () => false,
        publish,
      });
    });
    await vi.advanceTimersByTimeAsync(2_000);
    expect(publish).not.toHaveBeenCalled();
    // Other editors follow up in case the leading client went offline.
    await vi.advanceTimersByTimeAsync(5_000);
    expect(publish).toHaveBeenCalledWith('finished');
  });

  it('stays quiet while disabled', async () => {
    const publish = vi.fn(async () => true);
    createRoot((disposeRoot) => {
      dispose = disposeRoot;
      createStatusPublisher({
        status: () => 'finished',
        enabled: () => false,
        leads: () => true,
        publish,
      });
    });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(publish).not.toHaveBeenCalled();
  });
});

describe('createRoundReporter', () => {
  it('reports the latest earlier round, then each new round once, from players only', () => {
    const reported: number[] = [];
    const [results, setResults] = createSignal<RoundResult[]>([
      result(0),
      result(1),
    ]);
    const [userId, setUserId] = createSignal<string | undefined>(ANN);
    createRoot((disposeRoot) => {
      dispose = disposeRoot;
      createRoundReporter({
        results,
        userId,
        enabled: () => true,
        report: async (round) => {
          reported.push(round.round);
        },
      });
    });
    expect(reported).toEqual([1]);

    setResults([result(0), result(1), result(2)]);
    setResults([result(0), result(1), result(2)]);
    expect(reported).toEqual([1, 2]);

    setUserId(CAT);
    setResults([result(0), result(1), result(2), result(3)]);
    expect(reported).toEqual([1, 2]);
  });
});
