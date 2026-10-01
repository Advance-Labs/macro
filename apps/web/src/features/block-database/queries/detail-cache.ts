/**
 * The cached database detail as option and view ops change it: patched at
 * once, so the change shows before the server answers, and read again when
 * the server refuses it.
 */
import type {
  DatabaseOp,
  DatabaseView,
  OpResult,
} from '@core/database-sql/generated/types';
import { queryClient } from '@queries/client';
import {
  applyDatabaseOps,
  applyDatabaseTableVersions,
  invalidateDatabase,
} from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { TableDetail } from '@service-storage/generated/schemas/tableDetail';
import { errAsync, okAsync, type ResultAsync } from 'neverthrow';
import type { DatabaseOpFailure } from '../core/write-failure';

/**
 * A table's views as the server sends them. The wire is the engine's
 * camelCase `DatabaseView`; the generated OpenAPI type spells a board's
 * fields in snake case because utoipa ignores serde's `rename_all_fields`.
 */
export function tableViews(table: TableDetail): DatabaseView[] {
  return table.views as unknown as DatabaseView[];
}

function withTableViews(
  table: TableDetail,
  views: DatabaseView[]
): TableDetail {
  return { ...table, views: views as unknown as TableDetail['views'] };
}

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
        ? withTableViews(table, change(tableViews(table)))
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
