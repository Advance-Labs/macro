import { afterEach, describe, expect, test } from 'bun:test';
import type {
  ColumnCast,
  DatabaseDetail,
  Table,
} from '../generated/storage/types.gen';
import { Macro } from '../src/macro';

const originalFetch = globalThis.fetch;
const databaseId = '0198a4cc-e138-7670-a308-a6b766602700';
const tableId = '0198a4cc-e138-7670-a308-a6b766602701';
const columnId = '0198a4cc-e138-7670-a308-a6b766602702';
const definitionId = '0198a4cc-e138-7670-a308-a6b766602709';
const host = 'https://storage.example.test';

function client() {
  return new Macro({ token: 'user-token', hosts: { storage: host } });
}

const ticketsTable: Table = {
  id: tableId,
  database_id: databaseId,
  name: 'Tickets',
  position: 'a0',
  version: 7,
};

/** One table, Tickets, whose one column is bound to the "Name" definition. */
const support: DatabaseDetail = {
  database: {
    id: databaseId,
    name: 'Support',
    owner_id: 'owner',
    created_at: '2026-10-01T00:00:00Z',
    trashed_at: null,
  },
  grant: 'owner',
  tables: [
    {
      table: ticketsTable,
      sql_name: 'tickets',
      read_sql_name: 'tickets',
      views: [],
      columns: [
        {
          column: {
            id: columnId,
            table_id: tableId,
            property_definition_id: definitionId,
            display_name: null,
            position: 'a0',
            infer_type: false,
            config: null,
          },
          definition: {
            definition: {
              id: definitionId,
              display_name: 'Name',
              data_type: 'STRING',
              is_multi_select: false,
              specific_entity_type: null,
              is_metadata: false,
              is_system: false,
              owner: { scope: 'database', database_id: databaseId },
              created_at: '2026-10-01T00:00:00Z',
              updated_at: '2026-10-01T00:00:00Z',
            },
            property_options: [],
          },
          shared_outside_database: false,
          sql_name: 'name',
          writable: true,
        },
      ],
    },
  ],
};

