import { createEffect, createMemo, createSignal, on } from 'solid-js';
import type { GamePeer } from '../context/game-room-source';
import {
  centeredPaddles,
  clampPaddle,
  computerPaddle,
  extrapolateBall,
  PONG_PADDLE_SPEED,
  PONG_POINTS_TO_WIN,
  type PongBall,
  type PongCourtSnapshot,
  type PongPoint,
  type PongScore,
  type PongSeat,
  serveBall,
  stepPongBall,
} from '../core/games/pong';
import { createRandom, type Random, randomSeed } from '../core/random';
import { createFrameLoop } from './create-frame-loop';
import type { GameRoom } from './create-game-room';
import type { TurnMatchState } from './create-turn-match';
import { createHeldKeys, DOWN_KEYS, UP_KEYS } from './held-keys';

/** Snapshots and paddle updates go out about fifteen times a second. */
const SEND_EVERY_MS = 66;
/** A still paddle is re-sent now and then so a rejoining host picks it up. */
const RESEND_EVERY_MS = 1_000;
const SERVE_DELAY_MS = 1_200;
/** Extrapolate at most this far past the host's last update. */
const MAX_EXTRAPOLATION_MS = 300;

export type PongCourtView = {
  ball: PongBall | undefined;
  paddles: readonly [number, number];
};

type Practice = {
  scores: [number, number];
  winner: PongSeat | undefined;
};

function peerFor(peers: GamePeer[], userId: string | undefined) {
  return userId ? peers.filter((peer) => peer.userId === userId) : [];
}

/**
 * Real-time play for a Pong room. The first seat's client runs the ball and
 * records each point in the room log; the second seat streams its paddle;
 * everyone else draws the first seat's snapshots, extrapolated between
 * updates. Alone in the lobby, a player can practice against the computer.
 */
