import type {
  OpenSplitResult,
  OpenWithSplitOptions,
  SplitContent,
} from '@components/app/split-layout/layoutManager';
import { beforeEach, expect, it, vi } from 'vitest';
import { useSendMessageToPeople } from './channels';

const mocks = vi.hoisted(() => ({
  send: vi.fn(),
  direct: vi.fn(),
  group: vi.fn(),
  open: vi.fn<
    (content: SplitContent, options?: OpenWithSplitOptions) => OpenSplitResult
  >(),
}));
vi.mock('@components/app/split-layout/layout', () => ({
  useSplitLayout: () => ({ openWithSplit: mocks.open }),
}));
vi.mock('@core/component/Toast/Toast', () => ({ toast: { failure: vi.fn() } }));
vi.mock('@core/context/user', () => ({ useUserId: () => () => 'owner' }));
vi.mock('@core/user/contactService', () => ({ invalidateContacts: vi.fn() }));
vi.mock('@queries/channel/channels', () => ({
  invalidateListChannels: vi.fn(),
}));
vi.mock('@queries/channel/get-or-create-dm', () => ({
  useGetOrCreateDirectMessageMutation: () => ({ mutateAsync: mocks.direct }),
  useGetOrCreatePrivateChannelMutation: () => ({ mutateAsync: mocks.group }),
}));
vi.mock('@queries/messages/mutations', () => ({
  newMessageId: () => '019f694c-d7c0-7000-8000-000000000001',
  useSendMessageMutation: () => ({ mutateAsync: mocks.send }),
}));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.direct.mockResolvedValue({ channel_id: 'resolved-dm' });
  mocks.group.mockResolvedValue({ channel_id: 'resolved-group' });
  mocks.send.mockResolvedValue({ id: 'message' });
  mocks.open.mockReturnValue({ status: 'navigating' });
});

it.each([['recipient'], ['recipient', 'other']])(
  'authorizes the resolved destination before sending to %j',
  async (...users) => {
    const { sendToUsers } = useSendMessageToPeople();
    const order: string[] = [];
    const grant = vi.fn(async () => {
      order.push('grant');
    });
    mocks.send.mockImplementation(async () => {
      order.push('message');
      return { id: 'message' };
    });
    await sendToUsers({
      users,
      content: '',
      mentions: [],
      attachments: [{ entity_type: 'initiative', entity_id: 'project' }],
      beforeSend: grant,
    });
    expect(order).toEqual(['grant', 'message']);
    expect(grant).toHaveBeenCalledWith(
      users.length === 1 ? 'resolved-dm' : 'resolved-group'
    );
    expect(mocks.send.mock.calls[0][0].message.attachments).toEqual([
      { entity_type: 'initiative', entity_id: 'project' },
    ]);
  }
);

it('does not post an attachment when its destination grant fails', async () => {
  const { sendToChannel } = useSendMessageToPeople();
  await expect(
    sendToChannel({
      channelId: 'channel',
      content: '',
      mentions: [],
      attachments: [{ entity_type: 'initiative', entity_id: 'project' }],
      beforeSend: async () => {
        throw new Error('Only owner');
      },
    })
  ).rejects.toThrow('Only owner');
  expect(mocks.send).not.toHaveBeenCalled();
});

it('routes to the returned message only after authorization and sending finish', async () => {
  const { sendToUsers } = useSendMessageToPeople();
  const order: string[] = [];
  mocks.send.mockImplementation(async () => {
    order.push('send');
    return { id: 'returned-message' };
  });
  mocks.open.mockImplementation(() => {
    order.push('open');
    return { status: 'navigating' };
  });
  await sendToUsers({
    users: ['recipient'],
    content: '',
    mentions: [],
    beforeSend: async () => {
      order.push('grant');
    },
    navigate: { navigate: true, mergeHistory: true },
  });
  expect(order).toEqual(['grant', 'send', 'open']);
  expect(mocks.open).toHaveBeenCalledWith(
    { type: 'channel', id: 'resolved-dm' },
    expect.objectContaining({ activate: true, mergeHistory: true })
  );
  const update = mocks.open.mock.calls[0][1]?.search?.channels;
  expect(typeof update).toBe('function');
  const target =
    typeof update === 'function'
      ? update({
          tab: ['threads'],
          messageId: ['old'],
          threadId: ['old-root'],
          latest: ['true'],
        })
      : update;
  expect(target).toEqual({
    tab: ['threads'],
    messageId: ['returned-message'],
    seek: [expect.any(String)],
  });
});

it('keeps deferred navigation explicit and gives repeated callbacks fresh requests', async () => {
  const { sendToChannel } = useSendMessageToPeople();
  const result = await sendToChannel({
    channelId: 'channel',
    content: '',
    mentions: [],
  });
  expect(mocks.open).not.toHaveBeenCalled();
  await result?.navigateToChannel();
  await result?.navigateToChannel();
  expect(mocks.open).toHaveBeenCalledTimes(2);
  const targets = mocks.open.mock.calls.map(([, options]) => {
    const update = options?.search?.channels;
    return typeof update === 'function' ? update(undefined) : update;
  });
  expect(targets[0]).toMatchObject({
    messageId: ['message'],
    seek: [expect.any(String)],
  });
  expect(targets[1]).toMatchObject({
    messageId: ['message'],
    seek: [expect.any(String)],
  });
  expect(targets[1]?.seek).not.toEqual(targets[0]?.seek);
});
