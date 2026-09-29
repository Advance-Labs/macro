import { globalSplitManager } from '@app/signal/splitLayout';
import type { ComposeTaskProps } from '@block-md/component/ComposeTask';
import type { SplitId } from '@components/app/split-layout/layoutManager';
import { ReactiveMap } from '@solid-primitives/map';
import type { Accessor } from 'solid-js';
import type { CreatableBlock } from './types';

/** How a destination's task composer creates the task and shows where it goes. */
export type DestinationTaskComposer = Pick<
  ComposeTaskProps,
  'createTask' | 'onSuccess' | 'projectName'
>;

/**
 * Where the create menu puts a new task while a split shows a container for
 * one. A project registers itself, so `c` then `t` inside a project adds the
 * task to it — just as cmd+k offers the focused split's own commands. Only the
 * Task entry is placed; every other entry creates as it does anywhere else.
 */
export type CreateDestination = {
  /** Names the destination beside the entries that create into it. */
  label: string;
  taskComposer: DestinationTaskComposer;
};

const destinations = new ReactiveMap<
  SplitId,
  Accessor<CreateDestination | undefined>
>();

/**
 * Offer a destination for as long as the split shows it. The accessor is read
 * whenever the menu needs it, so it can withdraw the destination (e.g. while
 * the viewer cannot add to it) by returning undefined.
 */
export function registerCreateDestination(
  splitId: SplitId,
  destination: Accessor<CreateDestination | undefined>
): () => void {
  destinations.set(splitId, destination);
  return () => {
    if (destinations.get(splitId) === destination) destinations.delete(splitId);
  };
}

/**
 * The destination of the split the user is working in. That is the active
 * split, which keeps its place while focus moves into the create menu, the
 * command menu, or the sidebar.
 */
export function activeCreateDestination(): CreateDestination | undefined {
  const splitId = globalSplitManager()?.activeSplitId();
  return splitId ? destinations.get(splitId)?.() : undefined;
}

/** Names where an entry's creation lands, for the entries a destination places. */
export function createDestinationHint(
  item: Pick<CreatableBlock, 'blockName'>
): string | undefined {
  if (item.blockName !== 'task') return;
  const destination = activeCreateDestination();
  return destination && `In ${destination.label}`;
}
