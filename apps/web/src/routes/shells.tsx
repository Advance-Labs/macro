import { SplitRouter } from '@app/lib/split-router';
import { dismissBootShell } from '@components/app/boot-shell';
import { AppChrome, PageContent } from '@components/app/Layout';
import { SplitLayout } from '@components/app/split-layout/SplitLayout';
import { useIsAuthenticated } from '@core/auth';
import { type JSX, onMount, type ParentProps, Show } from 'solid-js';

/** Takes over from index.html's boot shell once this shell has drawn. */
function useDismissBootShell() {
  onMount(dismissBootShell);
}

/** Sign-in, onboarding, invite, and handoff pages: the page alone. */
export function AuthShell(): JSX.Element {
  useDismissBootShell();
  return (
    <PageContent>
      <SplitRouter.Outlet />
    </PageContent>
  );
}

function FocusedPage(props: ParentProps) {
  useDismissBootShell();
  return (
    <div class="min-h-0 flex-1 overflow-y-auto bg-page text-ink">
      {props.children}
    </div>
  );
}

/** Booking links: a scrolling page on the page background, without app chrome. */
export function FocusedShell(): JSX.Element {
  return (
    <FocusedPage>
      <SplitRouter.Outlet />
    </FocusedPage>
  );
}

/**
 * A form's respond page: anonymous visitors get the focused shell, so a public
 * form never hits login; signed-in respondents keep the app chrome around it.
 * While sign-in is unknown the focused shell shows.
 */
export function withFormRespondShell(Page: () => JSX.Element) {
  return function FormRespondShell(): JSX.Element {
    const isAuthenticated = useIsAuthenticated();
    return (
      <Show
        when={isAuthenticated() === true}
        fallback={
          <FocusedPage>
            <Page />
          </FocusedPage>
        }
      >
        <AppPage>
          <Page />
        </AppPage>
      </Show>
    );
  };
}

/** A call: full screen, without app chrome. */
export function withMeetingShell(Page: () => JSX.Element) {
  return function MeetingShell(): JSX.Element {
    useDismissBootShell();
    return (
      <PageContent>
        <Page />
      </PageContent>
    );
  };
}

function AppPage(props: ParentProps) {
  useDismissBootShell();
  return <AppChrome>{props.children}</AppChrome>;
}

/** The app: its chrome around the split layout, which renders every pane. */
export function AppShell(): JSX.Element {
  return (
    <AppPage>
      <SplitLayout />
    </AppPage>
  );
}
