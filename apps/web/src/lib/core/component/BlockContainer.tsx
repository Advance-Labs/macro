import { type BlockName, useBlockId, useBlockName } from '@core/block';
import { useEntitySubscription } from '@service-connection/client';
import { children, createEffect, type FlowProps } from 'solid-js';
import { match } from 'ts-pattern';
import { blockLiveTrackingEnabledSignal } from '../internal/BlockLoader';
import { blockElementSignal } from '../signal/blockElement';

// TODO: handle nested state
const getBlockElementId = (blockId: string) => `block-${blockId}`;

function resolveEntityType(blockName: BlockName) {
  return match(blockName)
    .with('chat', 'channel', 'project', 'database', (entityType) => entityType)
    .otherwise(() => 'document' as const);
}

interface BlockContainerProps extends FlowProps {
  title?: string;
  attachHotkeyScope?: (element: HTMLElement) => void;
}

/** @deprecated Use DocumentBlockContainer instead, it handles loading state and all kinds of great things!
 * @see DocumentBlockContainer
 * For internal use only.
 */
export function BlockContainer(props: BlockContainerProps) {
  const setElement = blockElementSignal.set;
  const liveTrackingEnabled = blockLiveTrackingEnabledSignal.get;
  const blockId = useBlockId();
  const blockName = useBlockName();
  useEntitySubscription(() =>
    liveTrackingEnabled() && blockId && blockName
      ? { entity_type: resolveEntityType(blockName), entity_id: blockId }
      : undefined
  );

  const resolved = children(() => props.children);
  createEffect(() => {
    const resolved_ = resolved();
    if (!(resolved_ instanceof HTMLElement)) {
      console.error('BlockContainer must be used with a single HTMLElement');
      return;
    }
    resolved_.id = getBlockElementId(blockId);
    resolved_.dataset.blockType = blockName;
    setElement(resolved_);
    props.attachHotkeyScope?.(resolved_);
  });

  return (
    <div class="relative size-full portal-scope">
      <div class="overflow-hidden size-full">{resolved()}</div>
    </div>
  );
}
