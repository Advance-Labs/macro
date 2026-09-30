import { databaseSqlAnswer } from '@core/database-sql/answer';
import { databaseSqlCatalog } from '@core/database-sql/catalog';
import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import { readDatabaseSql } from '@queries/database-sql/create-database-sql-query';
import {
  createSavedDatabaseQuery,
  useDatabaseQueryDefinition,
} from '@queries/storage/database-queries';
import {
  fetchViewerDatabases,
  useViewerDatabases,
} from '@queries/storage/databases';
import { useDatabaseTableChanges } from '@queries/storage/databases-sync';
import { databasesKeys } from '@queries/storage/keys';
import { useEntitySubscription } from '@service-connection/client';
import { storageServiceClient } from '@service-storage/client';
import type { Accessor } from 'solid-js';
import type { QueryCapabilities } from '../context/query-context';
import { createLiveQuerySource, type LiveQuerySource } from './query-source';
import { createQuestionCapabilities } from './question-capabilities';

/** Production transport adapters; the composer only receives these narrow capabilities. */
export const queryCapabilities: QueryCapabilities = createQuestionCapabilities({
  generate: async (input) => {
    const { generateDatabaseQuery } = await import(
      '@service-cognition/database-query'
    );
    return generateDatabaseQuery(input);
  },
  // A draft question may read any database the viewer can reach.
  read: async (sql) => {
    const databases = await fetchViewerDatabases();
    const catalog = databaseSqlCatalog(databases);
    return databaseSqlAnswer(
      await readDatabaseSql({ catalog, sql }),
      catalog,
      databases
    );
  },
  describe: (databaseId) =>
    queryClient.fetchQuery({
      queryKey: databasesKeys.detail(databaseId).queryKey,
      queryFn: () =>
        throwOnErr(() =>
          storageServiceClient.databases.get({ id: databaseId })
        ),
      staleTime: 0,
    }),
});

/** A saved question's live answer, run in the browser. */
export function createSavedQuestionSource(
  queryId: Accessor<string>
): LiveQuerySource {
  const definition = useDatabaseQueryDefinition(queryId);
  const viewer = useViewerDatabases();
  return createLiveQuerySource({
    statement: () =>
      definition.isSuccess
        ? {
            sql: definition.data.definition.query,
            databaseId: definition.data.databaseId ?? undefined,
          }
        : undefined,
    databases: viewer.databases,
    loadError: () => (definition.isError ? definition.error : viewer.error()),
    subscribe: (onChange) =>
      useDatabaseTableChanges((change) => onChange(change.tableId)),
  });
}

/** Saved queries are immutable: every new SQL text becomes a new row. */
export async function saveQuestionSql(input: {
  sql: string;
  databaseId?: string;
}): Promise<string> {
  const saved = await createSavedDatabaseQuery({
    definition: { version: 1, query: input.sql },
    ...(input.databaseId ? { databaseId: input.databaseId } : {}),
  });
  return saved.id;
}

export function trackQueryDatabase(id: string, onRefresh: () => void) {
  useEntitySubscription(
    () => ({ entity_type: 'database', entity_id: id }),
    onRefresh
  );
}
