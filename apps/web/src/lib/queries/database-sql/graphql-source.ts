/**
 * The SQL engine's row source in the browser: database tables are read as
 * Soup database rows through the app's GraphQL client, so every page goes
 * through the normalized cache like any other Soup query; `people` come
 * from the contacts query.
 *
 * Rows are ordered by creation, newest first: Soup's cursor orders by a
 * timestamp, so a statement without `ORDER BY` lists rows in that order.
 * Each row carries its position, so `ORDER BY row_position` gives the grid's
 * order.
 */

import { readRecordsByKeys, selectRecords } from '@app/lib/graphql-cache';
import type { RowSource } from '@core/database-sql/driver';
import type {
  Bin,
  Catalog,
  CatalogTable,
  Cell,
  GqlQuery,
  KeyHint,
  Page,
  Propf,
  Row,
} from '@core/database-sql/protocol';
import type { CacheHost } from '@graphql-cache/host/types';
import { buildGraphqlEntitySoupInput } from '@queries/soup/graphql/entity-input';
import {
  materializeReconciledSoup,
  soupReconciliationBaseline,
} from '@queries/soup/graphql/reconciliation';
import {
  type GraphqlDatabaseRowExpr,
  type GraphqlEntityFilterAst,
  type GraphqlFilterPropertiesExpr,
  GroupSoupDocument,
  type GroupSoupQuery,
  type GroupSoupQueryVariables,
  SoupDocument,
  type SoupInput,
  SoupItemFieldsFragmentDoc,
  type SoupPropertyFieldsFragment,
  type SoupQuery,
  type SoupQueryVariables,
} from '@service-storage/graphql/generated/graphql';
import type { GraphqlSoupItem } from '@service-storage/graphql-soup';
import type { Client, RequestPolicy } from '@urql/core';
import { match, P } from 'ts-pattern';
import { NIL as NIL_UUID, v5 as uuidV5 } from 'uuid';

/** One person the viewer can see, as the `people` table lists them. */
export interface Person {
  id: string;
  name: string;
  email: string;
}

export interface GraphqlRowSourceCapabilities {
  /** The app's GraphQL client; its exchanges decide cache or network. */
  client: Client;
  /** The statement's catalog, for the value kinds of grouped columns. */
  catalog: Catalog;
  requestPolicy: RequestPolicy;
  /** Everyone the viewer can see. */
  people: () => Promise<Person[]>;
  /**
   * Table membership as the local filter index knows it. A read from the
   * network records each first page as evidence; a later read with
   * `reconcile` set answers a table that fit in one page from the cache's
   * index instead, so rows the cache learned about since show up.
   */
  membership?: LocalMembership;
}

/** Local reconciliation for first pages, shared across runs of a statement. */
export interface LocalMembership {
  host: Pick<CacheHost, 'entityFilter' | 'readRecordsByKeys'>;
  /** The last network first page for each Soup input. */
  baselines: Map<
    string,
    { items: readonly GraphqlSoupItem[]; complete: boolean }
  >;
  reconcile: boolean;
}

/** Past this many join values a narrowed filter costs more than it saves. */
const MAX_KEY_HINT_VALUES = 100;

/**
 * `people` rows need UUID ids: the RFC 4122 OID namespace names a person's
 * row as the engine's own tests do (`Uuid::NAMESPACE_OID`).
 */
const PERSON_ROW_NAMESPACE = '6ba7b812-9dad-11d1-80b4-00c04fd430c8';

type DatabaseRowItem = Extract<
  GraphqlSoupItem,
  { __typename: 'GraphqlSoupDatabaseRow' }
>;

const rowSelection = selectRecords(SoupItemFieldsFragmentDoc);

