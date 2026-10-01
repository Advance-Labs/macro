import { databaseSqlSchema } from '@core/database-sql/catalog';
import type { Outcome } from '@core/database-sql/generated/types';
import { throwOnErr } from '@core/util/result';
import {
  createDatabaseSqlQuery,
  type DatabaseSqlQueryCapabilities,
  type DatabaseSqlStatement,
  refreshInBackground,
  sameDatabaseSqlStatement,
} from '@queries/database-sql/create-database-sql-query';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { TableDetail } from '@service-storage/generated/schemas/tableDetail';
import { useQueries } from '@tanstack/solid-query';
import { err, ok, okAsync, type Result, ResultAsync } from 'neverthrow';
import { type Accessor, createMemo, mapArray } from 'solid-js';
import type {
  DatabaseRelationFailure,
  DatabaseRelationSource,
} from '../context/relation-source';
import type { DatabaseRelatedRow } from '../core/database-relations';
import { tableRowsStatement } from '../sql';

/** A table's rows by id, named by its title column. */
export function relatedRows(
  table: TableDetail,
  outcome: Outcome
): DatabaseRelatedRow[] {
  const title =
    table.columns.find(
      (column) =>
        !column.column.config &&
        column.definition.definition.data_type === 'STRING' &&
        !column.definition.definition.is_multi_select
    ) ??
    table.columns.find(
      (column) =>
        !column.column.config && !column.definition.definition.is_multi_select
    );
  const titleIndex = outcome.columns.findIndex(
    (column) => !!title && column.column === title.definition.definition.id
  );
  return outcome.rowIds.map((id, index) => {
    const cell = outcome.rows[index]?.[titleIndex];
    const name =
      cell?.type === 'text' || cell?.type === 'number'
        ? String(cell.value).trim()
        : '';
    return { id, name: name || 'Unnamed' };
  });
}

/** A related table: the one a relation column's rows belong to. */
export type DatabaseRelationTarget = { databaseId: string; tableId: string };

/** One live read per target table, shared by every visible relation cell. */
export function createDatabaseRelations(props: {
  targets: Accessor<DatabaseRelationTarget[]>;
  /** Where the engine reads rows from; the app's GraphQL client by default. */
  read?: DatabaseSqlQueryCapabilities;
  /** Calls back with the table of each change the gateway reports. */
  onTableChanged: (listener: (tableId: string) => void) => void;
}) {
  const targets = createMemo(() => [
    ...new Map(
      props.targets().map((target) => [target.tableId, target] as const)
    ).values(),
  ]);
  const databases = createMemo(() => [
    ...new Set(targets().map((target) => target.databaseId)),
  ]);
  const details = useQueries(() => ({
    queries: databases().map((id) => ({
      queryKey: databasesKeys.detail(id).queryKey,
      queryFn: () =>
        throwOnErr(() => storageServiceClient.databases.get({ id })),
      staleTime: 30_000,
      throwOnError: false,
    })),
  }));
  const reads = createMemo(
    mapArray(
      () => targets().map((target) => target.tableId),
      (tableId) => {
        const detail = () => {
          const target = targets().find(
            (candidate) => candidate.tableId === tableId
          );
          return target && details[databases().indexOf(target.databaseId)];
        };
        const loaded = () => {
          const query = detail();
          return query?.isSuccess ? query.data : undefined;
        };
        const table = () =>
          loaded()?.tables.find((entry) => entry.table.id === tableId);
        const statement = createMemo(
          (): DatabaseSqlStatement | undefined => {
            const database = loaded();
            const target = table();
            return (
              database &&
              target && {
                schema: databaseSqlSchema([{ ...database, tables: [target] }]),
                scope: database.database.id,
                sql: tableRowsStatement(target.sql_name),
              }
            );
          },
          undefined,
          { equals: sameDatabaseSqlStatement }
        );
        const query = createDatabaseSqlQuery(statement, props.read);
        const rows = createMemo(() => {
          const outcome = query.outcome();
          const target = table();
          return outcome && target ? relatedRows(target, outcome) : [];
        });
        return { tableId, detail, table, query, rows };
      }
    )
  );
  props.onTableChanged((tableId) => {
    const read = reads().find((candidate) => candidate.tableId === tableId);
    if (read) refreshInBackground(read.query);
  });
  return (tableId: string): DatabaseRelationSource => {
    const read = () =>
      reads().find((candidate) => candidate.tableId === tableId);
    return {
      name: () => read()?.table()?.table.name ?? 'Related records',
      rows: () => read()?.rows() ?? [],
      loading: () => {
        const current = read();
        return (
          !!current?.detail()?.isPending ||
          (!!current?.table() &&
            !current.query.outcome() &&
            current.query.error() === undefined)
        );
      },
      error: () => {
        const current = read();
        if (current?.detail()?.isError || current?.query.error() !== undefined)
          return 'Related records could not be loaded.';
        if (!current?.detail()?.isPending && !current?.table())
          return 'This related table is unavailable.';
      },
      refresh: () => {
        const current = read();
        if (!current) return okAsync(undefined);
        const refetch = async (): Promise<
          Result<void, DatabaseRelationFailure>
        > =>
          (await current.detail()?.refetch())?.isError
            ? err({ kind: 'table-unavailable' })
            : ok(undefined);
        return new ResultAsync(refetch()).andThen(() =>
          current.query.refresh()
        );
      },
    };
  };
}
