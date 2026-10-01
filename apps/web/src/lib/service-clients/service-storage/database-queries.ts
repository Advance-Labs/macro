/** Saved database queries: immutable SQL rows a document answer points at. */
import { SERVER_HOSTS } from '@core/constant/servers';
import {
  type FetchWithTokenErrorCode,
  fetchWithToken,
} from '@core/util/fetchWithToken';
import type { ObjectLike, ResultError } from '@core/util/result';
import { ResultAsync } from 'neverthrow';
import { match, P } from 'ts-pattern';
import type { ErrorResponse } from './generated/schemas/errorResponse';
import type { SavedQuery } from './generated/schemas/savedQuery';
import type { SaveQueryRequest } from './generated/schemas/saveQueryRequest';

/**
 * Why a saved query was refused: it does not compile (400), is not a SELECT
 * (403), is gone or not visible (404), or reads too much (422).
 */
export type SavedQueryErrorCode =
  | FetchWithTokenErrorCode
  | 'INVALID_QUERY'
  | 'READ_ONLY'
  | 'BUDGET_EXCEEDED';

const dssHost = SERVER_HOSTS['document-storage-service'];

function isErrorResponse(body: unknown): body is ErrorResponse {
  return (
    !!body &&
    typeof body === 'object' &&
    'message' in body &&
    typeof body.message === 'string'
  );
}

/** The body's `message` is the compiler's, verbatim. */
async function errorResponseHandler(
  response: Response
): Promise<ResultError<SavedQueryErrorCode>> {
  const text = await response.text();
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    // A proxy's plain-text body is the message.
  }
  return {
    code: match(response.status)
      .returnType<SavedQueryErrorCode>()
      .with(400, () => 'INVALID_QUERY')
      .with(401, () => 'UNAUTHORIZED')
      .with(403, () => 'READ_ONLY')
      .with(404, () => 'NOT_FOUND')
      .with(422, () => 'BUDGET_EXCEEDED')
      .with(P.number.gte(500), () => 'SERVER_ERROR')
      .otherwise(() => 'HTTP_ERROR'),
    message:
      isErrorResponse(body) && body.message
        ? body.message
        : text || `HTTP error! status: ${response.status}`,
  };
}

function savedQueriesFetch<T extends ObjectLike>(
  path: string,
  init?: RequestInit
): ResultAsync<T, ResultError<SavedQueryErrorCode>[]> {
  return new ResultAsync(
    fetchWithToken<T, SavedQueryErrorCode>(`${dssHost}${path}`, {
      ...init,
      errorResponseHandler,
    })
  );
}

export function saveDatabaseQuery(request: SaveQueryRequest) {
  return savedQueriesFetch<SavedQuery>('/databases/queries', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export function getDatabaseQuery(queryId: string) {
  return savedQueriesFetch<SavedQuery>(
    `/databases/queries/${encodeURIComponent(queryId)}`
  );
}
