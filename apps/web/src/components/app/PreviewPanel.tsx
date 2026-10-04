import { FileEntityDetail } from '@app/components/entity-detail/FileEntityDetail';
import { HotkeyScope } from '@core/hotkey/HotkeyScope';
import { useHotkeyDOMScope } from '@core/hotkey/hotkeys';
import type { BlockOrchestrator } from '@core/orchestrator';
import { createContextProvider } from '@solid-primitives/context';
import deepEqual from 'fast-deep-equal';
import {
  type Accessor,
  createMemo,
  createRenderEffect,
  createSignal,
  type JSX,
  lazy,
  Match,
  on,
  Show,
  Suspense,
  Switch,
} from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { ViewShell } from '../view-shell/ViewShell';
import type {
  PreviewBlockTarget,
  PreviewPanelSelection,
} from './previewTarget';
import { SidePanel } from './side-panel';
import {
  createPriorityCollapseController,
  PriorityCollapseOverflowSensor,
} from './split-layout/components/PriorityCollapseOverflowSensor';
import {
  SplitPanelContext,
  type SplitPanelContextType,
} from './split-layout/context';

const ImageBlock = lazy(async () => ({
  default: (await import('@app/features/block-image/ImageBlock')).ImageBlock,
}));
const ImageBlockTopBar = lazy(async () => ({
  default: (await import('@app/features/block-image/ImageBlockTopBar'))
    .ImageBlockTopBar,
}));
export const [PreviewPanelContext, useMaybePreviewPanel] =
  createContextProvider(
    (props: {
      previewTarget: PreviewBlockTarget;
      previewEntity?: PreviewPanelSelection;
      onFocusOut?: VoidFunction;
    }) => ({
      previewTarget: () => props.previewTarget,
      previewEntity: () => props.previewEntity,
      onFocusOut: () => props.onFocusOut?.(),
    })
  );

export type PreviewFrameProps = {
  splitPanelContext: SplitPanelContextType;
  onFocusOut?: VoidFunction;
  ref?: (el: HTMLElement) => void;
  headerLeading?: JSX.Element;
  /** A new value marks a new location, which hands focus back to the host. */
  locationKey?: Accessor<unknown>;
  children: JSX.Element;
};

/**
 * Inline preview chrome: header and toolbar slots, focus containment, and a
 * split-panel context scoped to the preview. Content mounts as children.
 */
export function PreviewFrame(props: PreviewFrameProps) {
  const scopedLayoutRefs: SplitPanelContextType['layoutRefs'] = {};
  const headerCollapseController = createPriorityCollapseController();
  const toolbarCollapseController = createPriorityCollapseController();
  const [interactedWith, setInteractedWith] = createSignal(false);
  const [attachHotkeys, previewHotkeyScope] =
    useHotkeyDOMScope('preview-panel');

  createRenderEffect(
    on(
      () => props.locationKey?.(),
      () => setInteractedWith(false)
    )
  );

  return (
    <HotkeyScope scope={previewHotkeyScope}>
      <div
        ref={(element) => {
          attachHotkeys(element);
          props.ref?.(element);
        }}
        class="flex size-full min-h-0 flex-col"
        onFocusIn={(event) => {
          if (interactedWith()) return;
          if (event.target.hasAttribute('data-allow-focus-in-preview')) {
            setInteractedWith(true);
            return;
          }
          const relatedTarget = event.relatedTarget;
          if (
            relatedTarget instanceof HTMLElement &&
            !event.currentTarget.contains(relatedTarget)
          ) {
            relatedTarget.focus();
          } else if (props.onFocusOut) {
            props.onFocusOut();
          } else {
            (event.target as HTMLElement).blur?.();
          }
        }}
        onPointerDown={() => setInteractedWith(true)}
        tabIndex={-1}
      >
        <ViewShell.TopBar
          ref={headerCollapseController.setRow}
          class="relative w-full py-0 touch:flex"
        >
          <Show when={props.headerLeading}>
            <div class="flex shrink-0 items-center">{props.headerLeading}</div>
          </Show>
          <PriorityCollapseOverflowSensor
            controller={headerCollapseController}
            truncateAsLastResort
            class="relative h-full min-w-0 shrink overflow-hidden"
            contentClass={
              props.headerLeading
                ? 'flex h-full items-center gap-1 pl-0.5'
                : 'flex h-full items-center gap-1'
            }
            contentRef={(element) => {
              scopedLayoutRefs.headerLeft = element;
            }}
          />
          <div
            class="flex h-full grow shrink items-center justify-end gap-1"
            ref={(ref) => {
              scopedLayoutRefs.headerRight = ref;
            }}
          />
        </ViewShell.TopBar>
        <div
          ref={toolbarCollapseController.setRow}
          class="relative flex min-h-0 w-full shrink-0 items-center justify-between px-2"
        >
          <PriorityCollapseOverflowSensor
            controller={toolbarCollapseController}
            class="min-w-0 flex-1 overflow-hidden"
            contentClass="flex items-center gap-1"
            contentRef={(element) => {
              scopedLayoutRefs.toolbarLeft = element;
            }}
          />
          <div
            class="flex h-full items-center gap-1"
            ref={(ref) => {
              scopedLayoutRefs.toolbarRight = ref;
            }}
          />
        </div>
        <div class="min-h-0 flex-1">
          <SplitPanelContext.Provider
            value={{
              ...props.splitPanelContext,
              splitHotkeyScope: previewHotkeyScope,
              isInlinePreview: true,
              layoutRefs: scopedLayoutRefs,
              headerCollapser: headerCollapseController.collapser,
              toolbarCollapser: toolbarCollapseController.collapser,
            }}
          >
            <Suspense>{props.children}</Suspense>
          </SplitPanelContext.Provider>
        </div>
      </div>
    </HotkeyScope>
  );
}

