import {
  useParamNavigationCount,
  useUrlParams,
} from '@core/component/ParamsProvider';
import { Permissions } from '@core/component/SharePermissions';
import type { GetChatResponse } from '@service-cognition/generated/schemas/getChatResponse';
import { AccessLevel } from '@service-storage/generated/schemas/accessLevel';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { type Ok, ok } from 'neverthrow';
import {
  type Accessor,
  createEffect,
  createSignal,
  type JSX,
  on,
  onCleanup,
  onMount,
  Show,
} from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatBlock } from './ChatBlock';
import type { ChatProps } from './component/Chat';

const mocks = vi.hoisted(() => ({
  fetch: vi.fn(),
  authenticated: (): boolean => true,
  panel: undefined as { splitHotkeyScope: string } | undefined,
  scope: vi.fn(),
  attach: vi.fn(),
  commands: vi.fn(),
  track: vi.fn(),
  pageView: vi.fn(),
  analytics: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  sidePanel: vi.fn(),
  applied: vi.fn(),
  mounted: vi.fn(),
  disposed: vi.fn(),
}));
vi.mock('@queries/cognition/chat-data', () => ({
  fetchAndCacheChat: mocks.fetch,
}));
vi.mock('@core/auth', () => ({
  useIsAuthenticated: () => () => mocks.authenticated(),
}));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanel: () => mocks.panel,
}));
vi.mock('@core/hotkey/hotkeys', () => ({
  useHotkeyDOMScope: () => {
    mocks.scope();
    return [mocks.attach, 'local-chat'];
  },
}));
vi.mock('@app/features/next-soup/actions', () => ({
  useBlockEntityCommands: mocks.commands,
}));
vi.mock('@app/lib/analytics/analytics-context', () => ({
  useAnalytics: () => ({ pageView: mocks.pageView, track: mocks.analytics }),
}));
vi.mock('@core/internal/trackBlockOpened', () => ({ track: mocks.track }));
vi.mock('@queries/client', () => ({ useQueryClient: () => ({}) }));
vi.mock('@queries/preview', () => ({ useItemRawName: () => () => undefined }));
vi.mock('@service-connection/client', () => ({
  useEntitySubscription: (entity: Accessor<{ entity_id: string }>) => {
    const id = entity().entity_id;
    mocks.subscribe(id);
    onCleanup(() => mocks.unsubscribe(id));
  },
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalNotificationSource: () => ({}),
}));
vi.mock('@notifications', () => ({
  DebouncedNotificationReadMarker: () => null,
}));
vi.mock('@components/app/side-panel', () => ({
  SidePanel: { Layout: (props: { children: JSX.Element }) => props.children },
}));
vi.mock('./component/sidepanel/ChatSidePanelSections', () => ({
  ChatSidePanelSections: (props: {
    chatId: string;
    data: GetChatResponse['chat'];
  }) => {
    mocks.sidePanel(props.chatId, props.data);
    return null;
  },
}));
vi.mock('@core/component/EntityLoadGate', () => ({
  toEntityLoadError: (error: unknown) => (error ? 'LOAD_FAILED' : undefined),
  EntityLoadGate: (props: {
    result: { data: Accessor<unknown>; error: Accessor<unknown> };
    children: JSX.Element;
    onRetry: () => void;
  }) => (
    <Show
      when={props.result.error()}
      fallback={
        <Show
          when={props.result.data()}
          fallback={<div data-testid="loading" />}
        >
          {props.children}
        </Show>
      }
    >
      <button onClick={props.onRetry}>Retry</button>
    </Show>
  ),
}));
vi.mock('./component/Chat', () => ({
  Chat: (props: ChatProps) => {
    const params = useUrlParams({ message: 'message_id' });
    const request = useParamNavigationCount('message_id');
    const id = props.chatId;
    onMount(() => mocks.mounted(id));
    onCleanup(() => mocks.disposed(id));
    createEffect(on(props.pendingLocation, (target) => mocks.applied(target)));
    return (
      <div
        data-testid="chat"
        data-id={props.chatId}
        data-edit={props.canEdit()}
        data-permissions={props.permissions()}
        data-scope={props.scopeId}
        data-message={params.message()}
        data-request={request()}
      >
        {props.name()}
      </div>
    );
  },
}));

