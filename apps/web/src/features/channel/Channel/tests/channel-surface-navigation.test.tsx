import type { fetchResolvedChannelMessage } from '@queries/messages/timeline';
import { cleanup, render, waitFor } from '@solidjs/testing-library';
import { createSignal, Show } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ChannelHandle, ChannelProps } from '../Channel';
import {
  ChannelMessages,
  ChannelSurface,
  type ChannelTargetRequest,
} from '../ChannelSurface';

const mocks = vi.hoisted(() => ({
  mounts: [] as ChannelProps[],
  fetch: vi.fn<typeof fetchResolvedChannelMessage>(),
  findRoot: vi.fn(() => undefined),
  findThread: vi.fn(() => undefined),
}));

vi.mock('@queries/messages/timeline', () => ({
  fetchResolvedChannelMessage: mocks.fetch,
  findTopLevelMessageInMessageTimeline: mocks.findRoot,
  findThreadIdInMessageTimeline: mocks.findThread,
}));
vi.mock('../Channel', () => ({
  Channel: (props: ChannelProps) => {
    mocks.mounts.push(props);
    return null;
  },
}));

beforeEach(() => {
  mocks.mounts = [];
  vi.clearAllMocks();
});
afterEach(cleanup);

function setup(request: ChannelTargetRequest, messagesMounted = true) {
  const [target, setTarget] = createSignal(request);
  const [showMessages, setShowMessages] = createSignal(messagesMounted);
  const snapshot = {};
  const handle = {
    goToMessage: vi.fn<ChannelHandle['goToMessage']>(),
    goToLatest: vi.fn(),
    getMessagesStateSnapshot: vi.fn(() => snapshot),
  } satisfies ChannelHandle;
  render(() => (
    <ChannelSurface
      channelId="channel"
      targetRequest={target()}
      initialMessagesState={snapshot}
    >
      <Show when={showMessages()}>
        <ChannelMessages />
      </Show>
    </ChannelSurface>
  ));
  const mount = () => mocks.mounts.at(-1)!;
  const ready = () => mount().onHandleReady?.(handle);
  return { handle, mount, ready, setTarget, setShowMessages, snapshot };
}

function deferredReply() {
  type Resolved = Awaited<ReturnType<typeof fetchResolvedChannelMessage>>;
  let resolve!: (message: Resolved) => void;
  const promise = new Promise<Resolved>((finish) => {
    resolve = finish;
  });
  mocks.fetch.mockReturnValueOnce(promise);
  return {
    promise,
    resolve: () =>
      resolve({
        id: 'reply',
        parent: { type: 'channel', id: 'channel' },
        kind: 'thread_reply',
        thread_id: 'root',
        created_at: '2026-10-04T00:00:00Z',
      }),
  };
}

it('holds cold latest until the messages handle is ready and does not replay it on remount', () => {
  const test = setup({ kind: 'latest' });
  expect(test.mount().targetMessageId).toBeUndefined();
  expect(test.mount().initialMessagesStateSnapshot).toBe(test.snapshot);
  expect(test.handle.goToLatest).not.toHaveBeenCalled();
  test.ready();
  expect(test.handle.goToLatest).toHaveBeenCalledOnce();
  test.setShowMessages(false);
  test.setShowMessages(true);
  test.ready();
  expect(test.handle.goToLatest).toHaveBeenCalledOnce();
  expect(test.mount().initialMessagesStateSnapshot).toBe(test.snapshot);
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it('applies only the newest target queued before messages become ready', () => {
  const test = setup({ kind: 'latest' });
  test.setTarget({
    kind: 'message',
    messageId: 'first-reply',
    threadId: 'first-root',
  });
  test.setTarget({
    kind: 'message',
    messageId: 'last-reply',
    threadId: 'last-root',
  });
  expect(test.handle.goToMessage).not.toHaveBeenCalled();
  test.ready();
  expect(test.handle.goToMessage).toHaveBeenCalledExactlyOnceWith(
    'last-root',
    'last-reply'
  );
  expect(test.handle.goToLatest).not.toHaveBeenCalled();
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it('applies fresh message and latest requests after readiness, including repeated targets', () => {
  const test = setup({ kind: 'latest' });
  test.ready();
  const request = {
    kind: 'message' as const,
    messageId: 'reply',
    threadId: 'root',
  };
  test.setTarget(request);
  expect(test.handle.goToMessage).toHaveBeenCalledExactlyOnceWith(
    'root',
    'reply'
  );
  test.setTarget({ ...request });
  expect(test.handle.goToMessage).toHaveBeenCalledTimes(2);
  test.setTarget({ kind: 'latest' });
  expect(test.handle.goToLatest).toHaveBeenCalledTimes(2);
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it.each([true, false])(
  'retains asynchronous resolution when messages mount before resolution: %s',
  async (messagesMounted) => {
    const pending = deferredReply();
    const test = setup(
      { kind: 'message', messageId: 'reply' },
      messagesMounted
    );
    if (messagesMounted) {
      expect(test.mount().targetMessageId).toBeUndefined();
      test.ready();
    } else {
      expect(mocks.mounts).toHaveLength(0);
    }
    expect(test.handle.goToMessage).not.toHaveBeenCalled();
    pending.resolve();
    await pending.promise;
    await Promise.resolve();
    if (messagesMounted) {
      await waitFor(() => {
        expect(test.handle.goToMessage).toHaveBeenCalledExactlyOnceWith(
          'root',
          'reply'
        );
      });
    } else {
      test.setShowMessages(true);
      expect(test.mount().targetMessageId).toBe('root');
      expect(test.mount().targetMessageReplyId).toBe('reply');
      expect(test.mount().initialMessagesStateSnapshot).toBeUndefined();
      test.ready();
      expect(test.handle.goToMessage).not.toHaveBeenCalled();
    }
    expect(mocks.fetch).toHaveBeenCalledExactlyOnceWith(
      { type: 'channel', id: 'channel' },
      'reply'
    );
  }
);

it('ignores a stale asynchronous reply after a newer latest request', async () => {
  const pending = deferredReply();
  const test = setup({ kind: 'message', messageId: 'reply' });
  test.ready();
  test.setTarget({ kind: 'latest' });
  expect(test.handle.goToLatest).toHaveBeenCalledOnce();
  pending.resolve();
  await pending.promise;
  await Promise.resolve();
  expect(test.handle.goToMessage).not.toHaveBeenCalled();
  expect(test.handle.goToLatest).toHaveBeenCalledOnce();
});
