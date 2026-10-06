import type { Entry, PaneId } from '@app/lib/split-router/routes/types';
import { PaneContext } from '@app/lib/split-router/solid/context';
import { cleanup, render, screen, waitFor } from '@solidjs/testing-library';
import { createEffect, createSignal, on, onCleanup } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DirectBlock, type DirectBlockName } from './DirectBlock';

const mocks = vi.hoisted(() => ({
  register: vi.fn(),
  disposed: vi.fn(),
  applied: vi.fn(),
  routed: false,
  claim: 'block:chat:chat-1',
}));
vi.mock('@app/lib/split-router', () => ({
  useOptionalSplitRouter: () => (mocks.routed ? { routes: [] } : undefined),
  claimOf: () => mocks.claim,
}));
vi.mock('./GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => ({ registerBlockHandle: mocks.register }),
}));
vi.mock('@block-chat/ChatBlock', () => ({
  ChatBlock: (props: {
    chatId: string;
    params: Record<string, string>;
    navigationRequest: string;
  }) => {
    const id = props.chatId;
    onCleanup(() => mocks.disposed('chat', id));
    createEffect(
      on(
        () => [props.params, props.navigationRequest] as const,
        ([params]) => mocks.applied(params)
      )
    );
    return (
      <div
        data-testid="chat"
        data-id={props.chatId}
        data-request={props.navigationRequest}
      >
        {JSON.stringify(props.params)}
      </div>
    );
  },
}));
vi.mock('@app/features/block-spreadsheet/SpreadsheetBlock', () => ({
  default: (props: { documentId: string; share?: string }) => {
    const id = props.documentId;
    onCleanup(() => mocks.disposed('spreadsheet', id));
    return (
      <div
        data-testid="spreadsheet"
        data-id={props.documentId}
        data-share={props.share}
      />
    );
  },
}));

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  mocks.routed = false;
  mocks.claim = 'block:chat:chat-1';
  mocks.register.mockImplementation((type: string, id: string) => ({
    type,
    id,
  }));
});

describe('direct block app wiring', () => {
  const routeEntry = (): Entry => ({
    id: 'entry-1',
    props: { message_id: 'metadata' },
    location: {
      route: { matches: [{ id: 'chat-detail', params: { chatId: 'chat-1' } }] },
      search: { chat: { message_id: ['routed'] } },
    },
  });

  it('reads matching owner search and repeats route requests without remounting', async () => {
    mocks.routed = true;
    const [entry, setEntry] = createSignal(routeEntry());
    render(() => (
      <PaneContext.Provider
        value={{ pane: () => 'test-pane' as PaneId, entry, depth: () => 0 }}
      >
        <DirectBlock type="chat" id="chat-1" params={{ message_id: 'local' }} />
      </PaneContext.Provider>
    ));
    const chat = await screen.findByTestId('chat');
    expect(chat.textContent).toBe('{"message_id":"routed"}');
    const request = chat.dataset.request;
    setEntry({ ...routeEntry(), id: 'entry-2' });
    expect(chat.dataset.request).not.toBe(request);
    expect(screen.getByTestId('chat')).toBe(chat);
    expect(mocks.register).toHaveBeenCalledExactlyOnceWith('chat', 'chat-1');
  });

  it('updates params and repeats same-entry requests without an extra layout counter', async () => {
    mocks.routed = true;
    const [entry, setEntry] = createSignal(routeEntry());
    render(() => (
      <PaneContext.Provider
        value={{ pane: () => 'test-pane' as PaneId, entry, depth: () => 0 }}
      >
        <DirectBlock type="chat" id="chat-1" />
      </PaneContext.Provider>
    ));
    const chat = await screen.findByTestId('chat');
    const next = {
      ...routeEntry(),
      props: { message_id: 'props-target' },
      location: { route: routeEntry().location.route },
    };
    setEntry(next);
    expect(chat.textContent).toBe('{"message_id":"props-target"}');
    const calls = mocks.applied.mock.calls.length;
    setEntry({ ...next, props: { message_id: 'props-target' } });
    expect(mocks.applied).toHaveBeenCalledTimes(calls + 1);
    expect(mocks.register).toHaveBeenCalledOnce();
    expect(screen.getByTestId('chat')).toBe(chat);
  });

  it.each([false, true])(
    'does not borrow ancestor route targets for an unmatched or embedded host (%s)',
    async (nested) => {
      mocks.routed = true;
      mocks.claim = nested ? 'block:chat:chat-1' : 'block:chat:other';
      render(() => (
        <PaneContext.Provider
          value={{
            pane: () => 'test-pane' as PaneId,
            entry: routeEntry,
            depth: () => 0,
          }}
        >
          <DirectBlock
            type="chat"
            id="chat-1"
            params={{ message_id: 'local' }}
            nested={nested}
          />
        </PaneContext.Provider>
      ));
      expect((await screen.findByTestId('chat')).textContent).toBe(
        '{"message_id":"local"}'
      );
    }
  );

  it('normalizes explicit location params and repeats requests without recreating the host', async () => {
    const [request, setRequest] = createSignal(1);
    const [params, setParams] = createSignal<object>({
      message_id: ['old', 'latest'],
      ignored: 123,
    });
    render(() => (
      <DirectBlock
        type="chat"
        id="chat-1"
        params={params()}
        navigationRequest={request()}
      />
    ));
    const chat = await screen.findByTestId('chat');
    expect(chat.textContent).toBe('{"message_id":"latest"}');
    const previousRequest = chat.dataset.request;
    setRequest(2);
    expect(chat.dataset.request).not.toBe(previousRequest);
    setParams({ message_id: 'other' });
    expect(chat.textContent).toBe('{"message_id":"other"}');
    expect(screen.getByTestId('chat')).toBe(chat);
    expect(mocks.register).toHaveBeenCalledExactlyOnceWith('chat', 'chat-1');
  });

  it('replaces feature identity and releases the previous mounted host', async () => {
    const [type, setType] = createSignal<DirectBlockName>('chat');
    const [id, setId] = createSignal('first');
    const view = render(() => <DirectBlock type={type()} id={id()} />);
    await screen.findByTestId('chat');
    setId('second');
    await waitFor(() =>
      expect(screen.getByTestId('chat').dataset.id).toBe('second')
    );
    expect(mocks.disposed).toHaveBeenCalledWith('chat', 'first');
    setType('spreadsheet');
    await screen.findByTestId('spreadsheet');
    expect(mocks.disposed).toHaveBeenCalledWith('chat', 'second');
    view.unmount();
    expect(mocks.disposed).toHaveBeenCalledWith('spreadsheet', 'second');
  });

  it('keeps embedded hosts independent from the global handle registry', async () => {
    render(() => (
      <DirectBlock
        type="spreadsheet"
        id="embedded"
        params={{ share: 'true' }}
        nested
      />
    ));
    const spreadsheet = await screen.findByTestId('spreadsheet');
    expect(spreadsheet.dataset.share).toBe('true');
    expect(mocks.register).not.toHaveBeenCalled();
  });

  it('does not render a second host when its handle is already registered', async () => {
    mocks.register.mockReturnValue(undefined);
    render(() => <DirectBlock type="chat" id="duplicate" />);
    expect(await screen.findByText('Content already open.')).toBeTruthy();
    expect(screen.queryByTestId('chat')).toBeNull();
  });
});