function chatData(
  id: string,
  userAccessLevel: AccessLevel = AccessLevel.owner
): GetChatResponse {
  return {
    chat: { id, name: `Chat ${id}`, userId: 'owner', messages: [] },
    userAccessLevel,
  };
}

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

afterEach(cleanup);
beforeEach(() => {
  vi.resetAllMocks();
  mocks.authenticated = () => true;
  mocks.panel = undefined;
  mocks.fetch.mockImplementation(async (id: string) => ok(chatData(id)));
});

describe('direct ChatBlock lifecycle', () => {
  it('loads once without Block or router context and shares loaded data with the side panel', async () => {
    const data = chatData('chat-1');
    const pending = deferred<Ok<GetChatResponse, unknown>>();
    mocks.fetch.mockReturnValue(pending.promise);
    const view = render(() => <ChatBlock chatId="chat-1" />);
    expect(screen.getByTestId('loading')).toBeTruthy();
    expect(mocks.track).not.toHaveBeenCalled();
    expect(mocks.sidePanel).not.toHaveBeenCalled();
    pending.resolve(ok(data));
    const chat = await screen.findByTestId('chat');
    expect(chat.textContent).toBe('Chat chat-1');
    expect(mocks.fetch).toHaveBeenCalledExactlyOnceWith('chat-1');
    expect(mocks.sidePanel).toHaveBeenCalledExactlyOnceWith(
      'chat-1',
      data.chat
    );
    expect(mocks.track).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ itemId: 'chat-1', blockName: 'chat' })
    );
    expect(mocks.pageView).toHaveBeenCalledExactlyOnceWith('chat');
    expect(mocks.analytics).toHaveBeenCalledWith('open_entity', {
      entityType: 'chat',
      entityId: 'chat-1',
    });
    expect(mocks.subscribe).toHaveBeenCalledExactlyOnceWith('chat-1');
    view.unmount();
    expect(mocks.unsubscribe).toHaveBeenCalledExactlyOnceWith('chat-1');
    expect(mocks.disposed).toHaveBeenCalledExactlyOnceWith('chat-1');
  });

  it('retries a failed load without tracking the failed attempt', async () => {
    mocks.fetch.mockRejectedValueOnce(new Error('temporary failure'));
    render(() => <ChatBlock chatId="chat-1" />);
    const retry = await screen.findByRole('button', { name: 'Retry' });
    expect(mocks.track).not.toHaveBeenCalled();
    fireEvent.click(retry);
    await screen.findByTestId('chat');
    expect(mocks.fetch).toHaveBeenCalledTimes(2);
    expect(mocks.track).toHaveBeenCalledOnce();
  });

  it('applies only the latest cold target and repeats equal requests without reloading or remounting', async () => {
    const pending = deferred<Ok<GetChatResponse, unknown>>();
    mocks.fetch.mockReturnValue(pending.promise);
    const [params, setParams] = createSignal({ message_id: 'first' });
    const [request, setRequest] = createSignal(1);
    render(() => (
      <ChatBlock
        chatId="chat-1"
        params={params()}
        navigationRequest={request()}
      />
    ));
    setParams({ message_id: 'middle' });
    setParams({ message_id: 'latest' });
    setRequest(2);
    pending.resolve(ok(chatData('chat-1')));
    const chat = await screen.findByTestId('chat');
    expect(chat.dataset.message).toBe('latest');
    expect(mocks.applied).toHaveBeenCalledExactlyOnceWith({
      message_id: 'latest',
    });
    const navigationCount = Number(chat.dataset.request);
    setRequest(3);
    await waitFor(() => expect(mocks.applied).toHaveBeenCalledTimes(2));
    expect(Number(chat.dataset.request)).toBeGreaterThan(navigationCount);
    expect(screen.getByTestId('chat')).toBe(chat);
    expect(mocks.fetch).toHaveBeenCalledOnce();
    expect(mocks.mounted).toHaveBeenCalledOnce();
  });

  it('ignores a stale load after changing chat identity and cleans up the active chat', async () => {
    const pending = deferred<Ok<GetChatResponse, unknown>>();
    mocks.fetch.mockImplementation((id: string) =>
      id === 'chat-1' ? pending.promise : Promise.resolve(ok(chatData(id)))
    );
    const [id, setId] = createSignal('chat-1');
    const view = render(() => <ChatBlock chatId={id()} />);
    setId('chat-2');
    await waitFor(() =>
      expect(screen.getByTestId('chat').dataset.id).toBe('chat-2')
    );
    pending.resolve(ok(chatData('chat-1')));
    await Promise.resolve();
    expect(screen.getByTestId('chat').dataset.id).toBe('chat-2');
    expect(mocks.track).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ itemId: 'chat-2' })
    );
    expect(mocks.subscribe).toHaveBeenCalledExactlyOnceWith('chat-2');
    view.unmount();
    expect(mocks.unsubscribe).toHaveBeenCalledExactlyOnceWith('chat-2');
  });

  it('tracks each successfully loaded identity and releases the previous subscription', async () => {
    const [id, setId] = createSignal('chat-1');
    const view = render(() => <ChatBlock chatId={id()} />);
    await screen.findByTestId('chat');
    setId('chat-2');
    await waitFor(() =>
      expect(screen.getByTestId('chat').dataset.id).toBe('chat-2')
    );
    expect(mocks.track.mock.calls.map(([opened]) => opened.itemId)).toEqual([
      'chat-1',
      'chat-2',
    ]);
    expect(mocks.unsubscribe).toHaveBeenCalledExactlyOnceWith('chat-1');
    expect(mocks.fetch).toHaveBeenCalledTimes(2);
    view.unmount();
    expect(mocks.unsubscribe).toHaveBeenLastCalledWith('chat-2');
  });

  it.each([
    { inSplit: false, nested: false, scope: 'local-chat', local: true },
    { inSplit: true, nested: false, scope: 'split-wide', local: false },
    { inSplit: true, nested: true, scope: 'local-chat', local: true },
  ])(
    'owns the correct scope for $inSplit/$nested',
    async ({ inSplit, nested, scope, local }) => {
      mocks.panel = inSplit ? { splitHotkeyScope: 'split-wide' } : undefined;
      render(() => <ChatBlock chatId="chat-1" nested={nested} />);
      const chat = await screen.findByTestId('chat');
      expect(chat.dataset.scope).toBe(scope);
      expect(mocks.scope).toHaveBeenCalledTimes(local ? 1 : 0);
      expect(mocks.attach).toHaveBeenCalledTimes(local ? 1 : 0);
      expect(mocks.commands).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'chat-1', scopeId: scope })
      );
      expect(mocks.track).toHaveBeenCalledTimes(nested ? 0 : 1);
      expect(mocks.pageView).toHaveBeenCalledTimes(nested ? 0 : 1);
    }
  );

  it('keeps editing gated by loaded permissions and reactive authentication', async () => {
    const [authenticated, setAuthenticated] = createSignal(true);
    mocks.authenticated = authenticated;
    render(() => <ChatBlock chatId="chat-1" />);
    const chat = await screen.findByTestId('chat');
    expect(chat.dataset.permissions).toBe(String(Permissions.OWNER));
    expect(chat.dataset.edit).toBe('true');
    setAuthenticated(false);
    expect(chat.dataset.edit).toBe('false');
    expect(mocks.fetch).toHaveBeenCalledOnce();
  });

  it('does not grant editing to a view-only chat', async () => {
    mocks.fetch.mockResolvedValue(ok(chatData('chat-1', AccessLevel.view)));
    render(() => <ChatBlock chatId="chat-1" />);
    const chat = await screen.findByTestId('chat');
    expect(chat.dataset.edit).toBe('false');
    expect(chat.dataset.permissions).toBe(String(Permissions.CAN_VIEW));
  });
});
