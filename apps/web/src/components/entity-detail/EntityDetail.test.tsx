import type { PreviewRouteOwner } from '@components/app/preview-panel-context';
import { cleanup, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';
import { EntityDetail } from './EntityDetail';

vi.mock('@app/features/block-image/ImageBlock', () => ({
  ImageBlock: () => null,
}));
vi.mock('@app/features/drive-view/views/CanvasDetail', () => ({
  CanvasDetail: () => null,
}));
vi.mock('@app/features/drive-view/views/CodeDetail', () => ({
  CodeDetail: () => null,
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
vi.mock('@channel/Channel/ChannelDetail', () => ({
  ChannelDetail: () => null,
  ChannelDetailTopBar: () => null,
}));
vi.mock('./FileEntityDetail', () => ({ FileEntityDetail: () => null }));
vi.mock('@app/features/next-soup/utils', () => ({
  getChannelEntityTarget: () => undefined,
  getDocumentCommentTarget: () => undefined,
  calendarViewTargetForEntity: () => undefined,
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => ({}),
}));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanelOrThrow: () => ({}),
}));
vi.mock('@components/app/PreviewPanel', () => ({
  PreviewPanel: (props: {
    routeOwner?: PreviewRouteOwner;
    navigationRequest?: number | string;
  }) => (
    <output data-testid="preview-capabilities">
      {JSON.stringify({
        owner: props.routeOwner ?? null,
        request: props.navigationRequest ?? null,
      })}
    </output>
  ),
}));

afterEach(cleanup);

const target = {
  type: 'document' as const,
  id: 'sheet-1',
  fileType: 'spreadsheet',
};
const capabilities = () =>
  JSON.parse(screen.getByTestId('preview-capabilities').textContent!);

it('does not grant route ownership to an ordinary local fallback', () => {
  render(() => <EntityDetail target={target} />);
  expect(capabilities()).toEqual({ owner: null, request: null });
});

it('forwards and clears explicit route ownership and repeat requests reactively', () => {
  const owner: PreviewRouteOwner = {
    blockType: 'spreadsheet',
    blockId: 'sheet-1',
  };
  const [routeOwner, setRouteOwner] = createSignal<
    PreviewRouteOwner | undefined
  >(owner);
  const [request, setRequest] = createSignal<number | string | undefined>(
    'first'
  );
  render(() => (
    <EntityDetail
      target={target}
      routeOwner={routeOwner()}
      navigationRequest={request()}
    />
  ));
  expect(capabilities()).toEqual({ owner, request: 'first' });
  setRequest('repeat');
  expect(capabilities()).toEqual({ owner, request: 'repeat' });
  setRouteOwner(undefined);
  setRequest(undefined);
  expect(capabilities()).toEqual({ owner: null, request: null });
});
