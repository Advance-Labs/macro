import { For } from 'solid-js';
import { GameCard } from '../components/game-card';
import { useGamesContext } from '../context/games-context';
import {
  formatScore,
  GAME_CATALOG,
  GAME_KINDS,
  type GameKind,
} from '../core/catalog';
import { teamRecord } from '../core/leaderboard';

/** Every game with its team record; the host decides how new rooms open. */
export function GamesHubView(props: {
  creating: GameKind | undefined;
  onCreate: (kind: GameKind) => void;
}) {
  const games = useGamesContext();
  const leaderboards = games.createLeaderboards();
  const hasTeam = () => leaderboards.leaderboards()?.teamId !== undefined;

  const record = (kind: GameKind) => {
    const best = teamRecord(leaderboards.leaderboards(), kind);
    if (!best) return 'No record yet';
    const scoring = GAME_CATALOG[kind].scoring;
    const who = games.displayName(best.userId);
    if (scoring.t === 'wins')
      return `${hasTeam() ? 'Most wins' : 'Your wins'}: ${who} · ${best.value}`;
    return `${hasTeam() ? 'Team record' : 'Your best'}: ${formatScore(scoring.unit, best.value)} · ${who}`;
  };

  return (
    <div class="size-full overflow-y-auto">
      <div class="mx-auto flex w-full max-w-5xl flex-col gap-6 p-4 sm:p-8">
        <header class="flex flex-col gap-1">
          <h1 class="font-semibold text-ink text-xl">Games</h1>
          <p class="text-ink-muted text-sm">
            A quick break while your agents work. Every game is a room you can
            share with people or a channel: editors play, everyone else watches.
          </p>
        </header>
        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <For each={GAME_KINDS}>
            {(kind) => (
              <GameCard
                definition={GAME_CATALOG[kind]}
                record={record(kind)}
                creating={props.creating === kind}
                onCreate={() => props.onCreate(kind)}
              />
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
