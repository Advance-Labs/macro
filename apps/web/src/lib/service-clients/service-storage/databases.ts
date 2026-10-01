/** The `/databases` routes of `crates/databases`, mounted by the document storage service. */
import { SERVER_HOSTS } from '@core/constant/servers';
import type { DatabaseOp } from '@core/database-sql/generated/types';
import {
  type FetchWithTokenErrorCode,
  type FetchWithTokenInit,
  fetchWithToken,
} from '@core/util/fetchWithToken';
import type { ObjectLike, ResultError } from '@core/util/result';
import { ResultAsync } from 'neverthrow';
import { match, P } from 'ts-pattern';
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
import type { SharePermissionV2 } from './generated/schemas/sharePermissionV2';
import type { StarterDatabase } from './generated/schemas/starterDatabase';
import type { Table } from './generated/schemas/table';
import type { UpdateChannelSharePermission } from './generated/schemas/updateChannelSharePermission';
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

function statusCode<Invalid extends string>(
  status: number,
  invalid: Invalid
): FetchWithTokenErrorCode | Invalid {
  if (status === 400) return invalid;
  return match(status)
    .returnType<FetchWithTokenErrorCode>()
    .with(401, () => 'UNAUTHORIZED')
    .with(403, () => 'FORBIDDEN')
    .with(404, () => 'NOT_FOUND')
    .with(409, () => 'CONFLICT')
    .with(410, () => 'GONE')
    .with(P.number.gte(500), () => 'SERVER_ERROR')
    .otherwise(() => 'HTTP_ERROR');
}

/**
 * One `/databases` request. A 400 is the route's own refusal, `invalid`;
 * every other failure keeps the transport's code and the service's message.
 */
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
        code: statusCode<Invalid | 'HTTP_ERROR'>(
          response.status,
          invalid ?? 'HTTP_ERROR'
        ),
        message: (await errorBody(response)).message,
      }),
    })
  );
}

const json = (body: object) => JSON.stringify(body);

export const databasesClient = {
  importTable({ id, request }: { id: string; request: ImportTable }) {
    return databasesFetch<Table, 'INVALID_SCHEMA'>(`/databases/${id}/import`, {
      method: 'POST',
      body: json(request),
      invalid: 'INVALID_SCHEMA',
    });
  },

  getPermissions({ id }: { id: string }) {
    return databasesFetch<SharePermissionV2>(`/databases/${id}/permissions`);
  },

  updatePermissions(params: {
    id: string;
    channelSharePermissions: UpdateChannelSharePermission[];
  }) {
    return databasesFetch<SharePermissionV2, 'INVALID_SHARING'>(
      `/databases/${params.id}/permissions`,
      {
        method: 'PATCH',
        body: json({ channelSharePermissions: params.channelSharePermissions }),
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

  create({ name }: { name: string }) {
    return databasesFetch<Database, 'INVALID_SCHEMA'>('/databases', {
      method: 'POST',
      body: json({ name }),
      invalid: 'INVALID_SCHEMA',
    });
  },

  createTable({ id, name }: { id: string; name: string }) {
    return databasesFetch<Table, 'INVALID_SCHEMA'>(`/databases/${id}/tables`, {
      method: 'POST',
      body: json({ name }),
      invalid: 'INVALID_SCHEMA',
    });
  },

  renameTable(params: { id: string; tableId: string } & RenameTableRequest) {
    return databasesFetch<Table, 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/${params.tableId}`,
      {
        method: 'PATCH',
        body: json({ name: params.name, previousName: params.previousName }),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  /**
   * Set the tab order. `tableIds` names every table of the database exactly
   * once; a stale list is refused, and the caller refetches.
   */
  reorderTables(params: { id: string; tableIds: string[] }) {
    return databasesFetch<Table[], 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/order`,
      {
        method: 'PUT',
        body: json({ tableIds: params.tableIds }),
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
      { method: 'POST', body: json(request), invalid: 'INVALID_SCHEMA' }
    );
  },

  renameColumn(
    params: {
      id: string;
      tableId: string;
      columnId: string;
    } & RenameColumnRequest
  ) {
    return databasesFetch<RenameColumnOutcome, 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/${params.tableId}/columns/${params.columnId}`,
      {
        method: 'PATCH',
        body: json({ name: params.name, previousName: params.previousName }),
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
      { method: 'PATCH', body: json(params.request), invalid: 'INVALID_SCHEMA' }
    );
  },

  /** The dry run of a type change: what each menu type does to the values. */
  columnCasts(params: { id: string; tableId: string; columnId: string }) {
    return databasesFetch<ColumnCast[]>(
      `/databases/${params.id}/tables/${params.tableId}/columns/${params.columnId}/casts`
    );
  },

  deleteColumn(
    params: {
      id: string;
      tableId: string;
      columnId: string;
    } & DeleteColumnRequest
  ) {
    return databasesFetch<ColumnSchemaOutcome, 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/${params.tableId}/columns/${params.columnId}`,
      {
        method: 'DELETE',
        body: json({ baseVersion: params.baseVersion }),
        invalid: 'INVALID_SCHEMA',
      }
    );
  },

  reorderColumns(
    params: { id: string; tableId: string } & ReorderColumnsRequest
  ) {
    return databasesFetch<ColumnSchemaOutcome, 'INVALID_SCHEMA'>(
      `/databases/${params.id}/tables/${params.tableId}/columns/order`,
      {
        method: 'PATCH',
        body: json({
          columnIds: params.columnIds,
          baseVersion: params.baseVersion,
        }),
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
      { method: 'POST', body: json(params.request), invalid: 'INVALID_SCHEMA' }
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
      { method: 'POST', body: json(request), invalid: 'INVALID_SCHEMA' }
    );
  },

  /** Apply a batch of the engine's typed ops to a database, together or not at all. */
  applyOps({
    id,
    request,
  }: {
    id: string;
    request: { ops: DatabaseOp[] };
  }): ResultAsync<ApplyOpsResponse, DatabaseOpsError[]> {
    let refusal: OpRefusalResponse | null = null;
    return new ResultAsync(
      fetchWithToken<ApplyOpsResponse, 'INVALID_OP'>(
        `${documentStorageHost}/databases/${id}/ops`,
        {
          method: 'POST',
          body: json(request),
          errorResponseHandler: async (response) => {
            const { body, message } = await errorBody(response);
            refusal = isOpRefusal(body) ? body : null;
            return { code: statusCode(response.status, 'INVALID_OP'), message };
          },
        }
      )
    ).mapErr((errors) =>
      errors.map((error) => ({
        ...error,
        refusal: error.code === 'INVALID_OP' ? refusal : null,
      }))
    );
  },

  /** Where a board's cards sit: each placed card's lane and key there. */
  viewPositions({ id, viewId }: { id: string; viewId: string }) {
    return databasesFetch<ViewPositionsResponse>(
      `/databases/${id}/views/${viewId}/positions`
    );
  },

  /** Tell the database's other viewers where the caller is. Responds 204. */
  shareAwareness(databaseId: string, state: Awareness) {
    return databasesFetch<Record<string, never>>(
      `/databases/${databaseId}/awareness`,
      { method: 'PUT', body: json(state) }
    );
  },
};
