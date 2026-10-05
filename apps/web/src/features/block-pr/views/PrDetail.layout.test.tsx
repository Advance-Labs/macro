import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal, type JSX, Show } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PrForeignEntityData } from '../data/queries';

const mocks = vi.hoisted(() => ({
  sessionVisible: (): boolean => true,
  pageView: vi.fn(),
  track: vi.fn(),
  data: undefined as PrForeignEntityData | undefined,
  controller: undefined as
    | {
        available: () => boolean;
        layout: {
          open: ReturnType<typeof vi.fn>;
          changesVisible: () => boolean;
        };
      }
    | undefined,
}));

vi.mock('@app/features/agent-changes/agent-changes', () => ({
  AgentChangesSplit: (props: { children: JSX.Element }) => (
    <div data-testid="changes-split">
      <Show when={mocks.sessionVisible()}>
        <div data-testid="split-host-panel">{props.children}</div>
      </Show>
    </div>
  ),
  ChangesToggle: () => <button>Changes</button>,
}));
vi.mock('@app/features/agent-changes/context/agent-changes-controller', () => ({
  useOptionalAgentChanges: () => mocks.controller,
}));
vi.mock('../component/PrChanges', () => ({
  PrChangesProvider: (props: {
    children: JSX.Element;
    pullRequestTitle?: string;
    pullRequestChangeCounts?: { additions: number; deletions: number };
  }) => (
    <div
      data-testid="changes-provider"
      data-pr-title={props.pullRequestTitle}
      data-pr-additions={props.pullRequestChangeCounts?.additions}
      data-pr-deletions={props.pullRequestChangeCounts?.deletions}
    >
      {props.children}
    </div>
  ),
}));
vi.mock('@app/components/view-shell', () => ({
  ViewShell: {
    TopBar: (props: { children: JSX.Element }) => (
      <header>{props.children}</header>
    ),
  },
  ViewBreadcrumbs: {
    Outlet: (props: { class?: string }) => (
      <span class={props.class}>Pull request location</span>
    ),
    Item: (props: {
      children: (item: {
        isActive: () => boolean;
        onSelect: () => void;
      }) => JSX.Element;
    }) => (
      <div data-testid="reviews-breadcrumb">
        {props.children({ isActive: () => true, onSelect: () => {} })}
      </div>
    ),
    Button: (props: { children: JSX.Element; class?: string }) => (
      <button class={props.class}>{props.children}</button>
    ),
  },
}));
vi.mock('@app/lib/analytics/analytics-context', () => ({
  useAnalytics: () => ({ pageView: mocks.pageView, track: mocks.track }),
}));
vi.mock('@app/lib/split-router', () => ({ useRouteParams: () => ({}) }));
vi.mock('@app/routes/routes', () => ({ reviewsPrRoute: {} }));
vi.mock('@components/app/side-panel', () => ({
  SidePanel: {
    Root: (props: { children: JSX.Element }) => <div>{props.children}</div>,
    Layout: (props: { children: JSX.Element }) => <div>{props.children}</div>,
    Toggle: () => null,
  },
}));
vi.mock('@components/app/split-panel', () => ({
  SplitPanel: { CloseButton: () => null },
}));
vi.mock('@components/app/split-layout/components/SplitFileMenu', () => ({
  SplitFileMenu: () => <button aria-label="PR menu">Menu</button>,
}));
vi.mock('@core/component/SharePermissions', () => ({
  Permissions: { CAN_VIEW: 1 },
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalNotificationSource: () => undefined,
}));
vi.mock('@notifications', () => ({
  DebouncedNotificationReadMarker: () => null,
}));
vi.mock('@ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@ui')>()),
  Scroll: (props: { children: JSX.Element }) => <div>{props.children}</div>,
  Button: (props: { children: JSX.Element }) => (
    <button>{props.children}</button>
  ),
  Layer: (props: { children: JSX.Element }) => <div>{props.children}</div>,
}));
vi.mock(
  '@core/component/LexicalMarkdown/component/core/StaticMarkdown',
  () => ({
    StaticMarkdown: () => null,
    StaticMarkdownContext: (props: { children: JSX.Element }) => props.children,
  })
);
vi.mock('@entity/components/GithubLabelPill', () => ({
  GithubLabelPills: () => null,
}));
vi.mock('../component/PrStatus', () => ({
  PR_PILL_CLASS: '',
  PrStatusIcon: () => null,
  PrStatusChip: () => null,
}));
vi.mock('../component/PrSkeletons', () => ({
  PrTitleSkeleton: () => <span>PR detail skeleton</span>,
  PrMetadataSkeleton: () => null,
  PrDescriptionSkeleton: () => null,
  PrTimelineSkeleton: () => null,
}));
vi.mock('../component/PrTimeline', () => ({ PrTimeline: () => null }));
vi.mock('../component/sidepanel/PrSidePanelSections', () => ({
  PrSidePanelSections: () => null,
}));
vi.mock('../data/prDiscussionSource', () => ({
  createPrDiscussionSource: () => ({}),
}));
vi.mock('../data/queries', () => ({
  usePrForeignEntityQuery: () => ({
    isPending: mocks.data === undefined,
    data: mocks.data,
    status: mocks.data ? 'success' : 'pending',
    refetch: vi.fn(),
  }),
}));
vi.mock('@tanstack/solid-query', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tanstack/solid-query')>()),
  useQueryClient: () => ({ invalidateQueries: vi.fn() }),
}));
vi.mock('@queries/storage/github-pull-requests', () => ({
  useRefreshGithubPullRequest: () => {},
}));