export type PreviewPanelProps = {
  target: PreviewBlockTarget | undefined;
  /** Re-open the same target without replacing its mounted block. */
  navigationRequest?: number;
  /** Live selection metadata when a row opened this route. */
  selectedEntity?: PreviewPanelSelection;
  /** Only the remaining legacy feature previews require an orchestrator. */
  orchestrator?: BlockOrchestrator;
  splitPanelContext: SplitPanelContextType;
  onFocusOut?: VoidFunction;
  ref?: (el: HTMLElement) => void;
  headerLeading?: JSX.Element;
};

function sameLocation(left: PreviewBlockTarget, right: PreviewBlockTarget) {
  return (
    left.blockType === right.blockType &&
    left.blockId === right.blockId &&
    deepEqual(left.params, right.params)
  );
}

function PreviewBlock(
  props: PreviewPanelProps & { target: PreviewBlockTarget }
) {
  const orchestrator = props.orchestrator;
  if (!orchestrator)
    throw new Error('Legacy previews require a block orchestrator');

  const blockInstance = createMemo<
    ReturnType<BlockOrchestrator['createBlockInstance']> | undefined
  >((previous) => {
    const { blockType, blockId, aliasContext, params } = props.target;

    if (previous?.type === blockType && previous.id === blockId) {
      return previous;
    }

    return orchestrator.createBlockInstance(blockType, blockId, {
      aliasContext,
      params,
    });
  });

  // Fresh objects for the same block and params are not a navigation. Only a
  // new block or a new location should drive the block or reset focus.
  const location = createMemo(() => props.target, undefined, {
    equals: sameLocation,
  });
  const navigation = createMemo(
    () => ({ target: location(), request: props.navigationRequest ?? 0 }),
    undefined,
    {
      equals: (a, b) =>
        a.request === b.request && sameLocation(a.target, b.target),
    }
  );
  const locate = async (target: PreviewBlockTarget) => {
    const handle = await orchestrator.getBlockHandle(
      target.blockId,
      target.blockType
    );
    if (target.params) await handle?.goToLocationFromParams(target.params);
    else if (target.blockType === 'channel') await handle?.goToLatest();
  };

  createRenderEffect(
    on(navigation, ({ target }) => {
      if (!blockInstance()) return;
      void locate(target);
    })
  );

  return (
    <PreviewFrame
      splitPanelContext={props.splitPanelContext}
      onFocusOut={props.onFocusOut}
      ref={props.ref}
      headerLeading={props.headerLeading}
      locationKey={navigation}
    >
      <PreviewPanelContext
        previewTarget={props.target}
        previewEntity={props.selectedEntity}
        onFocusOut={props.onFocusOut}
      >
        <Show when={blockInstance()}>
          {(instance) => <Dynamic component={instance().element} />}
        </Show>
      </PreviewPanelContext>
    </PreviewFrame>
  );
}

function ImagePreview(
  props: PreviewPanelProps & { target: PreviewBlockTarget }
) {
  const navigation = createMemo(
    () => ({ target: props.target, request: props.navigationRequest ?? 0 }),
    undefined,
    {
      equals: (a, b) =>
        a.request === b.request && sameLocation(a.target, b.target),
    }
  );

  return (
    <PreviewFrame
      splitPanelContext={props.splitPanelContext}
      onFocusOut={props.onFocusOut}
      ref={props.ref}
      headerLeading={props.headerLeading}
      locationKey={navigation}
    >
      <PreviewPanelContext
        previewTarget={props.target}
        previewEntity={props.selectedEntity}
        onFocusOut={props.onFocusOut}
      >
        <SidePanel.Root persistKey="image" defaultOpen={false} floating>
          <ImageBlock documentId={props.target.blockId}>
            {(context, content) => (
              <FileEntityDetail
                documentId={context.documentId}
                data={context.data}
                documentMetadata={context.documentMetadata}
                userAccessLevel={context.userAccessLevel}
                onDownload={context.download}
                content={content}
              >
                {(detailContext) => (
                  <ImageBlockTopBar
                    documentId={context.documentId}
                    context={detailContext}
                  />
                )}
              </FileEntityDetail>
            )}
          </ImageBlock>
        </SidePanel.Root>
      </PreviewPanelContext>
    </PreviewFrame>
  );
}

/**
 * Renders an admitted block target inline. Hosts use createPreviewSelectionGuard
 * before changing the target so conflicts never replace their current detail view.
 */
export function PreviewPanel(props: PreviewPanelProps) {
  return (
    <div class="flex size-full min-h-0">
      <Show when={props.target}>
        {(target) => (
          <Switch>
            <Match when={target().blockType === 'image'}>
              <ImagePreview {...props} target={target()} />
            </Match>
            <Match when={true}>
              <PreviewBlock {...props} target={target()} />
            </Match>
          </Switch>
        )}
      </Show>
    </div>
  );
}
