import type { SplitId } from '@components/app/split-layout/layoutManager';
import { createMemo, createRoot, createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';

const host = vi.hoisted(() => ({
  activeSplitId: (): string | undefined => undefined,
}));

vi.mock('@app/signal/splitLayout', () => ({
  globalSplitManager: () => ({ activeSplitId: () => host.activeSplitId() }),
}));

import {
  activeCreateDestination,
  type CreateDestination,
  createDestinationHint,
  registerCreateDestination,
} from './create-destination';

const projectSplit = 'project-split' as SplitId;
const otherSplit = 'other-split' as SplitId;

function destination(label = 'Launch'): CreateDestination {
  return { label, taskComposer: { projectName: label } };
}

const cleanups: (() => void)[] = [];
function register(
  splitId: SplitId,
  value: () => CreateDestination | undefined
) {
  const unregister = registerCreateDestination(splitId, value);
  cleanups.push(unregister);
  return unregister;
}

afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
  host.activeSplitId = () => undefined;
});

describe('create destinations', () => {
  it('belong to the active split only', () => {
    const launch = destination();
    register(projectSplit, () => launch);

    host.activeSplitId = () => projectSplit;
    expect(activeCreateDestination()).toBe(launch);

    host.activeSplitId = () => otherSplit;
    expect(activeCreateDestination()).toBeUndefined();

    host.activeSplitId = () => undefined;
    expect(activeCreateDestination()).toBeUndefined();
  });

  it('are withdrawn when their split stops showing them', () => {
    host.activeSplitId = () => projectSplit;
    const unregister = register(projectSplit, () => destination());
    unregister();
    expect(activeCreateDestination()).toBeUndefined();
  });

  it('keep a remounted destination when the previous one cleans up late', () => {
    host.activeSplitId = () => projectSplit;
    const previous = register(projectSplit, () => destination('Old'));
    const next = destination('New');
    register(projectSplit, () => next);
    previous();
    expect(activeCreateDestination()).toBe(next);
  });

  it('follow registration, the destination and the active split reactively', () => {
    const [active, setActive] = createSignal<string | undefined>(otherSplit);
    const [label, setLabel] = createSignal<string | undefined>(undefined);
    host.activeSplitId = active;

    createRoot((dispose) => {
      const hint = createMemo(() =>
        createDestinationHint({ blockName: 'task' })
      );
      expect(hint()).toBeUndefined();
      setActive(projectSplit);
      expect(hint()).toBeUndefined();
      const unregister = register(projectSplit, () => {
        const current = label();
        return current ? destination(current) : undefined;
      });
      expect(hint()).toBeUndefined();
      setLabel('Launch');
      expect(hint()).toBe('In Launch');
      setLabel('Renamed');
      expect(hint()).toBe('In Renamed');
      setActive(otherSplit);
      expect(hint()).toBeUndefined();
      setActive(projectSplit);
      expect(hint()).toBe('In Renamed');
      unregister();
      expect(hint()).toBeUndefined();
      dispose();
    });
  });

  it('only name the destination beside Task', () => {
    host.activeSplitId = () => projectSplit;
    register(projectSplit, () => destination());
    expect(createDestinationHint({ blockName: 'task' })).toBe('In Launch');
    for (const blockName of ['md', 'initiative', 'project', 'channel'] as const)
      expect(createDestinationHint({ blockName })).toBeUndefined();
  });
});
