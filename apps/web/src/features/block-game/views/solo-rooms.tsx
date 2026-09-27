import { UserIcon } from '@core/component/UserIcon';
import { Button } from '@ui';
import {
  type Accessor,
  createEffect,
  createMemo,
  createSignal,
  For,
  type JSX,
  on,
  Show,
} from 'solid-js';
import { GameStatusBadge } from '../components/game-status-badge';
import { LeaderboardList } from '../components/leaderboard-list';
import { MinesweeperBoard } from '../components/minesweeper-board';
import { PanelSection, RoomLayout } from '../components/room-layout';
import { SnakeBoard } from '../components/snake-board';
import { TwentyFortyEightBoard } from '../components/twenty-forty-eight-board';
import { type ScoreOutcome, useGamesContext } from '../context/games-context';
import {
  formatScore,
  type GameKind,
  gameDefinition,
  type ScoreUnit,
} from '../core/catalog';
import { flagsRemaining } from '../core/games/minesweeper';
import { highestTile } from '../core/games/twenty-forty-eight';
import type { GameRoom } from '../primitives/create-game-room';
import { createStatusPublisher } from '../primitives/create-room-reporting';
import { createSoloRoom } from '../primitives/create-solo-room';
import {
  createMinesweeperRun,
  createSnakeRun,
  createTwentyFortyEightRun,
  type RunPhase,
} from '../primitives/create-solo-runs';
import { createSwipe, directionFromKey } from '../primitives/direction-input';
import { TeamLeaderboardPanel } from './team-leaderboard-panel';

type RoomProps = { room: GameRoom; documentId: string };

function unitOf(kind: GameKind): ScoreUnit {
  const scoring = gameDefinition(kind).scoring;
  return scoring.t === 'high-score' ? scoring.unit : 'points';
}

/** Record a finished run in the room and on the team leaderboard. */
function createFinisher(room: GameRoom, kind: GameKind) {
  const games = useGamesContext();
  const solo = createSoloRoom(room, kind);
  const [outcome, setOutcome] = createSignal<ScoreOutcome>();

  async function finish(score: number) {
    setOutcome(undefined);
    solo.recordRun(score);
    // A run that never scored stays in the room without claiming a record.
    if (score > 0) setOutcome(await games.submitScore(kind, score));
  }

  return { solo, outcome, finish: (score: number) => void finish(score) };
}

