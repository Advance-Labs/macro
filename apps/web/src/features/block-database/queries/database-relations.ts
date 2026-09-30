import { databaseSqlAnswer } from '@core/database-sql/answer';
import { databaseSqlCatalog } from '@core/database-sql/catalog';
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
import type {
  DatabaseColumnDetail,
  DatabaseTableDetail,
  QueryResult,
} from '@service-storage/databases';
import { useQueries } from '@tanstack/solid-query';
import { type Accessor, createMemo, mapArray } from 'solid-js';
import type { DatabaseRelationSource } from '../context/relation-source';
import type { DatabaseRelatedRow } from '../core/database-relations';
import { ROW_ID_COLUMN, resultColumnName, tableRowsStatement } from '../sql';

export function relatedRows(
  table: DatabaseTableDetail,
  result: QueryResult | undefined
): DatabaseRelatedRow[] {
  if (!result) return [];
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
  const rowIndex = result.columns.findIndex(
    (column) => column.name === ROW_ID_COLUMN
  );
  const titleIndex = result.columns.findIndex(
    (column) => !!title && column.name === resultColumnName(title)
  );
  return result.rows.flatMap((row) =>
    typeof row[rowIndex] === 'string'
      ? [
          {
            id: String(row[rowIndex]),
            name: String(row[titleIndex] ?? '').trim() || 'Unnamed',
          },
        ]
      : []
  );
}

/** One live read per target table, shared by every visible relation cell. */
export function createDatabaseRelations(props: {
  columns: Accessor<DatabaseColumnDetail[]>;
  /** Where the engine reads rows from; the app's GraphQL client by default. */
  read?: DatabaseSqlQueryCapabilities;
  /** Calls back with the table of each change the gateway reports. */
  onTableChanged: (listener: (tableId: string) => void) => void;
}) {
  const targets = createMemo(() => {
    const unique = new Map<string, { databaseId: string; tableId: string }>();
    for (const column of props.columns()) {
      const config = column.column.config;
      if (config?.kind === 'link')
        unique.set(config.table_id, {
          databaseId: config.database_id,
          tableId: config.table_id,
        });
    }
    return [...unique.values()];
  });
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
                catalog: databaseSqlCatalog(
                  [{ ...database, tables: [target] }],
                  database.database.id
                ),
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
          const current = statement();
          const target = table();
          return outcome && current && target
            ? relatedRows(
                target,
                databaseSqlAnswer(outcome, current.catalog, []).results[0]
              )
            : [];
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
      refresh: async () => {
        await read()?.detail()?.refetch({ throwOnError: true });
        await read()?.query.refresh();
      },
    };
  };
}
