import type { Outcome, Step } from '@core/database-sql/protocol';
import type { DatabaseSqlQueryCapabilities } from '@queries/database-sql/create-database-sql-query';
import { databasesKeys } from '@queries/storage/keys';
import type {
  DatabaseDetail,
  ExecOutcome,
  ExecRequest,
} from '@service-storage/databases';
import type { SoupQuery } from '@service-storage/graphql/generated/graphql';
import { cleanup, render, waitFor } from '@solidjs/testing-library';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { CombinedError, createClient, type Exchange } from '@urql/core';
import { err, ok } from 'neverthrow';
import { type Accessor, createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { empty, fromValue, mergeMap, pipe } from 'wonka';
import {
  type DatabaseRowsSource,
  DatabaseWriteOutcomeUnknown,
} from '../context/table-source';
import {
  type DatabaseViewConfig,
  defaultDatabaseView,
} from '../core/database-view';
import type { DatabaseRowMutation } from '../core/table';
import { createDraftRows } from '../primitives/draft-rows';
import { createTableController } from '../primitives/table-controller';
import { createDatabaseRowsSource } from './table-rows';

const transport = vi.hoisted(() => ({
  get: vi.fn(),
  inferColumnType: vi.fn(),
}));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: { databases: transport },
}));

function detail(sqlName = '"guests"'): DatabaseDetail {
  return {
    database: {
      id: 'db',
      name: 'Personal',
      owner_id: 'owner',
      created_at: '',
      trashed_at: null,
    },
    grant: 'owner',
    tables: [
      {
        table: {
          id: 'guests-table',
          database_id: 'db',
          name: 'Guests',
          position: 'a',
          version: 5,
        },
        sql_name: sqlName,
        columns: [
          {
            column: {
              id: 'name',
              table_id: 'guests-table',
              property_definition_id: 'definition',
              position: 'a',
              config: null,
            },
            sql_name: '"Name"',
            writable: true,
            definition: {
              definition: {
                id: 'definition',
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
          },
        ],
      },
    ],
  };
}

/** The engine's answer for `SELECT * FROM "guests"` over one row. */
function guests(
  rows: { id: string; name: string | null }[] = [{ id: 'record', name: 'Ada' }]
): Outcome {
  return {
    columns: [{ name: 'Name', column: 'definition', kind: 'text' }],
    rows: rows.map((row) => [
      row.name === null ? null : { type: 'text', value: row.name },
    ]),
    rowIds: rows.map((row) => row.id),
    readTables: ['guests-table'],
    truncated: false,
    insertedRowIds: [],
    changesApplied: 0,
    failures: [],
  };
}
const written: ExecOutcome = {
  results: [],
  changes_applied: 1,
  inserted_row_ids: [],
  new_versions: { 'guests-table': 6 },
  read_tables: [],
  read_versions: {},
  truncated_tables: [],
};
const edit: DatabaseRowMutation = {
  kind: 'cell',
  rowId: 'record',
  columnId: 'name',
  value: 'Grace',
};
const clients: QueryClient[] = [];

/**
 * The browser engine, answering each statement it compiles from `answer`
 * after one Soup page. Soup fails while `offline` says so; `answer` throws
 * a string the way the engine refuses a statement.
 */
function engine(
  answer: (sql: string) => Outcome | Promise<Outcome> = () => guests(),
  offline: () => boolean = () => false
) {
  const reads: string[] = [];
  const exchange: Exchange = () => (incoming) =>
    pipe(
      incoming,
      mergeMap((operation) => {
        if (operation.kind === 'teardown') return empty;
        if (offline())
          return fromValue({
            operation,
            error: new CombinedError({ networkError: new Error('Offline') }),
            stale: false,
            hasNext: false,
          });
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
  const fetch: Step = {
    step: 'fetch',
    id: 0,
    query: { type: 'soup', table: 'guests-table', propf: null, keyHint: null },
    needs: ['definition'],
    cursor: null,
    limit: 500,
  };
  const read: DatabaseSqlQueryCapabilities = {
    client: () => client,
    cacheHost: () => undefined,
    people: async () => [],
    open: async (_catalog, sql) => {
      reads.push(sql);
      const outcome = await answer(sql);
      return {
        start: () => fetch,
        feed_page: () => ({ step: 'done', ...outcome }),
        feed_bins: () => {
          throw 'no bins';
        },
        free: () => {},
      };
    },
  };
  return { read, reads };
}

function setup(
  initialDetail: DatabaseDetail,
  exec: (request: ExecRequest) => Promise<ExecOutcome>,
  options: {
    read?: DatabaseSqlQueryCapabilities;
    addOption?: DatabaseRowsSource['addOption'];
    onSource?: (source: DatabaseRowsSource) => void;
    view?: Accessor<DatabaseViewConfig>;
  } = {}
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  clients.push(client);
  client.setQueryData(databasesKeys.detail('db').queryKey, initialDetail);
  // Like the app, a write's versions land in the cached schema.
  const applyVersions = vi.fn((versions: Record<string, number>) =>
    client.setQueryData(
      databasesKeys.detail('db').queryKey,
      (previous: DatabaseDetail | undefined) =>
        previous && {
          ...previous,
          tables: previous.tables.map((entry) => ({
            ...entry,
            table: {
              ...entry.table,
              version: versions[entry.table.id] ?? entry.table.version,
            },
          })),
        }
    )
  );
  let tableChanged: (version: number) => void = () => {};
  let source!: DatabaseRowsSource;
  function Harness() {
    source = createDatabaseRowsSource({
      databaseId: 'db',
      // Deliberately retain old props: retries must use the refreshed cache.
      table: () => initialDetail.tables[0],
      view: options.view ?? defaultDatabaseView,
      exec,
      read: options.read ?? engine().read,
      onTableChanged: (listener) => {
        tableChanged = listener;
      },
      applyVersions,
      addOption: options.addOption ?? (async () => {}),
    });
    options.onSource?.(source);
    return null;
  }
  const { unmount } = render(() => (
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>
  ));
  return {
    source,
    client,
    applyVersions,
    unmount,
    tableChanged: (version: number) => tableChanged(version),
  };
}

afterEach(() => {
  cleanup();
  for (const client of clients) client.clear();
  clients.length = 0;
  vi.resetAllMocks();
});

describe('database view reads', () => {
  it("runs the view's statement in the engine and keeps the previous rows while a changed view loads", async () => {
    const [view, setView] = createSignal(defaultDatabaseView());
    let finishSearch!: (outcome: Outcome) => void;
    const { read, reads } = engine((sql) =>
      sql.includes('LIKE')
        ? new Promise((resolve) => {
            finishSearch = resolve;
          })
        : guests()
    );
    const exec = vi.fn<(request: ExecRequest) => Promise<ExecOutcome>>();
    const { source } = setup(detail(), exec, { read, view });
    await waitFor(() =>
      expect(source.snapshot()?.rows).toEqual([
        { rowId: 'record', cells: { name: 'Ada' } },
      ])
    );

    setView({ ...defaultDatabaseView(), search: 'grace' });
    await waitFor(() =>
      expect(reads.at(-1)).toBe(
        'SELECT * FROM "guests" WHERE "Name" LIKE \'%grace%\' ORDER BY row_position'
      )
    );
    expect(source.loading()).toBe(false);
    expect(source.refreshing()).toBe(true);
    expect(source.snapshot()?.rows).toEqual([
      { rowId: 'record', cells: { name: 'Ada' } },
    ]);

    finishSearch(guests([{ id: 'other', name: 'Grace' }]));
    await waitFor(() =>
      expect(source.snapshot()?.rows).toEqual([
        { rowId: 'other', cells: { name: 'Grace' } },
      ])
    );
    expect(reads[0]).toBe('SELECT * FROM "guests" ORDER BY row_position');
    expect(exec).not.toHaveBeenCalled();
  });

  it('reads the rows the view retains by id, apart from its statement', async () => {
    const { read, reads } = engine((sql) =>
      sql.includes('row_id IN')
        ? guests([{ id: 'kept', name: 'Hidden' }])
        : guests()
    );
    const { source } = setup(detail(), vi.fn(), {
      read,
      view: () => ({ ...defaultDatabaseView(), search: 'ada' }),
    });
    const [retained, setRetained] = createSignal<string[]>([]);
    source.retain(retained);
    await waitFor(() => expect(source.snapshot()?.retained).toEqual([]));
    expect(reads).toEqual([
      'SELECT * FROM "guests" WHERE "Name" LIKE \'%ada%\' ORDER BY row_position',
    ]);

    setRetained(['kept']);
    await waitFor(() =>
      expect(source.snapshot()?.retained).toEqual([
        { rowId: 'kept', cells: { name: 'Hidden' } },
      ])
    );
    expect(reads.at(-1)).toBe(
      'SELECT * FROM "guests" WHERE row_id IN (\'kept\')'
    );
    expect(source.snapshot()?.rows).toEqual([
      { rowId: 'record', cells: { name: 'Ada' } },
    ]);
  });

  it("reads again when another viewer changes the table, but not for this writer's own change", async () => {
    let name = 'Ada';
    const { read, reads } = engine(() => guests([{ id: 'record', name }]));
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source, tableChanged } = setup(detail(), exec, { read });
    await waitFor(() => expect(source.snapshot()?.version).toBe(5));

    await source.write(edit, 5);
    await source.refresh();
    expect(source.snapshot()?.version).toBe(6);
    const afterOwnWrite = reads.length;
    tableChanged(6);
    expect(reads).toHaveLength(afterOwnWrite);

    name = 'Grace';
    tableChanged(7);
    await waitFor(() =>
      expect(source.snapshot()).toEqual({
        version: 7,
        rows: [{ rowId: 'record', cells: { name: 'Grace' } }],
        retained: [],
      })
    );
    expect(reads).toHaveLength(afterOwnWrite + 1);
  });

  it('shows a failed read as the grid error and keeps the rows it had', async () => {
    let offline = false;
    const { read } = engine(
      () => guests(),
      () => offline
    );
    const { source, tableChanged } = setup(detail(), vi.fn(), { read });
    await waitFor(() => expect(source.snapshot()?.rows).toHaveLength(1));

    offline = true;
    tableChanged(9);
    await waitFor(() => expect(source.error()?.message).toContain('Offline'));
    expect(source.snapshot()).toEqual({
      version: 5,
      rows: [{ rowId: 'record', cells: { name: 'Ada' } }],
      retained: [],
    });
  });
});

describe('database rows SQL names', () => {
  it('replaces a relation cell with a list of row ids', async () => {
    const schema = detail();
    const column = schema.tables[0].columns[0];
    column.column.config = {
      kind: 'link',
      database_id: 'db',
      table_id: 'customers',
    };
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source } = setup(schema, exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    expect(source.columns()[0]).toMatchObject({
      writable: true,
      isMultiSelect: true,
      relation: { tableId: 'customers' },
    });
    exec.mockResolvedValueOnce(written);
    await source.write(
      {
        kind: 'cell',
        rowId: 'record',
        columnId: 'name',
        value: '["customer-1","customer-2"]',
      },
      5
    );
    expect(exec).toHaveBeenLastCalledWith({
      sql: "UPDATE \"guests\" SET \"Name\" = ['customer-1', 'customer-2'] WHERE row_id = 'record'",
    });
  });

  it('creates a Customer-first row with its links in one INSERT without assigning its identity', async () => {
    const schema = detail();
    const column = schema.tables[0].columns[0];
    column.column.config = {
      kind: 'link',
      database_id: 'db',
      table_id: 'customers',
    };
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source } = setup(schema, exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    exec.mockClear();
    exec.mockResolvedValueOnce({
      ...written,
      inserted_row_ids: ['new-record'],
    });
    const result = await source.write(
      { kind: 'create', values: { name: '["customer-1"]' } },
      5
    );
    expect(exec).toHaveBeenCalledTimes(1);
    expect(exec).toHaveBeenCalledWith({
      sql: 'INSERT INTO "guests" ("Name") VALUES ([\'customer-1\'])',
    });
    expect(result.insertedRowIds).toEqual(['new-record']);
  });

  it('refuses relation writes when the column is read-only', async () => {
    const schema = detail();
    const column = schema.tables[0].columns[0];
    column.column.config = {
      kind: 'link',
      database_id: 'db',
      table_id: 'customers',
    };
    column.writable = false;
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source } = setup(schema, exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    exec.mockClear();
    expect(source.columns()[0].writable).toBe(false);
    await expect(source.write(edit, 5)).rejects.toThrow('read-only');
    expect(exec).not.toHaveBeenCalled();
  });

  it('distinguishes an uncertain INSERT response from a definitive SQL refusal', async () => {
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source } = setup(detail(), exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    exec.mockRejectedValueOnce(
      Object.assign(new Error('Connection closed'), { code: 'HTTP_ERROR' })
    );
    await expect(
      source.write({ kind: 'create', values: { name: 'Ada' } }, 5)
    ).rejects.toBeInstanceOf(DatabaseWriteOutcomeUnknown);

    const refused = Object.assign(new Error('Invalid value'), {
      code: 'SQL_ERROR',
    });
    transport.get.mockResolvedValue(ok(detail()));
    exec.mockRejectedValueOnce(refused);
    await expect(
      source.write({ kind: 'create', values: { name: 'Ada' } }, 5)
    ).rejects.toBe(refused);
  });

  it('reads through the quoted display name and matches result columns by display name', async () => {
    const { read, reads } = engine();
    const { source } = setup(detail('"Guest List"'), vi.fn(), { read });
    await waitFor(() =>
      expect(source.snapshot()?.rows).toEqual([
        { rowId: 'record', cells: { name: 'Ada' } },
      ])
    );
    expect(reads).toEqual(['SELECT * FROM "Guest List" ORDER BY row_position']);
  });

  it('refreshes only this database after SQL_ERROR and rebuilds the physical write name on explicit retry', async () => {
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source, client, applyVersions } = setup(detail(), exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    const collision = Object.assign(new Error('no such table: guests'), {
      code: 'SQL_ERROR',
    });
    const refreshed = detail('"Personal Guests"');
    transport.get.mockResolvedValue(ok(refreshed));
    exec.mockRejectedValueOnce(collision).mockResolvedValueOnce(written);

    await expect(source.write(edit, 5)).rejects.toBe(collision);
    expect(transport.get).toHaveBeenCalledExactlyOnceWith({ id: 'db' });
    expect(exec).toHaveBeenCalledTimes(1);
    expect(applyVersions).not.toHaveBeenCalled();
    expect(client.getQueryData(databasesKeys.detail('db').queryKey)).toEqual(
      refreshed
    );

    await expect(source.write(edit, 5)).resolves.toEqual({
      insertedRowIds: [],
      version: 6,
    });
    expect(exec).toHaveBeenLastCalledWith({
      sql: 'UPDATE "Personal Guests" SET "Name" = \'Grace\' WHERE row_id = \'record\'',
    });
    expect(transport.get).toHaveBeenCalledTimes(1);
    expect(applyVersions).toHaveBeenCalledExactlyOnceWith({
      'guests-table': 6,
    });
  });

  it('retains the original error and blocks stale writes until schema recovery succeeds', async () => {
    const exec = vi
      .fn<(request: ExecRequest) => Promise<ExecOutcome>>()
      .mockResolvedValue(written);
    const { source } = setup(detail(), exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    const collision = Object.assign(new Error('no such table: guests'), {
      code: 'SQL_ERROR',
    });
    transport.get.mockResolvedValue(
      err([{ code: 'HTTP_ERROR', message: 'Connection lost' }])
    );
    exec.mockRejectedValueOnce(collision).mockResolvedValueOnce(written);

    await expect(source.write(edit, 5)).rejects.toBe(collision);
    await expect(source.write(edit, 5)).rejects.toBe(collision);
    expect(transport.get).toHaveBeenCalledTimes(2);
    expect(exec).toHaveBeenCalledTimes(1);

    transport.get.mockResolvedValue(ok(detail('"Personal Guests"')));
    await expect(source.write(edit, 5)).resolves.toEqual({
      insertedRowIds: [],
      version: 6,
    });
    expect(transport.get).toHaveBeenCalledTimes(3);
    expect(exec).toHaveBeenCalledTimes(2);
    expect(exec).toHaveBeenLastCalledWith({
      sql: 'UPDATE "Personal Guests" SET "Name" = \'Grace\' WHERE row_id = \'record\'',
    });
  });

  it('recovers a read of a stale table name through the refreshed schema', async () => {
    let refused = true;
    const { read, reads } = engine(() => {
      if (refused) {
        refused = false;
        throw 'no such table: guests';
      }
      return guests();
    });
    const { source } = setup(detail(), vi.fn(), { read });
    await waitFor(() =>
      expect(source.error()?.message).toBe('no such table: guests')
    );
    transport.get.mockResolvedValue(ok(detail('"Personal Guests"')));

    await source.refresh();
    await waitFor(() => expect(source.error()).toBeUndefined());
    expect(transport.get).toHaveBeenCalledExactlyOnceWith({ id: 'db' });
    expect(reads[0]).toBe('SELECT * FROM "guests" ORDER BY row_position');
    expect(reads.at(-1)).toBe(
      'SELECT * FROM "Personal Guests" ORDER BY row_position'
    );
    expect(source.snapshot()?.rows).toEqual([
      { rowId: 'record', cells: { name: 'Ada' } },
    ]);
  });
});

describe('accepted writes after switching tables', () => {
  it('reads the option schema version after disposal before finishing an accepted draft field', async () => {
    const initial = detail();
    const status = structuredClone(initial.tables[0].columns[0]);
    status.column.id = 'status';
    status.column.property_definition_id = 'status-definition';
    status.sql_name = '"Status"';
    status.definition.definition.id = 'status-definition';
    status.definition.definition.display_name = 'Status';
    initial.tables[0].columns.push(status);

    let version = 5;
    let persisted: { id: string; name: string; status: string | null }[] = [];
    let releaseCreate!: () => void;
    const createReady = new Promise<void>((resolve) => {
      releaseCreate = resolve;
    });
    const { read } = engine(() => ({
      ...guests([]),
      columns: [
        { name: 'Name', column: 'definition', kind: 'text' },
        { name: 'Status', column: 'status-definition', kind: 'text' },
      ],
      rows: persisted.map((row) => [
        { type: 'text', value: row.name },
        row.status === null ? null : { type: 'text', value: row.status },
      ]),
      rowIds: persisted.map((row) => row.id),
    }));
    const writes: ExecRequest[] = [];
    const exec = vi.fn(async (request: ExecRequest): Promise<ExecOutcome> => {
      writes.push(request);
      const creating = request.sql.startsWith('INSERT');
      if (creating) await createReady;
      if (creating)
        persisted = [
          { id: 'server-record', name: 'Accepted record', status: null },
        ];
      else persisted[0].status = 'In review';
      version += 1;
      return {
        ...written,
        inserted_row_ids: creating ? ['server-record'] : [],
        new_versions: { 'guests-table': version },
      };
    });
    const addOption = vi.fn(async () => {
      // Real option creation changes the schema and advances the table's CAS.
      version += 1;
    });
    let drafts!: ReturnType<typeof createDraftRows>;
    let controller!: ReturnType<typeof createTableController>;
    const { source, unmount } = setup(initial, exec, {
      read,
      addOption,
      onSource(source) {
        controller = createTableController(source);
        drafts = createDraftRows(controller);
      },
    });
    await waitFor(() => expect(source.snapshot()?.version).toBe(5));
    const draftId = drafts.blankId();
    const name = drafts.write(draftId, 'name', 'Accepted record');
    await waitFor(() => expect(writes).toHaveLength(1));
    const statusWrite = drafts.write(
      draftId,
      'status',
      'In review',
      'In review'
    );
    unmount();
    releaseCreate();

    expect(await Promise.all([name, statusWrite])).toEqual([true, true]);
    expect(addOption).toHaveBeenCalledExactlyOnceWith('status', 'In review');
    expect(writes).toEqual([
      {
        sql: 'INSERT INTO "guests" ("Name") VALUES (\'Accepted record\')',
      },
      {
        sql: 'UPDATE "guests" SET "Status" = \'In review\' WHERE row_id = \'server-record\'',
      },
    ]);
    expect(source.snapshot()).toEqual({
      version: 8,
      rows: [
        {
          rowId: 'server-record',
          cells: { name: 'Accepted record', status: 'In review' },
        },
      ],
      retained: [],
    });
    expect(controller.failure()).toBeUndefined();
  });

  it('keeps the last actual read when a refresh fails after disposal', async () => {
    let offline = false;
    const { read } = engine(
      () => guests(),
      () => offline
    );
    const { source, client, unmount } = setup(detail(), vi.fn(), { read });
    await waitFor(() => expect(source.snapshot()?.version).toBe(5));
    unmount();
    const newerSchema = detail();
    newerSchema.tables[0].table.version = 6;
    client.setQueryData(databasesKeys.detail('db').queryKey, newerSchema);
    await source.refresh();
    expect(source.snapshot()?.version).toBe(6);

    newerSchema.tables[0].table.version = 10;
    client.setQueryData(databasesKeys.detail('db').queryKey, {
      ...newerSchema,
    });
    offline = true;
    await expect(source.refresh()).rejects.toThrow('Offline');
    expect(source.snapshot()?.version).toBe(6);
    expect(source.snapshot()?.rows).toEqual([
      { rowId: 'record', cells: { name: 'Ada' } },
    ]);
  });
});

describe('first-entry column types', () => {
  function inferredDetail(
    dataType: 'STRING' | 'NUMBER' | 'ENTITY',
    entityType?: 'USER' | 'DOCUMENT'
  ) {
    const value = detail();
    const column = value.tables[0].columns[0];
    column.column.infer_type = false;
    column.definition.definition.data_type = dataType;
    column.definition.definition.specific_entity_type = entityType ?? null;
    return column;
  }

  it.each([
    ['12.5', 'NUMBER', '12.5'],
    ['00123', 'STRING', "'00123'"],
    ['hello', 'STRING', "'hello'"],
  ] as const)(
    'settles the first %s entry and writes with the acknowledged schema version',
    async (value, type, sqlValue) => {
      const initial = detail();
      initial.tables[0].columns[0].column.infer_type = true;
      const exec = vi.fn<(request: ExecRequest) => Promise<ExecOutcome>>(
        async () => written
      );
      const { source, client } = setup(initial, exec);
      await waitFor(() => expect(source.loading()).toBe(false));
      transport.inferColumnType.mockResolvedValue(
        ok({ column: inferredDetail(type), table_version: 6 })
      );
      exec.mockResolvedValue({
        ...written,
        new_versions: { 'guests-table': 7 },
      });
      await source.write({ kind: 'create', values: { name: value } }, 5);
      expect(transport.inferColumnType).toHaveBeenCalledExactlyOnceWith({
        id: 'db',
        tableId: 'guests-table',
        columnId: 'name',
        request: { data_type: type, base_version: 5 },
      });
      expect(exec).toHaveBeenLastCalledWith({
        sql: `INSERT INTO "guests" ("Name") VALUES (${sqlValue})`,
      });
      expect(
        client.getQueryData<DatabaseDetail>(databasesKeys.detail('db').queryKey)
          ?.tables[0].columns[0].column.infer_type
      ).toBe(false);
    }
  );

  it('uses the selected mention type and retains its entity ID', async () => {
    const initial = detail();
    initial.tables[0].columns[0].column.infer_type = true;
    const exec = vi.fn<(request: ExecRequest) => Promise<ExecOutcome>>(
      async () => written
    );
    const { source } = setup(initial, exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    transport.inferColumnType.mockResolvedValue(
      ok({ column: inferredDetail('ENTITY', 'USER'), table_version: 6 })
    );
    await source.write(
      {
        kind: 'cell',
        rowId: 'record',
        columnId: 'name',
        value: 'macro|ada@example.com',
        columnTypes: { name: { dataType: 'ENTITY', entityType: 'USER' } },
      },
      5
    );
    expect(transport.inferColumnType.mock.calls[0][0].request).toEqual({
      data_type: 'ENTITY',
      specific_entity_type: 'USER',
      base_version: 5,
    });
    expect(exec).toHaveBeenLastCalledWith({
      sql: 'UPDATE "guests" SET "Name" = \'macro|ada@example.com\' WHERE row_id = \'record\'',
    });
  });

  it('never infers a manually chosen text column or an empty value', async () => {
    const exec = vi.fn<(request: ExecRequest) => Promise<ExecOutcome>>(
      async () => written
    );
    const { source } = setup(detail(), exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    await source.write({ ...edit, value: '123' }, 5);
    expect(transport.inferColumnType).not.toHaveBeenCalled();
    expect(exec.mock.calls.at(-1)?.[0].sql).toContain("= '123'");
  });

  it('after a competing first-entry type decision, writes the value against the refreshed column', async () => {
    const initial = detail();
    initial.tables[0].columns[0].column.infer_type = true;
    const exec = vi.fn<(request: ExecRequest) => Promise<ExecOutcome>>(
      async () => written
    );
    const { source } = setup(initial, exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    transport.inferColumnType.mockResolvedValue(
      err([{ code: 'VERSION_CONFLICT', message: 'Column changed' }])
    );
    transport.get.mockResolvedValue(ok(detail()));
    await source.write({ ...edit, value: '123' }, 5);
    expect(transport.inferColumnType).toHaveBeenCalledOnce();
    expect(transport.get).toHaveBeenCalledOnce();
    expect(exec).toHaveBeenLastCalledWith({
      sql: 'UPDATE "guests" SET "Name" = \'123\' WHERE row_id = \'record\'',
    });
  });

  it('retries a failed value write using its own completed type change without inferring again', async () => {
    const initial = detail();
    initial.tables[0].columns[0].column.infer_type = true;
    const exec = vi.fn<(request: ExecRequest) => Promise<ExecOutcome>>(
      async () => written
    );
    const { source } = setup(initial, exec);
    await waitFor(() => expect(source.loading()).toBe(false));
    transport.inferColumnType.mockResolvedValue(
      ok({ column: inferredDetail('NUMBER'), table_version: 6 })
    );
    exec.mockRejectedValueOnce(new Error('Offline'));
    const mutation: DatabaseRowMutation = { ...edit, value: '12' };
    await expect(source.write(mutation, 5)).rejects.toThrow('Offline');
    await source.write(mutation, 5);
    expect(transport.inferColumnType).toHaveBeenCalledOnce();
    expect(exec).toHaveBeenLastCalledWith({
      sql: 'UPDATE "guests" SET "Name" = 12 WHERE row_id = \'record\'',
    });
  });
});
