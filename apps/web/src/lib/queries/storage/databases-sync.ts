/**
 * Gateway liveness for Macro Databases.
 *
 * Kept apart from `./databases` so the query module — which eager modules such
 * as the launcher import — does not pull the connection-gateway websocket into
 * the startup import graph.
 */
import { useUserId } from '@core/context/user';
import { createConnectionWebsocketEffect } from '@service-connection/websocket';
import {
  type DatabaseAwareness,
  databasesClient,
} from '@service-storage/databases';
import { ReactiveMap } from '@solid-primitives/map';
import { debounce } from '@solid-primitives/scheduled';
import { type Accessor, createEffect, on, onCleanup, untrack } from 'solid-js';
import { invalidateDatabase, invalidateDatabaseRows } from './databases';

/** Gateway message type published by `crates/databases` on every write. */
const TABLE_CHANGED_MESSAGE_TYPE = 'database_table_changed';
/** Gateway message type relaying one viewer's awareness to the others. */
const AWARENESS_MESSAGE_TYPE = 'database_awareness';

const AWARENESS_SEND_DEBOUNCE_MS = 150;
const AWARENESS_HEARTBEAT_MS = 20_000;
/** A viewer not heard from within this window is treated as gone. */
const AWARENESS_EXPIRY_MS = 45_000;
const AWARENESS_SWEEP_MS = 5_000;

type TableChangedMessage = {
  databaseId: string;
  tableId: string;
  version: number;
};

function parseMessageData<Data>(message: {
  type: string;
  data: unknown;
}): Data | undefined {
  try {
    return typeof message.data === 'string'
      ? JSON.parse(message.data)
      : (message.data as Data);
  } catch {
    console.error(`unparsable ${message.type} payload`, message);
    return undefined;
  }
}

/**
 * Re-read a database whenever the gateway reports one of its tables changed.
 *
 * Results are never pushed — the message carries only the table's new version,
 * and every viewer re-executes its own queries as itself.
 */
export function useDatabaseTableChangedSync(
  databaseId: () => string | undefined
) {
  createConnectionWebsocketEffect((message) => {
    if (message.type !== TABLE_CHANGED_MESSAGE_TYPE) return;
    const data = parseMessageData<TableChangedMessage>(message);
    if (!data?.databaseId || data.databaseId !== databaseId()) return;

    invalidateDatabase(data.databaseId);
    if (data.tableId) invalidateDatabaseRows(data.databaseId, data.tableId);
  });
}

/** Where this client is inside a database. */
export type LocalDatabaseAwareness = {
  tableId: string;
  rowId?: string;
  columnId?: string;
  editing?: boolean;
};

/** Where another viewer is inside the database. */
export type RemoteDatabaseAwareness = {
  userId: string;
  tableId: string;
  rowId?: string;
  columnId?: string;
  editing: boolean;
};

type AwarenessMessage = {
  databaseId: string;
  userId: string;
  state: DatabaseAwareness;
  ts: number;
};

type HeldAwareness = {
  state: DatabaseAwareness;
  /** Server timestamp, orders messages from the same viewer. */
  ts: number;
  /** Local clock, decides expiry so clock skew cannot drop live viewers. */
  receivedAt: number;
};

/**
 * Share where this client is and follow where everyone else is.
 *
 * The local state goes out debounced and as a 20 s heartbeat; leaving the
 * database tells the others to drop it. Remote states are held per user,
 * ignoring stale relays (including this client's own) and viewers silent for
 * longer than the heartbeat allows.
 */
export function useDatabaseAwareness(
  databaseId: () => string | undefined,
  local: () => LocalDatabaseAwareness | undefined
): { remote: Accessor<RemoteDatabaseAwareness[]> } {
  const userId = useUserId();
  const held = new ReactiveMap<string, HeldAwareness>();

  let announced: { databaseId: string; state: DatabaseAwareness } | undefined;
  const send = (id: string, state: DatabaseAwareness) => {
    void databasesClient.shareAwareness(id, state);
  };
  const leave = () => {
    if (!announced) return;
    send(announced.databaseId, {
      tableId: announced.state.tableId,
      left: true,
    });
    announced = undefined;
  };
  const share = () => {
    const id = untrack(databaseId);
    const state = untrack(local);
    if (!id || !state) {
      leave();
      return;
    }
    if (announced && announced.databaseId !== id) leave();
    const payload: DatabaseAwareness = { tableId: state.tableId };
    if (state.rowId !== undefined) payload.rowId = state.rowId;
    if (state.columnId !== undefined) payload.columnId = state.columnId;
    if (state.rowId !== undefined || state.columnId !== undefined)
      payload.editing = Boolean(state.editing);
    announced = { databaseId: id, state: payload };
    send(id, payload);
  };
  const shareSoon = debounce(share, AWARENESS_SEND_DEBOUNCE_MS);
  createEffect(
    on(
      () => {
        const state = local();
        return JSON.stringify([
          databaseId(),
          state?.tableId,
          state?.rowId,
          state?.columnId,
          state?.editing,
        ]);
      },
      () => shareSoon()
    )
  );
  const heartbeat = setInterval(() => {
    if (announced) send(announced.databaseId, announced.state);
  }, AWARENESS_HEARTBEAT_MS);

  createConnectionWebsocketEffect((message) => {
    if (message.type !== AWARENESS_MESSAGE_TYPE) return;
    const data = parseMessageData<AwarenessMessage>(message);
    if (!data?.databaseId || data.databaseId !== databaseId()) return;
    if (!data.userId || data.userId === userId()) return;
    const current = held.get(data.userId);
    if (current && current.ts > data.ts) return;
    if (data.state?.left) {
      held.delete(data.userId);
      return;
    }
    if (!data.state?.tableId) return;
    held.set(data.userId, {
      state: data.state,
      ts: data.ts,
      receivedAt: Date.now(),
    });
  });
  const sweep = setInterval(() => {
    const cutoff = Date.now() - AWARENESS_EXPIRY_MS;
    for (const [id, entry] of [...held.entries()]) {
      if (entry.receivedAt < cutoff) held.delete(id);
    }
  }, AWARENESS_SWEEP_MS);

  onCleanup(() => {
    shareSoon.clear();
    clearInterval(heartbeat);
    clearInterval(sweep);
    leave();
  });

  const remote = () =>
    [...held.entries()].map(([id, { state }]) => ({
      userId: id,
      tableId: state.tableId,
      rowId: state.rowId,
      columnId: state.columnId,
      editing: Boolean(state.editing),
    }));
  return { remote };
}