/** Everything solo rooms share around their board. */
function SoloRoomShell(props: {
  room: GameRoom;
  documentId: string;
  kind: GameKind;
  solo: ReturnType<typeof createSoloRoom>;
  phase: Accessor<RunPhase>;
  score: Accessor<number>;
  outcome: ScoreOutcome | undefined;
  message: JSX.Element;
  actions: JSX.Element;
  board: JSX.Element;
}) {
  const games = useGamesContext();
  const definition = gameDefinition(props.kind);
  const unit = unitOf(props.kind);
  const playing = () => props.phase() === 'playing';

  createStatusPublisher({
    status: () => props.solo.status(playing()),
    enabled: props.room.canPlay,
    // Spectators derive "playing now" from presence; the player publishes it.
    leads: () => playing() || props.room.wroteLatest(),
    publish: (status) => games.publishStatus(props.documentId, status),
  });
  // Presence is an external system: share whether we play, and how it goes.
  // Memoized so it is sent when the score changes, not on every game tick;
  // a running clock is shared to the second.
  const liveScore = createMemo(() => {
    if (!playing()) return undefined;
    const score = props.score();
    return unit === 'milliseconds' ? Math.floor(score / 1000) * 1000 : score;
  });
  createEffect(
    on(liveScore, (score) =>
      props.room.setPresence(
        score === undefined
          ? { activity: 'watching' }
          : { activity: 'playing', score }
      )
    )
  );

  const roomBests = () =>
    props.solo.bests().map((run, index) => ({
      userId: run.userId,
      rank: index + 1,
      value: run.score,
      at: run.at,
    }));

  return (
    <RoomLayout
      title={definition.title}
      status={
        <GameStatusBadge
          status={props.solo.status(playing())}
          category={definition.category}
        />
      }
      actions={props.actions}
      message={
        <Show
          when={props.phase() === 'over' && props.outcome?.improved}
          fallback={props.message}
        >
          New personal best: {formatScore(unit, props.outcome?.best ?? 0)} 🎉
        </Show>
      }
      board={props.board}
      sidebar={
        <>
          <Show when={props.solo.playingPeers().length > 0}>
            <PanelSection title="Playing now">
              <ul class="flex flex-col gap-1 px-2 text-sm">
                <For each={props.solo.playingPeers()}>
                  {(peer) => (
                    <li class="flex items-center gap-2">
                      <Show when={peer.userId}>
                        {(userId) => (
                          <UserIcon id={userId()} size="md" suppressClick />
                        )}
                      </Show>
                      <span class="min-w-0 flex-1 truncate text-ink">
                        {peer.userId ? games.displayName(peer.userId) : 'Guest'}
                      </span>
                      <Show when={peer.presence.score !== undefined}>
                        <span class="text-ink-muted tabular-nums">
                          {formatScore(unit, peer.presence.score ?? 0)}
                        </span>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </PanelSection>
          </Show>
          <PanelSection title="This room">
            <LeaderboardList
              rows={roomBests()}
              viewer={undefined}
              viewerId={games.userId()}
              displayName={games.displayName}
              formatValue={(value) => formatScore(unit, value)}
              empty={
                <p class="px-2 text-ink-subtle text-sm">
                  Finished runs show up here.
                </p>
              }
            />
          </PanelSection>
          <TeamLeaderboardPanel kind={props.kind} />
          <PanelSection title="How to play">
            <p class="px-2 text-ink-muted text-sm">{definition.howToPlay}</p>
          </PanelSection>
        </>
      }
    />
  );
}

/** A focusable board frame that routes arrow keys, WASD and swipes. */
function DirectionalFrame(props: {
  label: string;
  onDirection: (direction: 'up' | 'down' | 'left' | 'right') => void;
  onKey?: (event: KeyboardEvent) => boolean;
  ref?: (element: HTMLDivElement) => void;
  children: JSX.Element;
}) {
  const swipe = createSwipe(props.onDirection);
  return (
    <div
      ref={props.ref}
      class="flex w-full touch-none justify-center rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-edge-focus"
      tabindex={0}
      role="application"
      aria-label={props.label}
      onKeyDown={(event) => {
        if (props.onKey?.(event)) {
          event.preventDefault();
          return;
        }
        const direction = directionFromKey(event);
        if (!direction) return;
        event.preventDefault();
        props.onDirection(direction);
      }}
      onPointerDown={swipe.onPointerDown}
      onPointerUp={swipe.onPointerUp}
      onPointerCancel={swipe.onPointerCancel}
    >
      {props.children}
    </div>
  );
}

export function SnakeRoom(props: RoomProps) {
  const { solo, outcome, finish } = createFinisher(props.room, 'snake');
  const run = createSnakeRun({ onFinish: finish });
  let frame: HTMLDivElement | undefined;
  const start = () => {
    run.start();
    frame?.focus();
  };

  return (
    <SoloRoomShell
      room={props.room}
      documentId={props.documentId}
      kind="snake"
      solo={solo}
      phase={run.phase}
      score={run.score}
      outcome={outcome()}
      message={
        <Show
          when={run.phase() !== 'ready'}
          fallback="Press an arrow key or swipe to start."
        >
          {run.phase() === 'paused' ? 'Paused. ' : ''}
          {run.phase() === 'over' ? 'Game over. ' : ''}
          {run.score()} points · {run.state().apples} apples
        </Show>
      }
      actions={
        <>
          <Show when={run.phase() === 'playing' || run.phase() === 'paused'}>
            <Button size="sm" onClick={run.togglePause}>
              {run.phase() === 'paused' ? 'Resume' : 'Pause'}
            </Button>
          </Show>
          <Button variant="accent" size="sm" onClick={start}>
            {run.phase() === 'ready' ? 'Start' : 'New game'}
          </Button>
        </>
      }
      board={
        <DirectionalFrame
          ref={(element) => {
            frame = element;
          }}
          label="Snake. Use the arrow keys to steer; space pauses."
          onDirection={run.turn}
          onKey={(event) => {
            if (event.key !== ' ') return false;
            if (run.phase() === 'over') start();
            else run.togglePause();
            return true;
          }}
        >
          <SnakeBoard state={run.state()} />
        </DirectionalFrame>
      }
    />
  );
}

export function TwentyFortyEightRoom(props: RoomProps) {
  const { solo, outcome, finish } = createFinisher(
    props.room,
    'twenty_forty_eight'
  );
  const run = createTwentyFortyEightRun({ onFinish: finish });

  return (
    <SoloRoomShell
      room={props.room}
      documentId={props.documentId}
      kind="twenty_forty_eight"
      solo={solo}
      phase={run.phase}
      score={run.score}
      outcome={outcome()}
      message={
        <>
          {run.phase() === 'over' ? 'No moves left. ' : ''}
          {run.state().won && run.phase() !== 'over'
            ? 'You made 2048! Keep going. '
            : ''}
          {run.score().toLocaleString('en-US')} points · best tile{' '}
          {highestTile(run.state())}
        </>
      }
      actions={
        <Button variant="accent" size="sm" onClick={run.reset}>
          New game
        </Button>
      }
      board={
        <DirectionalFrame
          label="2048. Use the arrow keys or swipe to slide the tiles."
          onDirection={run.slide}
        >
          <TwentyFortyEightBoard state={run.state()} />
        </DirectionalFrame>
      }
    />
  );
}

export function MinesweeperRoom(props: RoomProps) {
  const { solo, outcome, finish } = createFinisher(props.room, 'minesweeper');
  const run = createMinesweeperRun({ onFinish: finish });
  const [flagMode, setFlagMode] = createSignal(false);
  const seconds = () => (run.elapsed() / 1000).toFixed(1);

  return (
    <SoloRoomShell
      room={props.room}
      documentId={props.documentId}
      kind="minesweeper"
      solo={solo}
      phase={run.phase}
      score={run.score}
      outcome={outcome()}
      message={
        <>
          {run.state().status === 'won' ? `Cleared in ${seconds()}s! ` : ''}
          {run.state().status === 'lost' ? 'Boom. ' : ''}
          {run.state().status === 'ready'
            ? 'Click any cell to start. '
            : `⏱ ${seconds()}s · `}
          🚩 {flagsRemaining(run.state())} left
        </>
      }
      actions={
        <>
          <Button
            size="sm"
            aria-pressed={flagMode()}
            onClick={() => setFlagMode((value) => !value)}
          >
            🚩 Flag mode
          </Button>
          <Button
            variant="accent"
            size="sm"
            onClick={() => {
              run.reset();
              setFlagMode(false);
            }}
          >
            New game
          </Button>
        </>
      }
      board={
        <MinesweeperBoard
          state={run.state()}
          flagMode={flagMode()}
          disabled={false}
          onReveal={run.reveal}
          onFlag={run.flag}
          onChord={run.chord}
        />
      }
    />
  );
}
