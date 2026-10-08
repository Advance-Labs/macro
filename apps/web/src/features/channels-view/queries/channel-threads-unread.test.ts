import type { UnifiedNotification } from '@notifications/types';
import type {
  NotificationState,
  SoupInput,
} from '@service-storage/graphql/generated/graphql';
import { createRoot, createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useChannelThreadsActivity } from './channel-threads-unread';

const mocks = vi.hoisted(() => ({
  userId: vi.fn(),
  graphqlFlag: vi.fn(),
  notifications: vi.fn(),
  query: vi.fn(),
  withLocalState: vi.fn(),
}));
vi.mock('@app/lib/analytics/posthog', () => ({
  useFeatureFlag: () => mocks.graphqlFlag,
}));
vi.mock('@core/context/user', () => ({ useUserId: () => mocks.userId }));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalNotificationSource: () => ({
    notifications: mocks.notifications,
    withLocalState: mocks.withLocalState,
  }),
}));
vi.mock('@queries/channel/thread-unread-presence', () => ({
  createChannelThreadUnreadQuery: mocks.query,
}));
vi.mock('@service-storage/websocket', () => ({
  storageWS: { reconnectIfDisconnected: vi.fn() },
  createWebSocketJob: vi.fn(),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { addEventListener: vi.fn(), send: vi.fn() },
  state: () => 'closed',
  createConnectionBlockWebsocketEffect: vi.fn(),
  createConnectionWebsocketEffect: vi.fn(),
}));

const reply: UnifiedNotification = {
  id: 'notification',
  entity_id: 'channel',
  entity_type: 'channel',
  created_at: '2026-10-01T12:00:00Z',
  updated_at: '2026-10-01T12:00:00Z',
  state: 'unseen',
  sent: true,
  viewed_at: null,
  notification_event_type: 'channel_message_reply',
  notification_metadata: {
    tag: 'channel_message_reply',
    content: {
      channelType: 'directMessage',
      messageId: 'reply',
      threadId: 'root',
      messageContent: 'Hello',
    },
  },
};

let dispose: () => void;
afterEach(() => {
  dispose?.();
  vi.clearAllMocks();
});

function setup(graphql = true) {
  return createRoot((cleanup) => {
    dispose = cleanup;
    const [enabled, setEnabled] = createSignal(true);
    const [userId, setUserId] = createSignal<string | undefined>('user');
    const [graphqlEnabled, setGraphqlEnabled] = createSignal(graphql);
    const [loading, setLoading] = createSignal(true);
    const [notifications, setNotifications] = createSignal<
      UnifiedNotification[]
    >([]);
    const [witnesses, setWitnesses] = createSignal<
      {
        id: string;
        state: NotificationState;
        channelId?: string;
        eventType?: string;
      }[]
    >([]);
    const [localState, setLocalState] =
      createSignal<UnifiedNotification['state']>();
    mocks.userId.mockImplementation(userId);
    mocks.graphqlFlag.mockImplementation(() => ({ enabled: graphqlEnabled() }));
    mocks.notifications.mockImplementation(notifications);
    mocks.withLocalState.mockImplementation(
      ({ state }) => localState() ?? state
    );
    mocks.query.mockImplementation(
      (_input: () => SoupInput, queryEnabled: () => boolean) => ({
        get isEnabled() {
          return queryEnabled();
        },
        get isLoading() {
          return loading();
        },
        get data() {
          if (loading() || !queryEnabled())
            throw new Error('Read inactive resource');
          return witnesses();
        },
      })
    );
    const activity = useChannelThreadsActivity(enabled);
    return {
      unread: () => activity.kind() !== 'none',
      activity,
      setEnabled,
      setUserId,
      setGraphqlEnabled,
      setLoading,
      setNotifications,
      setWitnesses,
      setLocalState,
    };
  });
}

