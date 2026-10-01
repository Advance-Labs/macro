/** The cached database detail, patched as option and view ops are sent and read again when one is refused. */
import type { DatabaseOp } from '@core/database-sql/generated/types';
import { queryClient } from '@queries/client';
import {
  applyDatabaseOps,
  applyDatabaseTableVersions,
  invalidateDatabase,
} from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import type { OpResult } from '@service-storage/generated/schemas/opResult';
import { errAsync, okAsync, type ResultAsync } from 'neverthrow';
import type { DatabaseOpFailure } from '../core/write-failure';

/** Change the cached detail in place; nothing happens before it is first read. */
export function patchDetail(
  databaseId: string,
  change: (detail: DatabaseDetail) => DatabaseDetail
) {
  queryClient.setQueryData(
    databasesKeys.detail(databaseId).queryKey,
    (previous: DatabaseDetail | undefined) => previous && change(previous)
  );
}

/** Change one table's cached views. */
export function patchViews(
  databaseId: string,
  tableId: string,
  change: (views: DatabaseView[]) => DatabaseView[]
) {
  patchDetail(databaseId, (detail) => ({
    ...detail,
    tables: detail.tables.map((table) =>
      table.table.id === tableId
        ? { ...table, views: change(table.views) }
        : table
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
