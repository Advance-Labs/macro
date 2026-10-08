import type { InputSnapshot } from '@channel/Input';
import type { ChannelInputProps } from '@channel/Input/ChannelInput';
import { cleanup, render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ThreadReplyChannelInput } from '../ThreadReplyChannelInput';

const mocks = vi.hoisted(() => ({
  input: undefined as ChannelInputProps | undefined,
  send: vi.fn(),
}));
vi.mock('@core/context/user', () => ({ useUserId: () => () => 'user' }));
vi.mock('@queries/messages/mutations', () => ({
  newMessageId: () => 'optimistic-reply',
  useSendMessageMutation: () => ({ mutate: mocks.send }),
}));
vi.mock('@queries/messages/typing', () => ({
  usePostTypingUpdateMutation: () => ({ mutate: vi.fn() }),
}));
vi.mock('../../use-channel-bot-mention-users', () => ({
  useMessageBotMentionUsers: () => () => [],
}));
vi.mock('../../Input', () => ({
  createInputAttachmentTracker: () => ({}),
  ChannelInput: (props: ChannelInputProps) => {
    mocks.input = props;
    return <div data-testid="composer" />;
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const snapshot: InputSnapshot = {
  value: 'Reply',
  mentions: [],
  attachments: [],
};

function fixture(keepOpen = false) {
  const [draft, setDraft] = createSignal<InputSnapshot | undefined>(snapshot);
  const onExit = vi.fn();
  render(() => (
    <ThreadReplyChannelInput
      parent={{ type: 'channel', id: 'channel' }}
      threadId="root"
      replyInputState={draft}
      setReplyInputState={setDraft}
      keepOpen={keepOpen}
      onExit={onExit}
    />
  ));
  return { input: mocks.input!, draft, setDraft, onExit };
}

describe('persistent thread reply composer', () => {
  it('does not autofocus and discards a draft without exiting reply mode', () => {
    const { input, draft, onExit } = fixture(true);
    expect(input.autofocus).toBe(false);
    input.onClose?.(snapshot);
    expect(draft()).toBeUndefined();
    expect(onExit).not.toHaveBeenCalled();
  });

  it('clears the draft after sending without exiting reply mode', () => {
    const { input, draft, onExit } = fixture(true);
    input.onSend?.(snapshot);
    expect(mocks.send).toHaveBeenCalledOnce();
    expect(draft()).toBeUndefined();
    expect(onExit).not.toHaveBeenCalled();
  });

  it('preserves the on-demand composer exit behavior', () => {
    const { input, draft, onExit } = fixture();
    expect(input.onClose).toBeTypeOf('function');
    input.onSend?.(snapshot);
    expect(draft()).toBeUndefined();
    expect(onExit).toHaveBeenCalledOnce();
  });

  it('restores a failed send without overwriting a newer draft', () => {
    const { input, draft, setDraft } = fixture(true);
    input.onSend?.(snapshot);
    const failure = mocks.send.mock.calls[0][1].onError;
    failure();
    expect(draft()).toEqual(snapshot);
    const newerDraft = { ...snapshot, value: 'New draft' };
    setDraft(newerDraft);
    failure();
    expect(draft()).toEqual(newerDraft);
  });
});
