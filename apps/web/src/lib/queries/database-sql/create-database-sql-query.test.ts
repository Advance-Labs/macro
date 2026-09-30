import type { OpenEngine } from '@core/database-sql/driver';
import type { Catalog, Page, Step } from '@core/database-sql/protocol';
import type { CacheHost } from '@graphql-cache/host/types';
import type {
  CacheRevision,
  EntityFilterCacheResult,
} from '@graphql-cache/protocol';
import type { SoupQuery } from '@service-storage/graphql/generated/graphql';
import { createClient, type Exchange, type Operation } from '@urql/core';
import { createRoot } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { empty, fromValue, mergeMap, pipe } from 'wonka';
import { createDatabaseSqlQuery } from './create-database-sql-query';

const DEALS = '01990000-0000-7000-8000-00000000d001';
const NAME = '01990000-0000-7000-8000-00000000c001';
const ACME = '01990000-0000-7000-8000-00000000e001';
const GLOBEX = '01990000-0000-7000-8000-00000000e002';

type SoupItem = SoupQuery['user']['soup']['items'][number];

const catalog: Catalog = {
  tables: [
    {
      id: DEALS,
      database: 'crm',
      name: 'deals',
      source: 'database',
      columns: [{ id: NAME, name: 'name', kind: { kind: 'text' } }],
    },
  ],
};

function deal(id: string, name: string, createdAt: string): SoupItem {
  return {
    __typename: 'GraphqlSoupDatabaseRow',
    id,
    tableId: DEALS,
    databaseId: 'db000000-0000-0000-0000-000000000001',
    position: 'a',
    ownerId: 'macro|owner@databases.test',
    creatorId: null,
    createdAt,
    updatedAt: createdAt,
    cacheProjection: null,
    frecencyScore: null,
    entityType: 'DATABASE_ROW',
    displayName: null,
    isFavorited: false,
    notifications: [],
    properties: [
      {
        id: `${id}-name`,
        propertyDefinitionId: NAME,
        displayName: 'name',
        dataType: 'STRING',
        isMultiSelect: false,
        specificEntityType: null,
        isSystem: false,
        isMetadata: false,
        value: { __typename: 'GraphqlStringPropertyValue', stringValue: name },
      },
    ],
  };
}

const acme = deal(ACME, 'Acme', '2026-01-01T00:00:00Z');
const globex = deal(GLOBEX, 'Globex', '2026-01-02T00:00:00Z');

/** `SELECT name FROM crm.deals`, answered with the names fed, in order. */
const names: OpenEngine = async () => {
  const fetch: Step = {
    step: 'fetch',
    id: 0,
    query: { type: 'soup', table: DEALS, propf: null, keyHint: null },
    needs: [NAME],
    cursor: null,
    limit: 500,
  };
  const done = (page: Page): Step => ({
    step: 'done',
    columns: [{ name: 'name', column: NAME, kind: 'text' }],
    rows: page.rows.map((row) => [row.cells[NAME] ?? null]),
    rowIds: page.rows.map((row) => row.id),
    readTables: [DEALS],
    truncated: false,
    insertedRowIds: [],
    changesApplied: 0,
    failures: [],
  });
  return {
    start: () => fetch,
    feed_page: (_id, page) => done(page),
    feed_bins: () => {
      throw 'no bins';
    },
    free: () => {},
  };
};

let dispose: (() => void) | undefined;

beforeEach(() => {
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  vi.restoreAllMocks();
});

describe('createDatabaseSqlQuery', () => {
  it('shows a row the cache learns about without reading the network again', async () => {
    const requests: Operation[] = [];
    const exchange: Exchange = () => (incoming) =>
      pipe(
        incoming,
        mergeMap((operation) => {
          if (operation.kind === 'teardown') return empty;
          requests.push(operation);
          const data: SoupQuery = {
            user: {
              id: 'macro|viewer@databases.test',
              emailLinks: [],
              soup: { items: [acme], nextCursor: null },
            },
          };
          return fromValue({ operation, data, stale: false, hasNext: false });
        })
      );
    const client = createClient({
      url: 'http://test.invalid/graphql',
      exchanges: [exchange],
    });
    const revision = 'revision-2' as CacheRevision;
    let cacheChanged = (_revision: CacheRevision) => {};
    const reconciled: EntityFilterCacheResult = {
      kind: 'reconciled',
      revision,
      keys: [
        `GraphqlSoupDatabaseRow:${GLOBEX}`,
        `GraphqlSoupDatabaseRow:${ACME}`,
      ],
      retainedKeys: [],
      optimistic: false,
    };
    const entityFilter = vi.fn<CacheHost['entityFilter']>(
      async () => reconciled
    );
    const host = {
      onCacheChanged: (callback: (revision: CacheRevision) => void) => {
        cacheChanged = callback;
        return () => {};
      },
      entityFilter,
      readRecordsByKeys: async () => ({
        revision,
        records: [
          { recordKey: `GraphqlSoupDatabaseRow:${GLOBEX}`, record: globex },
          { recordKey: `GraphqlSoupDatabaseRow:${ACME}`, record: acme },
        ],
      }),
    } satisfies Pick<
      CacheHost,
      'onCacheChanged' | 'entityFilter' | 'readRecordsByKeys'
    >;

    const query = createRoot((cleanup) => {
      dispose = cleanup;
      return createDatabaseSqlQuery(
        () => ({ catalog, sql: 'SELECT name FROM crm.deals' }),
        {
          client: () => client,
          cacheHost: () => host,
          people: async () => [],
          open: names,
        }
      );
    });

    await vi.waitFor(() =>
      expect(query.outcome()?.rows).toEqual([[{ type: 'text', value: 'Acme' }]])
    );
    expect(
      requests.map((operation) => operation.context.requestPolicy)
    ).toEqual(['cache-and-network']);

    cacheChanged(revision);

    await vi.waitFor(() =>
      expect(query.outcome()?.rows).toEqual([
        [{ type: 'text', value: 'Globex' }],
        [{ type: 'text', value: 'Acme' }],
      ])
    );
    expect(requests).toHaveLength(1);
    expect(entityFilter).toHaveBeenCalledWith({
      filters: requests[0]?.variables?.input.initial.filters,
      sortMethod: 'CREATED_AT',
      sortDirection: 'DESC',
      limit: 500,
      baseline: [
        {
          key: `GraphqlSoupDatabaseRow:${ACME}`,
          sortTimestamp: '2026-01-01T00:00:00Z',
        },
      ],
    });
  });
});
