import {
  createParamsState,
  ParamsProvider,
  useParamNavigationCount,
  useUrlParams,
} from '@core/component/ParamsProvider';
import { AccessLevel } from '@service-storage/generated/schemas/accessLevel';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import {
  type Accessor,
  createSignal,
  type JSX,
  onCleanup,
  Show,
} from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import SpreadsheetBlock from './SpreadsheetBlock';

const mocks = vi.hoisted(() => ({
  enabled: (): boolean => true,
  authenticated: (): boolean => true,
  panel: undefined as { splitHotkeyScope: string } | undefined,
  load: vi.fn(),
  session: vi.fn(),
  dispose: vi.fn(),
  scope: vi.fn(),
  commands: vi.fn(),
  track: vi.fn(),
  analytics: vi.fn(),
  share: vi.fn(),
}));
vi.mock('./primitives/use-spreadsheet-access', () => ({
  useSpreadsheetAccess: () => () => mocks.enabled(),
}));
vi.mock('./queries/spreadsheet-document', () => ({
  loadSpreadsheetDocument: mocks.load,
}));
vi.mock('./spreadsheet-mentions', () => ({ spreadsheetMentions: {} }));
vi.mock('./queries/spreadsheet-session', () => ({
  createSpreadsheetSession: (options: { documentId: string }) => {
    mocks.session(options.documentId);
    onCleanup(() => mocks.dispose(options.documentId));
    return { peers: () => [] };
  },
}));
vi.mock('./primitives/create-spreadsheet-store', () => ({
  createSpreadsheetStore: (options: { canEdit: Accessor<boolean> }) => ({
    canEdit: options.canEdit,
    ready: () => true,
  }),
}));
vi.mock('./views/SpreadsheetEditor', () => ({
  SpreadsheetEditor: (props: {
    name: string;
    store: { canEdit: Accessor<boolean> };
  }) => (
    <div data-testid="editor" data-edit={props.store.canEdit()}>
      {props.name}
    </div>
  ),
}));
vi.mock('./SpreadsheetComments', () => ({
  SpreadsheetComments: (props: {
    canComment: Accessor<boolean>;
    isOwner: Accessor<boolean>;
    children: (location: () => undefined, comments: object) => JSX.Element;
  }) => {
    const params = useUrlParams({ comment: 'comment_id' });
    const count = useParamNavigationCount('comment_id');
    return (
      <div
        data-testid="comments"
        data-comment={params.comment()}
        data-request={count()}
        data-write={props.canComment()}
        data-owner={props.isOwner()}
      >
        {props.children(() => undefined, {})}
      </div>
    );
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
      fallback={<Show when={props.result.data()}>{props.children}</Show>}
    >
      <button onClick={props.onRetry}>Retry</button>
    </Show>
  ),
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
    return [vi.fn(), 'local-sheet'];
  },
}));
vi.mock('@app/features/next-soup/actions', () => ({
  useBlockEntityCommands: mocks.commands,
}));
vi.mock('@app/lib/analytics/analytics-context', () => ({
  useAnalytics: () => ({ pageView: mocks.analytics, track: vi.fn() }),
}));
vi.mock('@core/internal/trackBlockOpened', () => ({ track: mocks.track }));
vi.mock('@core/orchestrator', () => ({ createMethodRegistration: vi.fn() }));
vi.mock('@core/context/user', () => ({ useUserId: () => () => 'user' }));
vi.mock('@queries/client', () => ({ useQueryClient: () => ({}) }));
vi.mock('@queries/preview', () => ({ useItemRawName: () => () => undefined }));
vi.mock('@service-sync/source', () => ({
  createSyncServiceSource: () => ({ source: {}, doInitialSync: vi.fn() }),
}));
vi.mock('@service-connection/client', () => ({
  useEntitySubscription: vi.fn(),
}));
vi.mock('@core/state/liveIndicators', () => ({
  useUserIndicators: () => () => [],
}));
vi.mock('@core/component/LiveIndicators', () => ({
  LiveIndicators: () => null,
}));
vi.mock('@core/component/TopBar/shareModal', () => ({
  useShareModal: () => mocks.share,
}));
vi.mock('@core/component/TopBar/ShareButton', () => ({
  ShareTrigger: () => null,
  getShareDrawerRecipientInput: () => null,
}));
vi.mock('@app/features/chat/ChatWithAgentButton', () => ({
  ChatWithAgentButton: () => null,
}));
vi.mock('@components/app/ResponsiveBlockToolbar', () => ({
  ResponsiveBlockToolbar: () => null,
}));
vi.mock('@components/app/split-layout/components/SplitHeader', () => ({
  SplitHeaderLeft: (props: { children: JSX.Element }) => props.children,
  SplitHeaderRight: (props: { children: JSX.Element }) => props.children,
}));
vi.mock('@components/app/split-layout/components/SplitLabel', () => ({
  StaticSplitLabel: () => null,
}));
vi.mock('@entity', () => ({
  createRenameDssEntityMutation: () => ({ mutate: vi.fn() }),
}));
vi.mock('@filesystem/download', () => ({ downloadFile: vi.fn() }));
vi.mock('@ui', () => ({ Badge: () => null }));

