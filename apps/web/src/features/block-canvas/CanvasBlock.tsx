import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import { useAnalytics } from '@app/lib/analytics/analytics-context';
import {
  DocumentFileSidePanelSections,
  SidePanel,
} from '@components/app/side-panel';
import { useSplitPanel } from '@components/app/split-layout/layoutUtils';
import type { NestedState } from '@core/block';
import type { BlockMethodsFor } from '@core/blockMethodRegistry';
import {
  EntityLoadGate,
  toEntityLoadError,
} from '@core/component/EntityLoadGate';
import {
  getPermissions,
  hasPermissions,
  Permissions,
} from '@core/component/SharePermissions';
import { useHotkeyDOMScope } from '@core/hotkey/hotkeys';
import { track } from '@core/internal/trackBlockOpened';
import {
  createMethodRegistration,
  type OwnedBlockHandle,
} from '@core/orchestrator';
import { useQueryClient } from '@queries/client';
import { createMemo, createResource, onMount, Show } from 'solid-js';
import { CanvasDocument } from './component/CanvasDocument';
import { TopBar } from './component/TopBar';
import type { CanvasView } from './context/canvas-document-context';
import {
  type CanvasDocumentData,
  loadCanvasDocument,
  useCanvasDocumentSubscription,
} from './queries/canvas-document';

export type CanvasBlockParams = {
  view?: CanvasView;
};

export type CanvasBlockProps = CanvasBlockParams & {
  documentId: string;
  params?: object;
  navigationRequest?: number | string;
  nested?: NestedState<'canvas'>;
  embedded?: boolean;
  handle?: OwnedBlockHandle<BlockMethodsFor<'canvas'>>;
};

/** App-facing Canvas host. It owns loading, permissions, tracking, and chrome. */
export function CanvasBlock(props: CanvasBlockProps) {
  return (
    <Show when={props.documentId} keyed>
      {(documentId) => <CanvasLoad {...props} documentId={documentId} />}
    </Show>
  );
}

function CanvasLoad(props: CanvasBlockProps) {
  const [document, { refetch }] = createResource(
    () => props.documentId,
    loadCanvasDocument
  );
  const data = () =>
    document.state === 'ready' || document.state === 'refreshing'
      ? document.latest
      : undefined;

  return (
    <EntityLoadGate
      result={{
        data,
        error: () => toEntityLoadError(document.error),
        isPending: () =>
          document.state === 'pending' || document.state === 'unresolved',
      }}
      onRetry={() => void refetch()}
      loadErrorTitle="Unable to load this canvas"
    >
      <Show when={data()} keyed>
        {(loaded) => <CanvasSession {...props} data={loaded} />}
      </Show>
    </EntityLoadGate>
  );
}

function CanvasSession(props: CanvasBlockProps & { data: CanvasDocumentData }) {
  const panel = useSplitPanel();
  const localScope = Boolean(props.nested) || !panel;
  const [attachScope, scopeId] = localScope
    ? useHotkeyDOMScope('canvas')
    : ([undefined, panel.splitHotkeyScope] as const);
  const permissions = () => getPermissions(props.data.userAccessLevel);
  const canEdit = () =>
    hasPermissions(permissions(), Permissions.CAN_EDIT) && !props.nested;
  const entity = () => ({
    type: 'document' as const,
    id: props.documentId,
    name: props.data.documentMetadata.documentName ?? 'Unknown Filename',
    fileType: 'canvas' as const,
    ownerId: props.data.documentMetadata.owner,
  });
  useBlockEntityCommands({
    id: props.documentId,
    scopeId,
    resolveEntity: entity,
  });
  useCanvasDocumentSubscription(() => props.documentId);

  const analytics = useAnalytics();
  const client = useQueryClient();
  onMount(() => {
    if (props.nested || props.embedded) return;
    track({
      itemId: props.documentId,
      blockName: 'canvas',
      client: () => client,
    });
    analytics.pageView('canvas');
    analytics.track('open_entity', {
      entityType: 'canvas',
      entityId: props.documentId,
    });
  });

  const locationParams = createMemo(() => {
    props.navigationRequest;
    return Object.fromEntries(
      Object.entries(props.params ?? {}).flatMap(([key, raw]) => {
        const value = Array.isArray(raw) ? raw.at(-1) : raw;
        return typeof value === 'string' ? [[key, value]] : [];
      })
    );
  });
  const view = () =>
    props.view ??
    readCanvasView((props.params as { view?: unknown } | undefined)?.view);
  const registerMethods = (
    methods: Partial<import('./component/CanvasDocument').CanvasDocumentMethods>
  ) => {
    createMethodRegistration(() => props.handle, methods);
  };
  const nested = () => props.nested !== undefined;

  return (
    <CanvasDocument
      documentId={props.documentId}
      file={props.data.file}
      canEdit={canEdit()}
      hotkeyScope={scopeId}
      isNested={nested()}
      portalScope={nested() ? 'block' : 'split'}
      view={view()}
      locationParams={locationParams()}
      onLocationChange={props.nested?.parentContext?.canvas?.onLocationChange}
      registerMethods={registerMethods}
    >
      {(content, chrome) => (
        <div
          class="portal-scope relative flex size-full min-h-0 min-w-0 select-none flex-col overflow-hidden"
          data-block-type="canvas"
          tabindex={-1}
          ref={(element) => attachScope?.(element)}
          on:click={(event) => {
            if (nested()) event.stopPropagation();
          }}
        >
          <Show when={!nested()} fallback={content}>
            <SidePanel.Layout
              floating
              defaultOpen={false}
              persistKey={`file:${props.documentId}`}
              headerToggle={false}
            >
              <DocumentFileSidePanelSections
                documentId={props.documentId}
                documentName={props.data.documentMetadata.documentName}
                canEdit={canEdit()}
              />
              <div class="flex size-full min-w-0 flex-col overflow-hidden">
                <TopBar
                  documentId={props.documentId}
                  documentMetadata={props.data.documentMetadata}
                  userAccessLevel={props.data.userAccessLevel}
                  sourceFile={props.data.file}
                  savedFile={chrome.savedFile}
                  hotkeyScope={scopeId}
                  chrome={chrome}
                />
                {content}
              </div>
            </SidePanel.Layout>
          </Show>
        </div>
      )}
    </CanvasDocument>
  );
}

function readCanvasView(value: unknown): CanvasView | undefined {
  if (!value || typeof value !== 'object') return;
  const view = value as Partial<CanvasView>;
  if (
    typeof view.x !== 'number' ||
    typeof view.y !== 'number' ||
    typeof view.scale !== 'number'
  ) {
    return;
  }
  return { x: view.x, y: view.y, scale: view.scale };
}
