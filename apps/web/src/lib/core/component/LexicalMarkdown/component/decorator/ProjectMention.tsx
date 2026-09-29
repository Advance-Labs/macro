import { openProject } from '@app/features/projects/open-project';
import { useProjectIdentityQuery } from '@app/features/projects/queries/project-identity';
import { useSplitLayout } from '@components/app/split-layout/layout';
import { useUserId } from '@core/context/user';
import { openInNewSplitForMention } from '@core/util/openInNewSplit';
import { useNativeSplitNavigationHandler } from '@core/util/useSplitNavigationHandler';
import {
  $isInitiativeMentionNode,
  HISTORIC_TAG,
  type InitiativeMentionDecoratorProps,
  SKIP_DOM_SELECTION_TAG,
  SKIP_SCROLL_INTO_VIEW_TAG,
} from '@macro-inc/lexical-core';
import StackIcon from '@phosphor/stack.svg';
import { cn } from '@ui';
import {
  $getNodeByKey,
  COMMAND_PRIORITY_NORMAL,
  KEY_ENTER_COMMAND,
} from 'lexical';
import { createEffect, useContext } from 'solid-js';
import { LexicalWrapperContext } from '../../context/LexicalWrapperContext';
import { autoRegister } from '../../plugins';
import { MentionTooltip } from './MentionTooltip';

/** A task project chip; the node stores it as an initiative. */
export function ProjectMention(props: InitiativeMentionDecoratorProps) {
  const wrapper = useContext(LexicalWrapperContext);
  const editor = wrapper?.editor;
  const layout = useSplitLayout();
  const userId = useUserId();
  const query = useProjectIdentityQuery(
    () => (wrapper?.skipPreviewFetch ? '' : props.id),
    userId
  );
  // Guard resource reads: a pending chip must never suspend its editor.
  const project = () => (query.isSuccess ? query.data : undefined);
  const label = () => {
    if (query.isError) return 'Private project';
    return project()?.name || props.label || 'Project';
  };

  // Keep the stored label current so plain-text and AI readers see the name.
  createEffect(() => {
    const name = project()?.name;
    if (!editor || !name || name === props.label) return;
    editor.update(
      () => {
        const node = $getNodeByKey(props.key);
        if ($isInitiativeMentionNode(node)) node.setLabel(name);
      },
      {
        tag: [HISTORIC_TAG, SKIP_DOM_SELECTION_TAG, SKIP_SCROLL_INTO_VIEW_TAG],
        discrete: true,
      }
    );
  });

  const selected = () =>
    wrapper?.selection?.type === 'node' &&
    wrapper.selection.nodeKeys.has(props.key);

  const open = (event: MouseEvent | KeyboardEvent | null) => {
    if (!layout || !project()) return;
    openProject(layout, props.id, {
      newSplit: openInNewSplitForMention(event?.shiftKey, event !== null),
    });
  };

  if (editor)
    autoRegister(
      editor.registerCommand(
        KEY_ENTER_COMMAND,
        (event) => {
          if (!selected() || !project()) return false;
          open(event);
          return true;
        },
        COMMAND_PRIORITY_NORMAL
      )
    );

  const navigation = useNativeSplitNavigationHandler<HTMLSpanElement>(
    (event) => {
      event.stopPropagation();
      open(event);
    }
  );

  return (
    <span class="relative">
      <span
        data-initiative-mention="true"
        data-initiative-id={props.id}
        data-initiative-label={label()}
        class={cn(
          'py-0.5 cursor-default rounded-xs hover:bg-hover focus:bg-active pointer-events-auto',
          selected() && 'bg-active text-ink'
        )}
        title={label()}
        {...navigation}
      >
        <span class="relative top-[0.125em] size-[1em] inline-flex mx-1">
          <StackIcon class="size-full text-ink-muted" />
        </span>
        <span class="underline decoration-current/20 decoration-[max(1px,0.1em)] underline-offset-2">
          {label()}
        </span>
      </span>
      <MentionTooltip show={selected()} text="Open" />
    </span>
  );
}