import { ReviewsPrDetail } from '@app/features/reviews-view/components/ReviewsPrDetail';
import { PrDetailBody, PrDetailContent, StandalonePrDetail } from './PrDetail';

const id = '019a5faa-d2cd-7c55-8e8a-23aac4f0bc88';
const prData: PrForeignEntityData = {
  id,
  prRef: { owner: 'macro-inc', repo: 'macro', number: 1482 },
  pullRequest: {
    owner: 'macro-inc',
    repo: 'macro',
    number: 1482,
    displayName: 'macro-inc/macro#1482',
    githubKey: 'macro-inc/macro/pull/1482',
    name: 'Pull request title',
    url: 'https://github.com/macro-inc/macro/pull/1482',
    status: 'open',
    additions: 12,
    deletions: 3,
  },
};

beforeEach(() => {
  mocks.sessionVisible = () => true;
  mocks.data = undefined;
  mocks.controller = undefined;
});
afterEach(cleanup);

describe('PR changes layout', () => {
  it('places the standalone PR header and body inside one split host panel', () => {
    render(() => <StandalonePrDetail foreignEntityId={id} />);
    const panel = screen.getByTestId('split-host-panel');
    expect(screen.getAllByTestId('changes-split')).toHaveLength(1);
    expect(panel.closest('[data-testid="changes-provider"]')).not.toBeNull();
    expect(panel.querySelector('header')?.textContent).toContain(
      'Pull request'
    );
    expect(panel.querySelector('header')?.textContent).toContain('Changes');
    expect(panel.textContent).toContain('PR detail skeleton');
  });

  it('places the reviews PR header and body inside one split host panel', () => {
    render(() => <ReviewsPrDetail foreignEntityId={id} />);
    const panel = screen.getByTestId('split-host-panel');
    expect(screen.getAllByTestId('changes-split')).toHaveLength(1);
    expect(panel.closest('[data-testid="changes-provider"]')).not.toBeNull();
    expect(panel.querySelector('header')?.textContent).toContain(
      'Pull request location'
    );
    expect(panel.querySelector('header')?.textContent).toContain('Changes');
    expect(panel.textContent).toContain('PR detail skeleton');
    const breadcrumb = screen.getByText('Pull request location');
    expect(breadcrumb.classList.contains('flex-1')).toBe(false);
    expect(breadcrumb.classList.contains('overflow-hidden')).toBe(false);
    const menu = screen.getByRole('button', { name: 'PR menu' });
    const row = menu.parentElement?.parentElement;
    expect(row?.classList.contains('min-w-0')).toBe(true);
    expect(row?.classList.contains('min-w-16')).toBe(false);
    expect(row?.classList.contains('touch:min-w-8')).toBe(false);
    expect(row?.querySelector('button')?.classList.contains('min-w-8')).toBe(
      false
    );
    expect(row?.classList.contains('overflow-hidden')).toBe(false);
    menu.focus();
    expect(document.activeElement).toBe(menu);
    expect(
      screen.getByText('Changes').parentElement?.classList.contains('shrink-0')
    ).toBe(true);
  });

  it.each([
    ['standalone', StandalonePrDetail],
    ['reviews', ReviewsPrDetail],
  ] as const)(
    'passes the existing PR title and GitHub totals into the %s Changes provider',
    (_name, Host) => {
      mocks.data = prData;
      render(() => <Host foreignEntityId={id} />);
      expect(
        screen.getByTestId('changes-provider').getAttribute('data-pr-title')
      ).toBe('Pull request title');
      expect(
        screen.getByTestId('changes-provider').getAttribute('data-pr-additions')
      ).toBe('12');
      expect(
        screen.getByTestId('changes-provider').getAttribute('data-pr-deletions')
      ).toBe('3');
    }
  );

  it.each([
    ['standalone', StandalonePrDetail],
    ['reviews', ReviewsPrDetail],
  ] as const)(
    'omits placeholder metadata while the %s PR loads',
    (_name, Host) => {
      render(() => <Host foreignEntityId={id} />);
      const provider = screen.getByTestId('changes-provider');
      expect(provider.hasAttribute('data-pr-title')).toBe(false);
      expect(provider.hasAttribute('data-pr-additions')).toBe(false);
      expect(provider.hasAttribute('data-pr-deletions')).toBe(false);
    }
  );

  it.each([
    ['standalone', StandalonePrDetail],
    ['reviews', ReviewsPrDetail],
  ] as const)('preserves real zero totals for the %s PR', (_name, Host) => {
    mocks.data = {
      ...prData,
      pullRequest: { ...prData.pullRequest, additions: 0, deletions: 0 },
    };
    render(() => <Host foreignEntityId={id} />);
    const provider = screen.getByTestId('changes-provider');
    expect(provider.getAttribute('data-pr-additions')).toBe('0');
    expect(provider.getAttribute('data-pr-deletions')).toBe('0');
  });

  it.each([
    ['standalone', StandalonePrDetail],
    ['reviews', ReviewsPrDetail],
  ] as const)(
    'omits unavailable titles and partial totals for the %s PR',
    (_name, Host) => {
      mocks.data = {
        ...prData,
        pullRequest: {
          ...prData.pullRequest,
          name: undefined,
          deletions: undefined,
        },
      };
      render(() => <Host foreignEntityId={id} />);
      const provider = screen.getByTestId('changes-provider');
      expect(provider.hasAttribute('data-pr-title')).toBe(false);
      expect(provider.hasAttribute('data-pr-additions')).toBe(false);
      expect(provider.hasAttribute('data-pr-deletions')).toBe(false);
    }
  );
  it.each([
    ['standalone', StandalonePrDetail],
    ['reviews', ReviewsPrDetail],
  ] as const)(
    'opens Changes from the %s diff count pill without toggling it closed',
    (_name, Host) => {
      mocks.data = prData;
      const [visible, setVisible] = createSignal(false);
      const open = vi.fn(() => setVisible(true));
      mocks.controller = {
        available: () => true,
        layout: { open, changesVisible: visible },
      };
      render(() => <Host foreignEntityId={id} />);
      const pill = screen.getByRole('button', { name: 'Open Changes pane' });
      expect(pill.getAttribute('type')).toBe('button');
      expect(pill.getAttribute('aria-expanded')).toBe('false');
      expect(pill.textContent).toBe('+12−3');
      expect(pill.querySelector('.text-success')?.textContent).toBe('+12');
      expect(pill.querySelector('.text-failure')?.textContent).toBe('−3');
      fireEvent.click(pill);
      fireEvent.click(pill);
      expect(open).toHaveBeenCalledTimes(2);
      expect(pill.getAttribute('aria-expanded')).toBe('true');
    }
  );

  it.each([
    ['without a controller', undefined],
    ['when changes are unavailable', false],
  ] as const)('leaves the diff count passive %s', (_name, available) => {
    mocks.data = prData;
    if (available === false) {
      mocks.controller = {
        available: () => false,
        layout: { open: vi.fn(), changesVisible: () => false },
      };
    }
    render(() => <StandalonePrDetail foreignEntityId={id} />);
    expect(
      screen.queryByRole('button', { name: 'Open Changes pane' })
    ).toBeNull();
    const count = screen.getByText('+12').parentElement;
    expect(count?.tagName).toBe('SPAN');
    expect(count?.textContent).toBe('+12−3');
    expect(count?.querySelector('.text-failure')?.textContent).toBe('−3');
    if (mocks.controller)
      expect(mocks.controller.layout.open).not.toHaveBeenCalled();
  });

  it('keeps the legacy PR detail count passive outside a Changes provider', () => {
    mocks.data = prData;
    render(() => (
      <PrDetailBody
        foreignEntityId={id}
        data={prData}
        status="success"
        discussionSource={
          {} as Parameters<typeof PrDetailBody>[0]['discussionSource']
        }
        onRetry={() => {}}
      />
    ));
    expect(screen.getByText('+12').parentElement?.tagName).toBe('SPAN');
    expect(
      screen.queryByRole('button', { name: 'Open Changes pane' })
    ).toBeNull();
  });

  it.each([
    ['standalone', StandalonePrDetail],
    ['reviews', ReviewsPrDetail],
  ] as const)(
    'omits the standalone GitHub button in the %s host',
    (_name, Host) => {
      mocks.data = {
        id,
        prRef: { owner: 'macro-inc', repo: 'macro', number: 1482 },
        pullRequest: {
          owner: 'macro-inc',
          repo: 'macro',
          number: 1482,
          displayName: 'macro-inc/macro#1482',
          githubKey: 'macro-inc/macro/pull/1482',
          name: 'Pull request title',
          url: 'https://github.com/macro-inc/macro/pull/1482',
          status: 'open',
        },
      };
      render(() => <Host foreignEntityId={id} />);
      expect(screen.getByRole('button', { name: 'Changes' })).toBeTruthy();
      expect(
        screen.queryByRole('button', { name: /Open (on|in) GitHub/ })
      ).toBeNull();
    }
  );

  it('keeps the reviews breadcrumb mounted when the session panel is hidden', () => {
    mocks.sessionVisible = () => false;
    render(() => <ReviewsPrDetail foreignEntityId={id} />);
    expect(screen.getByTestId('reviews-breadcrumb')).toBeTruthy();
    expect(screen.queryByTestId('split-host-panel')).toBeNull();
    expect(
      screen
        .getByTestId('reviews-breadcrumb')
        .closest('[data-testid="changes-split"]')
    ).toBeNull();
  });

  it('does not create a nested split in PrDetailContent', () => {
    render(() => (
      <PrDetailContent
        foreignEntityId={id}
        status="pending"
        discussionSource={
          {} as Parameters<typeof PrDetailContent>[0]['discussionSource']
        }
        onRetry={() => {}}
      />
    ));
    expect(screen.getByText('PR detail skeleton')).toBeTruthy();
    expect(screen.queryByTestId('changes-split')).toBeNull();
  });
});