export function createGraphqlRowSource({
  client,
  catalog,
  requestPolicy,
  people,
  membership,
}: GraphqlRowSourceCapabilities): RowSource {
  return {
    page: (query, _needs, cursor, limit) =>
      match(query)
        .with({ type: 'soup' }, (soup) =>
          soupPage(
            client,
            requestPolicy,
            soupInput(soup, cursor, limit),
            membership
          )
        )
        .with({ type: 'people' }, ({ ids }) => peoplePage(catalog, people, ids))
        .with({ type: 'groupSoup' }, () => {
          throw new Error('a grouped query is read as bins, not pages');
        })
        .exhaustive(),
    bins: (query) =>
      match(query)
        .with({ type: 'groupSoup' }, (grouped) =>
          groupBins(client, requestPolicy, catalog, grouped)
        )
        .with({ type: P.union('soup', 'people') }, () => {
          throw new Error('only a grouped query has bins');
        })
        .exhaustive(),
  };
}

/** Rows of one table and nothing else, with the pushed-down filter. */
function tableFilters(
  table: string,
  propf: Propf | null,
  keyHint: KeyHint | null
): GraphqlEntityFilterAst {
  const base = buildGraphqlEntitySoupInput('DATABASE_ROW', NIL_UUID)?.initial
    ?.filters;
  if (!base) throw new Error('a database row Soup input is unavailable');
  let rows: GraphqlDatabaseRowExpr = { literal: { tableId: table } };
  let properties = propf ? propertiesExpr(propf) : undefined;
  const hint = keyHint ? narrowing(keyHint) : undefined;
  if (hint?.kind === 'rows') rows = { and: { left: rows, right: hint.expr } };
  if (hint?.kind === 'properties') {
    properties = properties
      ? { and: { left: properties, right: hint.expr } }
      : hint.expr;
  }
  return {
    ...base,
    databaseRowFilter: rows,
    ...(properties ? { propertiesFilter: properties } : {}),
  };
}

function soupInput(
  { table, propf, keyHint }: Extract<GqlQuery, { type: 'soup' }>,
  cursor: string | null,
  limit: number
): SoupInput {
  if (cursor) {
    return { continuation: { cursor, expand: true, sortDirection: 'DESC' } };
  }
  return {
    initial: {
      limit,
      expand: true,
      sortMethod: 'CREATED_AT',
      sortDirection: 'DESC',
      filters: tableFilters(table, propf, keyHint),
    },
  };
}

/** The engine's `propf` wire form as the GraphQL properties filter. */
function propertiesExpr(propf: Propf): GraphqlFilterPropertiesExpr {
  if ('&' in propf) {
    return {
      and: {
        left: propertiesExpr(propf['&'][0]),
        right: propertiesExpr(propf['&'][1]),
      },
    };
  }
  if ('|' in propf) {
    return {
      or: {
        left: propertiesExpr(propf['|'][0]),
        right: propertiesExpr(propf['|'][1]),
      },
    };
  }
  if ('!' in propf) return { not: propertiesExpr(propf['!']) };
  const { pd, v } = propf.l;
  return {
    literal: {
      propertyDefinitionId: pd,
      value: 'so' in v ? { selectOption: v.so } : { entityRef: v.er },
    },
  };
}

/** A balanced OR keeps a long list inside the filter depth limit. */
function balancedOr<Expr>(
  items: Expr[],
  or: (left: Expr, right: Expr) => Expr
): Expr | undefined {
  if (items.length < 2) return items[0];
  const middle = Math.floor(items.length / 2);
  const left = balancedOr(items.slice(0, middle), or);
  const right = balancedOr(items.slice(middle), or);
  return left && right ? or(left, right) : (left ?? right);
}

/**
 * A filter fetching only the joined rows the join can match. The fold
 * applies the join regardless, so a hint that cannot be expressed fetches
 * the whole table instead.
 */
