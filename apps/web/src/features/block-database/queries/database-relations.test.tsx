import type { Outcome } from '@core/database-sql/protocol';
import type { DatabaseSqlQueryCapabilities } from '@queries/database-sql/create-database-sql-query';
import type {
  DatabaseColumnDetail,
  DatabaseDetail,
} from '@service-storage/databases';
import type { SoupQuery } from '@service-storage/graphql/generated/graphql';
import { cleanup, render, waitFor } from '@solidjs/testing-library';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { createClient, type Exchange } from '@urql/core';
import { ok } from 'neverthrow';
import { afterEach, expect, it, vi } from 'vitest';
import { empty, fromValue, mergeMap, pipe } from 'wonka';
import type { DatabaseRelationSource } from '../context/relation-source';
import { createDatabaseRelations } from './database-relations';

const transport = vi.hoisted(() => ({ get: vi.fn() }));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: { databases: transport },
}));
const nameColumn: DatabaseColumnDetail = {
  column: {
    id: 'name',
    table_id: 'customers',
    property_definition_id: 'name-definition',
    position: 'a',
    config: null,
  },
  sql_name: '"Name"',
  writable: true,
  definition: {
    definition: {
      id: 'name-definition',
      owner: { scope: 'database', database_id: 'db' },
      display_name: 'Name',
      data_type: 'STRING',
      is_multi_select: false,
      specific_entity_type: null,
      created_at: '',
      updated_at: '',
      is_system: false,
      is_metadata: false,
    },
    property_options: [],
  },
};
const detail: DatabaseDetail = {
  database: {
    id: 'db',
    name: 'Support',
    owner_id: 'owner',
    created_at: '',
    trashed_at: null,
  },
  grant: 'owner',
  tables: [
    {
      table: {
        id: 'customers',
        database_id: 'db',
        name: 'Customers',
        position: 'a',
        version: 1,
      },
      sql_name: '"Customers"',
      columns: [nameColumn],
    },
  ],
};
/** The engine's answer for the customers table, after one Soup page. */
function engine(names: () => string[]) {
  const reads: string[] = [];
  const exchange: Exchange = () => (incoming) =>
    pipe(
      incoming,
      mergeMap((operation) => {
        if (operation.kind === 'teardown') return empty;
        const data: SoupQuery = {
          user: {
            id: 'macro|viewer@databases.test',
            emailLinks: [],
            soup: { items: [], nextCursor: null },
          },
        };
        return fromValue({ operation, data, stale: false, hasNext: false });
      })
    );
  const client = createClient({
    url: 'http://test.invalid/graphql',
    exchanges: [exchange],
  });
  const read: DatabaseSqlQueryCapabilities = {
    client: () => client,
    cacheHost: () => undefined,
    people: async () => [],
    open: async (_catalog, sql) => {
      reads.push(sql);
      const answer: Outcome = {
        columns: [{ name: 'Name', column: 'name-definition', kind: 'text' }],
        rows: names().map((name) => [{ type: 'text', value: name }]),
        rowIds: names().map((_, index) => `customer-${index + 1}`),
        readTables: ['customers'],
        truncated: false,
        insertedRowIds: [],
        changesApplied: 0,
        failures: [],
      };
      return {
        start: () => ({
          step: 'fetch',
          id: 0,
          query: {
            type: 'soup',
            table: 'customers',
            propf: null,
            keyHint: null,
          },
          needs: ['name-definition'],
          cursor: null,
          limit: 500,
        }),
        feed_page: () => ({ step: 'done', ...answer }),
        feed_bins: () => {
          throw 'no bins';
        },
        free: () => {},
      };
    },
  };
  return { read, reads };
}
let client: QueryClient;
afterEach(() => {
  cleanup();
  client?.clear();
  vi.clearAllMocks();
});

it('shares one target-table read across relation columns and reads it again when the table changes', async () => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  transport.get.mockResolvedValue(ok(detail));
  let names = ['Acme'];
  const { read, reads } = engine(() => names);
  let tableChanged: (tableId: string) => void = () => {};
  let first!: DatabaseRelationSource;
  let second!: DatabaseRelationSource;
  function Harness() {
    const relations = createDatabaseRelations({
      columns: () =>
        ['customer', 'billing-customer'].map((id) => ({
          ...nameColumn,
          column: {
            ...nameColumn.column,
            id,
            config: {
              kind: 'link' as const,
              database_id: 'db',
              table_id: 'customers',
            },
          },
        })),
      read,
      onTableChanged: (listener) => {
        tableChanged = listener;
      },
    });
    first = relations('customers');
    second = relations('customers');
    return null;
  }
  render(() => (
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>
  ));
  await waitFor(() =>
    expect(first.rows()).toEqual([{ id: 'customer-1', name: 'Acme' }])
  );
  expect(second.rows()).toEqual(first.rows());
  expect(first.name()).toBe('Customers');
  expect(transport.get).toHaveBeenCalledTimes(1);
  expect(reads).toEqual(['SELECT * FROM "Customers" ORDER BY row_position']);

  names = ['Acme renamed'];
  tableChanged('other-table');
  tableChanged('customers');
  await waitFor(() => expect(first.rows()[0].name).toBe('Acme renamed'));
  expect(second.rows()[0].name).toBe('Acme renamed');
  expect(reads).toHaveLength(2);
});
