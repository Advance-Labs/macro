import { cleanup, render, screen } from '@solidjs/testing-library';
import { createSignal, type JSX } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { EntityDetail } from './EntityDetail';
import { entityDetailTarget } from './entity-detail-target';

const mocks = vi.hoisted(() => ({ legacy: vi.fn() }));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalBlockOrchestrator: mocks.legacy,
}));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanelOrThrow: () => ({ splitHotkeyScope: 'detail' }),
}));
vi.mock('@components/app/DirectBlock', () => ({
  DirectBlock: (props: {
    type: string;
    id: string;
    navigationRequest?: string | number;
  }) => (
    <div
      data-testid="direct"
      data-type={props.type}
      data-id={props.id}
      data-request={props.navigationRequest}
    />
  ),
}));
vi.mock('@components/app/PreviewPanel', () => ({
  PreviewPanel: () => {
    throw new Error('Unexpected legacy preview');
  },
  PreviewPanelContext: (props: { children: JSX.Element }) => props.children,
  PreviewFrame: (props: {
    children: JSX.Element;
    headerLeading?: JSX.Element;
    locationKey: () => unknown;
  }) => (
    <div
      data-testid="frame"
      data-location={JSON.stringify(props.locationKey())}
    >
      {props.headerLeading}
      {props.children}
    </div>
  ),
}));
vi.mock('@core/constant/allBlocks', () => ({
  fileTypeToBlockName: (type: string) => type,
  fileTypeToResolvedBlockName: (type: string) => type,
  isBlockAlias: () => false,
  resolveBlockAlias: (type: string) => type,
}));
vi.mock('@app/features/next-soup/utils', () => ({
  getChannelEntityTarget: () => undefined,
  getDocumentCommentTarget: () => undefined,
}));
vi.mock('@channel/Channel/ChannelDetail', () => ({
  ChannelDetail: () => null,
  ChannelDetailTopBar: () => null,
}));
vi.mock('@app/features/drive-view/views/CanvasDetail', () => ({
  CanvasDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/CodeDetail', () => ({
  CodeDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/ImageDetail', () => ({
  ImageDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/MarkdownDetail', () => ({
  MarkdownDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/PdfDetail', () => ({
  PdfDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/UnknownDetail', () => ({
  UnknownDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/VideoDetail', () => ({
  VideoDetail: () => null,
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('direct entity detail hosts', () => {
  it('retains spreadsheet breadcrumbs and preview focus ownership without a legacy instance', () => {
    const [request, setRequest] = createSignal(1);
    render(() => (
      <EntityDetail
        target={entityDetailTarget.document({
          id: 'sheet-1',
          fileType: 'spreadsheet',
        })}
        navigationRequest={request()}
        previewHeaderLeading={<button>Back to Files</button>}
      />
    ));
    const host = screen.getByTestId('direct');
    const frame = screen.getByTestId('frame');
    expect(host.dataset.type).toBe('spreadsheet');
    expect(screen.getByRole('button', { name: 'Back to Files' })).toBeTruthy();
    const location = frame.dataset.location;
    setRequest(2);
    expect(frame.dataset.location).not.toBe(location);
    expect(host.dataset.request).toBe('2');
    expect(screen.getByTestId('direct')).toBe(host);
    expect(mocks.legacy).not.toHaveBeenCalled();
  });

  it('mounts chats directly inside the same preview frame', () => {
    render(() => <EntityDetail target={{ type: 'chat', id: 'chat-1' }} />);
    expect(screen.getByTestId('direct').dataset.type).toBe('chat');
    expect(screen.getByTestId('frame')).toBeTruthy();
    expect(mocks.legacy).not.toHaveBeenCalled();
  });
});
