import { describe, expect, it } from 'vitest';
import { channelNotificationKind } from '../channel-notification-kind';

const activity = {
  id: 'message',
  state: 'unseen' as const,
  eventType: 'channel_message_send',
};
const mention = {
  id: 'important',
  state: 'unseen' as const,
  eventType: 'channel_mention',
};

describe('channel notification indicators', () => {
  it('distinguishes ordinary activity from a mention', () => {
    expect(channelNotificationKind([])).toBe('none');
    expect(channelNotificationKind([activity])).toBe('activity');
    expect(channelNotificationKind([activity, mention])).toBe('important');
    expect(channelNotificationKind([mention, activity])).toBe('important');
  });

  it('ignores read, done, and deleted notifications', () => {
    expect(
      channelNotificationKind([
        { ...mention, state: 'seen' },
        { ...activity, state: 'done' },
        { ...mention, deletedAt: '2026-10-07T00:00:00Z' },
      ])
    ).toBe('none');
  });

  it('uses local read and done overrides without suppressing other activity', () => {
    expect(
      channelNotificationKind([activity, mention], (notification) =>
        notification.id === 'important' ? 'done' : notification.state
      )
    ).toBe('activity');
    expect(channelNotificationKind([activity, mention], () => 'seen')).toBe(
      'none'
    );
  });
  it('treats thread replies as important even when ordinary messages arrive later', () => {
    const reply = {
      ...activity,
      id: 'reply',
      eventType: 'channel_message_reply',
    };
    expect(channelNotificationKind([activity, reply])).toBe('important');
    expect(
      channelNotificationKind([
        { ...activity, eventType: 'channel_message_reaction' },
      ])
    ).toBe('activity');
  });
});
