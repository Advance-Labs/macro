import type { UnifiedNotification } from '@notifications/types';
import { describe, expect, it } from 'vitest';
import {
  unreadNotificationChip,
  unreadThreads,
} from '../unread-thread-navigation';

function reply(id: string, threadId: string, hour = 12): UnifiedNotification {
  return {
    id,
    entity_id: 'channel',
    entity_type: 'channel',
    state: 'unseen',
    sent: true,
    created_at: `2026-09-01T${hour}:00:00Z`,
    updated_at: '2026-09-01T00:00:00Z',
    viewed_at: null,
    notification_event_type: 'channel_message_reply',
    notification_metadata: {
      tag: 'channel_message_reply',
      content: {
        messageId: id,
        threadId,
        channelType: 'private',
        messageContent: 'Hello',
      },
    },
  };
}
const rows = [
  { id: 'old', created_at: '2026-08-01T00:00:00Z' },
  { id: 'middle', created_at: '2026-08-02T00:00:00Z' },
  { id: 'new', created_at: '2026-08-03T00:00:00Z' },
];
const order = { rows, unloaded: new Map() };
const visible = { first: 'middle', last: 'middle' };

describe('channel unread notification navigation', () => {
  it('counts three replies and a mention of one reply as one thread', () => {
    const mention = reply('third', 'old', 14);
    mention.notification_metadata = {
      tag: 'channel_mention',
      content: {
        messageId: 'third',
        threadId: 'old',
        channelType: 'private',
        messageContent: '@you',
      },
    };
    expect(
      unreadThreads([
        reply('first', 'old'),
        reply('second', 'old', 13),
        reply('third', 'old', 14),
        mention,
      ])
    ).toEqual([
      {
        threadId: 'old',
        messageId: 'third',
        createdAt: '2026-09-01T14:00:00Z',
      },
    ]);
  });

  it('compares actual instants even when timestamps differ in precision', () => {
    const early = {
      ...reply('early', 'old'),
      created_at: '2026-09-01T12:00:00Z',
    };
    const later = {
      ...reply('later', 'old'),
      created_at: '2026-09-01T12:00:00.500Z',
    };
    expect(unreadThreads([early, later])[0].messageId).toBe('later');
    expect(
      unreadThreads([
        early,
        {
          ...later,
          notification_metadata: reply('later', 'new').notification_metadata,
        },
      ])[0].threadId
    ).toBe('new');
  });

  it('excludes seen, done, reactions, and non-channel records', () => {
    const reaction = reply('reaction', 'new');
    reaction.notification_metadata = {
      tag: 'channel_message_reaction',
      content: {
        messageId: 'reaction',
        threadId: 'new',
        channelType: 'private',
        emoji: '👍',
        messageContent: 'Hello',
      },
    };
    expect(
      unreadThreads([
        { ...reply('seen', 'old'), state: 'seen' },
        { ...reply('done', 'old'), state: 'done' },
        { ...reply('document', 'old'), entity_type: 'document' },
        reaction,
      ])
    ).toEqual([]);
  });

  it('counts separate top-level messages separately and merges their thread notifications', () => {
    const root = (id: string): UnifiedNotification => ({
      ...reply(id, id),
      notification_metadata: {
        tag: 'channel_message_send',
        content: { messageId: id, channelType: 'private' },
      },
    });
    expect(
      unreadThreads([root('old'), root('new'), reply('reply', 'old', 13)])
    ).toHaveLength(2);
  });

  it('shows one chip for the most recent notification, with the total thread count', () => {
    const threads = unreadThreads([
      reply('a', 'new', 12),
      reply('b', 'old', 13),
    ]);
    expect(unreadNotificationChip(threads, order, visible)).toMatchObject({
      count: 2,
      direction: 'above',
      thread: { messageId: 'b' },
    });
    // A live delivery changes the target and direction, but never double-counts its thread.
    const live = unreadThreads([
      reply('a', 'new', 12),
      reply('b', 'old', 13),
      reply('c', 'new', 14),
    ]);
    expect(unreadNotificationChip(live, order, visible)).toMatchObject({
      count: 2,
      direction: 'below',
      thread: { messageId: 'c' },
    });
  });

  it('uses measured reply position within a tall thread and hides for a visible message', () => {
    const threads = unreadThreads([reply('a', 'middle')]);
    expect(
      unreadNotificationChip(threads, order, visible, 'above')?.direction
    ).toBe('above');
    expect(
      unreadNotificationChip(threads, order, visible, 'below')?.direction
    ).toBe('below');
    expect(
      unreadNotificationChip(threads, order, visible, 'visible')
    ).toBeUndefined();
    expect(unreadNotificationChip(threads, order, visible)?.direction).toBe(
      'below'
    );
  });

  it('follows the rendered row order when a live insert arrives out of timestamp order', () => {
    // Concurrent sends reach the client in delivery order, so the newest row
    // can carry an older timestamp than the rows rendered above it.
    const shuffled = {
      rows: [rows[0], rows[2], rows[1]],
      unloaded: new Map(),
    };
    const below = unreadThreads([reply('a', 'middle', 13)]);
    expect(
      unreadNotificationChip(below, shuffled, { first: 'new', last: 'new' })
        ?.direction
    ).toBe('below');
    const above = unreadThreads([reply('a', 'new', 13)]);
    expect(
      unreadNotificationChip(above, shuffled, {
        first: 'middle',
        last: 'middle',
      })?.direction
    ).toBe('above');
  });

  it('places an unloaded parent against the loaded window, not the viewport', () => {
    const older = { id: 'older', created_at: '2026-07-01T00:00:00Z' };
    const newer = { id: 'newer', created_at: '2026-09-01T00:00:00Z' };
    const window = (root: typeof older) => ({
      rows,
      unloaded: new Map([[root.id, root]]),
    });
    expect(
      unreadNotificationChip(
        unreadThreads([reply('a', 'older')]),
        window(older),
        { first: 'new', last: 'new' }
      )?.direction
    ).toBe('above');
    expect(
      unreadNotificationChip(
        unreadThreads([reply('a', 'newer')]),
        window(newer),
        { first: 'old', last: 'old' }
      )?.direction
    ).toBe('below');
  });

  it('waits for layout and an unloaded parent position instead of guessing from reply time', () => {
    const threads = unreadThreads([reply('a', 'old')]);
    expect(unreadNotificationChip(threads, order, undefined)).toBeUndefined();
    expect(
      unreadNotificationChip(
        threads,
        { rows: [], unloaded: new Map() },
        visible
      )
    ).toBeUndefined();
  });
});