export function createPong(
  room: GameRoom,
  match: TurnMatchState<PongScore, PongPoint>,
  options: { random?: Random; now?: () => number } = {}
) {
  const random = options.random ?? createRandom(randomSeed());
  const now = options.now ?? (() => performance.now());
  const keys = createHeldKeys();
  let pointer: number | undefined;

  const phase = match.phase;
  const round = () => {
    const current = phase();
    return current.t === 'lobby' ? -1 : current.round;
  };
  const playing = () => phase().t === 'playing';
  const seat = () => match.mySeat();

  const hostSnapshot = createMemo((): PongCourtSnapshot | undefined => {
    const snapshots = peerFor(room.peers(), match.match().seats[0]).flatMap(
      (peer) => {
        const snapshot = peer.presence.court;
        return snapshot && snapshot.round === round() ? [snapshot] : [];
      }
    );
    return snapshots.sort((a, b) => b.seq - a.seq)[0];
  });
  const guestPaddle = createMemo(() =>
    peerFor(room.peers(), match.match().seats[1])
      .map((peer) => peer.presence.paddle)
      .find((paddle) => paddle !== undefined)
  );

  const [court, setCourt] = createSignal<PongCourtView>({
    ball: undefined,
    paddles: centeredPaddles(),
  });
  const [practice, setPractice] = createSignal<Practice>();

  // Mutable simulation state, published to `court` once per frame.
  let own = centeredPaddles()[0];
  let other = centeredPaddles()[1];
  let ball: PongBall | undefined;
  let serveAt = 0;
  let serveToward: PongSeat = 1;
  let lastSent = Number.NEGATIVE_INFINITY;
  let sentPaddle: number | undefined;
  let seq = 0;
  let seenSeq: number | undefined;
  let receivedAt = 0;

  const resetCourt = (toward: PongSeat) => {
    own = centeredPaddles()[0];
    other = centeredPaddles()[1];
    ball = undefined;
    serveToward = toward;
    serveAt = now() + SERVE_DELAY_MS;
    setCourt({ ball: undefined, paddles: centeredPaddles() });
  };

  const moveOwnPaddle = (dtSec: number) => {
    const axis = keys.axis(UP_KEYS, DOWN_KEYS);
    if (axis !== 0) {
      pointer = undefined;
      own = clampPaddle(own + axis * PONG_PADDLE_SPEED * dtSec);
    } else if (pointer !== undefined) {
      own = clampPaddle(pointer);
    }
  };

  /** Runs the ball; returns the seat that scored, if any. */
  const runBall = (t: number, dtSec: number, paddles: [number, number]) => {
    if (!ball) {
      if (t >= serveAt) ball = serveBall(serveToward, random);
      return undefined;
    }
    const step = stepPongBall(ball, paddles, dtSec);
    ball = step.ball;
    if (step.scorer === undefined) return undefined;
    // The player who lost the point receives the next serve.
    serveToward = step.scorer === 0 ? 1 : 0;
    ball = undefined;
    serveAt = t + SERVE_DELAY_MS;
    return step.scorer;
  };

  const hostFrame = (t: number, dtSec: number) => {
    moveOwnPaddle(dtSec);
    other = guestPaddle() ?? other;
    const paddles: [number, number] = [own, other];
    const scorer = runBall(t, dtSec, paddles);
    if (scorer !== undefined) match.move({ scorer });
    setCourt({ ball, paddles });
    if (t - lastSent >= SEND_EVERY_MS) {
      lastSent = t;
      seq += 1;
      room.setPresence({
        activity: 'playing',
        court: { round: round(), seq, ball, paddles },
      });
    }
  };

  const followerFrame = (t: number, dtSec: number, isGuest: boolean) => {
    if (isGuest) {
      moveOwnPaddle(dtSec);
      const moved =
        sentPaddle === undefined || Math.abs(own - sentPaddle) > 0.05;
      if (
        (moved && t - lastSent >= SEND_EVERY_MS) ||
        t - lastSent >= RESEND_EVERY_MS
      ) {
        lastSent = t;
        sentPaddle = own;
        room.setPresence({ activity: 'playing', paddle: own });
      }
    }
    const snapshot = hostSnapshot();
    if (!snapshot) {
      setCourt({
        ball: undefined,
        paddles: isGuest ? [centeredPaddles()[0], own] : centeredPaddles(),
      });
      return;
    }
    if (snapshot.seq !== seenSeq) {
      seenSeq = snapshot.seq;
      receivedAt = t;
    }
    const age = Math.min(MAX_EXTRAPOLATION_MS, t - receivedAt) / 1000;
    setCourt({
      ball: snapshot.ball ? extrapolateBall(snapshot.ball, age) : undefined,
      paddles: isGuest ? [snapshot.paddles[0], own] : snapshot.paddles,
    });
  };

  const practiceFrame = (t: number, dtSec: number) => {
    const current = practice();
    if (!current || current.winner !== undefined) return;
    moveOwnPaddle(dtSec);
    other = computerPaddle(other, ball, dtSec);
    const paddles: [number, number] = [own, other];
    const scorer = runBall(t, dtSec, paddles);
    setCourt({ ball, paddles });
    if (scorer === undefined) return;
    const scores: [number, number] = [...current.scores];
    scores[scorer] += 1;
    const won = scores.findIndex((score) => score >= PONG_POINTS_TO_WIN);
    setPractice({
      scores,
      winner: won === 0 || won === 1 ? won : undefined,
    });
  };

  const practicing = () => {
    const current = practice();
    return !!current && current.winner === undefined && !playing();
  };

  createFrameLoop({
    running: () => playing() || practicing(),
    onFrame: (dtMs, t) => {
      const dtSec = dtMs / 1000;
      if (playing()) {
        const mine = seat();
        if (mine === 0) hostFrame(t, dtSec);
        else followerFrame(t, dtSec, mine === 1);
        return;
      }
      practiceFrame(t, dtSec);
    },
  });

  // Each round starts from a clean court, serving to alternate sides; the
  // simulation state is imperative, so it is reset from an effect.
  const roundKey = createMemo(() => (playing() ? round() : undefined));
  createEffect(
    on(roundKey, (key) => {
      if (key === undefined) return;
      setPractice(undefined);
      resetCourt(key % 2 === 0 ? 1 : 0);
    })
  );

  return {
    /** The court to draw; the ball only shows while a game is running. */
    court: (): PongCourtView => {
      const view = court();
      return playing() || practice() ? view : { ...view, ball: undefined };
    },
    practice,
    /** Practice is local, so anyone can play it while no match runs. */
    canPractice: () => !playing(),
    startPractice: () => {
      if (playing()) return;
      setPractice({ scores: [0, 0], winner: undefined });
      resetCourt(1);
    },
    stopPractice: () => {
      setPractice(undefined);
      resetCourt(1);
    },
    /** Which paddle this client moves, if any. */
    controlledSeat: (): PongSeat | undefined => {
      if (practicing() || practice()?.winner !== undefined) return 0;
      const mine = seat();
      return playing() && (mine === 0 || mine === 1) ? mine : undefined;
    },
    pointer: (y: number) => {
      pointer = y;
    },
    keyDown: (key: string) => keys.press(key),
    keyUp: (key: string) => keys.release(key),
    blur: () => keys.clear(),
  };
}

export type PongState = ReturnType<typeof createPong>;
