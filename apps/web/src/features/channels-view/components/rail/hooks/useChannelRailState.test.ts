import { createRoot, createSignal } from 'solid-js';
import { expect, it, vi } from 'vitest';
import { useChannelRailItemState } from './useChannelRailState';

const mocks = vi.hoisted(() => ({ rail: vi.fn() }));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalNotificationSource: () => ({ mutedEntities: () => [] }),
}));
vi.mock('../ChannelsRailContext', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../ChannelsRailContext')>()),
  useChannelsRail: mocks.rail,
}));

it('uses only unread threads for channel rows in Threads, not general channel activity', () => {
  createRoot((dispose) => {
    const [tab, setTab] = createSignal('threads');
    const [enabled, setEnabled] = createSignal(true);
    const [channelId, setChannelId] = createSignal('one');
    mocks.rail.mockReturnValue({
      railId: 'rail',
      tab,
      threadsEnabled: enabled,
      selectedChannel: () => undefined,
      threadsChannelId: () => undefined,
      list: { focus: { key: () => undefined } },
      channelActivity: {
        unreadChannelIds: () => new Set(['one']),
        notificationKind: () => 'important',
        callStatuses: () => new Map(),
        incomingCallIds: () => new Map(),
      },
      threadsActivity: {
        forChannel: (id: string) => (id === 'two' ? 'activity' : 'none'),
      },
    });
    const state = useChannelRailItemState(channelId);
    expect(state().unread).toBe(true);
    expect(state().notificationKind).toBe('none');
    setChannelId('two');
    expect(state().notificationKind).toBe('activity');
    setTab('browse');
    expect(state().notificationKind).toBe('important');
    setEnabled(false);
    expect(state().notificationKind).toBeUndefined();
    dispose();
  });
});
