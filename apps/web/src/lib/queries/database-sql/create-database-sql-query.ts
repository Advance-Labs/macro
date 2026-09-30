/**
 * A live SQL statement: the engine builds the statement's catalog from its
 * schema, runs in the browser over the GraphQL row source, and runs again
 * whenever the normalized cache changes, reading the cache the second time. Cell edits reach the result through the cached
 * rows; rows the cache learns about through the local filter index. A
 * changed statement keeps the last answer until its own arrives.
 */

import {
  type OpenEngine,
  type RowSource,
  runDatabaseSql,
} from '@core/database-sql/driver';
import type {
  Catalog,
  Outcome,
  Schema,
} from '@core/database-sql/generated/types';
import { buildDatabaseSqlCatalog } from '@core/database-sql/wasm-module';
import { idToDisplayName, idToEmail } from '@core/user/util';
import type { CacheHost } from '@graphql-cache/host/types';
import { queryClient } from '@queries/client';
import { contactsQueryOptions } from '@queries/contacts/contacts';
import { subscribeToVisibleCacheChanges } from '@queries/subscribe-to-visible-cache-changes';
import {
  getGraphqlSoupCacheHost,
  getGraphqlSoupClient,
} from '@service-storage/graphql-soup';
import type { Client, RequestPolicy } from '@urql/core';
import {
  type Accessor,
  batch,
  createEffect,
  createSignal,
  on,
  onCleanup,
  untrack,
} from 'solid-js';
import {
  createGraphqlRowSource,
  type LocalMembership,
  type Person,
} from './graphql-source';

/** A statement and the databases its catalog is built from. */
export interface DatabaseSqlStatement {
  schema: Schema;
  /** The database the statement is written from: its tables win name ties. */
  scope?: string;
  sql: string;
}

/** Equal statements answer alike, so a rebuilt but unchanged one need not rerun. */
export function sameDatabaseSqlStatement(
  left: DatabaseSqlStatement | undefined,
  right: DatabaseSqlStatement | undefined
): boolean {
  return (
    left?.sql === right?.sql &&
    left?.scope === right?.scope &&
    JSON.stringify(left?.schema) === JSON.stringify(right?.schema)
  );
}

/** Builds a statement's catalog; the wasm engine unless a test says otherwise. */
export type BuildCatalog = (schema: Schema, scope?: string) => Promise<Catalog>;

export interface DatabaseSqlQueryCapabilities {
  client: () => Client;
  cacheHost: () =>
    | Pick<CacheHost, 'onCacheChanged' | 'entityFilter' | 'readRecordsByKeys'>
    | undefined;
  people: () => Promise<Person[]>;
  /** The engine; the wasm module unless a test says otherwise. */
  open?: OpenEngine;
  catalog?: BuildCatalog;
}

export interface DatabaseSqlQuery {
  /** The last answer; kept while a later run, of this statement or a changed one, is in flight. */
  outcome: Accessor<Outcome | undefined>;
  /** The catalog the last answer was read against. */
  catalog: Accessor<Catalog | undefined>;
  /** Why the last run failed, until one succeeds. */
  error: Accessor<unknown>;
  loading: Accessor<boolean>;
  /** Read the statement's tables from the server again; rejects when that read fails. */
  refresh: () => Promise<void>;
}

/** The app's GraphQL client and cache, and the contacts query for people. */
export function productionDatabaseSqlCapabilities(): DatabaseSqlQueryCapabilities {
  return {
    client: getGraphqlSoupClient,
    cacheHost: getGraphqlSoupCacheHost,
    people: async () => {
      const { contacts } = await queryClient.fetchQuery(contactsQueryOptions());
      return contacts.map((id) => ({
        id,
        name: idToDisplayName(id),
        email: idToEmail(id),
      }));
    },
  };
}

