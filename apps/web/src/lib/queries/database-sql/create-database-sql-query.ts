/**
 * A live SQL statement: the engine runs in the browser over the GraphQL row
 * source, and runs again whenever the normalized cache changes, reading the
 * cache the second time. Cell edits reach the result through the cached
 * rows; rows the cache learns about through the local filter index. A
 * changed statement keeps the last answer until its own arrives.
 */

import {
  type OpenEngine,
  type RowSource,
  runDatabaseSql,
} from '@core/database-sql/driver';
import type { Catalog, Outcome } from '@core/database-sql/protocol';
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

export interface DatabaseSqlStatement {
  catalog: Catalog;
  sql: string;
}

export interface DatabaseSqlQueryCapabilities {
  client: () => Client;
  cacheHost: () =>
    | Pick<CacheHost, 'onCacheChanged' | 'entityFilter' | 'readRecordsByKeys'>
    | undefined;
  people: () => Promise<Person[]>;
  /** The engine; the wasm module unless a test says otherwise. */
  open?: OpenEngine;
}

export interface DatabaseSqlQuery {
  /** The last answer; kept while a later run, of this statement or a changed one, is in flight. */
  outcome: Accessor<Outcome | undefined>;
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
  const [error, setError] = createSignal<unknown>();
  const [loading, setLoading] = createSignal(false);
  let latest = 0;
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
    const source: RowSource = createGraphqlRowSource({
      client: capabilities.client(),
      catalog: current.catalog,
      requestPolicy,
      people: capabilities.people,
      membership: host ? { host, baselines, reconcile } : undefined,
    });
    setLoading(true);
    try {
      const answer = await runDatabaseSql(current.catalog, current.sql, {
        source,
        ...(capabilities.open ? { open: capabilities.open } : {}),
      });
      if (run !== latest) return;
      // A cache change that left the answer alone keeps the same outcome.
      if (JSON.stringify(untrack(outcome)) !== JSON.stringify(answer)) {
        setOutcome(answer);
      }
      setError(undefined);
    } catch (failure) {
      if (run !== latest) return;
      setError(failure);
      throw failure;
    } finally {
      if (run === latest) setLoading(false);
    }
  };
  // A failure is kept in `error`; only an explicit refresh rejects with it.
  const quietly = (running: Promise<void>) => running.catch(() => {});

  createEffect(
    on(statement, (current) => {
      baselines = new Map();
      latest += 1;
      setError(undefined);
      setLoading(false);
      if (!current) {
        setOutcome(undefined);
        return;
      }
      void quietly(run(current, 'cache-and-network', false));
    })
  );

  // Rerun against the cache whenever it changes; the engine is cheap next to
  // a network read, and an unchanged cache answers the same rows.
  createEffect(() => {
    const host = capabilities.cacheHost();
    if (!host) return;
    onCleanup(
      subscribeToVisibleCacheChanges(host, () => {
        const current = untrack(statement);
        return current ? quietly(run(current, 'cache-first', true)) : undefined;
      })
    );
  });

  onCleanup(() => {
    latest += 1;
  });

  return {
    outcome,
    error,
    loading,
    refresh: async () => {
      const current = untrack(statement);
      if (current) await run(current, 'network-only', false);
    },
  };
}

/** One read of a statement from the network, for an answer nothing keeps live. */
export function readDatabaseSql(
  { catalog, sql }: DatabaseSqlStatement,
  capabilities: DatabaseSqlQueryCapabilities = productionDatabaseSqlCapabilities()
): Promise<Outcome> {
  return runDatabaseSql(catalog, sql, {
    source: createGraphqlRowSource({
      client: capabilities.client(),
      catalog,
      requestPolicy: 'network-only',
      people: capabilities.people,
    }),
    ...(capabilities.open ? { open: capabilities.open } : {}),
  });
}
