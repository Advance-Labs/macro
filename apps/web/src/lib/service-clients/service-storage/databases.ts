/** The `/databases` routes of `crates/databases`, mounted by the document storage service. */
import { SERVER_HOSTS } from '@core/constant/servers';
import type { DatabaseOp } from '@core/database-sql/generated/types';
import {
  type FetchWithTokenErrorCode,
  type FetchWithTokenInit,
  fetchWithToken,
} from '@core/util/fetchWithToken';
import type { ObjectLike, ResultError } from '@core/util/result';
import { statusError } from '@core/util/safeFetch';
import { ResultAsync } from 'neverthrow';
import type { AddColumnOptionsRequest } from './generated/schemas/addColumnOptionsRequest';
import type { ApplyOpsResponse } from './generated/schemas/applyOpsResponse';
import type { Awareness } from './generated/schemas/awareness';
import type { ChangeColumnTypeRequest } from './generated/schemas/changeColumnTypeRequest';
import type { ColumnCast } from './generated/schemas/columnCast';
import type { ColumnDetail } from './generated/schemas/columnDetail';
import type { ColumnSchemaOutcome } from './generated/schemas/columnSchemaOutcome';
import type { ColumnTypeChangeOutcome } from './generated/schemas/columnTypeChangeOutcome';
import type { CreateColumnRequest } from './generated/schemas/createColumnRequest';
import type { CreateColumnResponse } from './generated/schemas/createColumnResponse';
import type { CreateDatabaseRequest } from './generated/schemas/createDatabaseRequest';
import type { CreateTableRequest } from './generated/schemas/createTableRequest';
import type { Database } from './generated/schemas/database';
import type { DatabaseDetail } from './generated/schemas/databaseDetail';
import type { DeleteColumnRequest } from './generated/schemas/deleteColumnRequest';
import type { ErrorResponse } from './generated/schemas/errorResponse';
import type { ImportTable } from './generated/schemas/importTable';
import type { InferColumnTypeOutcome } from './generated/schemas/inferColumnTypeOutcome';
import type { InferColumnTypeRequest } from './generated/schemas/inferColumnTypeRequest';
import type { ListedDatabase } from './generated/schemas/listedDatabase';
import type { OpRefusalResponse } from './generated/schemas/opRefusalResponse';
import type { RenameColumnOutcome } from './generated/schemas/renameColumnOutcome';
import type { RenameColumnRequest } from './generated/schemas/renameColumnRequest';
import type { RenameTableRequest } from './generated/schemas/renameTableRequest';
import type { ReorderColumnsRequest } from './generated/schemas/reorderColumnsRequest';
import type { ReorderTablesRequest } from './generated/schemas/reorderTablesRequest';
import type { SharePermissionV2 } from './generated/schemas/sharePermissionV2';
import type { StarterDatabase } from './generated/schemas/starterDatabase';
import type { Table } from './generated/schemas/table';
import type { UpdateSharePermissionRequestV2 } from './generated/schemas/updateSharePermissionRequestV2';
import type { ViewPositionsResponse } from './generated/schemas/viewPositionsResponse';

/** A schema change the service refused as invalid (400), e.g. a taken name. */
export type DatabaseSchemaErrorCode =
  | FetchWithTokenErrorCode
  | 'INVALID_SCHEMA';

/** A batch of `/ops` the service refused (400); nothing of it was written. */
type DatabaseOpsErrorCode = FetchWithTokenErrorCode | 'INVALID_OP';

/** An `/ops` failure; an `INVALID_OP` names the op, row and column it refused. */
export type DatabaseOpsError = ResultError<DatabaseOpsErrorCode> & {
  refusal: OpRefusalResponse | null;
};

const documentStorageHost = SERVER_HOSTS['document-storage-service'];

function isErrorResponse(body: unknown): body is ErrorResponse {
  return (
    !!body &&
    typeof body === 'object' &&
    'message' in body &&
    typeof body.message === 'string'
  );
}

function isOpRefusal(body: unknown): body is OpRefusalResponse {
  return isErrorResponse(body) && 'op' in body && typeof body.op === 'number';
}