function intercept(
  respond: (request: Request) => Response | Promise<Response>,
) {
  globalThis.fetch = (async (input) =>
    respond(
      input instanceof Request ? input : new Request(input),
    )) as typeof fetch;
}

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe('Database', () => {
  test('renames with the previously read table name and reloads the schema', async () => {
    let current = support;
    let reads = 0;
    let renameBody: unknown;
    intercept(async (request) => {
      if (request.method === 'PATCH') {
        expect(request.url).toBe(
          `${host}/databases/${databaseId}/tables/${tableId}`,
        );
        renameBody = await request.json();
        const renamed: Table = { ...ticketsTable, name: 'Issues', version: 8 };
        current = {
          ...support,
          tables: support.tables.map((table) => ({ ...table, table: renamed })),
        };
        return Response.json(renamed);
      }
      reads++;
      return Response.json(current);
    });
    const table = await client().databases.byId(databaseId).table('Tickets');
    expect(table).toBeDefined();
    await table?.rename('Issues');
    expect(renameBody).toEqual({ name: 'Issues', previousName: 'Tickets' });
    await expect(table?.name()).resolves.toBe('Issues');
    expect(reads).toBe(2);
  });

  test('column rename uses its placement name and then reloads the placement', async () => {
    let current = support;
    let renameBody: unknown;
    intercept(async (request) => {
      if (request.method === 'PATCH') {
        expect(request.url).toBe(
          `${host}/databases/${databaseId}/tables/${tableId}/columns/${columnId}`,
        );
        renameBody = await request.json();
        current = {
          ...support,
          tables: support.tables.map((table) => ({
            ...table,
            columns: table.columns.map((column) => ({
              ...column,
              column: { ...column.column, display_name: 'Summary' },
            })),
          })),
        };
        return Response.json({ table_versions: { [tableId]: 8 } });
      }
      return Response.json(current);
    });
    const table = await client().databases.byId(databaseId).table('Tickets');
    const column = (await table?.columns())?.[0];
    expect(column).toBeDefined();
    await column?.rename('Summary');
    expect(renameBody).toEqual({ name: 'Summary', previousName: 'Name' });
    await expect(column?.name()).resolves.toBe('Summary');
  });

  test('forwards first-value inference version and rejects another database handle', async () => {
    const requests: Request[] = [];
    intercept((request) => {
      requests.push(request);
      return Response.json(request.method === 'GET' ? support : { version: 8 });
    });
    const macro = client();
    const database = macro.databases.byId(databaseId);
    const table = await database.table('Tickets');
    const column = (await table?.columns())?.[0];
    if (!column) throw new Error('Missing fixture column');
    await column.inferType({
      dataType: 'ENTITY',
      specificEntityType: 'USER',
      baseVersion: 7,
    });
    expect(requests[1]?.url).toBe(
      `${host}/databases/${databaseId}/tables/${tableId}/columns/${columnId}/infer-type`,
    );
    await expect(requests[1]?.json()).resolves.toEqual({
      data_type: 'ENTITY',
      specific_entity_type: 'USER',
      base_version: 7,
    });
    await expect(
      macro.databases
        .byId('other')
        .inferColumnType(column, { dataType: 'NUMBER', baseVersion: 7 }),
    ).rejects.toThrow('does not belong');
    expect(requests).toHaveLength(2);
  });

  test('guards type, ordering and deletion writes with the table version last read', async () => {
    const writes: { url: string; method: string; body: unknown }[] = [];
    let reads = 0;
    intercept(async (request) => {
      if (request.method === 'GET') {
        reads++;
        return Response.json(support);
      }
      writes.push({
        url: request.url,
        method: request.method,
        body: await request.json(),
      });
      return Response.json(
        request.url.endsWith('/type')
          ? {
              cleared_cells: 2,
              trimmed_cells: 1,
              table_versions: { [tableId]: 8 },
            }
          : { table_versions: { [tableId]: 8 } },
      );
    });
    const database = client().databases.byId(databaseId);
    const table = await database.table('Tickets');
    const column = (await table?.columns())?.[0];
    if (!table || !column) throw new Error('Missing fixture column');
    const outcome = await column.changeType({
      dataType: 'ENTITY',
      isMultiSelect: true,
      linkTo: table,
      clearInvalid: true,
    });
    expect(outcome).toEqual({
      cleared_cells: 2,
      trimmed_cells: 1,
      table_versions: { [tableId]: 8 },
    });
    await table.reorderColumns([column]);
    await column.delete();
    expect(writes).toEqual([
      {
        method: 'PATCH',
        url: `${host}/databases/${databaseId}/tables/${tableId}/columns/${columnId}/type`,
        body: {
          baseVersion: 7,
          dataType: 'ENTITY',
          isMultiSelect: true,
          linkToTableId: tableId,
          linkToDatabaseId: databaseId,
          clearInvalid: true,
        },
      },
      {
        method: 'PATCH',
        url: `${host}/databases/${databaseId}/tables/${tableId}/columns/order`,
        body: { columnIds: [columnId], baseVersion: 7 },
      },
      {
        method: 'DELETE',
        url: `${host}/databases/${databaseId}/tables/${tableId}/columns/${columnId}`,
        body: { baseVersion: 7 },
      },
    ]);
    await database.schema();
    expect(reads).toBe(4);
    await expect(
      client().databases.byId('other').deleteColumn(column),
    ).rejects.toThrow('does not belong');
    expect(writes).toHaveLength(3);
  });

  test('keeps the import identity and exposes owner-managed recipient grants', async () => {
    const writes: { url: string; body: unknown }[] = [];
    const permissions = {
      id: databaseId,
      owner: 'owner',
      channelSharePermissions: [],
    };
    intercept(async (request) => {
      if (request.method !== 'GET')
        writes.push({ url: request.url, body: await request.json() });
      return Response.json(
        request.url.endsWith('/import') ? { id: tableId } : permissions,
      );
    });
    const database = client().databases.byId(databaseId);
    const request = {
      requestId: '0198a4cc-e138-7670-a308-a6b766602703',
      name: 'Contacts',
      columns: ['Name', 'Postal code'],
      rows: [['Ada', '00123']],
    };
    expect((await database.importTable(request)).id).toBe(tableId);
    expect((await database.importTable(request)).id).toBe(tableId);
    expect(writes.slice(0, 2)).toEqual([
      { url: `${host}/databases/${databaseId}/import`, body: request },
      { url: `${host}/databases/${databaseId}/import`, body: request },
    ]);
    expect(await database.sharePermissions()).toEqual(permissions);
    const grants = {
      channelSharePermissions: [
        {
          operation: 'add' as const,
          channelId: 'channel',
          accessLevel: 'view' as const,
        },
      ],
    };
    expect(await database.updateSharePermissions(grants)).toEqual(permissions);
    expect(writes[2]).toEqual({
      url: `${host}/databases/${databaseId}/permissions`,
      body: grants,
    });
  });

  test('applies ops in one request and returns one result per op', async () => {
    const rowId = '0198a4cc-e138-7670-a308-a6b766602704';
    const writes: { url: string; method: string; body: unknown }[] = [];
    intercept(async (request) => {
      writes.push({
        url: request.url,
        method: request.method,
        body: await request.json(),
      });
      return Response.json({
        results: [
          {
            kind: 'rows_written',
            affected: 1,
            inserted: [rowId],
            tableVersion: 8,
          },
          { kind: 'rows_written', affected: 1, inserted: [], tableVersion: 8 },
        ],
      });
    });
    const database = client().databases.byId(databaseId);
    const results = await database.applyOps([
      {
        kind: 'insert_rows',
        table: tableId,
        rows: [
          [{ column: columnId, value: { type: 'text', value: 'Printer jam' } }],
        ],
      },
      { kind: 'delete_rows', table: tableId, rows: [rowId] },
    ]);
    expect(writes).toEqual([
      {
        method: 'POST',
        url: `${host}/databases/${databaseId}/ops`,
        body: {
          ops: [
            {
              kind: 'insert_rows',
              table: tableId,
              rows: [
                [
                  {
                    column: columnId,
                    value: { type: 'text', value: 'Printer jam' },
                  },
                ],
              ],
            },
            { kind: 'delete_rows', table: tableId, rows: [rowId] },
          ],
        },
      },
    ]);
    expect(results).toEqual([
      { kind: 'rows_written', affected: 1, inserted: [rowId], tableVersion: 8 },
      { kind: 'rows_written', affected: 1, inserted: [], tableVersion: 8 },
    ]);
  });

  test('reorders tables by handle, deletes one, and reloads the schema after each', async () => {
    const otherTableId = '0198a4cc-e138-7670-a308-a6b766602705';
    const writes: { url: string; method: string; body: unknown }[] = [];
    let reads = 0;
    intercept(async (request) => {
      if (request.method === 'GET') {
        reads++;
        return Response.json({
          database: { id: databaseId, name: 'Support' },
          tables: [
            { table: { id: tableId, name: 'Tickets' }, columns: [], views: [] },
            {
              table: { id: otherTableId, name: 'Customers' },
              columns: [],
              views: [],
            },
          ],
        });
      }
      if (request.method === 'DELETE') {
        writes.push({ url: request.url, method: 'DELETE', body: undefined });
        return new Response(null, { status: 204 });
      }
      writes.push({
        url: request.url,
        method: request.method,
        body: await request.json(),
      });
      return Response.json([
        { id: otherTableId, name: 'Customers' },
        { id: tableId, name: 'Tickets' },
      ]);
    });
    const database = client().databases.byId(databaseId);
    const [tickets, customers] = await database.tables();
    if (!tickets || !customers) throw new Error('Missing fixture tables');
    const reordered = await database.reorderTables([customers, tickets]);
    expect(reordered.map((table) => table.id)).toEqual([otherTableId, tableId]);
    expect(reordered[0]?.database).toBe(database);
    await database.schema();
    await customers.delete();
    await database.schema();
    expect(writes).toEqual([
      {
        method: 'PUT',
        url: `${host}/databases/${databaseId}/tables/order`,
        body: { tableIds: [otherTableId, tableId] },
      },
      {
        method: 'DELETE',
        url: `${host}/databases/${databaseId}/tables/${otherTableId}`,
        body: undefined,
      },
    ]);
    expect(reads).toBe(3);
    await expect(
      client().databases.byId('other').reorderTables([tickets]),
    ).rejects.toThrow('does not belong');
    await expect(
      client().databases.byId('other').deleteTable(tickets),
    ).rejects.toThrow('does not belong');
    expect(writes).toHaveLength(2);
  });

  test('lists what a column can be cast to', async () => {
    const casts: ColumnCast[] = [
      {
        data_type: 'NUMBER',
        is_multi_select: false,
        specific_entity_type: null,
        relation: false,
        cast: 'checked',
        failures: 2,
        examples: ['n/a', 'soon'],
        summary: "2 values aren't numbers",
        reason: null,
      },
    ];
    const urls: string[] = [];
    intercept((request) => {
      urls.push(request.url);
      return Response.json(request.url.endsWith('/casts') ? casts : support);
    });
    const table = await client().databases.byId(databaseId).table('Tickets');
    const column = (await table?.columns())?.[0];
    if (!column) throw new Error('Missing fixture column');
    expect(await column.casts()).toEqual(casts);
    expect(urls[1]).toBe(
      `${host}/databases/${databaseId}/tables/${tableId}/columns/${columnId}/casts`,
    );
  });

  test("exposes a table's views as handles and reads a board's card positions", async () => {
    const viewId = '0198a4cc-e138-7670-a308-a6b766602706';
    const rowId = '0198a4cc-e138-7670-a308-a6b766602707';
    const optionId = '0198a4cc-e138-7670-a308-a6b766602708';
    const board = {
      id: viewId,
      databaseId,
      tableId,
      name: 'By status',
      position: 'a0',
      query: { filter: null, sort: [] },
      layout: {
        kind: 'board',
        groupBy: columnId,
        cardFields: [columnId],
        hideEmptyLanes: false,
        lanes: [{ option: optionId }],
      },
      createdAt: '2026-10-01T00:00:00Z',
      updatedAt: '2026-10-01T00:00:00Z',
    };
    const urls: string[] = [];
    intercept((request) => {
      urls.push(request.url);
      if (request.url.endsWith('/positions'))
        return Response.json({
          positions: [{ row: rowId, lane: optionId, position: 'a0' }],
        });
      return Response.json({
        database: { id: databaseId, name: 'Support' },
        tables: [
          {
            table: { id: tableId, name: 'Tickets' },
            columns: [],
            views: [board],
          },
        ],
      });
    });
    const table = await client().databases.byId(databaseId).table('Tickets');
    if (!table) throw new Error('Missing fixture table');
    const views = await table.views();
    expect(views.map((view) => view.id)).toEqual([viewId]);
    const view = views[0];
    if (!view) throw new Error('Missing fixture view');
    expect(view.table).toBe(table);
    await expect(view.name()).resolves.toBe('By status');
    await expect(view.position()).resolves.toBe('a0');
    await expect(view.layout()).resolves.toEqual({
      kind: 'board',
      groupBy: columnId,
      cardFields: [columnId],
      hideEmptyLanes: false,
      lanes: [{ option: optionId }],
    });
    await expect(view.query()).resolves.toEqual({ filter: null, sort: [] });
    expect(await view.positions()).toEqual([
      { row: rowId, lane: optionId, position: 'a0' },
    ]);
    expect(urls).toEqual([
      `${host}/databases/${databaseId}`,
      `${host}/databases/${databaseId}/views/${viewId}/positions`,
    ]);
  });
});
