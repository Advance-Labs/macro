import { cleanup, render, screen } from '@solidjs/testing-library';
import { createSignal, type ParentProps } from 'solid-js';
import { createStore } from 'solid-js/store';
import { afterEach, expect, it, vi } from 'vitest';

const state = vi.hoisted(() => ({
  search: {} as { messageId: string; latest: boolean; seek: string },
  request: (): number => 0,
}));
const pass = (props: ParentProps) => props.children;

vi.mock('@service-storage/websocket', () => ({
  storageWS: { reconnectIfDisconnected: vi.fn() },
  createWebSocketJob: vi.fn(),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { addEventListener: vi.fn(), send: vi.fn() },
  state: () => 'closed',
  createConnectionWebsocketEffect: vi.fn(),
}));

vi.mock('@app/lib/split-router', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@app/lib/split-router')>()),
  useParams: () => ({ channelId: 'channel' }),
  createSearchParams: () => [state.search],
}));
vi.mock('@app/components/entity-detail/EntityDetail', () => ({
  entityDetailBlockType: () => 'channel',
  EntityDetail: (props: { navigationRequest?: number | string }) => (
    <output data-testid="request" data-request={props.navigationRequest} />
  ),
}));
vi.mock('@app/components/view-shell', () => ({
  ViewBreadcrumbs: {
    Root: (props: ParentProps) => pass(props),
    Item: () => null,
  },
  ViewShell: { TopBar: (props: ParentProps) => pass(props) },
}));
vi.mock('@components/app/side-panel', () => ({
  SidePanel: { Root: (props: ParentProps) => pass(props) },
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => ({}),
}));
vi.mock('@components/app/PreviewPanel', () => ({ PreviewPanel: () => null }));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanelOrThrow: () => ({}),
  useSplitDisplayName: () => {},
}));
vi.mock('@core/component/TopBar/ShareButton', () => ({
  ShareTrigger: () => null,
}));
vi.mock('@core/component/TopBar/shareModal', () => ({
  useDocumentShareModal: () => () => {},
}));
vi.mock('@channel/Channel/ChannelDetail', () => ({
  ChannelDetailTopBar: () => null,
}));
vi.mock('../home-view-context', () => ({
  useHomeView: () => ({
    previewTarget: () => ({ blockType: 'channel', blockId: 'channel' }),
    previewNavigationRequest: () => state.request(),
    closePreview: () => {},
    openPreview: () => {},
  }),
}));
vi.mock('./HomeReturnBreadcrumb', () => ({ HomeReturnBreadcrumb: () => null }));

import { HomeEntityDetailRouteView } from './HomeEntityDetailRouteView';

afterEach(cleanup);

it('forwards request identity only for an actual channel target or explicit preview reopen', () => {
  const [search, setSearch] = createStore({
    messageId: '',
    latest: false,
    seek: 'orphan',
  });
  const [request, setRequest] = createSignal(0);
  state.search = search;
  state.request = request;
  render(() => <HomeEntityDetailRouteView />);
  const output = screen.getByTestId('request');
  expect(output.dataset.request).toBeUndefined();
  setSearch('messageId', 'message');
  expect(output.dataset.request).toBe('0:orphan');
  setSearch({ messageId: '', latest: true });
  expect(output.dataset.request).toBe('0:orphan');
  setSearch({ latest: false, seek: '' });
  expect(output.dataset.request).toBeUndefined();
  setRequest(1);
  expect(output.dataset.request).toBe('1:');
});
