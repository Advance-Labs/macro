import { ResizeZoneContext } from '@core/component/Resize/Resize';
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@solidjs/testing-library';
import { createSignal, useContext } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentChangesContext } from '../context/agent-changes-context';
import { AgentChangesControllerProvider } from '../context/agent-changes-controller';
import { createLocalPaneViewState } from '../pane-view-state';
import {
  type AgentChangesController,
  createAgentChanges,
} from '../primitives/create-agent-changes';
import { createMemoryStorage } from '../tests/memory-storage';
import {
  createMockAgentChangesContext,
  MOCK_PATCH,
  mockChangeset,
} from '../tests/mock-context';
import { AgentChangesSplit } from './AgentChangesSplit';
import { ChangesPane } from './ChangesPane';
import {
  ChangesHandoff,
  ChangesToggle,
  ReviewNotesDock,
} from './SessionChangesControls';

const device = vi.hoisted(() => ({
  touch: false,
  reducedMotion: false,
  width: 1200,
}));
const dimensions = vi.hoisted(() => ({
  width: undefined as (() => number) | undefined,
}));
vi.mock('@core/mobile/isTouchDevice', () => ({
  isTouchDevice: () => device.touch,
}));
beforeEach(() => {
  vi.stubGlobal(
    'matchMedia',
    vi.fn((query: string) => ({
      matches:
        query === '(prefers-reduced-motion: reduce)' && device.reducedMotion,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(() => true),
    }))
  );
});
afterEach(() => {
  device.touch = false;
  device.reducedMotion = false;
  device.width = 1200;
  dimensions.width = undefined;
  vi.unstubAllGlobals();
});

// Pierre mounts a custom element and highlights with shiki; the pane test
// covers everything around it and leaves the diff body to the browser.
vi.mock('@app/components/diff-view/pierre/PierreFileDiff', () => ({
  PierreFileDiff: (props: { path: string; diffStyle: string }) => (
    <div
      data-testid="diff"
      data-path={props.path}
      data-style={props.diffStyle}
    />
  ),
}));

// jsdom has no ResizeObserver. Nested tree zones use the outer pane's solved
// width so spotlight tests exercise real container growth and shrinkage.
vi.mock('@solid-primitives/resize-observer', () => ({
  createResizeObserver: () => {},
  createElementSize: () => {
    const parentWidth = useContext(ResizeZoneContext)?.sizeOf('agent-changes');
    return {
      get width() {
        return parentWidth?.() || dimensions.width?.() || device.width;
      },
      height: 800,
    };
  },
}));

// Module-load quarantine, not a dependency substitute: the connection-gateway
// websocket connects when imported, which jsdom cannot do.
vi.mock('@service-connection/websocket', () => ({
  ws: { send() {}, addEventListener() {}, removeEventListener() {} },
  state: () => 'closed',
  createConnectionBlockWebsocketEffect() {},
  createConnectionWebsocketEffect() {},
  parseWebsocketPayload: () => undefined,
}));

// The block registry globs every block definition (and their heavy
// dependencies) at import time; the pane never reads it.
vi.mock('@core/constant/allBlocks', () => ({
  blocks: {},
  blockAcceptedMimetypeToFileExtension: {},
  blockAcceptedFileExtensionToMimeType: {},
}));

vi.mock('@service-storage/websocket', () => ({
  storageWS: { send() {}, addEventListener() {}, removeEventListener() {} },
  createWebSocketJob: () => Promise.reject(new Error('no websocket in tests')),
}));

function mount(
  context: AgentChangesContext,
  ui: () => ReturnType<typeof ChangesPane>
) {
  const [dismissed, setDismissed] = createSignal<string>();
  let controller!: AgentChangesController;
  const result = render(() => {
    controller = createAgentChanges({
      context,
      storage: createMemoryStorage(),
      view: createLocalPaneViewState(),
      dismissed: [dismissed, setDismissed],
    });
    return (
      <AgentChangesControllerProvider value={controller}>
        {ui()}
      </AgentChangesControllerProvider>
    );
  });
  return { ...result, controller: () => controller };
}

function readyContext() {
  return createMockAgentChangesContext({
    summary: { capturing: false, changeset: mockChangeset() },
    patch: MOCK_PATCH,
  });
}

