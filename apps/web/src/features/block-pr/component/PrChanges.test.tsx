import { useAgentChanges } from '@app/features/agent-changes/context/agent-changes-controller';
import { createLocalPaneViewState } from '@app/features/agent-changes/pane-view-state';
import {
  createMockAgentChangesContext,
  mockChangeset,
} from '@app/features/agent-changes/tests/mock-context';
import { cleanup, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createPrChangesSource } from '../data/pr-changes';
import { PrChangesProvider } from './PrChanges';

vi.mock('@app/features/agent-changes/agent-changes', () => ({
  copyText: async () => true,
}));
vi.mock(
  '@app/features/agent-changes/pane-view-state',
  async (importOriginal) => ({
    ...(await importOriginal<
      typeof import('@app/features/agent-changes/pane-view-state')
    >()),
    createPaneViewState: () => createLocalPaneViewState(),
  })
);
vi.mock('../data/pr-changes', () => ({
  createPrChangesSource: vi.fn(
    () =>
      createMockAgentChangesContext({
        summary: { capturing: false, changeset: mockChangeset() },
      }).source
  ),
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { success() {}, failure() {} },
}));
vi.mock('@core/util/url', () => ({ openExternalUrl() {} }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function HostMetadata() {
  const { context, changeCounts } = useAgentChanges();
  return (
    <>
      <span aria-label="PR title">
        {context.host.pullRequestTitle?.() ?? 'unavailable'}
      </span>
      <span aria-label="PR counts">
        {JSON.stringify(changeCounts()) ?? 'unavailable'}
      </span>
    </>
  );
}

describe('PrChangesProvider', () => {
  it('forwards reactive PR metadata without falling back to captured counts', () => {
    const [title, setTitle] = createSignal<string | undefined>();
    const [counts, setCounts] = createSignal<
      { additions: number; deletions: number } | undefined
    >({ additions: 12, deletions: 3 });
    render(() => (
      <PrChangesProvider
        foreignEntityId="pr-1"
        pullRequestTitle={title()}
        pullRequestChangeCounts={counts()}
      >
        <HostMetadata />
      </PrChangesProvider>
    ));
    expect(screen.getByLabelText('PR title').textContent).toBe('unavailable');
    // The source's captured totals are +3 / −1, not these GitHub totals.
    expect(screen.getByLabelText('PR counts').textContent).toBe(
      JSON.stringify({ additions: 12, deletions: 3 })
    );
    setTitle('Loaded PR title');
    expect(screen.getByLabelText('PR title').textContent).toBe(
      'Loaded PR title'
    );
    setCounts(undefined);
    expect(screen.getByLabelText('PR counts').textContent).toBe('unavailable');
    setCounts({ additions: 0, deletions: 0 });
    expect(screen.getByLabelText('PR counts').textContent).toBe(
      JSON.stringify({ additions: 0, deletions: 0 })
    );
    expect(createPrChangesSource).toHaveBeenCalledOnce();
  });
});
