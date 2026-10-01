/** The cached database detail, patched as option and view ops are sent and read again when one is refused. */
import type { DatabaseOp } from '@core/database-sql/generated/types';
import { queryClient } from '@queries/client';
import {
  applyDatabaseOps,
  applyDatabaseTableVersions,
  invalidateDatabase,
} from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import type { OpResult } from '@service-storage/generated/schemas/opResult';
import { errAsync, okAsync, type ResultAsync } from 'neverthrow';
import type { DatabaseOpFailure } from '../core/write-failure';

/**
 * Change the cached detail in place; nothing happens before it is first read.
 * An in-flight read is cancelled first so its older answer cannot land over
 * the change.
 */
export async function patchDetail(
  databaseId: string,
  change: (detail: DatabaseDetail) => DatabaseDetail
): Promise<void> {
  const queryKey = databasesKeys.detail(databaseId).queryKey;
  await queryClient.cancelQueries({ queryKey, exact: true });
  queryClient.setQueryData(
    queryKey,
    (previous: DatabaseDetail | undefined) => previous && change(previous)
  );
}

/** Change one table's cached views. */
export function patchViews(
  databaseId: string,
  tableId: string,
  change: (views: DatabaseView[]) => DatabaseView[]
): Promise<void> {
  return patchDetail(databaseId, (detail) => ({
    ...detail,
    tables: detail.tables.map((table) =>
      table.table.id === tableId
        ? { ...table, views: change(table.views) }
        : table
    ),
  }));
}

/**
 * Change one cached column as the service answered it, at the table version
 * it answered with. A cached table already past that version may hold a
 * later change to the column, so it is left alone.
 */
export function patchTableColumn(params: {
  databaseId: string;
  tableId: string;
  columnId: string;
  tableVersion: number;
  change: (column: ColumnDetail) => ColumnDetail;
}): Promise<void> {
  return patchDetail(params.databaseId, (detail) => ({
    ...detail,
    tables: detail.tables.map((entry) =>
      entry.table.id === params.tableId &&
      entry.table.version <= params.tableVersion
        ? {
            ...entry,
            table: { ...entry.table, version: params.tableVersion },
            columns: entry.columns.map((column) =>
              column.column.id === params.columnId
                ? params.change(column)
                : column
            ),
          }
        : entry
    ),
  }));
}

function isResult<Kind extends OpResult['kind']>(
  result: OpResult | undefined,
  kind: Kind
): result is Extract<OpResult, { kind: Kind }> {
  return result?.kind === kind;
}

/**
 * Apply one op and pick its result. A refusal reads the detail again, so
 * whatever was patched in ahead of the answer gives way to what is stored.
 */
export function applyOp<Kind extends OpResult['kind']>(
  databaseId: string,
  tableId: string,
  op: DatabaseOp,
  kind: Kind
): ResultAsync<Extract<OpResult, { kind: Kind }>, DatabaseOpFailure> {
  return applyDatabaseOps(databaseId, [op])
    .mapErr((error): DatabaseOpFailure => ({ kind: 'ops', error }))
    .andThen((results) => {
      const [result] = results;
      if (!isResult(result, kind))
        return errAsync<Extract<OpResult, { kind: Kind }>, DatabaseOpFailure>({
          kind: 'unexpected-result',
        });
      applyDatabaseTableVersions(databaseId, {
        [tableId]: result.tableVersion,
      });
      return okAsync(result);
    })
    .orElse((failure) => {
      void invalidateDatabase(databaseId);
      return errAsync(failure);
    });
}
