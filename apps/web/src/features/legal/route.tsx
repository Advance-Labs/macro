import {
  defineRoute,
  useNavigate,
  useRouteParams,
} from '@app/lib/split-router';
import { useSplitPanelOrThrow } from '@components/app/split-layout/layoutUtils';
import {
  AppView,
  withAuth,
} from '@components/app/split-layout/split-router/app-route-shell';
import { lazy, onMount, Show } from 'solid-js';
import { z } from 'zod';
import { useLegalWorkspace } from './context/legal-workspace';
import { Editor } from './views/editor';
import { EnvelopeDetail } from './views/envelope-detail';

const Legal = lazy(async () => ({ default: (await import('./legal')).Legal }));

export const LegalRouteView = withAuth(() => {
  const panel = useSplitPanelOrThrow();
  const navigate = useNavigate();
  onMount(() => panel.handle.setDisplayName('Legal'));
  return (
    <AppView id="legal">
      <Legal
        navigation={{
          start: () => navigate({ route: legalNewEnvelopeRoute, params: {} }),
          open: (envelopeId) =>
            navigate({ route: legalEnvelopeRoute, params: { envelopeId } }),
          close: () => navigate({ route: legalRoute, params: {} }),
        }}
      />
    </AppView>
  );
});

function NewEnvelopeRouteView() {
  const workspace = useLegalWorkspace();
  onMount(workspace.begin);
  return <Editor workspace={workspace} />;
}

function EnvelopeRouteView() {
  const workspace = useLegalWorkspace();
  const params = useRouteParams(legalEnvelopeRoute);
  onMount(() => workspace.load(params.envelopeId));
  return (
    <Show
      when={workspace.active()?.id === params.envelopeId}
      fallback={
        <div class="grid flex-1 place-items-center text-sm text-ink-muted">
          {workspace.busy() ? 'Loading agreement…' : 'Agreement unavailable'}
        </div>
      }
    >
      <Show
        when={workspace.creating()}
        fallback={<EnvelopeDetail workspace={workspace} />}
      >
        <Editor workspace={workspace} />
      </Show>
    </Show>
  );
}

export const legalNewEnvelopeRoute = defineRoute({
  id: 'legal-new-envelope',
  path: 'new',
  component: NewEnvelopeRouteView,
});
export const legalEnvelopeRoute = defineRoute({
  id: 'legal-envelope',
  path: ':envelopeId',
  params: z.object({ envelopeId: z.string().uuid() }),
  component: EnvelopeRouteView,
  remountKey: ({ envelopeId }) => envelopeId,
  claim: ({ envelopeId }) => ({
    namespace: 'component',
    id: `legal-envelope:${envelopeId}`,
  }),
});
export const legalRoute = defineRoute({
  id: 'view-legal',
  path: 'legal',
  component: LegalRouteView,
  search: '*' as const,
  claim: () => ({ namespace: 'component', id: 'legal' }),
  children: [legalNewEnvelopeRoute, legalEnvelopeRoute],
});
