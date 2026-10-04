import { onCleanup, Show } from 'solid-js';
import {
  type CollabMarkdownControls,
  CollabMarkdownEditor,
  type CollabMarkdownEditorProps,
} from './CollabMarkdownEditor';
import {
  type CollabSurfaceParent,
  createCollabSurfaceSession,
} from './createCollabSurface';

export type CollabMdSurfaceControls = CollabMarkdownControls;
export type CollabMdSurfaceProps = Omit<
  CollabMarkdownEditorProps,
  'sourceId' | 'session'
> & {
  surfaceId: string;
  parent: CollabSurfaceParent;
  initialMarkdown?: string;
  optimisticSnapshot?: Uint8Array;
};

/** Remount the session and editor together when the surface or parent changes. */
export function CollabMdSurface(props: CollabMdSurfaceProps) {
  const identity = () =>
    JSON.stringify([
      props.surfaceId,
      props.parent.entityType,
      props.parent.entityId,
    ]);

  return (
    <Show when={identity()} keyed>
      {(_identity) => <CollabMdSurfaceSession {...props} />}
    </Show>
  );
}

function CollabMdSurfaceSession(props: CollabMdSurfaceProps) {
  const session = createCollabSurfaceSession(props.surfaceId, {
    parent: props.parent,
    initialMarkdown: props.initialMarkdown,
    optimisticSnapshot: props.optimisticSnapshot,
  });
  onCleanup(session.dispose);
  return (
    <CollabMarkdownEditor
      {...props}
      sourceId={props.surfaceId}
      session={session}
    />
  );
}
