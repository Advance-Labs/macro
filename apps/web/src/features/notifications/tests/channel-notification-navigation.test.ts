import { createAppSplitRouterMiddleware } from '@components/app/split-layout/split-router/app-middleware';
import {
  createRoutedDetailLayout,
  detailRoutes,
  routeFor,
  searchFor,
} from '@components/app/split-layout/tests/fixtures';
import { toast } from '@core/component/Toast/Toast';
import type { BlockOrchestrator } from '@core/orchestrator';
import { createRoot } from 'solid-js';
import { beforeEach, expect, it, onTestFinished, vi } from 'vitest';
import { openNotification } from '../notification-navigation';
import type { UnifiedNotification } from '../types';

vi.mock('@app/signal/splitLayout', () => ({
  globalSplitManager: () => undefined,
}));
vi.mock('@components/app/split-layout/componentRegistry', () => ({
  resolveComponent: () => ({ element: undefined }),
}));
vi.mock('@core/constant/settingsTabsConfig', () => ({
  settingsTabToSlug: (tab: string) => tab,
}));
// Import the real route graph without opening service sockets in jsdom.
vi.mock('@service-storage/websocket', () => ({
  storageWS: { reconnectIfDisconnected: vi.fn() },
  createWebSocketJob: vi.fn(),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { addEventListener: vi.fn(), send: vi.fn() },
  state: () => 'closed',
  createConnectionWebsocketEffect: vi.fn(),
}));
vi.mock('@core/component/Toast/Toast', () => ({ toast: { alert: vi.fn() } }));
vi.mock('@app/features/calendar-view/calendar-range', () => ({
  createCalendarRange: vi.fn(),
}));
vi.mock('@app/features/calendar-view/calendar-navigation', () => ({
  openCalendarView: vi.fn(),
}));
vi.mock('@app/lib/constants/file-metadata', async (importOriginal) => ({
  ...(await importOriginal<
    typeof import('@app/lib/constants/file-metadata')
  >()),
  isBlockAlias: () => false,
  itemToBlockName: (value: { fileType: string }) => value.fileType,
  resolveBlockAlias: (type: string) => type,
}));

beforeEach(() => vi.clearAllMocks());
vi.mock('@core/constant/featureFlags', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@core/constant/featureFlags')>()),
  enableCalendarUi: false,
  enableReminders: false,
  isFeatureEnabled: vi.fn(),
  USE_MACRO_PR_SUMMARY_BLOCK: true,
}));
vi.mock('@core/util/url', () => ({ openExternalUrl: vi.fn() }));
vi.mock('@queries/notification/user-notifications', () => ({
  getNotificationById: vi.fn(),
}));
vi.mock('@queries/reminders/reminders', () => ({ getReminderById: vi.fn() }));
vi.mock('../notification-resolvers', () => ({
  DefaultNotificationBlockNameResolver: vi.fn(),
}));
vi.mock('../notification-helpers', () => ({
  isChannelNotification: (notification: UnifiedNotification) =>
    [
      'channel_message_send',
      'channel_message_reply',
      'channel_mention',
    ].includes(notification.notification_metadata.tag),
}));
vi.mock('../notification-source', () => ({
  CHANNEL_EVENT_TYPES: [
    'channel_mention',
    'channel_message_send',
    'channel_message_reply',
    'document_mention',
  ],
}));
vi.mock('../notification-stacking', () => ({
  getMostRecentNotification: vi.fn(),
  stackNotifications: vi.fn(),
}));

