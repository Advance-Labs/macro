import { storageServiceClient } from '@service-storage/client';
import type { RoundOutcome, ScoreOutcome } from '../context/games-context';
import type { GameKind } from '../core/catalog';
import { invalidateLeaderboards } from './leaderboards';

/**
 * Record a finished run. Failures are logged rather than surfaced: a missed
 * leaderboard entry must never interrupt the game itself.
 */
export async function submitGameScore(
  kind: GameKind,
  score: number
): Promise<ScoreOutcome | undefined> {
  const result = await storageServiceClient.games.submitScore({
    kind,
    score: Math.round(score),
  });
  if (result.isErr()) {
    console.error('Failed to submit game score', result.error);
    return undefined;
  }
  if (result.value.improved) void invalidateLeaderboards();
  return { best: result.value.best, improved: result.value.improved };
}

/** Report a finished round; the server keeps the first report of each round. */
export async function reportGameRound(outcome: RoundOutcome): Promise<void> {
  const result = await storageServiceClient.games.reportRound({
    entityType: 'document',
    entityId: outcome.documentId,
    kind: outcome.kind,
    round: outcome.round,
    winnerUserId: outcome.winner,
    playerUserIds: outcome.players,
  });
  if (result.isErr()) {
    console.error('Failed to report game round', result.error);
    return;
  }
  if (result.value.recorded && outcome.winner) void invalidateLeaderboards();
}
