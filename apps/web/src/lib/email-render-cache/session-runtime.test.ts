import { webcrypto } from 'node:crypto';
import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { IndexedDbArtifacts } from './indexeddb';
import { digest } from './keys';
import {
  createEmailRenderSession,
  type EmailRenderSessionOptions,
} from './session-runtime';

const request = {
  messageId: 'message',
  threadId: 'thread',
  mailboxId: 'inbox',
  input: { html: '<p>Body</p>' },
  options: {},
};

class Channel {
  static instances: Channel[] = [];
  onmessage?: (event: MessageEvent<unknown>) => void;
  postMessage = vi.fn();
  close = vi.fn();
  constructor(readonly name: string) {
    Channel.instances.push(this);
  }
}

const sessions: ReturnType<typeof createEmailRenderSession>[] = [];
const values = new Map<string, string>();
const options = (): EmailRenderSessionOptions => ({
  origin: 'https://mail.example',
  environment: 'test',
  profileScope: 'profile',
  viewerId: 'viewer',
  enabled: true,
  native: false,
  mobile: false,
  waitForInvalidation: async () => {},
  onRemoteInvalidation() {},
});

function start(overrides: Partial<EmailRenderSessionOptions> = {}) {
  const session = createEmailRenderSession({ ...options(), ...overrides });
  sessions.push(session);
  return session;
}

beforeEach(() => {
  Channel.instances = [];
  values.clear();
  vi.stubGlobal('crypto', webcrypto);
  vi.stubGlobal('indexedDB', new IDBFactory());
  vi.stubGlobal('BroadcastChannel', Channel);
  vi.stubGlobal('navigator', {
    storage: { estimate: async () => ({ quota: 1024 ** 3, usage: 0 }) },
  });
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  });
});

afterEach(async () => {
  for (const session of sessions.splice(0)) await session.dispose();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it('clears cold artifacts and broadcasts logout while disabled, without Solid', async () => {
  const session = start({ enabled: false });
  expect(await indexedDB.databases()).toHaveLength(0);
  await session.invalidate(true);
  const namespace = await digest(
    JSON.stringify(['https://mail.example', 'test', 'profile', 'viewer'])
  );
  const store = new IndexedDbArtifacts(namespace, 1024);
  try {
    expect(await store.generation()).toBe(1);
    expect(values.size).toBe(0);
    expect(
      Channel.instances.some((channel) =>
        channel.postMessage.mock.calls.some(
          ([message]) => message.sessionEnded === true
        )
      )
    ).toBe(true);
    expect(() => session.cache.acquire(request)).toThrow('cancelled');
  } finally {
    store.close();
  }
});

it('disposes immediately and joins a pending reset when logout overtakes it', async () => {
  const pending = Promise.withResolvers<void>();
  const invalidate = vi
    .spyOn(IndexedDbArtifacts.prototype, 'invalidate')
    .mockImplementationOnce(() => pending.promise);
  const session = start({ enabled: false });
  const reset = session.invalidate();
  await vi.waitFor(() => expect(invalidate).toHaveBeenCalledOnce());
  expect(() => session.cache.acquire(request)).toThrow('cancelled');
  expect(values.size).toBe(1);
  let settled = false;
  async function logout() {
    await session.invalidate(true);
    settled = true;
  }
  const ended = logout();
  await vi.waitFor(() =>
    expect(
      Channel.instances.some((channel) =>
        channel.postMessage.mock.calls.some(
          ([message]) => message.sessionEnded === true
        )
      )
    ).toBe(true)
  );
  expect(settled).toBe(false);
  pending.resolve();
  await Promise.all([reset, ended]);
  expect(settled).toBe(true);
  expect(values.size).toBe(0);
  expect(invalidate).toHaveBeenCalledOnce();
});

it('does not open storage after disposal while waiting for a previous invalidation', async () => {
  const barrier = Promise.withResolvers<void>();
  const session = start({ waitForInvalidation: () => barrier.promise });
  await session.dispose();
  const open = vi.spyOn(indexedDB, 'open');
  barrier.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(open).not.toHaveBeenCalled();
  expect(
    Channel.instances.every((channel) => channel.close.mock.calls.length > 0)
  ).toBe(true);
});

it('invalidates before reporting another tab logout, without echoing it', async () => {
  const notify = vi.fn<EmailRenderSessionOptions['onRemoteInvalidation']>(
    () => {
      expect(() => session.cache.acquire(request)).toThrow('cancelled');
    }
  );
  const session = start({ onRemoteInvalidation: notify });
  await vi.waitFor(() => expect(Channel.instances[0]?.onmessage).toBeDefined());
  Channel.instances[0].onmessage?.({
    data: { kind: 'invalidate', sessionEnded: true },
  } as MessageEvent);
  expect(notify).toHaveBeenCalledWith(true, expect.any(Promise));
  await notify.mock.calls[0][1];
  await session.dispose();
  for (const channel of Channel.instances)
    expect(channel.postMessage).not.toHaveBeenCalled();
});

it('keeps a failed clear quarantined', async () => {
  vi.spyOn(IndexedDbArtifacts.prototype, 'invalidate').mockRejectedValue(
    new Error('Storage denied')
  );
  const session = start({ enabled: false });
  await session.invalidate(true);
  expect(values.size).toBe(1);
});

it('uses no artifact database on native', async () => {
  const session = start({ native: true });
  const lease = session.cache.acquire(request);
  expect((await lease.promise).html).toBe('<p>Body</p>');
  lease.release();
  await session.invalidate(true);
  expect(await indexedDB.databases()).toHaveLength(0);
});
