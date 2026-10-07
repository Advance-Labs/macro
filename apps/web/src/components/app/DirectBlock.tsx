import { claimOf, useOptionalSplitRouter } from '@app/lib/split-router';
import { PaneContext } from '@app/lib/split-router/solid/context';
import type { BlockName, NestedState } from '@core/block';
import deepEqual from 'fast-deep-equal';
import { createMemo, lazy, Show, useContext } from 'solid-js';
import { useGlobalBlockOrchestrator } from './GlobalAppState';
import { SplitPanelContext } from './split-layout/context';

const CanvasBlock = lazy(async () => ({
  default: (await import('@block-canvas/CanvasBlock')).CanvasBlock,
}));

export type DirectBlockName = 'canvas';

export function isDirectBlockName(name: BlockName): name is DirectBlockName {
  return name === 'canvas';
}

export type DirectBlockProps = {
  type: DirectBlockName;
  id: string;
  params?: object;
  navigationRequest?: number | string;
  nested?: NestedState<'canvas'>;
  embedded?: boolean;
};

function normalizeParams(params: object | undefined): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(params ?? {}).map(([key, raw]) => [
      key,
      Array.isArray(raw) ? raw.at(-1) : raw,
    ])
  );
}

/** App host wiring for features that no longer create legacy block instances. */
export function DirectBlock(props: DirectBlockProps) {
  const identity = createMemo(
    () => ({ type: props.type, id: props.id }),
    undefined,
    {
      equals: (left, right) => left.type === right.type && left.id === right.id,
    }
  );
  return (
    <Show when={identity()} keyed>
      {(target) => (
        <MountedDirectBlock {...props} type={target.type} id={target.id} />
      )}
    </Show>
  );
}

function MountedDirectBlock(props: DirectBlockProps) {
  const orchestrator = useGlobalBlockOrchestrator();
  const router = useOptionalSplitRouter();
  const pane = useContext(PaneContext);
  const panel = useContext(SplitPanelContext);
  const ownsRoute = () => {
    const entry = pane?.entry();
    if (
      props.nested ||
      props.embedded ||
      panel?.handle.isPopover() ||
      !router ||
      !entry
    )
      return false;
    const claim = claimOf(router.routes, entry.location.route);
    return (
      claim === `block:${props.type}:${props.id}` ||
      claim === `${props.type}:${props.id}`
    );
  };
  const params = createMemo(
    () => {
      if (!ownsRoute()) return normalizeParams(props.params);
      const entry = pane!.entry()!;
      const entryProps =
        typeof entry.props === 'object' && entry.props !== null
          ? entry.props
          : {};
      return {
        ...normalizeParams(entryProps),
        ...Object.assign(
          {},
          ...Object.values(entry.location.search ?? {}).map(normalizeParams)
        ),
      };
    },
    undefined,
    { equals: deepEqual }
  );
  const navigationRequest = () =>
    `${props.navigationRequest ?? ''}:${ownsRoute() ? (pane?.entry()?.id ?? '') : ''}`;
  const handle =
    props.nested || props.embedded
      ? undefined
      : orchestrator.registerBlockHandle('canvas', props.id);
  if (!props.nested && !props.embedded && !handle) return <DuplicateMount />;

  return (
    <CanvasBlock
      documentId={props.id}
      params={params()}
      navigationRequest={navigationRequest()}
      nested={props.nested}
      embedded={props.embedded}
      handle={handle}
    />
  );
}

function DuplicateMount() {
  return (
    <div class="flex size-full items-center justify-center text-sm text-ink-muted">
      Content already open.
    </div>
  );
}
