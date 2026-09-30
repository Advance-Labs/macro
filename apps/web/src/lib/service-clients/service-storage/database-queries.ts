/**
 * Saved database queries: immutable SQL rows a document answer points at.
 *
 * Hand-written mirror of the `/databases/queries` routes in `crates/databases`
 * until the storage-service OpenAPI generation covers them.
 */
import { SERVER_HOSTS } from '@core/constant/servers';
import {
  type FetchWithTokenErrorCode,
  type FetchWithTokenInit,
  fetchWithToken,
} from '@core/util/fetchWithToken';
import type { ObjectLike, ResultError } from '@core/util/result';
import type { Result } from 'neverthrow';
import { match } from 'ts-pattern';
import type { ExecErrorCode, ExecOutcome } from './databases';

export interface DatabaseQueryDefinition {
  version: 1;
  query: string;
}

export interface SavedDatabaseQuery {
  id: string;
  definition: DatabaseQueryDefinition;
  databaseId: string | null;
  createdBy: string;
  createdAt: string;
}

export interface SaveDatabaseQueryRequest {
  definition: DatabaseQueryDefinition;
  databaseId?: string;
}

export type DatabaseQueryErrorCode = ExecErrorCode | 'NOT_FOUND';

const dssHost = SERVER_HOSTS['document-storage-service'];

function databaseQueriesFetch<T extends ObjectLike>(
  path: string,
  init?: FetchWithTokenInit<DatabaseQueryErrorCode>
): Promise<
  Result<T, ResultError<FetchWithTokenErrorCode | DatabaseQueryErrorCode>[]>
> {
  return fetchWithToken<T, DatabaseQueryErrorCode>(`${dssHost}${path}`, {
    ...init,
    errorResponseHandler,
  });
}

/** Same mapping as `/databases/exec`: the body's `message` is the compiler's. */
async function errorResponseHandler(
  response: Response
): Promise<ResultError<FetchWithTokenErrorCode | DatabaseQueryErrorCode>> {
  const body = await response.text();
  let message = body || `HTTP error! status: ${response.status}`;
  try {
    const parsed: unknown = JSON.parse(body);
    if (
      parsed &&
      typeof parsed === 'object' &&
      'message' in parsed &&
      typeof parsed.message === 'string' &&
      parsed.message
    )
      message = parsed.message;
  } catch {
    // A proxy's plain-text body is the message.
  }
  const code = match(response.status)
    .returnType<FetchWithTokenErrorCode | DatabaseQueryErrorCode>()
    .with(400, () => 'SQL_ERROR')
    .with(403, () => 'READ_ONLY')
    .with(404, () => 'NOT_FOUND')
    .with(422, () => 'BUDGET_EXCEEDED')
    .otherwise(() => 'HTTP_ERROR');
  return { code, message };
}

export function saveDatabaseQuery(request: SaveDatabaseQueryRequest) {
  return databaseQueriesFetch<SavedDatabaseQuery>('/databases/queries', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export function getDatabaseQuery(queryId: string) {
  return databaseQueriesFetch<SavedDatabaseQuery>(
    `/databases/queries/${encodeURIComponent(queryId)}`
  );
}

/** Runs read-only as the viewer; the server re-checks access on every run. */
export function runDatabaseQuery(queryId: string) {
  return databaseQueriesFetch<ExecOutcome>(
    `/databases/queries/${encodeURIComponent(queryId)}/run`,
    { method: 'POST' }
  );
}