describe('shared unread thread badge', () => {
  it('uses bounded participant-scoped witnesses without reading pending data or the full feed', () => {
    const { unread, setLoading, setWitnesses, setLocalState } = setup();
    expect(unread()).toBe(false);
    const input = mocks.query.mock.calls[0][0]() as SoupInput;
    expect(input.initial?.limit).toBe(500);
    expect(JSON.stringify(input.initial?.filters)).toContain('user');
    expect(JSON.stringify(input.initial?.filters)).toContain('UNSEEN');
    expect(mocks.notifications).not.toHaveBeenCalled();
    setLoading(false);
    setWitnesses([{ id: 'witness', state: 'UNSEEN' }]);
    expect(unread()).toBe(true);
    setLocalState('seen');
    expect(unread()).toBe(false);
    setLocalState('done');
    expect(unread()).toBe(false);
    setLocalState(undefined);
    setWitnesses([{ id: 'witness', state: 'SEEN' }]);
    expect(unread()).toBe(false);
    setWitnesses([]);
    expect(unread()).toBe(false);
    expect(mocks.notifications).not.toHaveBeenCalled();
  });

  it('disables the query and indicator when the flag is off or no viewer exists', () => {
    const { unread, setEnabled, setUserId, setLoading, setWitnesses } = setup();
    setLoading(false);
    setWitnesses([{ id: 'witness', state: 'UNSEEN' }]);
    const queryEnabled = mocks.query.mock.calls[0][1];
    expect(unread()).toBe(true);
    setEnabled(false);
    expect(queryEnabled()).toBe(false);
    expect(unread()).toBe(false);
    setEnabled(true);
    setUserId(undefined);
    expect(queryEnabled()).toBe(false);
    expect(unread()).toBe(false);
  });

  it('uses existing notifications only for the REST transport', () => {
    const { unread, setNotifications, setLoading, setGraphqlEnabled } =
      setup(false);
    expect(unread()).toBe(false);
    setNotifications([reply]);
    expect(unread()).toBe(true);
    setNotifications([{ ...reply, state: 'seen' }]);
    expect(unread()).toBe(false);
    setNotifications([{ ...reply, state: 'done' }]);
    expect(unread()).toBe(false);
    setNotifications([{ ...reply, deleted_at: reply.created_at }]);
    expect(unread()).toBe(false);
    setNotifications([{ ...reply, entity_type: 'document' }]);
    expect(unread()).toBe(false);
    setNotifications([reply]);
    mocks.notifications.mockClear();
    setGraphqlEnabled(true);
    setLoading(false);
    expect(unread()).toBe(false);
    expect(mocks.notifications).not.toHaveBeenCalled();
  });

  it('ignores top-level sends but includes thread reactions and mentions', () => {
    const { unread, setNotifications } = setup(false);
    setNotifications([
      {
        ...reply,
        notification_metadata: {
          tag: 'channel_message_send',
          content: { channelType: 'directMessage', messageId: 'root' },
        },
      },
    ]);
    expect(unread()).toBe(false);
    setNotifications([
      {
        ...reply,
        notification_metadata: {
          tag: 'channel_message_reaction',
          content: {
            channelType: 'directMessage',
            messageId: 'reply',
            threadId: 'root',
            messageContent: 'Hello',
            emoji: '👍',
          },
        },
      },
    ]);
    expect(unread()).toBe(true);
    for (const threadId of [undefined, 'root']) {
      setNotifications([
        {
          ...reply,
          notification_metadata: {
            tag: 'channel_mention',
            content: {
              channelType: 'directMessage',
              messageId: 'message',
              threadId,
              messageContent: 'Hello',
            },
          },
        },
      ]);
      expect(unread()).toBe(true);
    }
  });
  it('reports unread thread activity for each channel independently', () => {
    const f = setup();
    f.setLoading(false);
    f.setWitnesses([
      {
        id: 'reply',
        channelId: 'one',
        state: 'UNSEEN',
        eventType: 'channel_message_reply',
      },
      {
        id: 'important',
        channelId: 'two',
        state: 'UNSEEN',
        eventType: 'channel_mention',
      },
      {
        id: 'read',
        channelId: 'three',
        state: 'SEEN',
        eventType: 'channel_mention',
      },
    ]);
    expect(f.activity.kind()).toBe('important');
    expect(f.activity.forChannel('one')).toBe('important');
    expect(f.activity.forChannel('two')).toBe('important');
    expect(f.activity.forChannel('three')).toBe('none');
    expect(f.activity.forChannel('other')).toBe('none');
    f.setLocalState('done');
    expect(f.activity.forChannel('two')).toBe('none');
    f.setEnabled(false);
    expect(f.activity.kind()).toBe('none');
  });

  it('does not count a top-level send as an unread thread in that channel', () => {
    const f = setup(false);
    f.setNotifications([
      {
        ...reply,
        entity_id: 'one',
        notification_metadata: {
          tag: 'channel_message_send',
          content: { messageId: 'message', channelType: 'directMessage' },
        },
      },
      { ...reply, entity_id: 'two' },
    ]);
    expect(f.activity.forChannel('one')).toBe('none');
    expect(f.activity.forChannel('two')).toBe('important');
  });
});
