import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { enableDatabases } from '@core/constant/featureFlags';
import {
  $createDatabaseQueryNode,
  $isDatabaseQueryNode,
  type DatabaseQueryData,
  type DatabaseQueryDecoratorProps,
} from '@macro-inc/lexical-core/nodes/DatabaseQueryNode';
import { cn } from '@ui';
import {
  $createNodeSelection,
  $createParagraphNode,
  $getNodeByKey,
  $isParagraphNode,
  $setSelection,
} from 'lexical';
import {
  createSignal,
  lazy,
  onCleanup,
  Show,
  Suspense,
  useContext,
} from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { LexicalWrapperContext } from '../../context/LexicalWrapperContext';
import { LazyDecorator } from './LazyDecorator';

const LiveQuestion = lazy(async () => {
  const module = await import('@app/features/database-query/database-query');
  return { default: module.DatabaseLiveQuestion };
});

export function DatabaseQuery(props: DatabaseQueryDecoratorProps) {
  const enabled = useFeatureFlag(enableDatabases);
  const wrapper = useContext(LexicalWrapperContext);
  const [editable, setEditable] = createSignal(
    wrapper?.editor.isEditable() ?? false
  );
  if (wrapper) onCleanup(wrapper.editor.registerEditableListener(setEditable));
  const canEdit = () => editable() && !!wrapper?.isInteractable();
  const isSelectedAsNode = () => {
    const selection = wrapper?.selection;
    return selection?.type === 'node' && selection.nodeKeys.has(props.key);
  };
  // Clicking the block's chrome selects it like an image; its controls, and
  // anything portaled out of it, keep their own clicks.
  const selectOnClick = (event: MouseEvent & { currentTarget: Element }) => {
    if (!wrapper || !editable() || isSelectedAsNode()) return;
    const target = event.target;
    if (!(target instanceof Element) || !event.currentTarget.contains(target))
      return;
    const control = target.closest(
      'button, a, input, select, textarea, summary, [role="button"], [role="menuitem"], [contenteditable="true"]'
    );
    if (control && event.currentTarget.contains(control)) return;
    wrapper.editor.update(() => {
      const selection = $createNodeSelection();
      selection.add(props.key);
      $setSelection(selection);
    });
  };
  const source = (): DatabaseQueryData => ({
    queryId: props.queryId,
    databaseId: props.databaseId,
    tableId: props.tableId,
    prompt: props.prompt,
    title: props.title,
    displayMode: props.displayMode,
    chart: props.chart,
  });
  const save = (data: DatabaseQueryData) => {
    if (!canEdit() || !wrapper) return;
    wrapper.editor.update(() => {
      const node = $getNodeByKey(props.key);
      if (!$isDatabaseQueryNode(node)) return;
      if (node.isInline() && data.displayMode !== 'scalar') {
        const parent = node.getParent();
        if ($isParagraphNode(parent) && parent.getChildrenSize() === 1)
          parent.replace($createDatabaseQueryNode(data));
        else {
          node
            .getTopLevelElementOrThrow()
            .insertAfter($createDatabaseQueryNode(data));
          node.remove();
        }
      } else if (!node.isInline() && data.displayMode === 'scalar') {
        node.replace(
          $createParagraphNode().append($createDatabaseQueryNode(data))
        );
      } else node.setQuery(data);
    });
  };
  const placeholder = () => (
    <span class="mx-0.5 rounded border border-edge-muted bg-hover px-1.5 py-0.5 text-sm text-ink">
      {props.title || props.chart?.title || 'Database answer'}
    </span>
  );
  return (
    <Show
      when={enabled().enabled && !wrapper?.skipPreviewFetch}
      fallback={placeholder()}
    >
      <Dynamic
        component={props.displayMode === 'scalar' ? 'span' : 'div'}
        data-database-query-selected={isSelectedAsNode() || undefined}
        class={cn('rounded-lg', isSelectedAsNode() && 'ring-3 ring-edge-muted')}
        onClick={props.displayMode === 'scalar' ? undefined : selectOnClick}
      >
        <LazyDecorator
          placeholder={placeholder()}
          render={() => (
            <Suspense fallback={placeholder()}>
              <LiveQuestion
                source={source()}
                onSave={canEdit() ? save : undefined}
              />
            </Suspense>
          )}
        />
      </Dynamic>
    </Show>
  );
}
