import type { ChannelEntity, ChannelThreadEntity } from '@entity';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { createSignal, For, type JSX, type ParentProps, Show } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ChannelThreadsView } from './ChannelThreadsView';

const activity = vi.hoisted(() => ({ markDone: vi.fn() }));
vi.mock('../queries/channel-thread-activity', () => ({
  useChannelThreadActivity: () => ({
    kind: () => 'activity',
    isDone: () => false,
    isPending: () => false,
    markDone: activity.markDone,
  }),
}));
vi.mock('@app/components/view-shell', () => ({
  ViewShell: {
    TopBar: (props: ParentProps) => <header>{props.children}</header>,
  },
}));
vi.mock('@channel/DebugSuspense', () => ({
  DebugSuspense: (props: ParentProps) => props.children,
}));
vi.mock('@core/messages/MessageThread', () => ({
  threadListItem: () => ({}),
  MessageThread: (props: {
    canWrite: boolean;
    keepReplyInputOpen?: boolean;
    collapsedReplyPreview?: string;
  }) => (
    <div data-preview={props.collapsedReplyPreview}>
      <Show when={props.canWrite && props.keepReplyInputOpen}>
        <textarea aria-label="Reply" />
      </Show>
    </div>
  ),
}));
vi.mock('@queries/messages/thread-replies', () => ({
  useMessageThreadQuery: () => ({ data: { state: { deleted_at: null } } }),
}));
vi.mock('@ui', () => ({
  Button: (
    props: ParentProps<{
      label?: string;
      disabled?: boolean;
      onClick?: () => void;
    }>
  ) => (
    <button
      aria-label={props.label}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  ),
  Scroll: (props: ParentProps) => <div>{props.children}</div>,
  cn: (...classes: unknown[]) => classes.filter(Boolean).join(' '),
}));
vi.mock('virtua/solid', () => ({
  Virtualizer: (props: {
    data: ChannelThreadEntity[];
    children: (thread: ChannelThreadEntity, index: () => number) => JSX.Element;
  }) => <For each={props.data}>{props.children}</For>,
}));
vi.mock('../queries/channel-threads', () => ({
  useChannelThreadsQuery: () => ({
    query: {},
    threads: () => [{ id: 'root', channelId: 'channel' }],
    isPending: () => false,
  }),
}));
vi.mock('./rail/ChannelRailItems', () => ({ ChannelAvatar: () => null }));

afterEach(cleanup);

const channel: ChannelEntity = {
  type: 'channel',
  id: 'channel',
  ownerId: 'user',
  name: 'Conversation',
  channelType: 'team',
  isParticipant: true,
};

describe('thread card presentation and reply permissions', () => {
  it('waits for channel metadata before showing the default reply input', () => {
    const [metadata, setMetadata] = createSignal<ChannelEntity>();
    const view = render(() => (
      <ChannelThreadsView
        channelId={undefined}
        resolveChannel={metadata}
        onOpenThread={() => {}}
      />
    ));
    expect(
      view.container.querySelector('[data-preview="latest-two"]')
    ).toBeTruthy();
    expect(view.queryByRole('textbox', { name: 'Reply' })).toBeNull();
    setMetadata(channel);
    expect(view.getByRole('textbox', { name: 'Reply' })).toBeTruthy();
    setMetadata({ ...channel, isParticipant: false });
    expect(view.queryByRole('textbox', { name: 'Reply' })).toBeNull();
  });

  it('treats known legacy channel rows without a participant flag as joined', () => {
    const view = render(() => (
      <ChannelThreadsView
        channelId={undefined}
        resolveChannel={() => ({ ...channel, isParticipant: undefined })}
        onOpenThread={() => {}}
      />
    ));
    expect(view.getByRole('textbox', { name: 'Reply' })).toBeTruthy();
  });
  it('marks notifications done without removing the thread or its reply input', () => {
    const view = render(() => (
      <ChannelThreadsView
        channelId={undefined}
        resolveChannel={() => channel}
        onOpenThread={() => {}}
      />
    ));
    fireEvent.click(view.getByRole('button', { name: 'Mark thread done' }));
    expect(activity.markDone).toHaveBeenCalledOnce();
    expect(
      view.container.querySelector('[data-channel-thread="root"]')
    ).toBeTruthy();
    expect(view.getByRole('textbox', { name: 'Reply' })).toBeTruthy();
  });
});