/** A failed response's body, and the message it gives. */
export async function errorBody(
  response: Response
): Promise<{ body: unknown; message: string }> {
  const text = await response.text();
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    // A proxy's plain-text body is the message.
  }
  return {
    body,
    message:
      isErrorResponse(body) && body.message
        ? body.message
        : text || `HTTP error! status: ${response.status}`,
  };
}

/** A 400 is the route's own refusal, `invalid`; any other status keeps safeFetch's code. */
function statusCode<Invalid extends string>(
  status: number,
  invalid: Invalid | undefined
): FetchWithTokenErrorCode | Invalid {
  return status === 400 && invalid !== undefined
    ? invalid
    : statusError(status).code;
}

/** One `/databases` request; a failure carries the service's own message. */
function databasesFetch<T extends ObjectLike, Invalid extends string = never>(
  path: string,
  init: Omit<FetchWithTokenInit, 'errorResponseHandler'> & {
    invalid?: Invalid;
  } = {}
): ResultAsync<T, ResultError<FetchWithTokenErrorCode | Invalid>[]> {
  const { invalid, ...request } = init;
  return new ResultAsync(
    fetchWithToken<T, Invalid>(`${documentStorageHost}${path}`, {
      ...request,
      errorResponseHandler: async (response) => ({
        code: statusCode(response.status, invalid),
        message: (await errorBody(response)).message,
      }),
    })
  );
}

/** The refusal the `/ops` error handler attached; transport failures (a 401) carry none. */
function withRefusal(
  error: ResultError<DatabaseOpsErrorCode>
): DatabaseOpsError {
  return {
    ...error,
    refusal:
      'refusal' in error && isOpRefusal(error.refusal) ? error.refusal : null,
  };
}

