import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import {
  CollaborationStatusIndicator,
  isCollaborationStatusVisible,
} from '@components/app/CollaborationStatusIndicator';
import { useGlobalNotificationSource } from '@components/app/GlobalAppState';
import {
  previewOwnsRoute,
  useMaybePreviewPanel,
} from '@components/app/preview-panel-context';
import { SidePanel } from '@components/app/side-panel';
import { HeaderIsland } from '@components/app/split-layout/components/HeaderIsland';
import { SplitHeaderRight } from '@components/app/split-layout/components/SplitHeader';
import { useCanAutofocusSplitContent } from '@components/app/split-layout/layoutUtils';
import { useNavigatedFromJK } from '@components/app/useNavigatedFromJK';
import { useBlockAliasedName, useBlockId, useIsNestedBlock } from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import type { ShareHostProps } from '@core/component/TopBar/ShareButton';
import {
  ENABLE_MARKDOWN_LIVE_COLLABORATION,
  ENABLE_MARKDOWN_SIDE_PANEL,
} from '@core/constant/featureFlags';
import { HotkeyScope, useHotkeyScopeOrCreate } from '@core/hotkey/HotkeyScope';
import { blockDataSignal as blockLoaderDataSignal } from '@core/internal/BlockLoader';
import { createMethodRegistration } from '@core/orchestrator';
import {
  blockErrorSignal,
  blockHandleSignal,
  blockSourceSignal,
} from '@core/signal/load';
import {
  useCanComment,
  useCanEdit,
  useIsDocumentOwner,
} from '@core/signal/permissions';
import { useBlockDocumentName } from '@core/util/currentBlockDocumentName';
import { DocumentDebouncedNotificationReadMarker } from '@notifications';
import { useInstructionsMdIdQuery } from '@queries/storage/instructions-md';
import { createEffect, on, Show, Suspense } from 'solid-js';
import { createMarkdownDocumentState } from '../context/markdown-document-state';
import type { MarkdownData } from '../definition';
import { markdownDetailSearch } from '../markdown-route';
import { createMarkdownRouteNavigation } from '../primitives/create-markdown-route-navigation';
import { loadMarkdownCachedSnapshot } from '../queries/markdown-document-operations';
import type { MarkdownDocumentKind, MarkdownDocumentSource } from '../types';
import { FindAndReplace } from './FindAndReplace';
import { MarkdownDocument, MarkdownDocumentContent } from './MarkdownDocument';
import { useMarkdownName } from './MarkdownNameProvider';
import { ModalsProvider } from './ModalsProvider';
import { MarkdownSidePanelSections } from './sidepanel/MarkdownSidePanelSections';
import { InstructionsTopBar, TopBar } from './TopBar';
import { useTaskBranchNameHotkey } from './useTaskBranchNameHotkey';

export interface BlockMarkdownProps {
  /**
   * A Loro snapshot to load while waiting for a remote snapshot.
   */
  optimisticSnapshot?: Uint8Array<ArrayBufferLike>;
}

function ManagedTopBar(props: ShareHostProps) {
  const { displayName } = useMarkdownName();
  return (
    <TopBar
      name={displayName}
      sharePermissions={props.sharePermissions}
      permissionOptions={props.permissionOptions}
    />
  );
}

