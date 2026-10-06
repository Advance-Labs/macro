import { claimOf, useOptionalSplitRouter } from '@app/lib/split-router';
import { PaneContext } from '@app/lib/split-router/solid/context';
import type { BlockName } from '@core/block';
import deepEqual from 'fast-deep-equal';
import { createMemo, lazy, Show, useContext } from 'solid-js';
import { useGlobalBlockOrchestrator } from './GlobalAppState';
import { SplitPanelContext } from './split-layout/context';

const ChatBlock = lazy(async () => ({
  default: (await import('@block-chat/ChatBlock')).ChatBlock,
}));
const SpreadsheetBlock = lazy(
  () => import('@app/features/block-spreadsheet/SpreadsheetBlock')
);

export type DirectBlockName = 'chat' | 'spreadsheet';

export function isDirectBlockName(name: BlockName): name is DirectBlockName {
  return name === 'chat' || name === 'spreadsheet';
}

export type DirectBlockProps = {
  type: DirectBlockName;
  id: string;
  params?: object;
  navigationRequest?: number | string;
  nested?: boolean;
};

function locationParams(
  params: DirectBlockProps['params']
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(params ?? {}).flatMap(([key, raw]) => {
      const value = Array.isArray(raw) ? raw.at(-1) : raw;
      return typeof value === 'string' ? [[key, value]] : [];
    })
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
    if (props.nested || panel?.handle.isPopover() || !router || !entry)
      return false;
    const claim = claimOf(router.routes, entry.location.route);
    return (
      claim === `block:${props.type}:${props.id}` ||
      claim === `${props.type}:${props.id}`
    );
  };
  const params = createMemo(
    () => {
      if (!ownsRoute()) return locationParams(props.params);
      const entry = pane!.entry()!;
      const entryProps =
        typeof entry.props === 'object' && entry.props !== null
          ? entry.props
          : {};
      return {
        ...locationParams(entryProps),
        ...Object.assign(
          {},
          ...Object.values(entry.location.search ?? {}).map(locationParams)
        ),
      };
    },
    undefined,
    { equals: deepEqual }
  );
  const navigationRequest = () =>
    `${props.navigationRequest ?? ''}:${ownsRoute() ? (pane?.entry()?.id ?? '') : ''}`;
  if (props.type === 'chat') {
    const handle = props.nested
      ? undefined
      : orchestrator.registerBlockHandle('chat', props.id);
    if (!props.nested && !handle) return <DuplicateMount />;
    return (
      <ChatBlock
        chatId={props.id}
        params={params()}
        navigationRequest={navigationRequest()}
        nested={props.nested}
        handle={handle}
      />
    );
  }
  const handle = props.nested
    ? undefined
    : orchestrator.registerBlockHandle('spreadsheet', props.id);
  if (!props.nested && !handle) return <DuplicateMount />;
  return (
    <SpreadsheetBlock
      documentId={props.id}
      params={params()}
      navigationRequest={navigationRequest()}
      nested={props.nested}
      handle={handle}
      share={params().share}
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
