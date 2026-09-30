import type { Catalog, Outcome } from '@core/database-sql/generated/types';
import type {
  DatabaseDetail,
  DatabaseTableDetail,
} from '@service-storage/databases';
import { describe, expect, it, vi } from 'vitest';
import { exportDatabaseTableCsv, importDatabaseTable } from './transfer';

const mocks = vi.hoisted(() => ({
  read: vi.fn(),
  import: vi.fn(),
  invalidate: vi.fn(),
}));
vi.mock('@queries/database-sql/create-database-sql-query', () => ({
  readDatabaseSql: mocks.read,
}));
vi.mock('@queries/storage/databases', () => ({
  invalidateDatabase: mocks.invalidate,
}));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: { databases: { importTable: mocks.import } },
}));
const table = {
  table: { id: 'table', database_id: 'database', name: 'Contacts' },
  sql_name: '"CRM"."Contacts"',
  columns: [
    {
      column: { id: 'name', display_name: 'Customer' },
      sql_name: '"Customer"',
      definition: {
        definition: {
          id: 'name-definition',
          display_name: 'Name',
          data_type: 'STRING',
          is_multi_select: false,
          specific_entity_type: null,
        },
        property_options: [],
      },
    },
  ],
} as unknown as DatabaseTableDetail;
const database = {
  database: { id: 'database', name: 'CRM' },
  grant: 'view',
  tables: [table],
} as unknown as DatabaseDetail;
/** The catalog the engine builds of `database`. */
const catalog: Catalog = {
  tables: [
    {
      id: 'table',
      databaseId: 'database',
      database: 'CRM',
      name: 'Contacts',
      source: 'database',
      columns: [
        {
          id: 'name-definition',
          placement: 'name',
          name: 'Customer',
          kind: { kind: 'text' },
        },
      ],
    },
  ],
};
function read(names: string[], truncated = false) {
  return { catalog, outcome: outcome(names, truncated) };
}
function outcome(names: string[], truncated = false): Outcome {
  return {
    columns: [{ name: 'Customer', column: 'name-definition', kind: 'text' }],
    rows: names.map((name) => [{ type: 'text', value: name }]),
    rowIds: names.map((_, index) => `id-${index}`),
    readTables: ['table'],
    truncated,
    insertedRowIds: [],
    changesApplied: 0,
  };
}

describe('CSV transfers', () => {
  it('exports user-facing headers, quotes values, and excludes internal row IDs, in table order', async () => {
    mocks.read.mockResolvedValueOnce(read(['00123', 'a,b']));
    const blob = await exportDatabaseTableCsv(database, table);
    const text = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = reject;
      reader.readAsText(blob);
    });
    expect(text).toBe('Customer\n00123\n"a,b"');
    expect(mocks.read).toHaveBeenCalledExactlyOnceWith({
      schema: {
        databases: [
          {
            id: 'database',
            name: 'CRM',
            tables: [
              {
                id: 'table',
                name: 'Contacts',
                columns: [
                  {
                    id: 'name',
                    definition: 'name-definition',
                    name: 'Customer',
                    property: {
                      dataType: 'STRING',
                      multi: false,
                      entityType: null,
                      relation: false,
                    },
                    options: [],
                  },
                ],
              },
            ],
          },
        ],
        platform: [],
      },
      scope: 'database',
      sql: 'SELECT * FROM "CRM"."Contacts" ORDER BY row_position',
    });
  });
  it('refuses a truncated read instead of downloading partial CSV', async () => {
    mocks.read.mockResolvedValueOnce(read([], true));
    await expect(exportDatabaseTableCsv(database, table)).rejects.toThrow(
      'too large'
    );
  });
  it('retains the same import identity and reports validation errors', async () => {
    const request = {
      requestId: 'request',
      name: 'Contacts',
      columns: ['Name'],
      rows: [['Ada']],
    };
    mocks.import.mockResolvedValueOnce({
      isErr: () => true,
      error: [{ code: 'INVALID_SCHEMA', message: 'Choose another name.' }],
    });
    await expect(
      importDatabaseTable('database', request)
    ).rejects.toMatchObject({
      message: 'Choose another name.',
      code: 'INVALID_SCHEMA',
    });
    mocks.import.mockResolvedValueOnce({
      isErr: () => false,
      value: { id: 'imported' },
    });
    expect(await importDatabaseTable('database', request)).toEqual({
      id: 'imported',
    });
    expect(mocks.import).toHaveBeenLastCalledWith({ id: 'database', request });
    expect(mocks.invalidate).toHaveBeenCalledWith('database');
  });
});