function narrowing(
  hint: KeyHint
):
  | { kind: 'rows'; expr: GraphqlDatabaseRowExpr }
  | { kind: 'properties'; expr: GraphqlFilterPropertiesExpr }
  | undefined {
  type Member = { kind: 'entity' | 'option' | 'other'; id: string };
  const members = hint.values.flatMap((value): Member[] =>
    match(value)
      .returnType<Member[]>()
      .with({ type: 'entities' }, ({ value }) =>
        value.map((id) => ({ kind: 'entity', id }))
      )
      .with({ type: 'options' }, ({ value }) =>
        value.map((id) => ({ kind: 'option', id }))
      )
      .otherwise(() => [{ kind: 'other', id: '' }])
  );
  if (
    members.length === 0 ||
    members.length > MAX_KEY_HINT_VALUES ||
    members.some((member) => member.kind === 'other')
  )
    return undefined;
  const column = hint.column;
  if (column === null) {
    const expr = balancedOr<GraphqlDatabaseRowExpr>(
      members.map(({ id }) => ({ literal: { id } })),
      (left, right) => ({ or: { left, right } })
    );
    return expr && { kind: 'rows', expr };
  }
  const expr = balancedOr<GraphqlFilterPropertiesExpr>(
    members.map(({ kind, id }) => ({
      literal: {
        propertyDefinitionId: column,
        value: kind === 'option' ? { selectOption: id } : { entityRef: id },
      },
    })),
    (left, right) => ({ or: { left, right } })
  );
  return expr && { kind: 'properties', expr };
}

async function soupPage(
  client: Client,
  requestPolicy: RequestPolicy,
  input: SoupInput,
  membership: LocalMembership | undefined
): Promise<Page> {
  const evidence = input.initial ? JSON.stringify(input) : undefined;
  if (evidence && membership?.reconcile) {
    const local = await reconciledPage(input, evidence, membership);
    if (local) return local;
  }
  const result = await client
    .query<SoupQuery, SoupQueryVariables>(
      SoupDocument,
      { input },
      { requestPolicy }
    )
    .toPromise();
  if (result.error) throw result.error;
  if (!result.data) throw new Error('the Soup query returned no data');
  const { items, nextCursor } = result.data.user.soup;
  if (evidence && membership && isNetworkRead(requestPolicy)) {
    membership.baselines.set(evidence, {
      items,
      complete: nextCursor === null,
    });
  }
  return { rows: items.map(tableRow), next: nextCursor };
}

function isNetworkRead(requestPolicy: RequestPolicy): boolean {
  return (
    requestPolicy === 'network-only' || requestPolicy === 'cache-and-network'
  );
}

/**
 * The first page as the local filter index reconciles it with the last
 * network page, when that page held the whole table. `undefined` leaves the
 * read to the GraphQL client.
 */
async function reconciledPage(
  input: SoupInput,
  evidence: string,
  { host, baselines }: LocalMembership
): Promise<Page | undefined> {
  const initial = input.initial;
  const baseline = baselines.get(evidence);
  if (!initial || !baseline?.complete) return undefined;
  const limit = initial.limit ?? 0;
  const baselineKeys = soupReconciliationBaseline(baseline.items, 'CREATED_AT');
  if (!baselineKeys) return undefined;
  const result = await host.entityFilter({
    filters: initial.filters ?? {},
    sortMethod: 'CREATED_AT',
    sortDirection: 'DESC',
    limit,
    baseline: baselineKeys,
  });
  if (result.kind !== 'reconciled' && result.kind !== 'complete')
    return undefined;
  // A full page may continue past the limit; only the server's cursor knows.
  if (result.keys.length >= limit) return undefined;
  const { records } = await readRecordsByKeys<GraphqlSoupItem>(
    host,
    rowSelection,
    result.keys
  );
  return {
    rows: materializeReconciledSoup(result.keys, records, baseline.items).map(
      tableRow
    ),
    next: null,
  };
}

function tableRow(item: GraphqlSoupItem): Row {
  if (item.__typename !== 'GraphqlSoupDatabaseRow') {
    throw new Error(`a table query returned a ${item.__typename}`);
  }
  return { id: item.id, position: item.position, cells: rowCells(item) };
}

/** A row's cells by property definition; an empty property is no cell. */
function rowCells(item: DatabaseRowItem): Record<string, Cell> {
  const cells: Record<string, Cell> = {};
  for (const property of item.properties) {
    const value = cell(property.value);
    if (value) cells[property.propertyDefinitionId] = value;
  }
  return cells;
}