export const databasesClient = {
  importTable({ id, request }: { id: string; request: ImportTable }) {
    return databasesFetch<Table, 'INVALID_SCHEMA'>(`/databases/${id}/import`, {
      method: 'POST',
      body: JSON.stringify(request),
      invalid: 'INVALID_SCHEMA',
    });
  },

  getPermissions({ id }: { id: string }) {
    return databasesFetch<SharePermissionV2>(`/databases/${id}/permissions`);
  },

  updatePermissions({
    id,
    ...request
  }: { id: string } & UpdateSharePermissionRequestV2) {
    return databasesFetch<SharePermissionV2, 'INVALID_SHARING'>(
      `/databases/${id}/permissions`,
      {
        method: 'PATCH',
        body: JSON.stringify(request),
        invalid: 'INVALID_SHARING',
      }
    );
  },

  list() {
    return databasesFetch<ListedDatabase[]>('/databases');
  },

  ensureStarter() {
    return databasesFetch<StarterDatabase>('/databases/starter', {
      method: 'POST',
    });
  },

  get({ id }: { id: string }) {
    return databasesFetch<DatabaseDetail>(`/databases/${id}`);
  },

  create(request: CreateDatabaseRequest) {
    return databasesFetch<Database, 'INVALID_SCHEMA'>('/databases', {
      method: 'POST',
      body: JSON.stringify(request),
      invalid: 'INVALID_SCHEMA',
    });
  },

  createTable({ id, ...request }: { id: string } & CreateTableRequest) {
    return databasesFetch<Table, 'INVALID_SCHEMA'>(`/databases/${id}/tables`, {
      method: 'POST',
      body: JSON.stringify(request),
      invalid: 'INVALID_SCHEMA',
    });
  },

  renameTable({
    id,
    tableId,
    ...request
  }: { id: string; tableId: string } & RenameTableRequest) {
    return databasesFetch<Table, 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/${tableId}`,
      {
        method: 'PATCH',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  /**
   * Set the tab order. `tableIds` names every table of the database exactly
   * once; a stale list is refused, and the caller refetches.
   */
  reorderTables({ id, ...request }: { id: string } & ReorderTablesRequest) {
    return databasesFetch<Table[], 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/order`,
      {
        method: 'PUT',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  createColumn({
    id,
    tableId,
    request,
  }: {
    id: string;
    tableId: string;
    request: CreateColumnRequest;
  }) {
    return databasesFetch<CreateColumnResponse, 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/${tableId}/columns`,
      {
        method: 'POST',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  renameColumn({
    id,
    tableId,
    columnId,
    ...request
  }: { id: string; tableId: string; columnId: string } & RenameColumnRequest) {
    return databasesFetch<RenameColumnOutcome, 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/${tableId}/columns/${columnId}`,
      {
        method: 'PATCH',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  changeColumnType(params: {
    id: string;
    tableId: string;
    columnId: string;
    request: ChangeColumnTypeRequest;
  }) {
    return databasesFetch<ColumnTypeChangeOutcome, 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/${params.tableId}/columns/${params.columnId}/type`,
      {
        method: 'PATCH',
        body: JSON.stringify(params.request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  /** The dry run of a type change: what each menu type does to the values. */
  columnCasts(params: { id: string; tableId: string; columnId: string }) {
    return databasesFetch<ColumnCast[]>(
      `/databases/${params.id}/tables/${params.tableId}/columns/${params.columnId}/casts`
    );
  },

  deleteColumn({
    id,
    tableId,
    columnId,
    ...request
  }: { id: string; tableId: string; columnId: string } & DeleteColumnRequest) {
    return databasesFetch<ColumnSchemaOutcome, 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/${tableId}/columns/${columnId}`,
      {
        method: 'DELETE',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  reorderColumns({
    id,
    tableId,
    ...request
  }: { id: string; tableId: string } & ReorderColumnsRequest) {
    return databasesFetch<ColumnSchemaOutcome, 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/${tableId}/columns/order`,
      {
        method: 'PATCH',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  inferColumnType(params: {
    id: string;
    tableId: string;
    columnId: string;
    request: InferColumnTypeRequest;
  }) {
    return databasesFetch<InferColumnTypeOutcome, 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/${params.tableId}/columns/${params.columnId}/infer-type`,
      {
        method: 'POST',
        body: JSON.stringify(params.request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  /**
   * Add select options to an existing column. Labels it already has are a
   * no-op; the answer is the column as it now stands.
   */
  addColumnOptions({
    id,
    tableId,
    columnId,
    request,
  }: {
    id: string;
    tableId: string;
    columnId: string;
    request: AddColumnOptionsRequest;
  }) {
    return databasesFetch<ColumnDetail, 'INVALID_SCHEMA'>(
      `/databases/${id}/tables/${tableId}/columns/${columnId}/options`,
      {
        method: 'POST',
        body: JSON.stringify(request),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  /** Apply a batch of the engine's typed ops to a database, together or not at all. */
  applyOps({
    id,
    request,
  }: {
    id: string;
    /** The engine's ops: the generated `ApplyOpsRequest` drops `null` from optional fields. */
    request: { ops: DatabaseOp[] };
  }): ResultAsync<ApplyOpsResponse, DatabaseOpsError[]> {
    return new ResultAsync(
      fetchWithToken<ApplyOpsResponse, 'INVALID_OP'>(
        `${documentStorageHost}/databases/${id}/ops`,
        {
          method: 'POST',
          body: JSON.stringify(request),
          errorResponseHandler: async (response): Promise<DatabaseOpsError> => {
            const { body, message } = await errorBody(response);
            const code = statusCode(response.status, 'INVALID_OP');
            return {
              code,
              message,
              refusal: code === 'INVALID_OP' && isOpRefusal(body) ? body : null,
            };
          },
        }
      )
    ).mapErr((errors) => errors.map(withRefusal));
  },

  /** Where a board's cards sit: each placed card's lane and key there. */
  viewPositions({ id, viewId }: { id: string; viewId: string }) {
    return databasesFetch<ViewPositionsResponse>(
      `/databases/${id}/views/${viewId}/positions`
    );
  },

  /** Tell the database's other viewers where the caller is. Responds 204. */
  shareAwareness({ id, state }: { id: string; state: Awareness }) {
    return databasesFetch<Record<string, never>>(`/databases/${id}/awareness`, {
      method: 'PUT',
      body: JSON.stringify(state),
    });
  },
};
