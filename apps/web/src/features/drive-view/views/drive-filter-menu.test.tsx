import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { createSignal, type ParentProps } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { DriveState } from '../core/types';
import { createDriveState } from '../primitives/drive-state';
import { DriveFilterMenu } from './drive-filter-menu';

// Exercise the real filter dropdown without initializing the app UI barrel.
vi.mock('@ui', async () => ({
  ...(await import('../../../components/ui/utils/classname')),
  ...(await import('../../../components/ui/components/Dropdown')),
  ...(await import('../../../components/ui/components/Layer')),
}));
vi.mock('../../../components/ui/components/Tooltip', () => ({
  Tooltip: (props: ParentProps) => props.children,
}));
vi.mock('@core/mobile/isTouchDevice', () => ({ isTouchDevice: () => false }));
vi.mock('@core/mobile/inputModality', () => ({ isModality: () => false }));
vi.mock('@core/component/EntityIcon', () => ({ EntityIcon: () => null }));
vi.mock('@core/component/UserIcon', () => ({ UserIcon: () => null }));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanelOrThrow: () => ({
    splitHotkeyScope: 'drive',
    isPanelActive: () => true,
  }),
}));
vi.mock('@app/components/view-shell/use-view-control-hotkeys', () => ({
  useViewControlHotkeys: () => {},
}));
vi.mock('@queries/contacts/contacts', () => ({
  useContacts: () => () => [],
}));
vi.mock('@app/lib/analytics/posthog', () => ({
  useFeatureFlag: () => () => ({ enabled: false }),
}));
vi.mock('@property/tags/use-tag-filter-group', () => ({
  useTagFilterGroup: () => () => ({ id: 'tags', label: 'Tags', options: [] }),
}));

const initialState: DriveState = {
  location: { kind: 'tab', tab: 'owned' },
  scope: 'default',
  sort: 'updated_at',
  search: '',
  facets: {},
  expandedFolderIds: [],
  favoritesOpen: true,
  rootOpen: true,
  tagsOpen: true,
};

const [state, setState] = createSignal<DriveState>(initialState);
const driveState = createDriveState({
  state,
  setState,
  folders: () => [],
  list: { reset: () => {} },
  showList: () => {},
});

vi.mock('../context/drive-context', () => ({
  useDriveView: () => ({
    state: driveState,
    actions: { userId: () => 'user' },
  }),
}));

let motionStyles: HTMLStyleElement;
beforeEach(() => {
  setState(initialState);
  motionStyles = document.createElement('style');
  motionStyles.textContent =
    '* { transition-duration: 0s; animation-name: none; }';
  document.head.append(motionStyles);
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
  vi.stubGlobal('scrollTo', vi.fn());
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  cleanup();
  motionStyles.remove();
  vi.unstubAllGlobals();
});

function selectOption(element: HTMLElement) {
  fireEvent(element, new MouseEvent('pointerup', { button: 0, bubbles: true }));
}

async function openSubmenu(label: string) {
  const row = await screen.findByRole('menuitem', { name: label });
  // Kobalte focuses the first row on a deferred tick; land after it.
  await waitFor(() => expect(document.activeElement).not.toBe(document.body));
  row.focus();
  fireEvent.keyDown(row, { key: 'ArrowRight' });
}

/**
 * An open menu marks the trigger's container `aria-hidden`, so role queries
 * cannot reach it. The trigger holds only an icon, which leaves the count
 * badge as the container's only text.
 */
async function openMenu() {
  const { container } = render(() => <DriveFilterMenu />);
  const trigger = screen.getByRole('button', { name: 'Filter files' });
  const badge = () => container.textContent;

  expect(badge()).toBe('');
  fireEvent.keyDown(trigger, { key: 'Enter' });
  await screen.findByRole('menu');

  return badge;
}

it('counts the type filters toggled from the menu without closing it', async () => {
  const badge = await openMenu();
  await openSubmenu('Type');

  selectOption(await screen.findByRole('menuitemcheckbox', { name: 'PDFs' }));
  expect(state().facets.type).toEqual(['file-pdf']);
  expect(badge()).toBe('1');

  selectOption(screen.getByRole('menuitemcheckbox', { name: 'Markdown' }));
  expect(state().facets.type).toEqual(['doc-markdown', 'file-pdf']);
  expect(badge()).toBe('2');

  selectOption(screen.getByRole('menuitemcheckbox', { name: 'PDFs' }));
  expect(state().facets.type).toEqual(['doc-markdown']);
  expect(badge()).toBe('1');
});

it('counts a file scope other than the default', async () => {
  const badge = await openMenu();
  await openSubmenu('Files');

  selectOption(await screen.findByRole('menuitemradio', { name: 'All files' }));
  expect(state().scope).toBe('all');
  expect(badge()).toBe('1');
});

it('drops the count once the filters are cleared', async () => {
  const badge = await openMenu();
  await openSubmenu('Type');
  selectOption(await screen.findByRole('menuitemcheckbox', { name: 'PDFs' }));
  expect(badge()).toBe('1');

  selectOption(screen.getByRole('menuitem', { name: 'Clear filters' }));
  expect(state().facets).toEqual({});
  expect(badge()).toBe('');
});