export function createDatabaseSqlQuery(
  statement: Accessor<DatabaseSqlStatement | undefined>,
  capabilities: DatabaseSqlQueryCapabilities = productionDatabaseSqlCapabilities()
): DatabaseSqlQuery {
  const [outcome, setOutcome] = createSignal<Outcome>();
  const [catalog, setCatalog] = createSignal<Catalog>();
  const [error, setError] = createSignal<unknown>();
  const [loading, setLoading] = createSignal(false);
  let latest = 0;
  // The cache may not hold what an in-flight network read will bring, so a
  // cache change waits for it instead of answering from older rows.
  let networkRead: Promise<void> | undefined;
  // First-page evidence for the local filter index, per statement.
  let baselines: LocalMembership['baselines'] = new Map();

  /** Rejects with the failure of this run, unless a later run replaced it. */
  const run = async (
    current: DatabaseSqlStatement,
    requestPolicy: RequestPolicy,
    reconcile: boolean
  ) => {
    const run = ++latest;
    const host = capabilities.cacheHost();
    setLoading(true);
    try {
      const built = await (capabilities.catalog ?? buildDatabaseSqlCatalog)(
        current.schema,
        current.scope
      );
      const source: RowSource = createGraphqlRowSource({
        client: capabilities.client(),
        catalog: built,
        requestPolicy,
        people: capabilities.people,
        membership: host ? { host, baselines, reconcile } : undefined,
      });
      const answer = await runDatabaseSql(built, current.sql, {
        source,
        ...(capabilities.open ? { open: capabilities.open } : {}),
      });
      if (run !== latest) return;
      // A cache change that left the answer alone keeps the same outcome.
      batch(() => {
        if (JSON.stringify(untrack(catalog)) !== JSON.stringify(built))
          setCatalog(built);
        if (JSON.stringify(untrack(outcome)) !== JSON.stringify(answer))
          setOutcome(answer);
        setError(undefined);
      });
    } catch (failure) {
      if (run !== latest) return;
      setError(failure);
      throw failure;
    } finally {
      if (run === latest) setLoading(false);
    }
  };

  createEffect(
    on(statement, (current) => {
      baselines = new Map();
      latest += 1;
      setError(undefined);
      setLoading(false);
      if (!current) {
        batch(() => {
          setOutcome(undefined);
          setCatalog(undefined);
        });
        return;
      }
      void settled(run(current, 'cache-and-network', false));
    })
  );

  // Rerun against the cache whenever it changes; the engine is cheap next to
  // a network read, and an unchanged cache answers the same rows.
  createEffect(() => {
    const host = capabilities.cacheHost();
    if (!host) return;
    onCleanup(
      subscribeToVisibleCacheChanges(host, () => {
        if (networkRead) return settled(networkRead);
        const current = untrack(statement);
        return current ? settled(run(current, 'cache-first', true)) : undefined;
      })
    );
  });

  onCleanup(() => {
    latest += 1;
  });

  return {
    outcome,
    catalog,
    error,
    loading,
    refresh: async () => {
      const current = untrack(statement);
      if (!current) return;
      const reading = run(current, 'network-only', false);
      networkRead = reading;
      try {
        await reading;
      } finally {
        if (networkRead === reading) networkRead = undefined;
      }
    },
  };
}

/** A run's failure is kept in `error`; only an explicit refresh rejects with it. */
async function settled(running: Promise<void>): Promise<void> {
  try {
    await running;
  } catch {
    // Kept in `error`.
  }
}

/** Refresh without waiting; a failure shows through the reader's own error. */
export function refreshInBackground(reader: {
  refresh: () => Promise<void>;
}): void {
  void settled(reader.refresh());
}

/** One read of a statement from the network, for an answer nothing keeps live. */
export async function readDatabaseSql(
  { schema, scope, sql }: DatabaseSqlStatement,
  capabilities: DatabaseSqlQueryCapabilities = productionDatabaseSqlCapabilities()
): Promise<{ catalog: Catalog; outcome: Outcome }> {
  const catalog = await (capabilities.catalog ?? buildDatabaseSqlCatalog)(
    schema,
    scope
  );
  const outcome = await runDatabaseSql(catalog, sql, {
    source: createGraphqlRowSource({
      client: capabilities.client(),
      catalog,
      requestPolicy: 'network-only',
      people: capabilities.people,
    }),
    ...(capabilities.open ? { open: capabilities.open } : {}),
  });
  return { catalog, outcome };
}