/** A property value as the engine reads it, matching the server's source. */
function cell(value: SoupPropertyFieldsFragment['value']): Cell | undefined {
  if (!value) return undefined;
  return match(value)
    .returnType<Cell>()
    .with({ __typename: 'GraphqlBooleanPropertyValue' }, ({ boolValue }) => ({
      type: 'bool',
      value: boolValue,
    }))
    .with({ __typename: 'GraphqlNumberPropertyValue' }, ({ numberValue }) => ({
      type: 'number',
      value: numberValue,
    }))
    .with({ __typename: 'GraphqlStringPropertyValue' }, ({ stringValue }) => ({
      type: 'text',
      value: stringValue,
    }))
    .with({ __typename: 'GraphqlDatePropertyValue' }, ({ dateValue }) => ({
      type: 'date',
      value: dateValue,
    }))
    .with(
      { __typename: 'GraphqlSelectOptionPropertyValue' },
      ({ optionIds }) => ({
        type: 'options',
        value: optionIds,
      })
    )
    .with(
      { __typename: 'GraphqlEntityReferencePropertyValue' },
      ({ references }) => ({
        type: 'entities',
        value: references.map((reference) => reference.entityId),
      })
    )
    .with({ __typename: 'GraphqlLinkPropertyValue' }, ({ urls }) => ({
      type: 'text',
      value: urls.join(' '),
    }))
    .exhaustive();
}

async function groupBins(
  client: Client,
  requestPolicy: RequestPolicy,
  catalog: Catalog,
  { table, propf, groupBy }: Extract<GqlQuery, { type: 'groupSoup' }>
): Promise<Bin[]> {
  const kind = catalog.tables
    .find((candidate) => candidate.id === table)
    ?.columns.find((column) => column.id === groupBy)?.kind;
  const result = await client
    .query<GroupSoupQuery, GroupSoupQueryVariables>(
      GroupSoupDocument,
      {
        input: {
          initial: {
            groupBy: {
              field: 'PROPERTY',
              propertyDefinitionId: groupBy,
              entityType: 'DATABASE_ROW',
            },
            // The bins' totals answer the count; one item each is enough.
            limit: 1,
            sortMethod: 'CREATED_AT',
            filters: tableFilters(table, propf, null),
          },
        },
      },
      { requestPolicy }
    )
    .toPromise();
  if (result.error) throw result.error;
  if (!result.data) throw new Error('the grouped Soup query returned no data');
  return result.data.user.groupSoup.bins.map(({ key, totalCount }) => ({
    // Soup files rows with an empty cell under the empty key.
    key:
      key === ''
        ? null
        : match(kind)
            .returnType<Cell>()
            .with({ kind: 'select' }, () => ({ type: 'options', value: [key] }))
            .with({ kind: 'entity' }, () => ({
              type: 'entities',
              value: [key],
            }))
            .otherwise(() => {
              throw new Error(`column ${groupBy} cannot be grouped by Soup`);
            }),
    count: totalCount,
  }));
}

async function peoplePage(
  catalog: Catalog,
  people: () => Promise<Person[]>,
  ids: string[] | null
): Promise<Page> {
  const table = catalog.tables.find(
    (candidate): candidate is CatalogTable => candidate.source === 'people'
  );
  if (!table) throw new Error('this catalog has no people table');
  const column = (name: string) => {
    const found = table.columns.find((candidate) => candidate.name === name);
    if (!found) throw new Error(`the people table has no ${name} column`);
    return found.id;
  };
  const [id, name, email] = [column('id'), column('name'), column('email')];
  const wanted = ids ? new Set(ids) : undefined;
  return {
    rows: (await people())
      .filter((person) => !wanted || wanted.has(person.id))
      .map((person) => ({
        id: uuidV5(person.id, PERSON_ROW_NAMESPACE),
        cells: {
          [id]: { type: 'entities', value: [person.id] },
          [name]: { type: 'text', value: person.name },
          [email]: { type: 'text', value: person.email },
        },
      })),
    next: null,
  };
}
