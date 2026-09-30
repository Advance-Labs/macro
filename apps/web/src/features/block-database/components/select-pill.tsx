import { TagDot } from '@property/tags/TagDot';
import { Badge } from '@ui';
import { Show } from 'solid-js';
import type { DatabaseViewColumn } from '../core/database-view';

/** An option value drawn like a task's: tags carry their stored colour. */
export function SelectPill(props: {
  label: string;
  column?: DatabaseViewColumn;
  empty?: boolean;
}) {
  const color = () => props.column?.optionColors?.[props.label];
  const isTag = () => props.column?.dataType === 'TAG';
  return (
    <Badge
      variant="outline"
      size="xs"
      class="min-w-0 max-w-full"
      classList={{ 'text-ink-placeholder': props.empty }}
      title={props.label}
    >
      <Show when={!props.empty && (isTag() || color())}>
        <TagDot color={color()} class="size-2" />
      </Show>
      <span class="truncate">{props.label}</span>
    </Badge>
  );
}
