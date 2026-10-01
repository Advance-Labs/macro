import KanbanIcon from '@phosphor/kanban.svg';
import TableIcon from '@phosphor/table.svg';
import { Button } from '@ui/components/Button';
import { Dialog } from '@ui/components/Dialog';
import { Panel } from '@ui/components/Panel';
import { TextField } from '@ui/components/TextField';
import type { ResultAsync } from 'neverthrow';
import { createSignal, Show } from 'solid-js';
import type { DatabaseViewColumn } from '../core/database-view';
import { boardGroupColumns } from '../core/views';
import {
  type DatabaseOpFailure,
  databaseOpMessage,
} from '../core/write-failure';
import { ViewSelect } from './view-select';

type NewViewLayout = 'table' | 'board';

/** A view to create: a table, or a board grouped by a single select. */
export type NewView = { name: string } & (
  | { layout: 'table' }
  | { layout: 'board'; groupBy: string }
);

/** Creates a table or board view of the current table, named and, for a board, grouped. */
export function NewViewDialog(props: {
  initialName: string;
  initialLayout?: NewViewLayout;
  columns: DatabaseViewColumn[];
  onSubmit: (view: NewView) => ResultAsync<void, DatabaseOpFailure>;
  onClose: () => void;
  returnFocus?: HTMLElement;
  returnFocusFallback?: HTMLElement;
}) {
  let nameInput: HTMLInputElement | undefined;
  const [name, setName] = createSignal(props.initialName);
  const [layout, setLayout] = createSignal<NewViewLayout>(
    props.initialLayout ?? 'table'
  );
  const groups = () => boardGroupColumns(props.columns);
  const [groupBy, setGroupBy] = createSignal(groups()[0]?.id);
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal('');
  const request = (): NewView | undefined => {
    const trimmed = name().trim();
    if (!trimmed) return undefined;
    if (layout() === 'table') return { name: trimmed, layout: 'table' };
    const group = groupBy();
    return group
      ? { name: trimmed, layout: 'board', groupBy: group }
      : undefined;
  };
  const incomplete = () => !request();
  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    const view = request();
    if (pending() || !view) return;
    setPending(true);
    setError('');
    const created = await props.onSubmit(view);
    setPending(false);
    created.match(props.onClose, (failure) =>
      setError(databaseOpMessage(failure, 'this view'))
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !pending() && props.onClose()}
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        nameInput?.focus();
        nameInput?.select();
      }}
      onCloseAutoFocus={(event) => {
        const target = props.returnFocus?.isConnected
          ? props.returnFocus
          : props.returnFocusFallback;
        if (target?.isConnected) {
          event.preventDefault();
          target.focus();
        }
      }}
      class="w-100 max-w-[calc(100vw-2rem)]"
    >
      <Panel>
        <Panel.Body>
          <form class="flex flex-col gap-5 p-5" onSubmit={submit}>
            <div class="flex flex-col gap-1">
              <Dialog.Title class="text-base font-semibold text-ink">
                New view
              </Dialog.Title>
              <Dialog.Description class="text-sm text-ink-muted">
                A different way to see the same records, shared with everyone
                who can open this database.
              </Dialog.Description>
            </div>
            <div
              class="grid grid-cols-2 gap-2"
              role="group"
              aria-label="View layout"
            >
              <button
                type="button"
                disabled={pending()}
                aria-pressed={layout() === 'table'}
                onClick={() => {
                  setLayout('table');
                  if (name() === 'Board view') setName('Table view');
                }}
                class="flex items-start gap-2.5 rounded-lg border px-3 py-2.5 text-left text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50"
                classList={{
                  'border-edge bg-hover': layout() === 'table',
                  'border-edge-muted': layout() !== 'table',
                }}
              >
                <TableIcon class="mt-0.5 size-4 shrink-0 text-ink-muted" />
                <span class="flex flex-col gap-0.5">
                  <span class="font-medium text-ink">Table</span>
                  <span class="text-xs text-ink-muted">Rows and columns</span>
                </span>
              </button>
              <button
                type="button"
                disabled={pending()}
                aria-pressed={layout() === 'board'}
                onClick={() => {
                  setLayout('board');
                  if (name() === 'Table view') setName('Board view');
                }}
                class="flex items-start gap-2.5 rounded-lg border px-3 py-2.5 text-left text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50"
                classList={{
                  'border-edge bg-hover': layout() === 'board',
                  'border-edge-muted': layout() !== 'board',
                }}
              >
                <KanbanIcon class="mt-0.5 size-4 shrink-0 text-ink-muted" />
                <span class="flex flex-col gap-0.5">
                  <span class="font-medium text-ink">Board</span>
                  <span class="text-xs text-ink-muted">
                    Cards grouped by a column
                  </span>
                </span>
              </button>
            </div>
            <Show when={layout() === 'board'}>
              <div class="flex items-center justify-between gap-3 text-sm">
                <span class="font-medium text-ink">Group by</span>
                <ViewSelect
                  label="Group board by"
                  value={groupBy()}
                  options={groups().map((column) => ({
                    value: column.id,
                    label: column.name,
                  }))}
                  onChange={setGroupBy}
                  placeholder="Choose a column"
                />
              </div>
              <Show when={!groups().length}>
                <p class="text-xs text-ink-muted">
                  Add a Select column to group cards.
                </p>
              </Show>
            </Show>
            <TextField
              value={name()}
              onChange={setName}
              readOnly={pending()}
              required
            >
              <TextField.Label>View name</TextField.Label>
              <TextField.Input
                ref={nameInput}
                maxlength={100}
                onFocus={(event) => event.currentTarget.select()}
                placeholder="e.g. In progress"
              />
            </TextField>
            <Show when={error()}>
              <p role="alert" class="text-xs text-failure">
                {error()}
              </p>
            </Show>
            <div class="flex justify-end gap-2">
              <Button
                variant="ghost"
                disabled={pending()}
                onClick={props.onClose}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                variant="cta"
                disabled={pending() || incomplete()}
              >
                {pending() ? 'Creating…' : 'Create view'}
              </Button>
            </div>
          </form>
        </Panel.Body>
      </Panel>
    </Dialog>
  );
}
