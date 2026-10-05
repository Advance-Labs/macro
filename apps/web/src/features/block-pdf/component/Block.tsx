import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import {
  previewOwnsRoute,
  useMaybePreviewPanel,
} from '@components/app/preview-panel-context';
import { SidePanel } from '@components/app/side-panel';
import { blockDataSignalAs, useBlockId, useIsNestedBlock } from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import type { ShareHostProps } from '@core/component/TopBar/ShareButton';
import { HotkeyScope, useHotkeyScopeOrCreate } from '@core/hotkey/HotkeyScope';
import { createMethodRegistration } from '@core/orchestrator';
import { blockElementSignal } from '@core/signal/blockElement';
import { blockHandleSignal, blockMetadataSignal } from '@core/signal/load';
import {
  useCanComment,
  useCanEdit,
  useIsDocumentOwner,
} from '@core/signal/permissions';
import { useBlockDocumentName } from '@core/util/currentBlockDocumentName';
import { useSearchParams } from '@solidjs/router';
import { createMemo, Show } from 'solid-js';
import { usePdfDocument } from '../context/pdf-document-context';
import type { PdfBlockData } from '../definition';
import { createPdfRouteTarget } from '../primitives/create-pdf-route-target';
import {
  type LocationBlockParams,
  type LocationSearchParams,
  URL_PARAMS,
} from '../signal/location';
import {
  PdfDocument,
  PdfDocumentContent,
  type PdfDocumentMethods,
} from './PdfDocument';
import { PdfSidePanelSections } from './sidepanel/PdfSidePanelSections';
import { Tabs } from './Tabs';
import { TopBar } from './TopBar';

export default function BlockPdf() {
  const documentId = useBlockId();
  const [attachHotkeyScope, hotkeyScope] = useHotkeyScopeOrCreate('pdf');
  const portalMount = blockElementSignal.get;
  useBlockEntityCommands({
    id: () => documentId,
    scopeId: () => hotkeyScope,
  });
  const isNested = useIsNestedBlock();
  const preview = useMaybePreviewPanel();
  const routeTarget = isNested
    ? () => undefined
    : createPdfRouteTarget(() => documentId);
  const target = createMemo(() => {
    if (!preview) return routeTarget();
    if (previewOwnsRoute(preview, 'pdf', documentId)) {
      const route = routeTarget();
      if (route) return route;
    }
    const current = preview.previewTarget();
    preview.navigationRequest();
    return current.blockId === documentId && current.params
      ? { ...(current.params as LocationBlockParams) }
      : undefined;
  });

  const metadata = blockMetadataSignal.get;
  const documentName = useBlockDocumentName('Unknown Filename');
  const canComment = useCanComment();
  const canEdit = useCanEdit();
  const isOwner = useIsDocumentOwner();
  const [searchParams] = useSearchParams();
  const blockHandle = blockHandleSignal.get;
  const data = blockDataSignalAs<PdfBlockData>('pdf');

  const registerMethods = (methods: Partial<PdfDocumentMethods>) =>
    createMethodRegistration(blockHandle, methods);

  return (
    <HotkeyScope scope={hotkeyScope}>
      <DocumentBlockContainer attachHotkeyScope={attachHotkeyScope}>
        <PdfDocument
          documentId={documentId}
          documentVersionId={metadata()?.documentVersionId}
          documentName={documentName()}
          documentProxy={data()?.documentProxy}
          viewLocation={data()?.viewLocation}
          modificationData={data()?.documentMetadata.modificationData}
          isNested={isNested}
          portalMount={portalMount()}
          permissions={{
            canComment: canComment(),
            canEdit: canEdit(),
            isOwner: isOwner(),
          }}
          locationParams={preview ? undefined : getLocationParams(searchParams)}
          navigationTarget={target()}
          registerMethods={registerMethods}
        >
          <PdfBlockContent permissionOptions={{ edit: false }} />
        </PdfDocument>
      </DocumentBlockContainer>
    </HotkeyScope>
  );
}

function PdfBlockContent(props: ShareHostProps) {
  const pdf = usePdfDocument();

  return (
    <Show when={!pdf.isNested()} fallback={<PdfDocumentContent />}>
      <SidePanel.Layout floating>
        <PdfSidePanelSections />
        <div class="flex size-full min-w-0 flex-col overflow-hidden">
          <TopBar
            sharePermissions={props.sharePermissions}
            permissionOptions={props.permissionOptions}
          />
          <Show when={pdf.tabs.isVisible()}>
            <div class="flex px-2 justify-between min-h-11 items-center gap-2">
              <div class="overflow-x-auto overflow-y-hidden grow customScrollbar w-0">
                <Tabs />
              </div>
            </div>
          </Show>
          <PdfDocumentContent />
        </div>
      </SidePanel.Layout>
    </Show>
  );
}

function getLocationParams(
  params: Partial<Record<string, string | string[] | undefined>>
): LocationSearchParams {
  const value = (key: string) => {
    const param = params[key];
    return Array.isArray(param) ? param[0] : param;
  };
  return {
    annotationId: value(URL_PARAMS.annotationId),
    searchPage: value(URL_PARAMS.searchPage),
    searchSnippet: value(URL_PARAMS.searchSnippet),
    searchRawQuery: value(URL_PARAMS.searchRawQuery),
    highlightTerms: value(URL_PARAMS.searchHighlightTerms),
    pageNumber: value(URL_PARAMS.pageNumber),
    yPos: value(URL_PARAMS.yPos),
    x: value(URL_PARAMS.x),
    width: value(URL_PARAMS.width),
    height: value(URL_PARAMS.height),
  };
}
