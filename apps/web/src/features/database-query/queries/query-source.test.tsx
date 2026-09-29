import { databaseQueryKeys } from '@queries/storage/keys';
import { databaseCompletionRequest } from '@service-cognition/database-query-prompt';
import type { DatabaseDetail, ExecOutcome } from '@service-storage/databases';
import { render, waitFor } from '@solidjs/testing-library';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { queryFocusTable, queryStarters } from '../core/query';
import { createLiveQuerySource, toQuerySchema } from './query-source';

const outcome: ExecOutcome = {
  results: [],
  changes_applied: 0,
  inserted_row_ids: [],
  new_versions: {},
  read_tables: ['watched'],
  read_versions: { watched: 4 },
  truncated_tables: [],
};
afterEach(() => vi.useRealTimers());
describe('query schema', () => {
  it('carries the pre-quoted SQL names and relation metadata into starters and the prompt', () => {
    const detail: DatabaseDetail = {
      database: {
        id: 'db',
        name: 'Renamed database',
        owner_id: 'owner',
        created_at: '',
        trashed_at: null,
      },
      grant: 'owner',
      tables: [
        {
          table: {
            id: 'legacy',
            database_id: 'db',
            name: 'Projects',
            position: 'a',
            version: 1,
          },
          sql_name: '"Projects"',
          columns: [],
        },
        {
          table: {
            id: 'contacts',
            database_id: 'db',
            name: 'Contacts',
            position: 'b',
            version: 1,
          },
          sql_name: '"Contacts"',
          columns: [],
        },
      ],
    };
    detail.tables[1].columns.push({
      column: {
        id: 'relation-column',
        table_id: 'contacts',
        property_definition_id: 'relation-definition',
        position: 'a',
        config: { kind: 'link', database_id: 'db', table_id: 'legacy' },
      },
      sql_name: '"Projects"',
      writable: false,
      definition: {
        definition: {
          id: 'relation-definition',
          owner: { scope: 'database', database_id: 'db' },
          display_name: 'Projects',
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
    });
    const schema = toQuerySchema(detail, 'contacts');
    expect(queryFocusTable(schema)?.name).toBe('Contacts');
    expect(queryFocusTable(schema)?.sqlName).toBe('"Contacts"');
    expect(queryStarters(schema)[0]).toMatchObject({
      prompt: 'How many records are in Contacts?',
      sql: 'SELECT COUNT(*) FROM "Contacts"',
    });
    expect(schema.tables[0].sqlName).toBe('"Projects"');
    expect(schema.tables.map((table) => table.sqlName)).toEqual([
      '"Projects"',
      '"Contacts"',
      'macro.people',
    ]);
    expect(schema.tables[1].columns[0]).toMatchObject({
      multiple: true,
      relation: {
        databaseId: 'db',
        tableId: 'legacy',
        writable: false,
      },
    });
    const request = databaseCompletionRequest(
      { prompt: 'Show contacts and their projects', sql: '', schema },
      'question'
    );
    expect(
      JSON.parse(request.prompt).schema.tables[1].columns[0].relation
    ).toEqual(schema.tables[1].columns[0].relation);
    expect(request.additional_instructions).toContain(
      'FROM invites i JOIN guests g ON i.guest = g.row_id'
    );
    expect(request.additional_instructions).toContain(
      'never compare a relation to a name, join by matching display names'
    );
    expect(request.additional_instructions).not.toMatch(
      /json_each|strftime|junction|readSqlName|SQLite/
    );
  });
});

describe('live query source', () => {
  it('recovers from a failed refresh when a newer dependency event arrives', async () => {
    const read = vi
      .fn<() => Promise<ExecOutcome>>()
      .mockResolvedValueOnce(outcome)
      .mockRejectedValueOnce(new Error('Temporary network failure'))
      .mockResolvedValue({ ...outcome, read_versions: { watched: 6 } });
    let emit!: (tableId: string, version: number) => void;
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    function Harness() {
      const query = createLiveQuerySource({
        sql: () => 'SELECT COUNT(*) FROM projects',
        read,
        subscribe: (callback) => {
          emit = callback;
        },
      });
      return <div>{query.status}</div>;
    }
    const result = render(() => (
      <QueryClientProvider client={client}>
        <Harness />
      </QueryClientProvider>
    ));
    await result.findByText('success');
    emit('watched', 5);
    await result.findByText('error');
    emit('watched', 6);
    await result.findByText('success');
    expect(read).toHaveBeenCalledTimes(3);
    result.unmount();
    client.clear();
  });

  it('coalesces newer dependency events and releases pending refreshes on unmount', async () => {
    const read = vi.fn(async () => outcome);
    let emit!: (tableId: string, version: number) => void;
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    function Harness() {
      createLiveQuerySource({
        sql: () => 'SELECT 1',
        read,
        subscribe: (callback) => {
          emit = callback;
        },
      });
      return null;
    }
    const result = render(() => (
      <QueryClientProvider client={client}>
        <Harness />
      </QueryClientProvider>
    ));
    await waitFor(() =>
      expect(
        client.getQueryData(databaseQueryKeys.answer('SELECT 1').queryKey)
      ).toEqual(outcome)
    );
    vi.useFakeTimers();
    emit('other', 9);
    emit('watched', 4);
    await vi.advanceTimersByTimeAsync(350);
    expect(read).toHaveBeenCalledTimes(1);
    emit('watched', 5);
    emit('watched', 6);
    await vi.advanceTimersByTimeAsync(299);
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(read).toHaveBeenCalledTimes(2);
    emit('watched', 7);
    result.unmount();
    await vi.advanceTimersByTimeAsync(500);
    expect(read).toHaveBeenCalledTimes(2);
    client.clear();
  });
});
