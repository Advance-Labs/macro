import {
  ChannelThreadUnreadPresenceDocument,
  type ChannelThreadUnreadPresenceQuery,
  type SoupInput,
} from '@service-storage/graphql/generated/graphql';
import { stringifyDocument } from '@urql/core';
import { createRoot, createSignal } from 'solid-js';
import { describe, expect, it, vi } from 'vitest';
import { createChannelThreadUnreadQuery } from './thread-unread-presence';

const mocks = vi.hoisted(() => ({
  query: vi.fn(),
  registerRevalidations: vi.fn<
    typeof import('../soup/graphql/active-queries').registerGraphqlSoupRevalidations
  >(() => vi.fn()),
  registerActiveQuery: vi.fn<
    typeof import('../soup/graphql/active-queries').registerActiveGraphqlSoupQuery
  >(() => vi.fn()),
  registerRefresh: vi.fn<
    typeof import('./register-notification-refresh').registerChannelNotificationRefresh
  >(() => vi.fn(async () => {})),
}));
vi.mock('@app/lib/urql-solid', () => ({
  createUrqlQuery: mocks.query,
}));
vi.mock('@service-storage/graphql-soup', () => ({
  getGraphqlSoupClient: () => ({}),
}));
vi.mock('../soup/graphql/active-queries', () => ({
  registerGraphqlSoupRevalidations: mocks.registerRevalidations,
  registerActiveGraphqlSoupQuery: mocks.registerActiveQuery,
}));
vi.mock('./register-notification-refresh', () => ({
  registerChannelNotificationRefresh: mocks.registerRefresh,
}));

const data = {
  user: {
    id: 'user',
    soup: {
      items: [
        {
          __typename: 'GraphqlSoupChannelMessage',
          id: 'root',
          channelId: 'channel',
          unreadThreadImportant: [
            { id: 'important', state: 'UNSEEN', eventType: 'channel_mention' },
          ],
          unreadThreadNotifications: [
            {
              id: 'notification',
              state: 'UNSEEN',
              eventType: 'channel_message_reaction',
            },
          ],
        },
        { __typename: 'GraphqlSoupChannel', id: 'channel' },
      ],
    },
  },
} satisfies ChannelThreadUnreadPresenceQuery;

describe('unread thread witness adapter', () => {
  it('selects only thread witnesses and registers live refreshes while enabled', () => {
    createRoot((dispose) => {
      const [enabled, setEnabled] = createSignal(true);
      const [input, setInput] = createSignal<SoupInput>({
        initial: { limit: 500 },
      });
      mocks.query.mockReturnValue({
        isFetching: false,
        isSuccess: true,
        data: [{ id: 'notification', state: 'UNSEEN' }],
      });
      createChannelThreadUnreadQuery(input, enabled);
      const options = mocks.query.mock.calls[0][0];
      expect(options().query).toBe(ChannelThreadUnreadPresenceDocument);
      // Narrowing events after the candidate bound can hide older unread replies.
      const document = stringifyDocument(ChannelThreadUnreadPresenceDocument);
      expect(document).toMatch(/states:\s*\[UNSEEN\]/);
      expect(document).toMatch(
        /unreadThreadNotifications:\s*notifications\(\s*filter:\s*\{\s*states:\s*\[UNSEEN\]\s*\}/
      );
      expect(document).toContain('unreadThreadImportant');
      expect(options().select(data)).toEqual([
        {
          id: 'notification',
          state: 'UNSEEN',
          eventType: 'channel_message_reaction',
          channelId: 'channel',
        },
        {
          id: 'important',
          state: 'UNSEEN',
          eventType: 'channel_mention',
          channelId: 'channel',
        },
      ]);
      expect(options().requestPolicy).toBe('cache-and-network');
      const revalidations = mocks.registerRevalidations.mock.calls[0][0];
      const refreshOptions = mocks.registerRefresh.mock.calls[0][0];
      expect(refreshOptions().reader).toMatchObject({
        enabled: true,
        filtered: true,
        notificationIds: ['notification'],
      });
      setInput({ initial: { limit: 400 } });
      expect(options().variables.input.initial.limit).toBe(400);
      expect(revalidations()[0].variables).toMatchObject({
        input: { initial: { limit: 400 } },
      });
      setEnabled(false);
      expect(options().enabled).toBe(false);
      expect(revalidations()).toEqual([]);
      expect(refreshOptions().reader.enabled).toBe(false);
      dispose();
      expect(
        mocks.registerRevalidations.mock.results[0].value
      ).toHaveBeenCalledOnce();
      expect(
        mocks.registerActiveQuery.mock.results[0].value
      ).toHaveBeenCalledOnce();
    });
  });
});