describe('ChangesPane', () => {
  it('returns to the touch conversation and uses unified diffs without a side tree', async () => {
    device.touch = true;
    const { controller } = mount(readyContext(), () => <ChangesPane />);
    controller().layout.open();
    controller().setDiffStyle('split');
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    expect(
      screen
        .getAllByTestId('diff')
        .every((diff) => diff.dataset.style === 'unified')
    ).toBe(true);
    expect(
      screen.queryByRole('button', { name: 'Expand changes to the full width' })
    ).toBeNull();
    expect(screen.getByRole('button', { name: 'Show file tree' })).toBeTruthy();
    expect(screen.queryByLabelText('Diff layout')).toBeNull();
    expect(
      screen.queryByRole('button', { name: /Switch to .* diff/ })
    ).toBeNull();
    const diffControls = within(
      screen.getByRole('group', { name: 'Diff controls' })
    );
    expect(diffControls.getByText('2 files')).toBeTruthy();
    fireEvent.click(diffControls.getByRole('button', { name: 'Collapse all' }));
    expect(screen.queryAllByTestId('diff')).toHaveLength(0);
    fireEvent.click(diffControls.getByRole('button', { name: 'Expand all' }));
    expect(screen.getAllByTestId('diff')).toHaveLength(2);
    expect(
      screen.queryByRole('group', { name: 'File tree controls' })
    ).toBeNull();
    fireEvent.click(
      screen.getByRole('button', { name: 'Back to conversation' })
    );
    expect(controller().layout.changesVisible()).toBe(false);
  });
  it('puts tree actions in the full-height tree and floats diff controls above the stack', async () => {
    const context = readyContext();
    context.setPullRequestUrl('https://github.com/macro-inc/macro/pull/1482');
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const header = screen.getByLabelText('Changes controls');
    const paneControls = within(header);
    const controls = screen.getByRole('group', { name: 'Diff controls' });
    const diffControls = within(controls);
    const treeHeader = screen.getByRole('group', {
      name: 'File tree controls',
    });
    const treeControls = within(treeHeader);
    const treePanel = treeHeader.closest('[data-resize-panel]');
    const diffPanel = controls.closest('[data-resize-panel]');
    expect(treePanel).toBeTruthy();
    expect(diffPanel).toBeTruthy();
    expect(treePanel).not.toBe(diffPanel);
    expect(treePanel?.parentElement).toBe(diffPanel?.parentElement);
    expect(
      treePanel?.contains(screen.getByRole('group', { name: 'Changed files' }))
    ).toBe(true);
    expect(diffPanel?.contains(screen.getAllByTestId('diff')[0])).toBe(true);
    expect(controls.className).not.toContain('border-b');
    expect(
      screen
        .getByRole('region', { name: 'Changes' })
        .querySelector('[style*="grid-area: toolbar"]')
    ).toBeNull();
    expect(screen.getByRole('region', { name: 'Changes' }).style.border).toBe(
      ''
    );
    const fileCount = treeControls.getByText('2 files');
    const treeToggle = treeControls.getByRole('button', {
      name: 'Hide file tree',
    });
    expect(
      fileCount.compareDocumentPosition(treeToggle) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
    expect(treeHeader.className).toContain('justify-between');
    expect(treeHeader.parentElement?.className).toContain('overflow-hidden');
    expect(treeHeader.nextElementSibling?.className).toContain(
      'overflow-y-auto'
    );
    expect(diffControls.queryByText('2 files')).toBeNull();
    expect(
      diffControls.queryByRole('button', { name: 'Hide file tree' })
    ).toBeNull();
    expect(
      treeControls.queryByRole('button', { name: 'Collapse all' })
    ).toBeNull();
    expect(
      diffControls.getByRole('button', { name: 'Collapse all' })
    ).toBeTruthy();
    expect(
      screen.getAllByRole('button', { name: 'Collapse all' })
    ).toHaveLength(1);
    expect(paneControls.queryByRole('heading', { name: 'Changes' })).toBeNull();
    const link = paneControls.getByRole('link', {
      name: 'View pull request #1482',
    });
    const spacer = controls.querySelector(':scope > .flex-1')!;
    const styleToggle = diffControls.getByLabelText('Diff layout');
    expect(
      styleToggle.compareDocumentPosition(spacer) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
    expect(link.textContent).toBe(
      'agent/unread-archived-sessions → main·#1482'
    );
    expect(link.getAttribute('href')).toBe(
      'https://github.com/macro-inc/macro/pull/1482'
    );
    expect(paneControls.queryByLabelText('Diff layout')).toBeNull();
    expect(diffControls.getByLabelText('Diff layout')).toBeTruthy();
    for (const name of ['Collapse all', 'Refresh pull request changes']) {
      expect(diffControls.getAllByRole('button', { name })).toHaveLength(1);
      expect(paneControls.queryByRole('button', { name })).toBeNull();
      expect(screen.getAllByRole('button', { name })).toHaveLength(1);
    }
    for (const name of [
      'Expand changes to the full width',
      'Close the changes pane',
    ]) {
      expect(paneControls.getAllByRole('button', { name })).toHaveLength(1);
      expect(diffControls.queryByRole('button', { name })).toBeNull();
    }
    fireEvent.click(
      treeControls.getByRole('button', { name: 'Hide file tree' })
    );
    expect(controller().layout.treeOpen()).toBe(false);
    expect(
      screen.queryByRole('group', { name: 'File tree controls' })
    ).toBeNull();
    expect(diffControls.getByText('2 files')).toBeTruthy();
    fireEvent.click(diffControls.getByRole('button', { name: 'Collapse all' }));
    expect(screen.queryAllByTestId('diff')).toHaveLength(0);
    fireEvent.click(diffControls.getByRole('button', { name: 'Expand all' }));
    expect(screen.getAllByTestId('diff')).toHaveLength(2);
    fireEvent.click(
      diffControls.getByRole('button', { name: 'Show file tree' })
    );
    expect(controller().layout.treeOpen()).toBe(true);
    expect(diffControls.queryByText('2 files')).toBeNull();
    expect(
      diffControls.getByRole('button', { name: 'Collapse all' })
    ).toBeTruthy();
    expect(
      diffControls.queryByRole('button', { name: /file tree/ })
    ).toBeNull();
    expect(
      treeControls.getByRole('button', { name: 'Hide file tree' })
    ).toBeTruthy();
    expect(
      treeControls.queryByRole('button', { name: 'Collapse all' })
    ).toBeNull();
  });

  it.each([1200, 500])(
    'keeps textual diff layout controls at %ipx',
    async (width) => {
      device.width = width;
      const { controller } = mount(readyContext(), () => <ChangesPane />);
      controller().layout.open();
      await waitFor(() =>
        expect(screen.getAllByTestId('diff')).toHaveLength(2)
      );
      const toggle = screen.getByLabelText('Diff layout');
      expect(toggle.classList.contains('hidden')).toBe(false);
      expect(toggle.className).not.toContain('changes-diff:');
      expect(
        screen.queryByRole('button', { name: /Switch to .* diff/ })
      ).toBeNull();
      fireEvent.click(within(toggle).getByRole('radio', { name: 'Split' }));
      expect(controller().diffStyle()).toBe('split');
      expect(
        screen
          .getAllByTestId('diff')
          .every((diff) => diff.dataset.style === 'split')
      ).toBe(true);
      fireEvent.click(within(toggle).getByRole('radio', { name: 'Unified' }));
      expect(controller().diffStyle()).toBe('unified');
      expect(
        screen
          .getAllByTestId('diff')
          .every((diff) => diff.dataset.style === 'unified')
      ).toBe(true);
    }
  );

  it('lists files and collapses individual or all diffs without viewed controls', async () => {
    const context = readyContext();
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();

    expect(
      screen.getByText('agent/unread-archived-sessions → main')
    ).toBeTruthy();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    expect(screen.queryByRole('button', { name: /^Viewed$/ })).toBeNull();
    expect(screen.queryByRole('progressbar')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Mark all viewed' })
    ).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Hide a.ts' }));
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Show a.ts' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Collapse all' }));
    expect(screen.queryAllByTestId('diff')).toHaveLength(0);
    fireEvent.click(screen.getByRole('button', { name: 'Expand all' }));
    expect(screen.getAllByTestId('diff')).toHaveLength(2);
  });

  it('supports a PR host with no agent capabilities', async () => {
    const context = readyContext();
    const { controller } = mount(
      {
        ...context,
        host: {
          ...context.host,
          scopeKey: () => 'pr:macro-inc/macro/1482',
          agent: undefined,
        },
      },
      () => (
        <>
          <ChangesPane />
          <ReviewNotesDock />
        </>
      )
    );
    controller().layout.open();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    controller().sendQueuedNotes();
    expect(context.sent).toEqual([]);
    expect(screen.queryByRole('button', { name: 'Send to agent' })).toBeNull();
  });

  it('shows refresh failures without discarding the current diff and allows retry', async () => {
    const context = readyContext();
    const refresh = vi
      .fn()
      .mockRejectedValueOnce(new Error('unavailable'))
      .mockResolvedValueOnce(undefined);
    context.source.refresh = refresh;
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    fireEvent.click(
      screen.getByRole('button', { name: /Refresh pull request changes/ })
    );
    await waitFor(() =>
      expect(
        screen.getByText('The changes could not be refreshed. Try again.')
      ).toBeTruthy()
    );
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() =>
      expect(
        screen.queryByText('The changes could not be refreshed. Try again.')
      ).toBeNull()
    );
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('explains that a linked GitHub PR is required', () => {
    const context = createMockAgentChangesContext({
      summary: {
        capturing: false,
        attempt: {
          startedAt: 't',
          finishedAt: 't',
          outcome: 'not_ready',
          error:
            'Link a GitHub pull request to this session to review its changes.',
        },
      },
    });
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    expect(screen.getByText('Pull request changes unavailable')).toBeTruthy();
    expect(
      screen.getByText(
        'Link a GitHub pull request to this session to review its changes.'
      )
    ).toBeTruthy();
    const header = within(screen.getByLabelText('Changes controls'));
    expect(header.queryByRole('button', { name: 'Collapse all' })).toBeNull();
    expect(header.queryByRole('button', { name: 'Hide file tree' })).toBeNull();
    expect(
      header.getByRole('button', { name: 'Close the changes pane' })
    ).toBeTruthy();
  });

  it('offers to capture when nothing has been captured, and refreshes', async () => {
    const context = createMockAgentChangesContext({
      summary: { capturing: false },
    });
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    fireEvent.click(screen.getByRole('button', { name: /Refresh changes/ }));
    await waitFor(() => expect(context.refreshes()).toBe(1));
  });

  it('opens the linked GitHub PR without creating another one', () => {
    const context = readyContext();
    const url = 'https://github.com/macro-inc/macro/pull/1482';
    context.setPullRequestUrl(url);
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    expect(
      screen.queryByRole('button', { name: 'Create pull request' })
    ).toBeNull();
    fireEvent.click(
      screen.getByRole('link', { name: 'View pull request #1482' })
    );
    expect(context.opened).toEqual([url]);
    expect(context.sent).toEqual([]);
  });

  it('keeps modified GitHub clicks native and the link updated with the PR', () => {
    const context = readyContext();
    const first = 'https://github.com/macro-inc/macro/pull/1482';
    const next = 'https://github.com/macro-inc/macro/pull/1483';
    context.setPullRequestUrl(first);
    mount(context, () => <ChangesPane />);
    const link = screen.getByRole('link', { name: 'View pull request #1482' });
    expect(link.getAttribute('target')).toBe('_blank');
    expect(link.getAttribute('rel')).toBe('noopener noreferrer');
    fireEvent.click(link, { ctrlKey: true });
    fireEvent.click(link, { metaKey: true });
    expect(context.opened).toEqual([]);
    context.setPullRequestUrl(next);
    expect(link.getAttribute('href')).toBe(next);
    fireEvent.click(screen.getByText('agent/unread-archived-sessions → main'));
    expect(context.opened).toEqual([next]);
  });

  it('shows an unlinked branch range when the PR URL is unavailable', () => {
    const context = readyContext();
    context.setPullRequestUrl(undefined);
    mount(context, () => <ChangesPane />);
    expect(
      screen.getByText('agent/unread-archived-sessions → main')
    ).toBeTruthy();
    expect(
      screen.queryByRole('link', { name: /View pull request/ })
    ).toBeNull();
  });

  it('hides and shows the file tree from the floating controls', async () => {
    const context = readyContext();
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    const tree = () => screen.queryByRole('group', { name: 'Changed files' });
    expect(tree()).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    await waitFor(() => expect(tree()).toBeNull());
    expect(controller().layout.treeOpen()).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    await waitFor(() => expect(tree()).toBeTruthy());
  });

  it('keeps authoritative square diff stats beside the GitHub link in split and full width', () => {
    const context = readyContext();
    context.setPullRequestUrl('https://github.com/macro-inc/macro/pull/1482');
    const [title, setTitle] = createSignal<string | undefined>(
      'Pull request title'
    );
    const [counts, setCounts] = createSignal<
      { additions: number; deletions: number } | undefined
    >({ additions: 8, deletions: 2 });
    context.host.pullRequestTitle = title;
    context.host.pullRequestChangeCounts = counts;
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    const header = screen.getByLabelText('Changes controls');
    const controls = within(header);
    expect(controls.queryByText('Pull request title')).toBeNull();
    const initialCounts = controls.getByLabelText('Pull request diff counts');
    expect(initialCounts.textContent).toBe('+8−2');
    const statsImage = within(initialCounts).getByRole('img', {
      name: '8 additions, 2 deletions',
    });
    expect(
      statsImage.querySelectorAll('.bg-success, .bg-failure')
    ).toHaveLength(5);

    fireEvent.click(
      controls.getByRole('button', { name: 'Expand changes to the full width' })
    );
    const titleElement = controls.getByText('Pull request title');
    const link = controls.getByRole('link', {
      name: 'View pull request #1482',
    });
    const countElement = controls.getByLabelText('Pull request diff counts');
    const metadata = link.parentElement;
    expect(titleElement.parentElement?.classList.contains('flex-wrap')).toBe(
      true
    );
    expect(titleElement.classList.contains('max-w-full')).toBe(true);
    expect(titleElement.nextElementSibling).toBe(metadata);
    expect(metadata?.classList.contains('flex-nowrap')).toBe(true);
    expect(metadata?.classList.contains('text-xs')).toBe(true);
    expect(countElement.parentElement).toBe(metadata);
    expect(titleElement.getAttribute('title')).toBe('Pull request title');
    expect(titleElement.classList.contains('truncate')).toBe(true);
    expect(titleElement.classList.contains('text-sm')).toBe(true);
    expect(titleElement.classList.contains('pt-1')).toBe(true);
    expect(link.classList.contains('hover:underline')).toBe(true);
    expect(link.classList.contains('rounded-full')).toBe(false);
    expect(link.classList.contains('hover:bg-hover')).toBe(false);
    expect(link.classList.contains('px-2')).toBe(false);
    expect(link.classList.contains('py-1')).toBe(false);
    expect(link.querySelector('svg')).toBeNull();
    expect(link.textContent).toBe(
      'agent/unread-archived-sessions → main·#1482'
    );
    expect(link.nextElementSibling).toBe(countElement);
    expect(countElement.textContent).toBe('+8−2');
    fireEvent.click(link);
    expect(context.opened).toEqual([
      'https://github.com/macro-inc/macro/pull/1482',
    ]);

    setTitle('Updated pull request title');
    setCounts({ additions: 12, deletions: 4 });
    expect(controls.getByText('Updated pull request title')).toBeTruthy();
    expect(countElement.textContent).toBe('+12−4');
    setTitle(undefined);
    setCounts(undefined);
    expect(controls.queryByText('Updated pull request title')).toBeNull();
    expect(controls.queryByLabelText('Pull request diff counts')).toBeNull();
    // Missing GitHub totals must not expose the captured +3 / −1 estimate.
    expect(header.textContent).not.toContain('+3');
    setTitle('Restored title');
    setCounts({ additions: 8, deletions: 2 });
    fireEvent.click(
      controls.getByRole('button', { name: 'Back to the split' })
    );
    expect(controls.queryByText('Restored title')).toBeNull();
    expect(
      controls.getByLabelText('Pull request diff counts').textContent
    ).toBe('+8−2');
  });
  it('keeps touch navigation outside the wrapping title and metadata', async () => {
    device.touch = true;
    const context = readyContext();
    context.setPullRequestUrl('https://github.com/macro-inc/macro/pull/1482');
    context.host.pullRequestTitle = () => 'A long pull request title';
    context.host.pullRequestChangeCounts = () => ({
      additions: 128,
      deletions: 64,
    });
    const { controller } = mount(context, () => <ChangesPane fullWidth />);
    controller().layout.open();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const controls = within(screen.getByLabelText('Changes controls'));
    const title = controls.getByText('A long pull request title');
    const link = controls.getByRole('link', {
      name: 'View pull request #1482',
    });
    const counts = controls.getByLabelText('Pull request diff counts');
    const navigation = controls.getByRole('button', {
      name: 'Back to conversation',
    });
    const wrappingGroup = title.parentElement;
    expect(wrappingGroup?.classList.contains('flex-wrap')).toBe(true);
    expect(link.parentElement?.parentElement).toBe(wrappingGroup);
    expect(counts.parentElement).toBe(link.parentElement);
    expect(link.nextElementSibling).toBe(counts);
    expect(wrappingGroup?.contains(navigation)).toBe(false);
    expect(navigation.closest('[aria-label="Changes controls"]')).toBe(
      wrappingGroup?.parentElement
    );
    fireEvent.click(link);
    expect(context.opened).toEqual([
      'https://github.com/macro-inc/macro/pull/1482',
    ]);
  });
  it('closes and spotlights from its header', () => {
    const context = readyContext();
    const { controller } = mount(context, () => <ChangesPane />);
    controller().layout.open();
    fireEvent.click(
      screen.getByRole('button', { name: 'Expand changes to the full width' })
    );
    expect(controller().layout.layout()).toBe('full');
    fireEvent.click(screen.getByRole('button', { name: 'Back to the split' }));
    expect(controller().layout.layout()).toBe('split');
    fireEvent.click(
      screen.getByRole('button', { name: 'Close the changes pane' })
    );
    expect(controller().layout.layout()).toBe('closed');
  });
});

