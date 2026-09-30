import { databaseSqlAnswer } from '@core/database-sql/answer';
import { databaseSqlCatalog } from '@core/database-sql/catalog';
import {
  createDatabaseSqlQuery,
  type DatabaseSqlQueryCapabilities,
  type DatabaseSqlStatement,
  refreshInBackground,
  sameDatabaseSqlStatement,
} from '@queries/database-sql/create-database-sql-query';
import type { DatabaseDetail } from '@service-storage/databases';
import { debounce } from '@solid-primitives/scheduled';
import { type Accessor, createMemo } from 'solid-js';
import type { QueryAnswer, QuerySchema } from '../core/query';

export function toQuerySchema(
  detail: DatabaseDetail,
  activeTableId?: string
): QuerySchema {
  // The one platform table the dialect exposes: `macro.people`, every
  // person the viewer can see, keyed by entity id so entity columns join to it.
  const platformTables: QuerySchema['tables'] = [
    {
      id: 'platform:people',
      name: 'People in your teams',
      sqlName: 'macro.people',
      primaryKey: 'id',
      columns: ['id', 'name', 'email'].map((name) => ({
        name,
        sqlName: `"${name}"`,
        type: 'String',
        multiple: false,
        options: [],
      })),
    },
  ];
  return {
    databaseId: detail.database.id,
    name: detail.database.name,
    focusTableId: detail.tables.find(({ table }) => table.id === activeTableId)
      ?.table.id,
    tables: [
      ...detail.tables.map((table) => ({
        id: table.table.id,
        name: table.table.name,
        sqlName: table.sql_name,
        primaryKey: 'row_id',
        columns: table.columns.map((column) => ({
          name:
            column.column.display_name ??
            column.definition.definition.display_name,
          sqlName: column.sql_name,
          type: column.definition.definition.data_type,
          multiple:
            column.definition.definition.is_multi_select ||
            column.column.config?.kind === 'link',
          relation:
            column.column.config?.kind === 'link'
              ? {
                  databaseId: column.column.config.database_id,
                  tableId: column.column.config.table_id,
                  writable: column.writable,
                }
              : undefined,
          options: column.definition.property_options.map((option) =>
            String(option.value.value)
          ),
        })),
      })),
      ...platformTables,
    ],
  };
}

/** A saved statement and the database it is scoped to. */
export type SavedStatement = { sql: string; databaseId?: string };

export type LiveQuerySource = {
  /** The last answer; kept while a rerun is in flight or after one fails. */
  answer: Accessor<QueryAnswer | undefined>;
  error: Accessor<unknown>;
  loading: Accessor<boolean>;
  /** Read the answer's tables from the server again. */
  refresh: () => Promise<void>;
};

/** Changes to a table an answer read rerun it once they settle. */
const RERUN_DELAY_MS = 300;

/**
 * Runs a saved query in the browser over every database the viewer can
 * reach, as the server would, and reruns it when a table it read changes.
 */
export function createLiveQuerySource(input: {
  /** Undefined until the saved query loads. */
  statement: Accessor<SavedStatement | undefined>;
  /** Undefined until they load. */
  databases: Accessor<DatabaseDetail[] | undefined>;
  /** Why the statement or the databases could not load. */
  loadError: Accessor<unknown>;
  subscribe: (onChange: (tableId: string) => void) => void;
  /** Where the engine reads rows from; the app's GraphQL client by default. */
  read?: DatabaseSqlQueryCapabilities;
}): LiveQuerySource {
  const statement = createMemo(
    (): DatabaseSqlStatement | undefined => {
      const saved = input.statement();
      const databases = input.databases();
      return (
        saved &&
        databases && {
          catalog: databaseSqlCatalog(databases, saved.databaseId),
          sql: saved.sql,
        }
      );
    },
    undefined,
    { equals: sameDatabaseSqlStatement }
  );
  const query = createDatabaseSqlQuery(statement, input.read);
  const answer = createMemo(() => {
    const outcome = query.outcome();
    const current = statement();
    const databases = input.databases();
    return outcome && current && databases
      ? databaseSqlAnswer(outcome, current.catalog, databases)
      : undefined;
  });
  const error = () => input.loadError() ?? query.error();
  const rerun = debounce(() => refreshInBackground(query), RERUN_DELAY_MS);
  input.subscribe((tableId) => {
    if (query.outcome()?.readTables.includes(tableId)) rerun();
  });
  return {
    answer,
    error,
    loading: () => query.loading() || (!answer() && error() === undefined),
    refresh: query.refresh,
  };
}
