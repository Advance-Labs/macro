import { ViewBreadcrumbs, ViewShell } from '@app/components/view-shell';
import { SplitRouter } from '@app/lib/split-router';
import { SplitPanel } from '@components/app/split-panel';
import { Show } from 'solid-js';
import { LegalSidebar } from './components/legal-sidebar';
import { LegalWorkspaceContext } from './context/legal-workspace';
import { createWorkspace, type LegalNavigation } from './primitives/workspace';
import { legalSource } from './queries/source';
import { Dashboard } from './views/dashboard';

/** Production composition for Legal inside Macro's split workspace. */
export function Legal(props: { navigation: LegalNavigation }) {
  const workspace = createWorkspace(legalSource, props.navigation);
  const location = () =>
    workspace.creating() ? 'new-envelope' : (workspace.active()?.id ?? 'legal');
  const title = () =>
    workspace.creating()
      ? 'Prepare for signature'
      : (workspace.active()?.title ?? 'All agreements');
  return (
    <LegalWorkspaceContext.Provider value={workspace}>
      <ViewBreadcrumbs.Root
        value={location()}
        onChange={(next) => {
          if (next === 'legal') workspace.close();
        }}
      >
        <ViewBreadcrumbs.Item
          value="legal"
          metadata={{ type: 'legal' }}
          order={0}
        >
          {(item) => (
            <ViewBreadcrumbs.ReturnButton
              isActive={item.isActive()}
              onClick={item.onSelect}
              tooltip="Legal"
            >
              Legal
            </ViewBreadcrumbs.ReturnButton>
          )}
        </ViewBreadcrumbs.Item>
        <Show when={location() !== 'legal'}>
          <ViewBreadcrumbs.Item
            value={location()}
            metadata={{ type: 'legal-envelope' }}
            order={1}
          >
            {(item) => (
              <ViewBreadcrumbs.Button
                isActive={item.isActive()}
                onClick={item.onSelect}
              >
                <span class="truncate">{title()}</span>
              </ViewBreadcrumbs.Button>
            )}
          </ViewBreadcrumbs.Item>
        </Show>
        <SplitPanel.Root data-legal-workspace="">
          <SplitPanel.Body>
            <ViewShell.Root
              asidePreferenceKey="legal"
              resizable
              main={{ preferredWidth: 800 }}
            >
              <ViewShell.Aside>
                <LegalSidebar workspace={workspace} />
              </ViewShell.Aside>
              <ViewShell.Main>
                <ViewShell.TopBar>
                  <ViewBreadcrumbs.Outlet aria-label="Legal location" />
                </ViewShell.TopBar>
                <Show when={workspace.error()}>
                  <div
                    role="alert"
                    class="mx-4 rounded-lg border border-failure px-4 py-3 text-sm text-failure"
                  >
                    {workspace.error()}
                  </div>
                </Show>
                <SplitRouter.Outlet
                  fallback={() => <Dashboard workspace={workspace} />}
                />
              </ViewShell.Main>
            </ViewShell.Root>
          </SplitPanel.Body>
        </SplitPanel.Root>
      </ViewBreadcrumbs.Root>
    </LegalWorkspaceContext.Provider>
  );
}
