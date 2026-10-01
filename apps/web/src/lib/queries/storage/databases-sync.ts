/** Gateway liveness for databases, apart from `./databases` to keep the websocket out of startup. */
import { useUserId } from '@core/context/user';
import { createConnectionWebsocketEffect } from '@service-connection/websocket';
import { databasesClient } from '@service-storage/databases';
import type { Awareness } from '@service-storage/generated/schemas/awareness';
import { ReactiveMap } from '@solid-primitives/map';
import { debounce } from '@solid-primitives/scheduled';
import { type Accessor, createEffect, on, onCleanup, untrack } from 'solid-js';
import { invalidateDatabase } from './databases';

/** Gateway message type published by `crates/databases` on every write. */
const TABLE_CHANGED_MESSAGE_TYPE = 'database_table_changed';
/** Gateway message type relaying one viewer's awareness to the others. */
const AWARENESS_MESSAGE_TYPE = 'database_awareness';

const AWARENESS_SEND_DEBOUNCE_MS = 150;
const AWARENESS_HEARTBEAT_MS = 20_000;
/** A viewer not heard from within this window is treated as gone. */
const AWARENESS_EXPIRY_MS = 45_000;
const AWARENESS_SWEEP_MS = 5_000;

/** One table's new version, as the gateway announces it. */
type DatabaseTableChange = {
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

/** Every table change the gateway reports; it carries only the new version, never rows. */
export function useDatabaseTableChanges(
  onChange: (change: DatabaseTableChange) => void
) {
  createConnectionWebsocketEffect((message) => {
    if (message.type !== TABLE_CHANGED_MESSAGE_TYPE) return;
    const data = parseMessageData<Partial<DatabaseTableChange>>(message);
    if (
      typeof data?.databaseId !== 'string' ||
      typeof data.tableId !== 'string' ||
      typeof data.version !== 'number'
    )
      return;
    onChange({
      databaseId: data.databaseId,
      tableId: data.tableId,
      version: data.version,
    });
  });
}

/** Re-read a database's schema whenever the gateway reports one of its tables changed. */
export function useDatabaseTableChangedSync(
  databaseId: () => string | undefined
) {
  useDatabaseTableChanges((change) => {
    if (change.databaseId === databaseId())
      void invalidateDatabase(change.databaseId);
  });
}

/** Where this client is inside a database. */
export type LocalDatabaseAwareness = Omit<Awareness, 'left'>;

/** Where another viewer is inside the database. */
type RemoteDatabaseAwareness = Omit<Awareness, 'left' | 'editing'> & {
  userId: string;
  editing: boolean;
};

type AwarenessMessage = {
  databaseId: string;
  userId: string;
  state: Awareness;
  ts: number;
};

type HeldAwareness = {
  state: Awareness;
  /** Server timestamp, orders messages from the same viewer. */
  serverTimestamp: number;
  /** Local clock, decides expiry so clock skew cannot drop live viewers. */
  receivedAt: number;
};

/**
 * Share where this client is, debounced and on a heartbeat, and follow everyone else,
 * dropping stale relays, this client's own, and viewers silent past the expiry.
 */
export function useDatabaseAwareness(
  databaseId: () => string | undefined,
  local: () => LocalDatabaseAwareness | undefined
): { remote: Accessor<RemoteDatabaseAwareness[]> } {
  const userId = useUserId();
  const held = new ReactiveMap<string, HeldAwareness>();

  let announced: { databaseId: string; state: Awareness } | undefined;
  const send = (id: string, state: Awareness) => {
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
    const payload: Awareness = { tableId: state.tableId };
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
    if (current && current.serverTimestamp > data.ts) return;
    if (data.state?.left) {
      held.delete(data.userId);
      return;
    }
    if (!data.state?.tableId) return;
    held.set(data.userId, {
      state: data.state,
      serverTimestamp: data.ts,
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
