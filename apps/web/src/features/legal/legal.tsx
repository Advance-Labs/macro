import { Show } from 'solid-js';
import { createWorkspace } from './primitives/workspace';
import { legalSource } from './queries/source';
import { Dashboard } from './views/dashboard';
import { Editor } from './views/editor';
import { EnvelopeDetail } from './views/envelope-detail';
export function Legal() {
  const workspace = createWorkspace(legalSource);
  return (
    <div class="h-full relative">
      <Show when={workspace.error()}>
        <div
          role="alert"
          class="absolute z-20 top-2 left-1/2 -translate-x-1/2 max-w-lg px-4 py-3 rounded-lg border border-failure bg-surface text-failure text-sm shadow-lg"
        >
          {workspace.error()}
        </div>
      </Show>
      <Show
        when={workspace.creating()}
        fallback={
          <Show
            when={workspace.active()}
            fallback={<Dashboard workspace={workspace} />}
          >
            <EnvelopeDetail workspace={workspace} />
          </Show>
        }
      >
        <Editor workspace={workspace} />
      </Show>
    </div>
  );
}
