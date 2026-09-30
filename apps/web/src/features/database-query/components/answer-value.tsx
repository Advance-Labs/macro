import { SelectPill } from '@app/features/block-database/components/select-pill';
import type { DatabaseViewColumn } from '@app/features/block-database/core/database-view';
import { For, Match, Switch } from 'solid-js';
import { useAnswerDisplay } from '../context/answer-display';
import type { ResultCell } from '../core/answer-cell';

/** One result value, drawn with the same pieces as the database grid's cells. */
export function ResultValue(props: {
  cell: ResultCell;
  column?: DatabaseViewColumn;
}) {
  const display = useAnswerDisplay();
  return (
    <Switch>
      <Match when={props.cell.kind === 'empty'}>
        <span class="opacity-40">—</span>
      </Match>
      <Match when={props.cell.kind === 'text' && props.cell}>
        {(cell) => cell().text}
      </Match>
      <Match when={props.cell.kind === 'markdown' && props.cell}>
        {(cell) => display.text(cell().markdown)}
      </Match>
      <Match when={props.cell.kind === 'boolean' && props.cell}>
        {(cell) => (
          <input
            type="checkbox"
            checked={cell().checked}
            disabled
            aria-label={cell().checked ? 'True' : 'False'}
            class="size-3.5 rounded border-edge-muted align-middle accent-ink disabled:opacity-50"
          />
        )}
      </Match>
      <Match when={props.cell.kind === 'options' && props.cell}>
        {(cell) => (
          <span class="inline-flex min-w-0 max-w-full flex-wrap gap-1 align-middle">
            <For each={cell().labels}>
              {(label) => <SelectPill label={label} column={props.column} />}
            </For>
          </span>
        )}
      </Match>
      <Match when={props.cell.kind === 'mentions' && props.cell}>
        {(cell) => (
          <span class="inline-flex min-w-0 max-w-full flex-wrap gap-x-2 gap-y-1 align-middle">
            <For each={cell().ids}>
              {(id) => display.mention(id, cell().entityType)}
            </For>
          </span>
        )}
      </Match>
    </Switch>
  );
}
