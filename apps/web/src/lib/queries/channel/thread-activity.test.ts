import {
  ChannelThreadActivityDocument,
  type ChannelThreadActivityQuery,
  type SoupInput,
} from '@service-storage/graphql/generated/graphql';
import { createRoot, createSignal } from 'solid-js';
import { expect, it, vi } from 'vitest';
import { createChannelThreadActivityQuery } from './thread-activity';

const mocks = vi.hoisted(() => ({
  query: vi.fn(),
  revalidations: vi.fn<
    typeof import('../soup/graphql/active-queries').registerGraphqlSoupRevalidations
  >(() => vi.fn()),
  active: vi.fn<
    typeof import('../soup/graphql/active-queries').registerActiveGraphqlSoupQuery
  >(() => vi.fn()),
  refresh: vi.fn<
    typeof import('./register-notification-refresh').registerChannelNotificationRefresh
  >(() => vi.fn(async () => {})),
}));
vi.mock('@app/lib/urql-solid', () => ({ createUrqlQuery: mocks.query }));
vi.mock('@service-storage/graphql-soup', () => ({
  getGraphqlSoupClient: () => ({}),
}));
vi.mock('../soup/graphql/active-queries', () => ({
  registerGraphqlSoupRevalidations: mocks.revalidations,
  registerActiveGraphqlSoupQuery: mocks.active,
}));
vi.mock('./register-notification-refresh', () => ({
  registerChannelNotificationRefresh: mocks.refresh,
}));

const row = {
  __typename: 'GraphqlSoupChannelMessage' as const,
  id: 'root',
  pendingThreadNotifications: [{ id: 'read', state: 'SEEN' as const }],
  unreadThreadNotifications: [
    {
      id: 'reply',
      state: 'UNSEEN' as const,
      eventType: 'channel_message_reply',
    },
  ],
  unreadThreadImportant: [
    { id: 'important', state: 'UNSEEN' as const, eventType: 'channel_mention' },
  ],
};
const data = {
  user: { id: 'user', soup: { items: [row] } },
} satisfies ChannelThreadActivityQuery;

it('selects bounded card evidence and registers live notification refreshes', () => {
  createRoot((dispose) => {
    const [input, setInput] = createSignal<SoupInput>({
      initial: { limit: 1 },
    });
    const [ready, setReady] = createSignal(false);
    mocks.query.mockReturnValue({
      isFetching: false,
      get isSuccess() {
        return ready();
      },
      get data() {
        if (!ready()) throw new Error('Read pending data');
        return {
          pending: row.pendingThreadNotifications,
          unread: [
            ...row.unreadThreadNotifications,
            ...row.unreadThreadImportant,
          ],
        };
      },
    });
    createChannelThreadActivityQuery(input);
    const options = mocks.query.mock.calls[0][0]();
    expect(options.query).toBe(ChannelThreadActivityDocument);
    expect(options.select(data)).toEqual({
      pending: row.pendingThreadNotifications,
      unread: [...row.unreadThreadNotifications, ...row.unreadThreadImportant],
    });
    const refresh = mocks.refresh.mock.calls[0][0];
    expect(refresh().reader.notificationIds).toEqual([]);
    setReady(true);
    expect(refresh().reader.notificationIds).toEqual([
      'read',
      'reply',
      'important',
    ]);
    setInput({ initial: { limit: 2 } });
    const registered = mocks.revalidations.mock.calls[0][0];
    expect(registered()[0].variables).toEqual({
      input: { initial: { limit: 2 } },
    });
    dispose();
    expect(mocks.revalidations.mock.results[0].value).toHaveBeenCalledOnce();
    expect(mocks.active.mock.results[0].value).toHaveBeenCalledOnce();
  });
});