export default function MarkdownBlockAdapter(props: BlockMarkdownProps) {
  const documentId = useBlockId();
  const [attachHotkeyScope, hotkeyScope] = useHotkeyScopeOrCreate('md');
  useBlockEntityCommands({
    id: () => documentId,
    scopeId: () => hotkeyScope,
  });
  const canAutofocus = useCanAutofocusSplitContent();
  const { navigatedFromJK } = useNavigatedFromJK();
  const currentBlockName = useBlockAliasedName();
  const kind: MarkdownDocumentKind =
    currentBlockName === 'task' ||
    currentBlockName === 'snippet' ||
    currentBlockName === 'skill'
      ? currentBlockName
      : 'document';
  useTaskBranchNameHotkey({
    documentId: () => documentId,
    kind: () => kind,
    scopeId: () => hotkeyScope,
  });
  const persistedName = useBlockDocumentName('');
  const fallbackName = useBlockDocumentName();
  const instructionsMdId = useInstructionsMdIdQuery();
  const isInstructions = () =>
    instructionsMdId.isSuccess && documentId === instructionsMdId.data;
  const markdownState = createMarkdownDocumentState();
  const preview = useMaybePreviewPanel();
  const ownsRoute = useOwnsSearchNamespace(markdownDetailSearch.namespace);
  const [routeSearch] = createSearchParams(markdownDetailSearch);
  createEffect(
    on(
      () => [
        JSON.stringify(preview?.previewTarget().params),
        preview?.navigationRequest(),
        ownsRoute(),
        routeSearch.documentId,
        routeSearch.nodeId,
        routeSearch.commentId,
        routeSearch.seek,
      ],
      () => {
        const target = preview?.previewTarget();
        if (
          ownsRoute() &&
          previewOwnsRoute(preview, 'md', documentId) &&
          routeSearch.documentId === documentId &&
          (routeSearch.nodeId || routeSearch.commentId)
        )
          return;
        if (target?.blockId === documentId && target.params)
          markdownState.params.navigate(
            target.params as Record<string, string>
          );
      }
    )
  );
  if (!useIsNestedBlock())
    createMarkdownRouteNavigation(
      () => documentId,
      markdownState.params.navigate
    );
  createMethodRegistration(blockHandleSignal.get, {
    goToLocationFromParams: markdownState.params.navigate,
  });
  const notificationSource = useGlobalNotificationSource();

  const rawData = blockLoaderDataSignal.get;
  const data = () => {
    const value = rawData() as
      | (MarkdownData & { __block?: string })
      | undefined;
    return value?.__block === 'md' ? value : undefined;
  };
  const source = blockSourceSignal.get;
  const documentSource = (): MarkdownDocumentSource => {
    const loaded = data();
    if (!loaded) return { type: 'loading' };

    const loadedSource = source();
    if (loadedSource?.type === 'sync-service' && loaded.syncSource) {
      return { type: 'sync', source: loaded.syncSource };
    }
    if (loadedSource?.type === 'dss' && loaded.dssFile) {
      return { type: 'dss', file: loaded.dssFile };
    }

    return { type: 'loading' };
  };
  const collaborationStatus = () => {
    const source = documentSource();
    return source.type === 'sync' ? source.source.status() : undefined;
  };

  const setLoadError = blockErrorSignal.set;
  const canComment = useCanComment();
  const canEdit = useCanEdit();
  const isOwner = useIsDocumentOwner();

  return (
    <HotkeyScope scope={hotkeyScope}>
      <DocumentBlockContainer attachHotkeyScope={attachHotkeyScope}>
        <MarkdownDocument
          documentId={documentId}
          kind={kind}
          state={markdownState}
          documentSource={documentSource()}
          permissions={{
            canComment: canComment(),
            canEdit: canEdit(),
            isOwner: isOwner(),
          }}
          persistedName={persistedName()}
          fallbackName={fallbackName()}
        >
          <ModalsProvider>
            <SidePanel.Layout floating>
              <Show when={ENABLE_MARKDOWN_SIDE_PANEL && !isInstructions()}>
                <MarkdownSidePanelSections />
              </Show>
              <div class="flex flex-col size-full">
                <div class="relative shrink-0">
                  <SplitHeaderRight>
                    <Show
                      when={isCollaborationStatusVisible(collaborationStatus())}
                    >
                      <HeaderIsland class="-order-1">
                        <CollaborationStatusIndicator
                          status={collaborationStatus()}
                        />
                      </HeaderIsland>
                    </Show>
                  </SplitHeaderRight>
                  <Suspense>
                    <Show
                      when={isInstructions()}
                      fallback={
                        <ManagedTopBar
                          permissionOptions={{
                            edit: ENABLE_MARKDOWN_LIVE_COLLABORATION,
                          }}
                        />
                      }
                    >
                      <InstructionsTopBar />
                    </Show>
                  </Suspense>
                  <Suspense>
                    <Show when={!isInstructions()}>
                      <div class="absolute right-4 top-1.5 z-action-menu flex justify-end">
                        <FindAndReplace />
                      </div>
                    </Show>
                  </Suspense>
                </div>
                <DocumentDebouncedNotificationReadMarker
                  notificationSource={notificationSource}
                  documentId={documentId}
                />
                <MarkdownDocumentContent
                  isInstructions={isInstructions()}
                  autoFocus={canAutofocus && !navigatedFromJK()}
                  doInitialSync={data()?.doInitialSync}
                  optimisticSnapshot={props.optimisticSnapshot}
                  loadCachedSnapshot={() =>
                    loadMarkdownCachedSnapshot(documentId)
                  }
                  onDataReady={() => setLoadError(null)}
                />
              </div>
            </SidePanel.Layout>
          </ModalsProvider>
        </MarkdownDocument>
      </DocumentBlockContainer>
    </HotkeyScope>
  );
}
