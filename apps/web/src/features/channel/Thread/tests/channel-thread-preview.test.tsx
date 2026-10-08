import type { Message, MessageListItem } from '@service-storage/messages';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { createSignal, For, type ParentProps } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ChannelThread } from '../ChannelThread';

const mocks = vi.hoisted(() => ({ replies: [] as Message[] }));
vi.mock('@core/context/user', () => ({ useUserId: () => () => 'user' }));
vi.mock('@core/user', () => ({
  tryMacroId: (id: string) => id,
  getDisplayName: () => 'User',
}));
vi.mock('@core/mobile/isTouchDevice', () => ({ isTouchDevice: () => false }));
vi.mock('@queries/gate', () => ({ queryReadyGate: () => true }));
vi.mock('@queries/messages/thread-replies', () => ({
  useThreadRepliesQuery: () => ({
    get data() {
      return mocks.replies;
    },
  }),
}));
vi.mock('@notifications/components/MarkMessageNotifications', () => ({
  MarkMessageNotifications: (props: ParentProps) => props.children,
}));
vi.mock('../../DebugSuspense', () => ({
  DebugSuspense: (props: ParentProps) => props.children,
}));
vi.mock('../../Message', () => ({
  ChannelMessage: (props: { message: Message }) => (
    <p>{props.message.content}</p>
  ),
}));
vi.mock('../../Channel/create-message-selection', () => ({
  createMessageSelection: () => ({
    selectedId: () => undefined,
    clear: vi.fn(),
  }),
}));
vi.mock('../create-thread-hotkeys', () => ({
  createThreadHotkeys: () => ({ attachReplyInputRef: () => {} }),
}));
vi.mock('../ThreadTypingIndicator', () => ({
  ThreadTypingIndicator: () => null,
}));
vi.mock('../Thread', async () => {
  const { ThreadEarlierReplies } = await import('../ThreadEarlierReplies');
  return {
    Thread: {
      Row: (props: ParentProps) => <div>{props.children}</div>,
      RootRail: () => null,
      RepliesBridgeRail: () => null,
      TerminalRail: () => null,
      RepliesContainer: (props: ParentProps) => <div>{props.children}</div>,
      EarlierReplies: ThreadEarlierReplies,
      ReplyList: (props: { replies: Message[] }) => (
        <div data-testid="replies">
          <For each={props.replies}>{(reply) => <p>{reply.content}</p>}</For>
        </div>
      ),
      ReplyInput: () => <textarea aria-label="Reply" />,
      ReplyAuthor: () => null,
      ActionsFooter: (props: ParentProps) => <div>{props.children}</div>,
      CollapsedIndicator: () => <button>Legacy more replies</button>,
      ReplyButton: () => <button>Legacy reply</button>,
    },
  };
});
afterEach(cleanup);

function message(id: string): Message {
  return {
    id,
    parent: { type: 'channel', id: 'channel' },
    sender_id: 'user',
    content: id,
    created_at: '2026-10-01T00:00:00Z',
    updated_at: '2026-10-01T00:00:00Z',
    attachments: [],
    mentions: [],
    reactions: [],
  };
}

function fixture(replyCount = 5) {
  mocks.replies = Array.from({ length: replyCount }, (_, index) =>
    message(`reply-${index}`)
  );
  const root: MessageListItem = {
    ...message('root'),
    state: {
      root_id: 'root',
      user_id: 'user',
      anchor: null,
      resolved: false,
      created_at: '2026-10-01T00:00:00Z',
      updated_at: '2026-10-01T00:00:00Z',
    },
    thread: { reply_count: replyCount, preview: mocks.replies.slice(-3) },
  };
  return render(() => {
    const [expanded, setExpanded] = createSignal(false);
    return (
      <ChannelThread
        data={() => root}
        parent={() => root.parent}
        isExpanded={expanded}
        setIsExpanded={setExpanded}
        collapsedReplyPreview="latest-two"
        keepReplyInputOpen
        isReplying={() => true}
        setIsReplying={() => true}
        replyInputState={() => undefined}
        setReplyInputState={() => undefined}
        replyInputFocusRequest={{
          pending: () => undefined,
          request: () => {},
          consume: () => false,
        }}
        isFindBarOpen={() => false}
      />
    );
  });
}

describe('threads view reply layout', () => {
  it('places the earlier-replies disclosure above the latest two replies and composer', () => {
    const view = fixture();
    const disclosure = view.getByRole('button', {
      name: 'Show 3 earlier replies',
    });
    const replies = view.getByTestId('replies');
    expect(
      disclosure.compareDocumentPosition(replies) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
    expect(view.queryByText('reply-2')).toBeNull();
    expect(view.getByText('reply-3')).toBeTruthy();
    expect(view.getByText('reply-4')).toBeTruthy();
    expect(view.getByRole('textbox', { name: 'Reply' })).toBeTruthy();
    expect(view.queryByText('Legacy more replies')).toBeNull();
    fireEvent.click(disclosure);
    expect(view.getByText('reply-0')).toBeTruthy();
    expect(
      view.queryByRole('button', { name: 'Hide earlier replies' })
    ).toBeNull();
    expect(
      view.queryByRole('button', { name: 'Show 3 earlier replies' })
    ).toBeNull();
    expect(view.getByTestId('replies').textContent).toBe(
      'reply-0reply-1reply-2reply-3reply-4'
    );
    expect(view.getByRole('textbox', { name: 'Reply' })).toBeTruthy();
    expect(view.getByText('reply-4')).toBeTruthy();
  });

  it.each([0, 1, 2])(
    'omits the disclosure when only %i replies exist',
    (count) => {
      const view = fixture(count);
      expect(view.queryByRole('button')).toBeNull();
      expect(view.getByRole('textbox', { name: 'Reply' })).toBeTruthy();
    }
  );
});