describe('session controls', () => {
  it('keeps the Changes toggle visible with ghost and active styling', () => {
    const context = readyContext();
    const { controller } = mount(context, () => (
      <AgentChangesSplit>
        <header aria-label="Session controls">
          <h1>Agent session</h1>
          <button>Share session</button>
          <ChangesToggle />
        </header>
        <p>Session transcript</p>
      </AgentChangesSplit>
    ));
    const toggle = screen.getByRole('button', { name: /^Changes/ });
    expect(toggle.textContent).toBe('Changes');
    expect(toggle.getAttribute('aria-pressed')).toBe('false');
    expect(toggle.getAttribute('data-variant')).toBe('ghost');
    expect(toggle.classList.contains('bg-accent-bg')).toBe(false);
    fireEvent.click(toggle);
    expect(controller().layout.layout()).toBe('split');
    expect(screen.getByRole('button', { name: /^Changes/ })).toBe(toggle);
    expect(toggle.getAttribute('aria-pressed')).toBe('true');
    expect(toggle.getAttribute('data-variant')).toBe('accent');
    expect(toggle.classList.contains('bg-accent-bg')).toBe(true);
    expect(toggle.classList.contains('text-accent')).toBe(true);
    const stats = screen.getByLabelText('Pull request diff counts');
    expect(stats.textContent).toBe('+3−1');
    fireEvent.click(toggle);
    expect(controller().layout.layout()).toBe('closed');
    expect(toggle.getAttribute('aria-pressed')).toBe('false');
    expect(toggle.getAttribute('data-variant')).toBe('ghost');
    expect(toggle.classList.contains('bg-accent-bg')).toBe(false);
    expect(screen.queryByLabelText('Changes controls')).toBeNull();
    fireEvent.click(toggle);
    expect(screen.getByRole('button', { name: 'Share session' })).toBeTruthy();
    const sessionHeader = screen.getByLabelText('Session controls');
    const changesHeader = screen.getByLabelText('Changes controls');
    const sessionPanel = sessionHeader.closest('[data-resize-panel]');
    const changesPanel = changesHeader.closest('[data-resize-panel]');
    expect(sessionPanel).toBeTruthy();
    expect(changesPanel).toBeTruthy();
    expect(changesPanel?.querySelector('.h-full')).toBeTruthy();
    expect(sessionPanel).not.toBe(changesPanel);
    expect(sessionPanel?.parentElement).toBe(changesPanel?.parentElement);
    fireEvent.click(
      screen.getByRole('button', { name: 'Expand changes to the full width' })
    );
    expect(controller().layout.layout()).toBe('full');
    expect(screen.queryByRole('button', { name: /^Changes/ })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Share session' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Back to the split' }));
    expect(controller().layout.layout()).toBe('split');
    expect(screen.getByRole('button', { name: 'Share session' })).toBeTruthy();
    expect(
      screen
        .getByRole('button', { name: /^Changes/ })
        .getAttribute('data-variant')
    ).toBe('accent');
    fireEvent.click(
      screen.getByRole('button', { name: 'Close the changes pane' })
    );
    expect(controller().layout.layout()).toBe('closed');
    expect(screen.getByRole('button', { name: /^Changes/ }).textContent).toBe(
      'Changes'
    );
    expect(screen.queryByLabelText('Changes controls')).toBeNull();
    expect(screen.getByRole('button', { name: 'Share session' })).toBeTruthy();
    expect(
      screen
        .getByRole('button', { name: /^Changes/ })
        .getAttribute('data-variant')
    ).toBe('ghost');
  });

  it('uses GitHub API totals without falling back to captured estimates', () => {
    const context = readyContext();
    const [counts, setCounts] = createSignal<
      { additions: number; deletions: number } | undefined
    >({ additions: 8, deletions: 2 });
    context.host.pullRequestChangeCounts = counts;
    const { controller } = mount(context, () => (
      <>
        <ChangesToggle />
        <ChangesPane />
      </>
    ));
    const toggle = screen.getByRole('button', { name: /Changes/ });
    const stats = () => screen.queryByLabelText('Pull request diff counts');
    expect(toggle.textContent).toBe('Changes');
    expect(stats()?.textContent).toBe('+8−2');

    // A missing or stale PR capture must not replace GitHub totals.
    context.setSummary(undefined);
    expect(stats()?.textContent).toBe('+8−2');
    context.setSummary({ capturing: true, changeset: mockChangeset() });
    expect(stats()?.textContent).toBe('+8−2');
    expect(toggle.querySelector('.animate-pulse')).toBeNull();
    expect(controller().changeCounts()).toEqual({ additions: 8, deletions: 2 });
    // Missing GitHub data must not expose the snapshot's +3 / −1 estimates.
    setCounts(undefined);
    expect(toggle.textContent).toBe('Changes');
    expect(stats()).toBeNull();
    expect(controller().changeCounts()).toBeUndefined();

    setCounts({ additions: 12, deletions: 0 });
    expect(stats()?.textContent).toBe('+12');
    setCounts({ additions: 0, deletions: 4 });
    expect(stats()?.textContent).toBe('−4');
    setCounts({ additions: 0, deletions: 0 });
    expect(stats()?.textContent).toBe('');
    expect(toggle.textContent).toBe('Changes');
    fireEvent.click(toggle);
    expect(controller().layout.changesVisible()).toBe(true);
  });

  it.each([1600, 1800])(
    'keeps pane owners and the dragged split width across spotlight toggles at %spx',
    async (width) => {
      device.width = width;
      const sessionMount = vi.fn();
      function Conversation() {
        sessionMount();
        return <input aria-label="Conversation draft" value="Keep my draft" />;
      }
      const { controller } = mount(readyContext(), () => (
        <AgentChangesSplit>
          <Conversation />
        </AgentChangesSplit>
      ));
      const draft = screen.getByLabelText(
        'Conversation draft'
      ) as HTMLInputElement;
      draft.value = 'An unsent draft';
      controller().layout.open();
      await waitFor(() =>
        expect(screen.getAllByTestId('diff')).toHaveLength(2)
      );
      const pane = screen.getByRole('region', { name: 'Changes' });
      const panel = pane.closest<HTMLElement>('[data-resize-panel]')!;
      const separator = panel.parentElement!.querySelector<HTMLElement>(
        ':scope > [role="separator"]'
      )!;
      const originalWidth = panel.style.width;
      fireEvent.keyDown(separator, { key: 'ArrowRight' });
      await Promise.resolve();
      const draggedWidth = panel.style.width;
      const draggedShare = controller().layout.changesShare();
      expect(draggedWidth).not.toBe(originalWidth);
      const diff = screen.getAllByTestId('diff')[0];
      const tree = screen
        .getByRole('group', { name: 'Changed files' })
        .closest<HTMLElement>('[data-resize-panel]')!;
      const treeWidth = tree.style.width;
      expect(treeWidth).toBe('256px');
      const scroller = pane.querySelector<HTMLElement>('[aria-busy]')!;
      scroller.scrollTop = 137;
      for (let attempt = 0; attempt < 3; attempt += 1) {
        fireEvent.click(
          screen.getByRole('button', {
            name: 'Expand changes to the full width',
          })
        );
        expect(panel.style.width).toBe(`${width}px`);
        expect(tree.style.width).toBe(treeWidth);
        const session = draft.closest<HTMLElement>(
          '[data-agent-changes-session]'
        )!;
        expect(session.hidden).toBe(true);
        expect(session.inert).toBe(true);
        expect(
          screen.queryByRole('textbox', { name: 'Conversation draft' })
        ).toBeNull();
        fireEvent.click(
          screen.getByRole('button', { name: 'Back to the split' })
        );
        expect(panel.style.width).toBe(draggedWidth);
        expect(tree.style.width).toBe(treeWidth);
        expect(controller().layout.changesShare()).toBe(draggedShare);
        expect(session.hidden).toBe(false);
        expect(session.inert).toBe(false);
        expect(screen.getByLabelText('Conversation draft')).toBe(draft);
        expect(draft.value).toBe('An unsent draft');
        expect(screen.getAllByTestId('diff')[0]).toBe(diff);
        expect(scroller.scrollTop).toBe(137);
      }
      expect(sessionMount).toHaveBeenCalledOnce();
    }
  );

  it('applies the latest spotlight request after a slide settles', async () => {
    const animations: {
      onfinish: (() => void) | null;
      cancel: ReturnType<typeof vi.fn>;
    }[] = [];
    const animate = vi.fn(function (this: HTMLElement, frames: Keyframe[]) {
      const animation = {
        onfinish: null as (() => void) | null,
        cancel: vi.fn(),
      };
      if (
        this.getAttribute('role') !== 'separator' &&
        frames.some((frame) => frame.transform)
      )
        animations.push(animation);
      return animation;
    });
    Object.defineProperty(HTMLElement.prototype, 'animate', {
      configurable: true,
      value: animate,
    });
    try {
      const { controller } = mount(readyContext(), () => (
        <AgentChangesSplit>
          <input aria-label="Conversation draft" />
        </AgentChangesSplit>
      ));
      controller().layout.open();
      await waitFor(() => expect(animations).toHaveLength(1));
      const pane = screen.getByRole('region', { name: 'Changes' });
      const panel = pane.closest<HTMLElement>('[data-resize-panel]')!;
      const splitWidth = panel.style.width;
      controller().layout.spotlight();
      controller().layout.spotlight();
      controller().layout.spotlight();
      expect(panel.style.width).toBe(splitWidth);
      animations[0].onfinish?.();
      expect(panel.style.width).toBe('1200px');
      controller().layout.close();
      await waitFor(() => expect(animations).toHaveLength(2));
      const draft = screen.getByRole('textbox', { name: 'Conversation draft' });
      const sessionContent = draft.closest<HTMLElement>(
        '[data-agent-changes-session]'
      )!;
      expect(sessionContent.style.width).toBe('1200px');
      expect(sessionContent.hasAttribute('data-slide-underlay')).toBe(true);
      expect(panel.style.width).toBe('1200px');
      controller().layout.open();
      await waitFor(() => expect(animations).toHaveLength(3));
      expect(sessionContent.hidden).toBe(false);
      expect(sessionContent.inert).toBe(true);
      expect(sessionContent.style.width).toBe('1200px');
      animations[1].onfinish?.();
      expect(screen.getByRole('region', { name: 'Changes' })).toBe(pane);
      expect(panel.style.width).toBe('1200px');
      animations[2].onfinish?.();
      expect(panel.style.width).toBe(splitWidth);
      expect(screen.getByRole('textbox', { name: 'Conversation draft' })).toBe(
        draft
      );
      expect(
        screen.getByRole('textbox', { name: 'Conversation draft' })
      ).toBeTruthy();
    } finally {
      Reflect.deleteProperty(HTMLElement.prototype, 'animate');
    }
  });

  it('animates touch changes and preserves the conversation draft on return', async () => {
    device.touch = true;
    const { controller } = mount(readyContext(), () => (
      <AgentChangesSplit>
        <ChangesToggle />
        <input aria-label="Conversation draft" value="Keep my draft" />
      </AgentChangesSplit>
    ));
    const draft = screen.getByLabelText('Conversation draft');
    controller().layout.open();
    const pane = screen.getByRole('region', { name: 'Changes' });
    expect(pane.closest('.absolute')).toBeTruthy();
    fireEvent.click(
      screen.getByRole('button', { name: 'Back to conversation' })
    );
    expect(screen.queryByRole('region', { name: 'Changes' })).toBeNull();
    expect(screen.getByLabelText('Conversation draft')).toBe(draft);
    expect((draft as HTMLInputElement).value).toBe('Keep my draft');
  });

  it.each([false, true])(
    'retains the pane through exit and cancels exit on reopen (touch: %s)',
    async (touch) => {
      device.touch = touch;
      const animations: Array<{
        element: HTMLElement;
        frames: Keyframe[];
        options: KeyframeAnimationOptions;
        onfinish: (() => void) | null;
        cancel: ReturnType<typeof vi.fn>;
      }> = [];
      const animate = vi.fn(function (
        this: HTMLElement,
        frames: Keyframe[],
        options: KeyframeAnimationOptions
      ) {
        const animation = {
          element: this,
          frames,
          options,
          onfinish: null as (() => void) | null,
          cancel: vi.fn(),
        };
        if (options.duration === 220) animations.push(animation);
        return animation;
      });
      Object.defineProperty(HTMLElement.prototype, 'animate', {
        configurable: true,
        value: animate,
      });
      try {
        const { controller, unmount } = mount(readyContext(), () => (
          <AgentChangesSplit>
            <p>Conversation</p>
          </AgentChangesSplit>
        ));
        controller().layout.open();
        await waitFor(() => expect(animations).toHaveLength(touch ? 1 : 3));
        const pane = screen.getByRole('region', { name: 'Changes' });
        const panel = pane.closest('[data-resize-panel]') as HTMLElement | null;
        const wrapper = (
          touch ? pane.closest('.absolute') : panel?.firstElementChild
        ) as HTMLElement;
        const paneAnimations = () =>
          animations.filter((animation) => animation.element === wrapper);
        expect(paneAnimations()[0].frames).toEqual([
          { transform: 'translateX(100%)' },
          { transform: 'translateX(0)' },
        ]);
        if (!touch) {
          const session = screen
            .getByText('Conversation')
            .closest<HTMLElement>('[data-resize-panel]')!;
          const gutter = panel!.parentElement!.querySelector<HTMLElement>(
            ':scope > [role="separator"]'
          )!;
          expect(
            animations.find((animation) => animation.element === session)
              ?.frames
          ).toEqual([{ width: '100%' }, { width: session.style.width }]);
          expect(
            animations.find((animation) => animation.element === gutter)?.frames
          ).toEqual([
            { transform: `translateX(${panel!.style.width})` },
            { transform: 'translateX(0)' },
          ]);
          expect(
            animations.every(
              (animation) => animation.options === paneAnimations()[0].options
            )
          ).toBe(true);
        }
        const width = panel?.style.width;
        if (!touch) expect(width).not.toBe('0px');
        await waitFor(() =>
          expect(screen.getAllByTestId('diff')).toHaveLength(2)
        );
        paneAnimations()[0].onfinish?.();
        controller().layout.close();
        await waitFor(() => expect(paneAnimations()).toHaveLength(2));
        expect(screen.getByRole('region', { name: 'Changes' })).toBe(pane);
        expect(pane.closest('[data-resize-panel]')).toBe(panel);
        expect(panel?.style.width).toBe(width);
        expect(screen.getAllByTestId('diff')).toHaveLength(2);
        expect(wrapper.inert).toBe(true);
        expect(paneAnimations()[1].frames).toEqual([
          { transform: 'translateX(0)' },
          { transform: 'translateX(100%)' },
        ]);
        expect(paneAnimations()[1].options.fill).toBe('forwards');
        const expectSplitExit = () => {
          const session = screen
            .getByText('Conversation')
            .closest<HTMLElement>('[data-resize-panel]')!;
          const gutter = panel!.parentElement!.querySelector<HTMLElement>(
            ':scope > [role="separator"]'
          )!;
          expect(
            panel?.parentElement?.classList.contains('overflow-hidden')
          ).toBe(true);
          expect(gutter.inert).toBe(true);
          expect(
            animations
              .filter((animation) => animation.element === session)
              .at(-1)?.frames
          ).toEqual([{ width: session.style.width }, { width: '100%' }]);
          expect(
            animations
              .filter((animation) => animation.element === gutter)
              .at(-1)
              ?.frames.at(-1)
          ).toEqual({
            transform: `translateX(${width})`,
          });
          expect(
            animations
              .slice(animations.indexOf(paneAnimations()[0]) + 1)
              .every(
                (animation) => animation.options === paneAnimations()[1].options
              )
          ).toBe(true);
        };
        if (!touch) expectSplitExit();
        // Browsers return a live computed-style object. Cancelling the old
        // animation changes those getters, so reversal must snapshot first.
        let cancelled = false;
        paneAnimations()[1].cancel.mockImplementation(() => {
          cancelled = true;
        });
        const computed = window.getComputedStyle;
        const styleSpy = vi
          .spyOn(window, 'getComputedStyle')
          .mockImplementation((node) => {
            if (node !== wrapper) return computed(node);
            const style = computed(node);
            Object.defineProperties(style, {
              transform: {
                get: () => (cancelled ? 'none' : 'matrix(1, 0, 0, 1, 200, 0)'),
              },
            });
            return style;
          });
        controller().layout.open();
        await waitFor(() => expect(paneAnimations()).toHaveLength(3));
        styleSpy.mockRestore();
        expect(paneAnimations()[2].frames[0]).toEqual({
          transform: 'matrix(1, 0, 0, 1, 200, 0)',
        });
        expect(paneAnimations()[1].cancel).toHaveBeenCalledOnce();
        paneAnimations()[1].onfinish?.();
        expect(screen.getByRole('region', { name: 'Changes' })).toBe(pane);
        expect(wrapper.inert).toBe(false);
        paneAnimations()[2].onfinish?.();
        if (!touch) {
          const session = screen
            .getByText('Conversation')
            .closest<HTMLElement>('[data-resize-panel]')!;
          const reversal = animations
            .filter((animation) => animation.element === session)
            .at(-1)!;
          expect(reversal.frames.at(-1)).toEqual({
            width: session.style.width,
          });
          expect(reversal.cancel).toHaveBeenCalledOnce();
          const gutter = panel!.parentElement!.querySelector<HTMLElement>(
            ':scope > [role="separator"]'
          )!;
          expect(gutter.inert).toBe(false);
          controller().layout.spotlight();
          expect(panel?.style.width).toBe('1200px');
        }
        controller().layout.close();
        await waitFor(() => expect(paneAnimations()).toHaveLength(4));
        if (!touch) {
          expect(panel?.style.width).toBe('1200px');
          const sessionContent =
            screen.getByText('Conversation').parentElement!;
          expect(sessionContent.hidden).toBe(false);
          expect(sessionContent.inert).toBe(false);
          expect(sessionContent.style.width).toBe('1200px');
          expect(sessionContent.parentElement?.style.width).toBe('0px');
          expect(sessionContent.hasAttribute('data-slide-underlay')).toBe(true);
        }
        paneAnimations()[3].onfinish?.();
        expect(paneAnimations()[3].cancel).toHaveBeenCalledOnce();
        expect(
          animations.every(
            (animation) => animation.cancel.mock.calls.length > 0
          )
        ).toBe(true);
        if (!touch) {
          expect(
            screen
              .getByText('Conversation')
              .closest<HTMLElement>('[data-resize-panel]')?.style.width
          ).toBe('1200px');
          const sessionContent =
            screen.getByText('Conversation').parentElement!;
          expect(sessionContent.hasAttribute('data-slide-underlay')).toBe(
            false
          );
          expect(sessionContent.style.width).toBe('');
        }
        expect(screen.queryByRole('region', { name: 'Changes' })).toBeNull();
        if (!touch) expect(panel?.isConnected).toBe(false);
        const count = animations.length;
        controller().layout.open();
        await waitFor(() =>
          expect(animations).toHaveLength(count + (touch ? 1 : 3))
        );
        const opening = animations.at(-1)!;
        unmount();
        expect(opening.cancel).toHaveBeenCalledOnce();
      } finally {
        Reflect.deleteProperty(HTMLElement.prototype, 'animate');
        vi.restoreAllMocks();
      }
    }
  );

  it.each([false, true])(
    'reverses the slide when closing during entry without fading (touch: %s)',
    async (touch) => {
      device.touch = touch;
      const animate = vi.fn(
        (_frames: Keyframe[], _options: KeyframeAnimationOptions) => ({
          onfinish: null as (() => void) | null,
          cancel: vi.fn(),
        })
      );
      Object.defineProperty(HTMLElement.prototype, 'animate', {
        configurable: true,
        value: animate,
      });
      try {
        const { controller } = mount(readyContext(), () => (
          <AgentChangesSplit>
            <p>Conversation</p>
          </AgentChangesSplit>
        ));
        controller().layout.open();
        await waitFor(() =>
          expect(
            animate.mock.calls.filter(([, options]) => options.duration === 220)
          ).toHaveLength(touch ? 1 : 3)
        );
        expect(animate.mock.lastCall?.[0]).toEqual([
          { transform: 'translateX(100%)' },
          { transform: 'translateX(0)' },
        ]);
        const pane = screen.getByRole('region', { name: 'Changes' });
        const wrapper = (
          touch
            ? pane.closest('.absolute')
            : pane.closest('[data-resize-panel]')?.firstElementChild
        ) as HTMLElement;
        const computed = window.getComputedStyle;
        vi.spyOn(window, 'getComputedStyle').mockImplementation((node) => {
          const style = computed(node);
          if (node === wrapper) {
            Object.defineProperties(style, {
              transform: { value: 'matrix(1, 0, 0, 1, 8, 0)' },
            });
          }
          return style;
        });
        const opening = animate.mock.results.at(-1)!.value;
        controller().layout.close();
        await waitFor(() =>
          expect(animate.mock.lastCall?.[0].at(-1)?.transform).toBe(
            'translateX(100%)'
          )
        );
        expect(opening.cancel).toHaveBeenCalledOnce();
        expect(animate.mock.lastCall?.[0]).toEqual([
          { transform: 'matrix(1, 0, 0, 1, 8, 0)' },
          { transform: 'translateX(100%)' },
        ]);
      } finally {
        Reflect.deleteProperty(HTMLElement.prototype, 'animate');
        vi.restoreAllMocks();
      }
    }
  );

  it.each([false, true])(
    'skips opening and closing animations with reduced motion (touch: %s)',
    async (touch) => {
      device.touch = touch;
      device.reducedMotion = true;
      const animate = vi.fn();
      Object.defineProperty(HTMLElement.prototype, 'animate', {
        configurable: true,
        value: animate,
      });
      try {
        const { controller, unmount } = mount(readyContext(), () => (
          <AgentChangesSplit>
            <p>Conversation</p>
          </AgentChangesSplit>
        ));
        controller().layout.open();
        await Promise.resolve();
        expect(screen.getByRole('region', { name: 'Changes' })).toBeTruthy();
        expect(animate).not.toHaveBeenCalled();
        controller().layout.close();
        expect(screen.queryByRole('region', { name: 'Changes' })).toBeNull();
        controller().layout.open();
        await Promise.resolve();
        expect(screen.getByRole('region', { name: 'Changes' })).toBeTruthy();
        controller().layout.close();
        expect(screen.queryByRole('region', { name: 'Changes' })).toBeNull();
        expect(animate).not.toHaveBeenCalled();
        unmount();
      } finally {
        Reflect.deleteProperty(HTMLElement.prototype, 'animate');
      }
    }
  );

  it('keeps counts out of the toggle before and after snapshots load', () => {
    const context = createMockAgentChangesContext();
    mount(context, () => <ChangesToggle />);
    const toggle = screen.getByRole('button', { name: /Changes/ });
    expect(toggle.textContent).toBe('Changes');
    context.setSummary({
      capturing: false,
      changeset: mockChangeset({ files: [], additions: 0, deletions: 0 }),
    });
    expect(toggle.textContent).toBe('Changes');
    context.setSummary({ capturing: false, changeset: mockChangeset() });
    expect(toggle.textContent).toBe('Changes');
  });

  it('hands off to the pane while it is closed, and can be dismissed', () => {
    const context = readyContext();
    const { controller } = mount(context, () => <ChangesHandoff />);
    expect(screen.getByText('Changes ready to review')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Review changes' }));
    expect(controller().layout.layout()).toBe('split');
    expect(controller().review.active()).toBe('apps/web/src/a.ts');
    expect(screen.queryByText('Changes ready to review')).toBeNull();

    controller().layout.close();
    expect(screen.getByText('Changes ready to review')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText('Changes ready to review')).toBeNull();
  });

  it('sends queued notes to the agent from the dock', () => {
    const context = readyContext();
    const { controller } = mount(context, () => <ReviewNotesDock />);
    expect(screen.queryByText(/review note/)).toBeNull();
    controller().review.addNote(
      {
        path: 'apps/web/src/a.ts',
        side: 'additions',
        lineNumber: 2,
        endLineNumber: 2,
      },
      'Use a constant'
    );
    expect(screen.getByText(/review note/).textContent).toContain('1');
    fireEvent.click(
      screen.getByRole('button', { name: /1 review note queued/ })
    );
    const editor = screen.getByLabelText(
      'Review note on apps/web/src/a.ts, line 2 (new)'
    ) as HTMLTextAreaElement;
    expect(editor.value).toBe('Use a constant');
    fireEvent.input(editor, { target: { value: 'Use a named constant' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send to agent' }));
    expect(context.sent[0]).toContain('`apps/web/src/a.ts`, line 2 (new)');
    expect(context.sent[0]).toContain('Use a named constant');
    expect(screen.queryByText(/review note/)).toBeNull();
  });

  it('renders nothing for a host that can never have changes', () => {
    const context = readyContext();
    const [coding, setCoding] = createSignal(false);
    const { controller } = mount(
      { ...context, host: { ...context.host, canHaveChanges: coding } },
      () => (
        <>
          <ChangesToggle />
          <ChangesHandoff />
          <ReviewNotesDock />
        </>
      )
    );
    controller().review.addNote(
      {
        path: 'apps/web/src/a.ts',
        side: 'additions',
        lineNumber: 2,
        endLineNumber: 2,
      },
      'Use a constant'
    );
    expect(screen.queryByRole('button', { name: /Changes/ })).toBeNull();
    expect(screen.queryByText('Changes ready to review')).toBeNull();
    expect(screen.queryByText(/review note/)).toBeNull();

    setCoding(true);
    expect(screen.getByRole('button', { name: /Changes/ })).toBeTruthy();
    expect(screen.getByText('Changes ready to review')).toBeTruthy();
    expect(screen.getByText(/review note/)).toBeTruthy();
  });

  it("opens the note's file from the expanded dock", () => {
    const context = readyContext();
    const { controller } = mount(context, () => <ReviewNotesDock />);
    controller().review.addNote(
      {
        path: 'apps/web/src/a.ts',
        side: 'additions',
        lineNumber: 2,
        endLineNumber: 2,
      },
      'Use a constant'
    );
    fireEvent.click(
      screen.getByRole('button', { name: /1 review note queued/ })
    );
    fireEvent.click(
      screen.getByRole('button', { name: /apps\/web\/src\/a.ts/ })
    );
    expect(controller().layout.changesVisible()).toBe(true);
    expect(controller().review.active()).toBe('apps/web/src/a.ts');
  });
});

describe('responsive Changes split', () => {
  it('opens narrow desktop Changes at full width without changing the saved split', async () => {
    device.width = 600;
    const { controller } = mount(readyContext(), () => (
      <AgentChangesSplit>
        <input aria-label="Conversation draft" />
      </AgentChangesSplit>
    ));
    const draft = screen.getByRole('textbox', {
      name: 'Conversation draft',
    }) as HTMLInputElement;
    fireEvent.input(draft, { target: { value: 'Keep my draft' } });
    controller().layout.setChangesShare(34);
    controller().layout.open();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const pane = screen.getByRole('region', { name: 'Changes' });
    expect(pane.closest<HTMLElement>('[data-resize-panel]')?.style.width).toBe(
      '600px'
    );
    expect(
      draft.closest<HTMLElement>('[data-agent-changes-session]')?.hidden
    ).toBe(true);
    expect(screen.queryByRole('separator')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Expand changes to the full width' })
    ).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Back to the split' })
    ).toBeNull();
    expect(controller().layout.layout()).toBe('split');
    expect(controller().layout.changesShare()).toBe(34);
    fireEvent.click(
      screen.getByRole('button', { name: 'Close the changes pane' })
    );
    expect(screen.getByRole('textbox', { name: 'Conversation draft' })).toBe(
      draft
    );
    expect(draft.value).toBe('Keep my draft');
  });

  it('retains the pane, diffs, draft, and split ratio when crossing the narrow breakpoint', async () => {
    const [width, setWidth] = createSignal(1200);
    dimensions.width = width;
    const { controller } = mount(readyContext(), () => (
      <AgentChangesSplit>
        <input aria-label="Conversation draft" />
      </AgentChangesSplit>
    ));
    const draft = screen.getByRole('textbox', {
      name: 'Conversation draft',
    }) as HTMLInputElement;
    fireEvent.input(draft, { target: { value: 'Unsent message' } });
    controller().layout.setChangesShare(34);
    controller().layout.open();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const pane = screen.getByRole('region', { name: 'Changes' });
    const diff = screen.getAllByTestId('diff')[0];
    const panel = pane.closest<HTMLElement>('[data-resize-panel]')!;
    const savedWidth = panel.style.width;
    const session = draft.closest<HTMLElement>('[data-agent-changes-session]')!;
    expect(session.hidden).toBe(false);
    setWidth(600);
    expect(session.hidden).toBe(true);
    expect(panel.style.width).toBe('600px');
    expect(
      screen.queryByRole('button', { name: 'Expand changes to the full width' })
    ).toBeNull();
    expect(screen.getByRole('region', { name: 'Changes' })).toBe(pane);
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    setWidth(1200);
    expect(session.hidden).toBe(false);
    expect(panel.style.width).toBe(savedWidth);
    expect(
      screen.getByRole('button', { name: 'Expand changes to the full width' })
    ).toBeTruthy();
    expect(controller().layout.layout()).toBe('split');
    expect(controller().layout.changesShare()).toBe(34);
    expect(screen.getByRole('textbox', { name: 'Conversation draft' })).toBe(
      draft
    );
    expect(draft.value).toBe('Unsent message');
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
  });
});
