import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import {
  previewOwnsRoute,
  useMaybePreviewPanel,
} from '@components/app/preview-panel-context';
import { FileSidePanelSections, SidePanel } from '@components/app/side-panel';
import { useSplitPanel } from '@components/app/split-layout/layoutUtils';
import {
  useBlockId,
  useBlockNestedContext,
  useIsNestedBlock,
} from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import { HotkeyScope, useHotkeyScopeOrCreate } from '@core/hotkey/HotkeyScope';
import { createMethodRegistration } from '@core/orchestrator';
import { blockElementSignal } from '@core/signal/blockElement';
import { blockFileSignal, blockHandleSignal } from '@core/signal/load';
import { useCanEdit } from '@core/signal/permissions';
import { useSearchParams } from '@solidjs/router';
import { createMemo, Show } from 'solid-js';
import type { CanvasView } from '../context/canvas-document-context';
import { createCanvasRouteTarget } from '../primitives/create-canvas-route-target';
import { CanvasDocument, type CanvasDocumentMethods } from './CanvasDocument';
import { TopBar } from './TopBar';

export type BlockCanvasProps = {
  view?: CanvasView;
};

export default function BlockCanvas(props: BlockCanvasProps) {
  const documentId = useBlockId();
  const [attachHotkeyScope, hotkeyScope] = useHotkeyScopeOrCreate('canvas');
  const portalMount = blockElementSignal.get;
  const isNested = useIsNestedBlock();
  const nestedContext = useBlockNestedContext<'canvas'>();
  const canEdit = useCanEdit();
  useBlockEntityCommands({
    id: () => documentId,
    scopeId: () => hotkeyScope,
  });
  const file = blockFileSignal.get;
  const blockHandle = blockHandleSignal.get;
  const [locationParams] = useSearchParams();
  const panel = useSplitPanel();
  const preview = useMaybePreviewPanel();
  const routeTarget = createCanvasRouteTarget(
    () => documentId,
    !isNested && !!panel && !panel.handle.isPopover()
  );
  const target = createMemo(() => {
    if (!preview) return routeTarget();
    if (previewOwnsRoute(preview, 'canvas', documentId)) {
      const route = routeTarget();
      if (route) return route;
    }
    const current = preview.previewTarget();
    preview.navigationRequest();
    return current.blockId === documentId && current.params
      ? { ...(current.params as Record<string, string>) }
      : undefined;
  });

  const registerMethods = (methods: Partial<CanvasDocumentMethods>) => {
    createMethodRegistration(blockHandle, methods);
  };

  return (
    <HotkeyScope scope={hotkeyScope}>
      <DocumentBlockContainer attachHotkeyScope={attachHotkeyScope}>
        <CanvasDocument
          documentId={documentId}
          file={file()}
          canEdit={canEdit()}
          isNested={isNested}
          portalMount={portalMount()}
          view={props.view}
          locationParams={preview ? {} : locationParams}
          navigationTarget={target()}
          onLocationChange={
            nestedContext?.parentContext?.canvas?.onLocationChange
          }
          registerMethods={registerMethods}
        >
          {(content) => (
            <div
              class="size-full select-none flex flex-col"
              on:click={(event) => {
                if (isNested) event.stopPropagation();
              }}
            >
              <Show when={!isNested} fallback={content}>
                <SidePanel.Layout defaultOpen={false} floating>
                  <FileSidePanelSections />
                  <div class="flex size-full min-w-0 flex-col overflow-hidden">
                    <TopBar permissionOptions={{ edit: false }} />
                    {content}
                  </div>
                </SidePanel.Layout>
              </Show>
            </div>
          )}
        </CanvasDocument>
      </DocumentBlockContainer>
    </HotkeyScope>
  );
}
