import { createRoot, createSignal } from 'solid-js';
import { describe, expect, it } from 'vitest';
import { createLocalPaneViewState } from '../pane-view-state';
import { createMemoryStorage } from '../tests/memory-storage';
import {
  createPaneLayout,
  DEFAULT_TREE_WIDTH,
  MAX_TREE_WIDTH,
  MIN_TREE_WIDTH,
} from './create-pane-layout';

const key = (scope: string) => `agent-changes:layout:${scope}`;

describe('createPaneLayout tree width', () => {
  it('persists per scope and restores a previous width', () => {
    createRoot((dispose) => {
      const storage = createMemoryStorage();
      const [scope, setScope] = createSignal<string>('one');
      const view = createLocalPaneViewState();
      const layout = createPaneLayout({
        sessionId: scope,
        layout: [view.layout, view.setLayout],
        storage,
      });
      expect(layout.treeWidth()).toBe(DEFAULT_TREE_WIDTH);
      layout.setTreeWidth(320);
      expect(JSON.parse(storage.getItem(key('one')) ?? '').treeWidth).toBe(320);
      setScope('two');
      expect(layout.treeWidth()).toBe(DEFAULT_TREE_WIDTH);
      layout.setTreeWidth(410);
      setScope('one');
      expect(layout.treeWidth()).toBe(320);
      dispose();
    });
  });

  it('accepts legacy layouts and rejects malformed widths', () => {
    for (const [raw, expected] of [
      [{ share: 38, treeOpen: false }, DEFAULT_TREE_WIDTH],
      [{ treeWidth: '400' }, DEFAULT_TREE_WIDTH],
      [{ treeWidth: null }, DEFAULT_TREE_WIDTH],
      [{ treeWidth: Infinity }, DEFAULT_TREE_WIDTH],
      [{ treeWidth: -100 }, MIN_TREE_WIDTH],
      [{ treeWidth: 9999 }, MAX_TREE_WIDTH],
      [{ treeWidth: 210 }, 210],
    ] as const) {
      createRoot((dispose) => {
        const storage = createMemoryStorage();
        storage.setItem(key('one'), JSON.stringify(raw));
        const view = createLocalPaneViewState();
        const layout = createPaneLayout({
          sessionId: () => 'one',
          layout: [view.layout, view.setLayout],
          storage,
        });
        expect(layout.treeWidth()).toBe(expected);
        if ('treeOpen' in raw) expect(layout.treeOpen()).toBe(false);
        layout.setTreeWidth(Number.NaN);
        expect(layout.treeWidth()).toBe(DEFAULT_TREE_WIDTH);
        layout.setTreeWidth(10);
        expect(layout.treeWidth()).toBe(MIN_TREE_WIDTH);
        layout.setTreeWidth(900);
        expect(layout.treeWidth()).toBe(MAX_TREE_WIDTH);
        dispose();
      });
    }
  });
});