const documentData = (id: string, access = AccessLevel.owner) => ({
  documentMetadata: { documentId: id, documentName: id, owner: 'user' },
  userAccessLevel: access,
  token: 'token',
  authorization: {},
});
beforeEach(() => {
  vi.clearAllMocks();
  mocks.enabled = () => true;
  mocks.authenticated = () => true;
  mocks.panel = undefined;
  mocks.load.mockImplementation(async (id: string) => documentData(id));
});
afterEach(cleanup);

describe('direct spreadsheet document host', () => {
  it('does not load or create a session when pilot access is disabled', () => {
    mocks.enabled = () => false;
    render(() => <SpreadsheetBlock documentId="disabled" />);
    expect(
      screen.getByText('Spreadsheets are not enabled for this account.')
    ).toBeTruthy();
    expect(mocks.load).not.toHaveBeenCalled();
    expect(mocks.session).not.toHaveBeenCalled();
  });

  it('loads once without Block context and rechecks authenticated edit and comment access', async () => {
    const [authenticated, setAuthenticated] = createSignal(true);
    mocks.authenticated = authenticated;
    const view = render(() => <SpreadsheetBlock documentId="sheet-1" />);
    const editor = await screen.findByTestId('editor');
    expect(mocks.load).toHaveBeenCalledExactlyOnceWith(
      'sheet-1',
      expect.objectContaining({ refetching: false })
    );
    expect(editor.dataset.edit).toBe('true');
    expect(screen.getByTestId('comments').dataset.write).toBe('true');
    setAuthenticated(false);
    expect(editor.dataset.edit).toBe('false');
    expect(screen.getByTestId('comments').dataset.write).toBe('false');
    expect(mocks.load).toHaveBeenCalledOnce();
    view.unmount();
    expect(mocks.dispose).toHaveBeenCalledExactlyOnceWith('sheet-1');
  });

  it.each([false, true])(
    'selects split or local shortcut ownership for nested=%s',
    async (nested) => {
      mocks.panel = { splitHotkeyScope: 'split' };
      render(() => <SpreadsheetBlock documentId="sheet-1" nested={nested} />);
      await screen.findByTestId('editor');
      expect(mocks.commands).toHaveBeenCalledWith(
        expect.objectContaining({ scopeId: nested ? 'local-sheet' : 'split' })
      );
      expect(mocks.scope).toHaveBeenCalledTimes(nested ? 1 : 0);
      expect(mocks.track).toHaveBeenCalledTimes(nested ? 0 : 1);
    }
  );

  it('releases the previous session before loading a different workbook', async () => {
    const [id, setId] = createSignal('first');
    const view = render(() => <SpreadsheetBlock documentId={id()} />);
    await screen.findByText('first');
    setId('second');
    expect(mocks.dispose).toHaveBeenCalledExactlyOnceWith('first');
    await screen.findByText('second');
    expect(mocks.session.mock.calls).toEqual([['first'], ['second']]);
    view.unmount();
    expect(mocks.dispose.mock.calls).toEqual([['first'], ['second']]);
  });

  it('delivers the latest cold-load target, isolates parent params, and repeats equivalent requests', async () => {
    let resolve!: (value: ReturnType<typeof documentData>) => void;
    mocks.load.mockReturnValue(
      new Promise<ReturnType<typeof documentData>>((done) => {
        resolve = done;
      })
    );
    const [params, setParams] = createSignal({ comment_id: 'first' });
    const [request, setRequest] = createSignal(1);
    const parent = createParamsState();
    parent.navigate({ comment_id: 'ancestor' });
    render(() => (
      <ParamsProvider state={parent} urlParams={{ comment_id: 'ancestor' }}>
        <SpreadsheetBlock
          documentId="sheet-1"
          params={params()}
          navigationRequest={request()}
        />
      </ParamsProvider>
    ));
    setParams({ comment_id: 'latest' });
    resolve(documentData('sheet-1'));
    const comments = await screen.findByTestId('comments');
    expect(comments.dataset.comment).toBe('latest');
    const count = Number(comments.dataset.request);
    setRequest(2);
    expect(Number(comments.dataset.request)).toBe(count + 1);
    expect(mocks.load).toHaveBeenCalledOnce();
  });

  it('retries failed loads without creating or tracking a failed session', async () => {
    mocks.load.mockRejectedValueOnce(new Error('offline'));
    render(() => <SpreadsheetBlock documentId="sheet-1" />);
    fireEvent.click(await screen.findByRole('button', { name: 'Retry' }));
    expect(mocks.session).not.toHaveBeenCalled();
    await screen.findByTestId('editor');
    expect(mocks.load).toHaveBeenCalledTimes(2);
    expect(mocks.session).toHaveBeenCalledExactlyOnceWith('sheet-1');
    expect(mocks.track).toHaveBeenCalledOnce();
  });

  it('reopens sharing for a new equivalent share request without reloading', async () => {
    const [request, setRequest] = createSignal(1);
    render(() => (
      <SpreadsheetBlock
        documentId="sheet-1"
        share="true"
        navigationRequest={request()}
      />
    ));
    await waitFor(() => expect(mocks.share).toHaveBeenCalledOnce());
    setRequest(2);
    expect(mocks.share).toHaveBeenCalledTimes(2);
    expect(mocks.load).toHaveBeenCalledOnce();
  });
});
