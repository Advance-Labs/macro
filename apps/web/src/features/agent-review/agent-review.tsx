import { createLegacyReviewState } from '@app/features/agent-changes/agent-changes';
/** Production boundary: session authority and route state are injected into the reader. */
import { isCoderHarness } from '@app/features/agents-view/core/agent-kind';
import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import GitDiffIcon from '@phosphor/git-diff.svg';
import { Button } from '@ui';
import {
  createMemo,
  createSignal,
  lazy,
  onCleanup,
  type ParentProps,
  Show,
} from 'solid-js';
import { useAgentSession } from '../block-agent/context/AgentSessionContext';
import {
  type ReviewHost,
  ReviewHostContext,
  useOptionalReviewHost,
  useReviewHost,
} from './context/review-context';
import { createReviewFile, createReviewSource } from './queries/review';
import { reviewSearch } from './review-search';

const ReviewWorkspace = lazy(() => import('./views/ReviewWorkspace'));

export function AgentReviewProvider(props: ParentProps) {
  const session = useAgentSession();
  const owned = useOwnsSearchNamespace(reviewSearch.namespace)();
  const [local, setLocal] = createSignal(reviewSearch.defaults);
  const routed = owned ? createSearchParams(reviewSearch) : undefined;
  const state = () => (routed ? routed[0] : local());
  const update = (
    patch: Partial<typeof reviewSearch.defaults>,
    push = false
  ) => {
    if (routed) routed[1](patch, { history: push ? 'push' : 'replace' });
    else setLocal((current) => ({ ...current, ...patch }));
  };
  const legacy = createLegacyReviewState({
    sessionId: session.sessionId,
    changeset: () => undefined,
  });
  const [navigation, setNavigation] = createSignal(0);
  const host: ReviewHost = {
    createSource: (revision, active) =>
      createReviewSource(session.sessionId, revision, active),
    createFile: (revision, path, active) =>
      createReviewFile(session.sessionId, revision, path, active),
    navigation,
    savedNotes: legacy.queued,
    sessionId: session.sessionId,
    userId: session.userId,
    displayName: session.displayName,
    canEdit: () => session.session()?.canEdit === true,
    available: () => isCoderHarness(session.session()?.harness),
    open: () => state().open,
    reviewId: () => state().id || undefined,
    revision: () => state().revision || undefined,
    target: () => state().target || undefined,
    thread: () => state().thread || undefined,
    show: () => update({ open: true }, true),
    back: () => update({ open: false }, true),
    selectRevision: (revision) => update({ revision, target: '', thread: '' }),
    copyLink: (href) => navigator.clipboard.writeText(href),
    openLink: (href) => {
      let url: URL;
      try {
        url = new URL(href, window.location.href);
      } catch {
        return false;
      }
      const allowed =
        url.origin === window.location.origin ||
        ['macro.com', 'dev.macro.com'].includes(url.hostname);
      const id = /^\/app\/(?:agent|agents|coders)\/([^/]+)\/?$/.exec(
        url.pathname
      )?.[1];
      if (
        !allowed ||
        id !== session.sessionId() ||
        url.searchParams.get('s0.review.open') !== 'true'
      )
        return false;
      setNavigation((n) => n + 1);
      const revision = Number(url.searchParams.get('s0.review.revision'));
      update(
        {
          open: true,
          id: url.searchParams.get('s0.review.id') ?? '',
          revision:
            Number.isSafeInteger(revision) && revision > 0 ? revision : 0,
          target: url.searchParams.get('s0.review.target') ?? '',
          thread: url.searchParams.get('s0.review.thread') ?? '',
        },
        true
      );
      return true;
    },
  };
  return (
    <ReviewHostContext.Provider value={host}>
      {props.children}
    </ReviewHostContext.Provider>
  );
}

/** The transcript stays mounted while review takes its full content area. */
export function ReviewSessionSurface(props: ParentProps) {
  const host = useReviewHost();
  const everOpened = createMemo(
    (opened: boolean) => opened || host.open(),
    false
  );
  return (
    <div
      class="relative flex size-full min-h-0 min-w-0 flex-col"
      ref={(element) => {
        const capture = (event: MouseEvent) => {
          if (
            event.defaultPrevented ||
            event.button !== 0 ||
            event.ctrlKey ||
            event.metaKey ||
            event.shiftKey ||
            event.altKey
          )
            return;
          const anchor =
            event.target instanceof Element
              ? event.target.closest('a[href]')
              : null;
          const href = anchor?.getAttribute('href');
          if (href && host.openLink(href)) {
            event.preventDefault();
            event.stopPropagation();
          }
        };
        element.addEventListener('click', capture, true);
        onCleanup(() => element.removeEventListener('click', capture, true));
      }}
    >
      <div
        class="flex size-full min-h-0 flex-col"
        classList={{ hidden: host.open() }}
        inert={host.open()}
      >
        {props.children}
      </div>
      <Show when={everOpened()}>
        <div
          class="absolute inset-0 flex min-h-0 flex-col bg-surface touch:pt-(--mobile-content-inset-top) touch:pb-(--mobile-content-inset-bottom)"
          classList={{ hidden: !host.open() }}
          inert={!host.open()}
        >
          <ReviewWorkspace />
        </div>
      </Show>
    </div>
  );
}
export function ReviewToggle() {
  const host = useOptionalReviewHost();
  return (
    <Show when={host?.available()}>
      <Button variant="ghost" size="sm" onClick={() => host?.show()}>
        <GitDiffIcon />
        Review changes
      </Button>
    </Show>
  );
}
