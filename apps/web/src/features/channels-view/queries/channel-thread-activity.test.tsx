import { MutationUndoProvider, useMutationUndoContext } from '@queries/undo';
import { cleanup, render } from '@solidjs/testing-library';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { createSignal } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useChannelThreadActivity } from './channel-thread-activity';

const mocks = vi.hoisted(() => ({
  query: vi.fn(),
  update: vi.fn(),
  patch: vi.fn(),
  localState: vi.fn(),
  undo: vi.fn(),
  redo: vi.fn(),
  success: vi.fn(() => 1),
  failure: vi.fn(),
  dismiss: vi.fn(),
}));
vi.mock('@queries/channel/thread-activity', () => ({
  createChannelThreadActivityQuery: mocks.query,
}));
vi.mock('@queries/notification/entity-mutations', () => ({
  updateNotificationsForEntities: mocks.update,
}));
vi.mock('@notifications/notification-source', () => ({
  setDoneOverride: mocks.patch,
}));
vi.mock('@notifications/notification-helpers', () => ({
  executeMarkNotificationsUndone: mocks.undo,
  executeMarkNotificationsDone: mocks.redo,
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalNotificationSource: () => ({ withLocalState: mocks.localState }),
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: {
    success: mocks.success,
    failure: mocks.failure,
    dismiss: mocks.dismiss,
  },
}));
vi.mock('@queries/soup/graphql/ast', () => ({
  makeGraphqlSoupInput: (input: unknown) => input,
}));

const mention = {
  id: 'known',
  state: 'UNSEEN' as const,
  eventType: 'channel_mention',
};
type Evidence = {
  pending: { id: string; state: 'UNSEEN' | 'SEEN' }[];
  unread: { id: string; state: 'UNSEEN' | 'SEEN'; eventType: string }[];
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((accept, decline) => {
    resolve = accept;
    reject = decline;
  });
  return { promise, resolve, reject };
}

function setup() {
  const [evidence, setEvidence] = createSignal<Evidence>({
    pending: [mention],
    unread: [mention],
  });
  const [overrides, setOverrides] = createSignal(new Map<string, boolean>());
  mocks.query.mockReturnValue({
    isSuccess: true,
    get data() {
      return evidence();
    },
  });
  mocks.localState.mockImplementation(
    (notification: { id: string; state: string }) =>
      overrides().has(notification.id)
        ? overrides().get(notification.id)
          ? 'done'
          : 'seen'
        : notification.state
  );
  mocks.patch.mockImplementation((ids: string[], done: boolean) => {
    const previous = overrides();
    setOverrides(
      new Map([...previous, ...ids.map((id) => [id, done] as const)])
    );
    return () => setOverrides(previous);
  });
  mocks.undo.mockImplementation(async (ids: string[]) => {
    mocks.patch(ids, false);
  });
  mocks.redo.mockImplementation(async (ids: string[]) => {
    mocks.patch(ids, true);
  });
  const response = deferred<{ id: string }[]>();
  mocks.update.mockReturnValue(response.promise);
  let activity!: ReturnType<typeof useChannelThreadActivity>;
  let undo!: ReturnType<typeof useMutationUndoContext>;
  function Controller() {
    activity = useChannelThreadActivity(() => 'root');
    undo = useMutationUndoContext();
    return null;
  }
  const client = new QueryClient({
    defaultOptions: { mutations: { retry: false } },
  });
  render(() => (
    <QueryClientProvider client={client}>
      <MutationUndoProvider>
        <Controller />
      </MutationUndoProvider>
    </QueryClientProvider>
  ));
  return { activity, undo, response, setEvidence, client };
}

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe('personal thread completion', () => {
  it('completes the canonical thread scope and uses exact receipt IDs for undo and redo', async () => {
    const f = setup();
    expect(f.activity.kind()).toBe('important');
    const operation = f.activity.markDone();
    await vi.waitFor(() => expect(mocks.update).toHaveBeenCalledOnce());
    expect(mocks.update).toHaveBeenCalledWith({
      entities: [{ type: 'channel_thread', id: 'root', messageId: 'root' }],
      operation: 'MARK_DONE',
    });
    expect(f.activity.kind()).toBe('none');
    expect(f.activity.isPending()).toBe(true);
    await f.activity.markDone();
    expect(mocks.update).toHaveBeenCalledOnce();
    f.response.resolve([{ id: 'known' }, { id: 'hidden' }]);
    await operation;
    expect(f.activity.isDone()).toBe(true);
    f.setEvidence({
      pending: [{ id: 'new', state: 'UNSEEN' }],
      unread: [
        { id: 'new', state: 'UNSEEN', eventType: 'channel_message_reply' },
      ],
    });
    expect(f.activity.kind()).toBe('important');
    expect(f.activity.isDone()).toBe(false);
    await f.undo.undo();
    expect(mocks.undo).toHaveBeenCalledWith(['known', 'hidden']);
    expect(f.activity.kind()).toBe('important');
    await f.undo.redo();
    expect(mocks.redo).toHaveBeenCalledWith(['known', 'hidden']);
    expect(mocks.update).toHaveBeenCalledOnce();
    expect(f.activity.kind()).toBe('important');
    f.client.clear();
  });

  it('rolls back failed completion without clearing the unread mention', async () => {
    const f = setup();
    const operation = f.activity.markDone();
    await vi.waitFor(() => expect(mocks.update).toHaveBeenCalledOnce());
    expect(f.activity.kind()).toBe('none');
    f.response.reject(new Error('Unavailable'));
    await operation;
    expect(f.activity.kind()).toBe('important');
    expect(f.activity.isDone()).toBe(false);
    expect(f.undo.canUndo()).toBe(false);
    expect(mocks.failure).toHaveBeenCalledWith('Failed to mark thread as done');
    f.client.clear();
  });

  it('keeps read notifications actionable until they are done', async () => {
    const f = setup();
    f.setEvidence({ pending: [{ id: 'read', state: 'SEEN' }], unread: [] });
    expect(f.activity.kind()).toBe('none');
    expect(f.activity.isDone()).toBe(false);
    const operation = f.activity.markDone();
    await vi.waitFor(() => expect(mocks.update).toHaveBeenCalledOnce());
    f.response.resolve([{ id: 'read' }]);
    await operation;
    expect(f.activity.isDone()).toBe(true);
    f.client.clear();
  });
});
