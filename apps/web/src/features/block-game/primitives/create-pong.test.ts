import { createRoot } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { readGameLog } from '../core/game-document';
import { pongRules } from '../core/games/pong';
import { createRandom } from '../core/random';
import { createLinkedRoomSources } from '../tests/linked-room-sources';
import { createGameRoom } from './create-game-room';
import { createPong } from './create-pong';
import { createTurnMatch } from './create-turn-match';

const ANN = 'macro|ann@macro.com';
const BOB = 'macro|bob@macro.com';

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  vi.useRealTimers();
});

function fakeFrames() {
  vi.useFakeTimers({
    toFake: [
      'setTimeout',
      'clearTimeout',
      'setInterval',
      'clearInterval',
      'Date',
      'performance',
      'requestAnimationFrame',
      'cancelAnimationFrame',
    ],
  });
}

function setupMatch() {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    const linked = createLinkedRoomSources([ANN, BOB]);
    const client = (index: 0 | 1, userId: string) => {
      const room = createGameRoom({
        source: linked.sources[index],
        userId: () => userId,
        canEdit: () => true,
        requestedKind: () => (index === 0 ? 'pong' : undefined),
      });
      const match = createTurnMatch(room, pongRules);
      const pong = createPong(room, match, { random: createRandom(3) });
      return { room, match, pong };
    };
    return { linked, ann: client(0, ANN), bob: client(1, BOB) };
  });
}

describe('createPong', () => {
  it('runs the ball on the first seat and streams it to the second', async () => {
    fakeFrames();
    const { linked, ann, bob } = setupMatch();
    ann.match.join();
    bob.match.join();
    expect(ann.match.phase().t).toBe('playing');
    expect(bob.pong.controlledSeat()).toBe(1);

    // Bob holds his paddle at the top; the first serve heads his way.
    bob.pong.keyDown('ArrowUp');
    await vi.advanceTimersByTimeAsync(1_500);
    expect(ann.pong.court().ball).toBeDefined();
    expect(linked.presence(0)?.court?.ball).toBeDefined();
    expect(bob.pong.court().ball).toBeDefined();
    // The host sees Bob's paddle move through his presence.
    expect(ann.pong.court().paddles[1]).toBeLessThan(20);

    await vi.advanceTimersByTimeAsync(3_000);
    const points = readGameLog(linked.docs[1]).filter(
      (entry) => entry.t === 'move'
    );
    expect(points.length).toBeGreaterThan(0);
    // Only the first seat records points, and both clients agree on them.
    expect(points.every((entry) => entry.by === ANN)).toBe(true);
    const phase = bob.match.phase();
    expect(phase.t === 'playing' && phase.state.scores[0]).toBeGreaterThan(0);

    // Once the match ends, no client keeps drawing a ball.
    ann.match.forfeit();
    expect(bob.match.phase().t).toBe('over');
    expect(bob.pong.court().ball).toBeUndefined();
    expect(ann.pong.court().ball).toBeUndefined();
  });

  it('plays a local practice game against the computer without writing', async () => {
    fakeFrames();
    const { linked, ann } = setupMatch();
    expect(ann.pong.canPractice()).toBe(true);
    ann.pong.startPractice();
    expect(ann.pong.controlledSeat()).toBe(0);
    // Ann never moves; the computer returns everything until someone wins.
    await vi.advanceTimersByTimeAsync(120_000);
    const practice = ann.pong.practice();
    expect(practice?.winner).toBeDefined();
    expect(Math.max(...(practice?.scores ?? [0]))).toBe(7);
    expect(readGameLog(linked.docs[0])).toEqual([]);
  });
});