async function setup(location: 'preview' | 'split' | 'closed') {
  const navigate = vi.fn();
  const createBlockInstance = vi.fn(() => ({
    element: undefined,
    dispose: vi.fn(),
    detach: vi.fn(),
  }));
  const orchestrator = {
    createBlockInstance,
    getBlockHandle: async () => ({ goToLocationFromParams: navigate }),
  } as unknown as BlockOrchestrator;
  const { manager: layout, router } = createRoot((dispose) => {
    const path =
      location === 'split'
        ? '/channel/channel'
        : location === 'preview'
          ? '/channels/channel'
          : '/channels';
    const result = createRoutedDetailLayout(
      orchestrator,
      `${path}/~/home`,
      detailRoutes,
      createAppSplitRouterMiddleware({
        isTouchDevice: () => location === 'split',
      })
    );
    onTestFinished(() => {
      result.router.dispose();
      dispose();
    });
    return result;
  });
  await router.settled();
  const [first, other] = layout.splits();
  layout.activateSplit(other.id);
  const activate = vi.fn(() => layout.activateSplit(first.id));
  const release = layout.registerOpenViews(() =>
    location === 'preview'
      ? [
          {
            owner: 'chat',
            content: { type: 'channel', id: 'channel' },
            activate,
          },
        ]
      : []
  );
  return {
    layout,
    router,
    activate,
    navigate,
    release,
    createBlockInstance,
    first,
  };
}

it.each([
  'channel_message_send',
  'channel_message_reply',
  'channel_mention',
] as const)(
  'reuses the route-backed Chat detail for %s and navigates to the notification target',
  async (tag) => {
    const { layout, router, navigate, release, createBlockInstance, first } =
      await setup('preview');
    const mount = first.mount;
    const ownerRoute = routeFor(router, first.id);
    const notification = {
      entity_id: 'channel',
      notification_metadata: {
        tag,
        content: { messageId: 'message', threadId: 'thread' },
      },
    } as UnifiedNotification;

    const result = await openNotification(notification, layout);
    await router.settled();

    expect(result.isOk()).toBe(true);
    expect(layout.activeSplitId()).toBe(first.id);
    expect(createBlockInstance).not.toHaveBeenCalled();
    expect(toast.alert).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
    expect(searchFor(router, first.id, 'channels')).toMatchObject({
      messageId: ['message'],
      ...(tag === 'channel_message_send' ? {} : { threadId: ['thread'] }),
      seek: [expect.any(String)],
    });
    expect(routeFor(router, first.id)).toEqual(ownerRoute);
    expect(layout.splits().find((split) => split.id === first.id)?.mount).toBe(
      mount
    );

    release();
    router.navigatePane(router.panes()[0]!, '/channels');
    await router.settled();
    await openNotification(notification, layout);
    await router.settled();
    expect(searchFor(router, first.id, 'channels')?.messageId).toEqual([
      'message',
    ]);
    expect(createBlockInstance).not.toHaveBeenCalled();
  }
);

it.each(['channel_invite', 'call_started'] as const)(
  'activates the existing preview for %s without opening a split',
  async (tag) => {
    const { layout, activate, navigate, createBlockInstance } =
      await setup('preview');
    await openNotification(
      {
        entity_id: 'channel',
        notification_metadata: { tag, content: {} },
      } as UnifiedNotification,
      layout
    );

    expect(activate).toHaveBeenCalledOnce();
    expect(createBlockInstance).not.toHaveBeenCalled();
    expect(toast.alert).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  }
);

it('reports an ordinary channel notification applied after preview activation', async () => {
  const { layout, activate } = await setup('preview');
  const onApplied = vi.fn();

  await openNotification(
    {
      entity_id: 'channel',
      notification_metadata: { tag: 'channel_invite', content: {} },
    } as UnifiedNotification,
    layout,
    false,
    undefined,
    undefined,
    { onApplied }
  );

  expect(activate).toHaveBeenCalledOnce();
  expect(onApplied).toHaveBeenCalledOnce();
});

it.each(['split', 'closed'] as const)(
  'preserves notification navigation when the channel is %s',
  async (location) => {
    const { layout, router, navigate, createBlockInstance, first } =
      await setup(location);
    await openNotification(
      {
        entity_id: 'channel',
        notification_metadata: {
          tag: 'channel_message_send',
          content: { messageId: 'message' },
        },
      } as UnifiedNotification,
      layout
    );
    await router.settled();

    if (location === 'split')
      expect(createBlockInstance).toHaveBeenCalledOnce();
    else expect(createBlockInstance).not.toHaveBeenCalled();
    if (location === 'split') expect(layout.activeSplitId()).toBe(first.id);
    expect(toast.alert).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
    expect(
      searchFor(router, layout.activeSplitId()!, 'channels')
    ).toMatchObject({
      messageId: ['message'],
      seek: [expect.any(String)],
    });
  }
);
