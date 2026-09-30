import { createRoot, createSignal } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import {
  type LocalDatabaseAwareness,
  useDatabaseAwareness,
} from './databases-sync';

const mock = vi.hoisted(() => ({
  event: undefined as
    | ((message: { type: string; data: unknown }) => void)
    | undefined,
  shareAwareness: vi.fn(async () => ({ isOk: () => true })),
}));
vi.mock('@service-connection/websocket', () => ({
  createConnectionWebsocketEffect: (handler: typeof mock.event) => {
    mock.event = handler;
  },
}));
vi.mock('@core/context/user', () => ({ useUserId: () => () => 'me' }));
vi.mock('@service-storage/databases', () => ({
  databasesClient: { shareAwareness: mock.shareAwareness },
}));
vi.mock('./databases', () => ({
  invalidateDatabase: vi.fn(),
  invalidateDatabaseRows: vi.fn(),
}));

function relay(
  userId: string,
  state: Record<string, unknown>,
  ts: number,
  databaseId = 'db'
) {
  mock.event?.({
    type: 'database_awareness',
    data: JSON.stringify({ databaseId, userId, state, ts }),
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-09-30T10:00:00Z'));
});
afterEach(() => {
  vi.useRealTimers();
  mock.shareAwareness.mockClear();
});

it('sends the local state debounced, heartbeats it, and announces leaving on dispose', () => {
  const [local, setLocal] = createSignal<LocalDatabaseAwareness | undefined>({
    tableId: 'tasks',
    rowId: 'row-1',
    columnId: 'name',
  });
  const dispose = createRoot((dispose) => {
    useDatabaseAwareness(() => 'db', local);
    return dispose;
  });
  expect(mock.shareAwareness).not.toHaveBeenCalled();
  setLocal({ tableId: 'tasks', rowId: 'row-1', columnId: 'notes' });
  setLocal({
    tableId: 'tasks',
    rowId: 'row-1',
    columnId: 'notes',
    editing: true,
  });
  vi.advanceTimersByTime(150);
  expect(mock.shareAwareness).toHaveBeenCalledTimes(1);
  expect(mock.shareAwareness).toHaveBeenCalledWith('db', {
    tableId: 'tasks',
    rowId: 'row-1',
    columnId: 'notes',
    editing: true,
  });
  vi.advanceTimersByTime(20_000);
  expect(mock.shareAwareness).toHaveBeenCalledTimes(2);
  expect(mock.shareAwareness).toHaveBeenLastCalledWith('db', {
    tableId: 'tasks',
    rowId: 'row-1',
    columnId: 'notes',
    editing: true,
  });
  setLocal({ tableId: 'tasks' });
  vi.advanceTimersByTime(150);
  expect(mock.shareAwareness).toHaveBeenLastCalledWith('db', {
    tableId: 'tasks',
  });
  dispose();
  expect(mock.shareAwareness).toHaveBeenLastCalledWith('db', {
    tableId: 'tasks',
    left: true,
  });
  expect(mock.shareAwareness).toHaveBeenCalledTimes(4);
});

it('merges remote states per user, drops stale relays, itself, other databases, leavers, and silent viewers', () => {
  const { remote, dispose } = createRoot((dispose) => ({
    remote: useDatabaseAwareness(
      () => 'db',
      () => ({ tableId: 'tasks' })
    ).remote,
    dispose,
  }));
  relay('alex', { tableId: 'tasks', rowId: 'row-1', columnId: 'name' }, 100);
  relay('me', { tableId: 'tasks', rowId: 'row-2', columnId: 'name' }, 101);
  relay(
    'sam',
    { tableId: 'tasks', rowId: 'row-3', columnId: 'name' },
    102,
    'other-db'
  );
  expect(remote()).toEqual([
    {
      userId: 'alex',
      tableId: 'tasks',
      rowId: 'row-1',
      columnId: 'name',
      editing: false,
    },
  ]);
  // An older relay arriving late never rewinds a viewer.
  relay('alex', { tableId: 'tasks', rowId: 'row-9', columnId: 'name' }, 99);
  expect(remote()[0].rowId).toBe('row-1');
  relay(
    'alex',
    { tableId: 'tasks', rowId: 'row-1', columnId: 'notes', editing: true },
    103
  );
  expect(remote()[0]).toMatchObject({ columnId: 'notes', editing: true });
  relay('sam', { tableId: 'people' }, 104);
  expect(remote().map((user) => user.userId)).toEqual(['alex', 'sam']);
  relay('sam', { tableId: 'people', left: true }, 105);
  expect(remote().map((user) => user.userId)).toEqual(['alex']);
  vi.advanceTimersByTime(30_000);
  relay('sam', { tableId: 'people' }, 106);
  vi.advanceTimersByTime(20_000);
  // Alex went quiet for 50 s; Sam refreshed 20 s ago.
  expect(remote().map((user) => user.userId)).toEqual(['sam']);
  dispose();
});
