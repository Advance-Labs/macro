/**
 * Server state for saved database queries.
 *
 * A saved query never changes, so its definition is cached forever; its
 * answer is viewer-specific and kept live by the caller's table liveness.
 */
import type { FetchWithTokenErrorCode } from '@core/util/fetchWithToken';
import type { ResultError } from '@core/util/result';
import { createQueryKeys } from '@lukemorales/query-key-factory';
import {
  type DatabaseQueryErrorCode,
  getDatabaseQuery,
  runDatabaseQuery,
  type SaveDatabaseQueryRequest,
  type SavedDatabaseQuery,
  saveDatabaseQuery,
} from '@service-storage/database-queries';
import type { ExecOutcome } from '@service-storage/databases';
import { useQuery } from '@tanstack/solid-query';
import type { Result } from 'neverthrow';
import type { Accessor } from 'solid-js';
import { queryClient } from '../client';
import { ExecError } from './databases';

export const savedDatabaseQueryKeys = createQueryKeys('saved-database-query', {
  definition: (queryId: string) => ({ queryKey: [queryId] }),
  run: (queryId: string) => ({ queryKey: [queryId] }),
});

function throwOnFailure<Value>(
  result: Result<
    Value,
    ResultError<FetchWithTokenErrorCode | DatabaseQueryErrorCode>[]
  >,
  fallback: string
): Value {
  if (result.isOk()) return result.value;
  const failure = result.error[0];
  if (failure?.code === 'NOT_FOUND')
    throw new ExecError('HTTP_ERROR', 'This saved question no longer exists.');
  throw new ExecError(
    failure?.code ?? 'HTTP_ERROR',
    failure?.message ?? fallback
  );
}

export async function fetchDatabaseQuery(
  queryId: string
): Promise<SavedDatabaseQuery> {
  return throwOnFailure(
    await getDatabaseQuery(queryId),
    'This saved question could not be loaded.'
  );
}

/** Run a saved query as the viewer. Throws an [`ExecError`] on failure. */
export async function fetchDatabaseQueryRun(
  queryId: string
): Promise<ExecOutcome> {
  return throwOnFailure(
    await runDatabaseQuery(queryId),
    'The database could not answer that question.'
  );
}

/** Save SQL as a new immutable query and seed its definition cache. */
export async function createSavedDatabaseQuery(
  request: SaveDatabaseQueryRequest
): Promise<SavedDatabaseQuery> {
  const saved = throwOnFailure(
    await saveDatabaseQuery(request),
    'The question could not be saved.'
  );
  queryClient.setQueryData(
    savedDatabaseQueryKeys.definition(saved.id).queryKey,
    saved
  );
  return saved;
}

export function useDatabaseQueryDefinition(queryId: Accessor<string>) {
  return useQuery(() => ({
    queryKey: savedDatabaseQueryKeys.definition(queryId()).queryKey,
    queryFn: () => fetchDatabaseQuery(queryId()),
    enabled: !!queryId(),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  }));
}

export function useDatabaseQueryRun(
  queryId: Accessor<string>,
  run: (queryId: string) => Promise<ExecOutcome> = fetchDatabaseQueryRun
) {
  return useQuery(() => {
    const id = queryId();
    return {
      queryKey: savedDatabaseQueryKeys.run(id).queryKey,
      queryFn: () => run(id),
      enabled: !!id,
      staleTime: 30_000,
      retry: false,
      refetchOnWindowFocus: true,
    };
  });
}
