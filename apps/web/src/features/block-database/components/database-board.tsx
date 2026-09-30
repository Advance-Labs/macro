import { isEditableInput } from '@core/util/isEditableInput';
import CheckIcon from '@phosphor/check.svg';
import GripIcon from '@phosphor/dots-six-vertical.svg';
import DotsIcon from '@phosphor/dots-three.svg';
import PlusIcon from '@phosphor/plus.svg';
import { Key } from '@solid-primitives/keyed';
import { Button } from '@ui/components/Button';
import { Dropdown } from '@ui/components/Dropdown';
import type { JSX } from 'solid-js';
import {
  createMemo,
  createSignal,
  createUniqueId,
  For,
  onCleanup,
  Show,
} from 'solid-js';
import {
  Kanban,
  KanbanCard,
  KanbanCardInsertion,
  KanbanHandle,
  KanbanLane,
} from '../../../components/kanban/kanban';
import {
  boardMoveValue,
  type DatabaseCellValue,
  type DatabaseViewColumn,
  databaseCellValues,
  groupDatabaseRows,
  orderDatabaseCards,
  orderDatabaseColumns,
  orderDatabaseGroups,
} from '../core/database-view';
import {
  type DatabaseRow,
  formatCellValue,
  rowTitle,
  rowValue,
  titleColumn,
} from '../core/table';
import { PropertyIcon } from './property-icon';
import { SelectPill } from './select-pill';

type BoardGroup = ReturnType<typeof groupDatabaseRows<DatabaseRow>>[number];

export type DatabaseCardPlacement = {
  rowId: string;
  value: DatabaseCellValue;
  beforeId?: string;
  toLane: string;
  fromLane: string;
};

function canMoveTo(column: DatabaseViewColumn, value: DatabaseCellValue) {
  return (
    value === null ||
    column.dataType === 'BOOLEAN' ||
    column.options.some((option) => String(option) === String(value))
  );
}

type DatabaseBoardProps = {
  rows: DatabaseRow[];
  columns: DatabaseViewColumn[];
  visibleColumnIds?: string[];
  renderTextValue?: (value: string) => JSX.Element;
  groupColumn: DatabaseViewColumn;
  groupOrder?: string[];
  cardOrder?: Record<string, string[]>;
  onGroupOrderChange?: (order: string[]) => void;
  canEdit: boolean;
  rowPending: (rowId: string) => boolean;
  createPending?: (intentId: string) => boolean;
  createComplete?: (intentId: string) => boolean;
  onOpen: (rowId: string) => void;
  onMove: (rowId: string, value: DatabaseCellValue) => Promise<boolean>;
  onPlace?: (placement: DatabaseCardPlacement) => Promise<boolean>;
  /** Resolves whether the record was saved. `open` asks to show it once it is. */
  onCreate: (
    value: DatabaseCellValue,
    title: string,
    intentId: string,
    options?: { open: true }
  ) => Promise<boolean>;
  onAddGroup?: (label: string) => Promise<void>;
  /** Hands the host a way to start a card, as the toolbar's New record does. */
  controlsRef?: (controls: DatabaseBoardControls) => void;
};

export type DatabaseBoardControls = {
  /** Starts an inline card in the first lane that takes records, unless unmounted. */
  addCard: () => boolean;
};

/** A card being typed into a lane, or saving there, before its record exists. */
type CardDraft = {
  id: string;
  lane: string;
  title: string;
  saving: boolean;
};

/** After Enter, a new card follows; after Shift+Enter, the record opens; after blur, neither. */
type DraftSubmission = 'next' | 'open' | 'stay';

export function DatabaseBoard(props: DatabaseBoardProps) {
  let viewport: HTMLDivElement | undefined;
  const groups = createMemo(() =>
    orderDatabaseGroups(
      groupDatabaseRows(props.rows, props.groupColumn, rowValue).map(
        (group) => ({
          ...group,
          rows: orderDatabaseCards(
            group.rows,
            props.cardOrder?.[group.key],
            (row) => row.rowId
          ),
        })
      ),
      props.groupOrder
    )
  );
  const [announcement, setAnnouncement] = createSignal('');
  const draftPrefix = createUniqueId();
  let draftSequence = 0;
  const [drafts, setDrafts] = createSignal<CardDraft[]>([]);
  const draftElements = new Map<string, HTMLElement>();
  const newButtons = new Map<string, HTMLElement>();
  const titleField = () => titleColumn(props.columns);
  const canSetTitle = () => Boolean(titleField()?.writable);
  const acceptsRecords = (group: BoardGroup) =>
    props.canEdit && canMoveTo(props.groupColumn, group.value);
  const isSaving = (draft: CardDraft) =>
    draft.saving || Boolean(props.createPending?.(draft.id));
  const laneDrafts = (lane: string) =>
    drafts().filter(
      (draft) => draft.lane === lane && !props.createComplete?.(draft.id)
    );
  const updateDraft = (id: string, change: Partial<CardDraft>) =>
    setDrafts((drafts) =>
      drafts.map((draft) => (draft.id === id ? { ...draft, ...change } : draft))
    );
  const removeDraft = (id: string) => {
    draftElements.delete(id);
    setDrafts((drafts) => drafts.filter((draft) => draft.id !== id));
  };
  function startDraft(lane: string) {
    const open = laneDrafts(lane).find(
      (draft) => !isSaving(draft) && !draft.title.trim()
    );
    const id = open?.id ?? `${draftPrefix}:${++draftSequence}`;
    if (!open)
      setDrafts((drafts) => [
        ...drafts.filter((draft) => !props.createComplete?.(draft.id)),
        { id, lane, title: '', saving: false },
      ]);
    draftElements.get(id)?.focus();
  }
  function cancelDraft(id: string, returnFocus: boolean) {
    const draft = drafts().find((draft) => draft.id === id);
    if (!draft || isSaving(draft)) return;
    removeDraft(id);
    if (returnFocus) newButtons.get(draft.lane)?.focus();
  }
  async function submitDraft(id: string, submission: DraftSubmission) {
    const draft = drafts().find((draft) => draft.id === id);
    const group = groups().find((group) => group.key === draft?.lane);
    if (!draft || !group || isSaving(draft)) return;
    const title = draft.title.trim();
    if (canSetTitle() && !title && submission !== 'open') {
      cancelDraft(id, submission === 'next');
      return;
    }
    updateDraft(id, { saving: true });
    if (submission === 'next') startDraft(draft.lane);
    const saved = await props.onCreate(
      group.value,
      title,
      id,
      ...(submission === 'open' ? [{ open: true } as const] : [])
    );
    if (saved) removeDraft(id);
    else updateDraft(id, { saving: false });
  }
  let disposed = false;
  onCleanup(() => {
    disposed = true;
  });
  props.controlsRef?.({
    addCard: () => {
      const lane = disposed ? undefined : groups().find(acceptsRecords);
      if (!lane) return false;
      startDraft(lane.key);
      return true;
    },
  });
  async function move(
    rowId: string,
    value: DatabaseCellValue,
    from?: DatabaseCellValue
  ) {
    const row = props.rows.find((row) => row.rowId === rowId);
    if (
      !row ||
      !props.canEdit ||
      !props.groupColumn.writable ||
      !canMoveTo(props.groupColumn, value) ||
      rowValue(row, props.groupColumn.id) === value
    )
      return;
    if (
      await props.onMove(
        rowId,
        boardMoveValue(
          props.groupColumn,
          rowValue(row, props.groupColumn.id),
          value,
          from
        )
      )
    )
      setAnnouncement(
        `${rowTitle(row, props.columns)} moved to ${value === null ? 'No ' + props.groupColumn.name : String(value)}.`
      );
  }
  function reorder(from: string, to: string, edge?: 'before' | 'after') {
    if (from === to || !props.onGroupOrderChange) return;
    const order = groups().map((group) => group.key);
    const source = order.indexOf(from);
    const target = order.indexOf(to);
    if (source < 0 || target < 0) return;
    order.splice(source, 1);
    const insertion =
      order.indexOf(to) +
      ((edge ?? (source < target ? 'after' : 'before')) === 'after' ? 1 : 0);
    if (insertion === source) return;
    order.splice(insertion, 0, from);
    props.onGroupOrderChange(order);
  }
  return (
    <div
      ref={viewport}
      class="min-h-0 flex-1 overflow-auto px-5 py-5 [&_button:focus-visible]:ring-2 [&_button:focus-visible]:ring-ink/50"
    >
      <p class="sr-only" role="status" aria-live="polite">
        {announcement()}
      </p>
      <Kanban
        getViewport={() => viewport}
        canDropCard={(drop) => {
          const target = groups().find((group) => group.key === drop.toLane);
          return (
            props.rows.some((row) => row.rowId === drop.id) &&
            !!target &&
            props.canEdit &&
            props.groupColumn.writable &&
            canMoveTo(props.groupColumn, target.value) &&
            (drop.fromLane !== drop.toLane || !!props.onPlace)
          );
        }}
        onDrop={(drop) => {
          if (drop.kind === 'lane')
            reorder(drop.fromLane, drop.toLane, drop.edge);
          else {
            const target = groups().find((group) => group.key === drop.toLane);
            const source = groups().find(
              (group) => group.key === drop.fromLane
            );
            const row = props.rows.find((row) => row.rowId === drop.id);
            if (!target || !row) return;
            if (props.onPlace)
              void props.onPlace({
                rowId: drop.id,
                value: boardMoveValue(
                  props.groupColumn,
                  rowValue(row, props.groupColumn.id),
                  target.value,
                  source?.value
                ),
                beforeId: drop.beforeId,
                toLane: drop.toLane,
                fromLane: drop.fromLane,
              });
            else if (drop.fromLane !== drop.toLane)
              void move(drop.id, target.value, source?.value);
          }
        }}
      >
        <div
          class="flex min-h-full min-w-fit items-start gap-4 pb-4"
          aria-label={`Board grouped by ${props.groupColumn.name}`}
        >
          <Key each={groups()} by="key">
            {(group) => (
              <BoardLane
                group={group()}
                {...props}
                groups={groups()}
                drafts={laneDrafts(group().key)}
                canSetTitle={canSetTitle()}
                titlePlaceholder={titleField()?.name ?? 'Record title'}
                acceptsRecords={acceptsRecords(group())}
                isSaving={isSaving}
                onStartDraft={() => startDraft(group().key)}
                onDraftInput={(id, title) => updateDraft(id, { title })}
                onSubmitDraft={(id, submission) =>
                  void submitDraft(id, submission)
                }
                onCancelDraft={cancelDraft}
                draftRef={(id, element) => draftElements.set(id, element)}
                newButtonRef={(element) => newButtons.set(group().key, element)}
                onMove={(rowId, value) =>
                  void move(rowId, value, group().value)
                }
                onReorder={(direction) => {
                  const index = groups().findIndex(
                    (item) => item.key === group().key
                  );
                  const target =
                    groups()[index + (direction === 'left' ? -1 : 1)];
                  if (target) reorder(group().key, target.key);
                }}
              />
            )}
          </Key>
          <Show
            when={
              props.canEdit &&
              props.groupColumn.writable &&
              props.groupColumn.dataType !== 'BOOLEAN' &&
              props.onAddGroup
            }
          >
            <NewBoardGroup
              column={props.groupColumn}
              onSave={async (label) => {
                await props.onAddGroup?.(label);
              }}
            />
          </Show>
        </div>
      </Kanban>
    </div>
  );
}

function BoardLane(
  props: Omit<DatabaseBoardProps, 'onMove'> & {
    group: BoardGroup;
    groups: BoardGroup[];
    drafts: CardDraft[];
    canSetTitle: boolean;
    titlePlaceholder: string;
    acceptsRecords: boolean;
    isSaving: (draft: CardDraft) => boolean;
    onStartDraft: () => void;
    onDraftInput: (id: string, title: string) => void;
    onSubmitDraft: (id: string, submission: DraftSubmission) => void;
    onCancelDraft: (id: string, returnFocus: boolean) => void;
    draftRef: (id: string, element: HTMLElement) => void;
    newButtonRef: (element: HTMLElement) => void;
    onReorder: (direction: 'left' | 'right') => void;
    onMove: (rowId: string, value: DatabaseCellValue) => void;
  }
) {
  const hasOpenDraft = () =>
    props.drafts.some((draft) => !props.isSaving(draft));
  const savingCount = () => props.drafts.filter(props.isSaving).length;
  return (
    <KanbanLane
      id={props.group.key}
      label={`${props.group.label} lane`}
      canReorder={!!props.onGroupOrderChange}
      onKeyDown={(event) => {
        // "n" adds a card to the lane holding focus, as Enter does on its header.
        if (
          event.key !== 'n' ||
          event.ctrlKey ||
          event.metaKey ||
          event.altKey ||
          event.shiftKey ||
          !props.acceptsRecords ||
          !(event.target instanceof Element) ||
          !event.currentTarget.contains(event.target) ||
          isEditableInput(event.target)
        )
          return;
        event.preventDefault();
        props.onStartDraft();
      }}
    >
      <KanbanHandle
        label={
          props.onGroupOrderChange
            ? `Reorder ${props.group.label} lane`
            : `${props.group.label} lane`
        }
        class="mb-2 flex min-h-9 items-center gap-2 rounded px-1.5 outline-none focus-visible:ring-2 focus-visible:ring-ink/50"
        onKeyDown={
          props.onGroupOrderChange || props.acceptsRecords
            ? (event) => {
                if (event.target !== event.currentTarget) return;
                if (
                  props.onGroupOrderChange &&
                  event.altKey &&
                  (event.key === 'ArrowLeft' || event.key === 'ArrowRight')
                ) {
                  event.preventDefault();
                  event.stopPropagation();
                  props.onReorder(event.key === 'ArrowLeft' ? 'left' : 'right');
                } else if (props.acceptsRecords && event.key === 'Enter') {
                  event.preventDefault();
                  props.onStartDraft();
                }
              }
            : undefined
        }
      >
        <SelectPill
          label={props.group.label}
          column={props.groupColumn}
          empty={props.group.value === null}
        />
        <span class="text-xs tabular-nums text-ink-placeholder">
          {props.group.rows.length + savingCount()}
        </span>
        <Show when={props.acceptsRecords}>
          <Button
            size="icon-sm"
            label={`Add record to ${props.group.label}`}
            tooltipDisabled
            class="ml-auto"
            title={`Add record to ${props.group.label}`}
            data-kanban-no-drag
            onClick={props.onStartDraft}
          >
            <PlusIcon class="size-3.5" />
          </Button>
        </Show>
      </KanbanHandle>
      <div class="flex min-h-10 flex-col gap-2">
        <div class="relative flex flex-col gap-2">
          <Key each={props.group.rows} by="rowId">
            {(row) => (
              <BoardCard
                row={row()}
                laneId={props.group.key}
                columns={props.columns}
                visibleColumnIds={props.visibleColumnIds}
                renderTextValue={props.renderTextValue}
                groupColumn={props.groupColumn}
                groups={props.groups}
                canEdit={props.canEdit && props.groupColumn.writable}
                pending={props.rowPending(row().rowId)}
                onOpen={props.onOpen}
                onMove={props.onMove}
              />
            )}
          </Key>
          <KanbanCardInsertion laneId={props.group.key} />
        </div>
        <Show when={props.group.rows.length === 0 && !props.drafts.length}>
          <div class="flex min-h-20 items-center justify-center rounded-lg border border-dashed border-edge-muted/70 px-4 text-xs text-ink-placeholder">
            {props.acceptsRecords ? 'Drop a record here' : 'No records'}
          </div>
        </Show>
        <Key each={props.drafts} by="id">
          {(draft) => (
            <NewBoardCard
              title={draft().title}
              titlePlaceholder={props.titlePlaceholder}
              canSetTitle={props.canSetTitle}
              saving={props.isSaving(draft())}
              ref={(element) => props.draftRef(draft().id, element)}
              onInput={(title) => props.onDraftInput(draft().id, title)}
              onSubmit={(submission) =>
                props.onSubmitDraft(draft().id, submission)
              }
              onCancel={(returnFocus) =>
                props.onCancelDraft(draft().id, returnFocus)
              }
            />
          )}
        </Key>
        <Show when={props.acceptsRecords && !hasOpenDraft()}>
          <Button
            ref={props.newButtonRef}
            variant="plain"
            size="xs"
            aria-label="New record"
            class="h-7 justify-start gap-1.5 rounded-md px-1.5 text-ink-placeholder"
            onClick={props.onStartDraft}
          >
            <PlusIcon class="size-3.5" />
            New
          </Button>
        </Show>
      </div>
    </KanbanLane>
  );
}

function NewBoardGroup(props: {
  column: DatabaseViewColumn;
  onSave: (label: string) => Promise<void>;
}) {
  const [adding, setAdding] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal('');
  let input: HTMLInputElement | undefined;
  const cancel = () => {
    if (!pending()) {
      setAdding(false);
      setError('');
    }
  };
  async function save(event: SubmitEvent) {
    event.preventDefault();
    const entered = draft().trim();
    if (!entered || pending()) return;
    if (
      props.column.dataType === 'SELECT_NUMBER' &&
      !Number.isFinite(Number(entered))
    ) {
      setError('Enter a valid number for this group.');
      return;
    }
    const label =
      props.column.dataType === 'SELECT_NUMBER'
        ? String(Number(entered))
        : entered;
    if (
      props.column.options.some(
        (option) => String(option).toLowerCase() === label.toLowerCase()
      )
    ) {
      setError('A group with this name already exists.');
      return;
    }
    setPending(true);
    setError('');
    try {
      await props.onSave(label);
      setAdding(false);
      setDraft('');
    } catch (error) {
      setError(
        error instanceof Error
          ? error.message
          : 'Could not add this group. Try again.'
      );
    } finally {
      setPending(false);
    }
  }
  return (
    <div class="w-64 shrink-0 pt-2">
      <Show
        when={adding()}
        fallback={
          <Button
            size="xs"
            class="gap-2"
            onClick={() => {
              setDraft('');
              setAdding(true);
              queueMicrotask(() => input?.focus());
            }}
          >
            <PlusIcon class="size-3.5" />
            New group
          </Button>
        }
      >
        <form
          class="rounded-xl border border-edge-muted bg-panel p-3 shadow-sm"
          onSubmit={(event) => void save(event)}
        >
          <p class="mb-2 block text-xs font-medium text-ink-muted">
            New {props.column.name.toLowerCase()} group
          </p>
          <input
            ref={input}
            value={draft()}
            maxlength={200}
            aria-label="New group name"
            placeholder={
              props.column.dataType === 'SELECT_NUMBER'
                ? 'Enter a number…'
                : 'Group name…'
            }
            readOnly={pending()}
            class="w-full rounded-md border border-edge-muted bg-input px-2.5 py-2 text-xs outline-none focus:border-ink/50"
            onInput={(event) => {
              setDraft(event.currentTarget.value);
              setError('');
            }}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                cancel();
              }
            }}
          />
          <Show when={error()}>
            <p role="alert" class="mt-2 text-xs leading-5 text-failure-ink">
              {error()}
            </p>
          </Show>
          <div class="mt-3 flex items-center gap-2">
            <Button
              size="sm"
              type="submit"
              disabled={pending() || !draft().trim()}
            >
              {pending() ? 'Adding…' : 'Add group'}
            </Button>
            <Button size="xs" disabled={pending()} onClick={cancel}>
              Cancel
            </Button>
          </div>
        </form>
      </Show>
    </div>
  );
}

function BoardCard(props: {
  row: DatabaseRow;
  laneId: string;
  columns: DatabaseViewColumn[];
  visibleColumnIds?: string[];
  renderTextValue?: (value: string) => JSX.Element;
  groupColumn: DatabaseViewColumn;
  groups: BoardGroup[];
  canEdit: boolean;
  pending: boolean;
  onOpen: (rowId: string) => void;
  onMove: (rowId: string, value: DatabaseCellValue) => void;
}) {
  const title = () => rowTitle(props.row, props.columns);
  const metadata = () =>
    orderDatabaseColumns(props.columns, props.visibleColumnIds)
      .filter(
        (column) =>
          column.id !== props.groupColumn.id &&
          column.id !== titleColumn(props.columns)?.id &&
          (props.visibleColumnIds === undefined ||
            props.visibleColumnIds.includes(column.id)) &&
          rowValue(props.row, column.id) !== null
      )
      .slice(0, 3);
  return (
    <KanbanCard
      id={props.row.rowId}
      laneId={props.laneId}
      canDrag={props.canEdit}
      pending={props.pending}
    >
      <button
        type="button"
        class="block min-w-0 w-full rounded-lg p-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-ink/50"
        onClick={() => {
          if (!props.pending) props.onOpen(props.row.rowId);
        }}
        aria-label={`Open ${title()}`}
        aria-disabled={props.pending}
      >
        <span
          class="block break-words text-[13px] font-medium leading-5 text-ink"
          classList={{ 'pr-10': props.canEdit }}
        >
          {props.renderTextValue && titleColumn(props.columns)
            ? props.renderTextValue(
                String(
                  rowValue(props.row, titleColumn(props.columns)!.id) ||
                    'Unnamed'
                )
              )
            : title()}
        </span>
        <Show when={metadata().length}>
          <span class="mt-3 flex flex-col gap-2 border-t border-edge-muted/50 pt-2.5">
            <For each={metadata()}>
              {(column) => (
                <span
                  class="flex min-w-0 items-center gap-2"
                  title={`${column.name}: ${formatCellValue(column, rowValue(props.row, column.id))}`}
                >
                  <PropertyIcon
                    type={column.dataType}
                    relation={!!column.relation}
                    class="size-3 shrink-0 text-ink-placeholder"
                  />
                  <Show
                    when={
                      !column.relation &&
                      (column.dataType.startsWith('SELECT_') ||
                        column.dataType === 'TAG')
                    }
                    fallback={
                      <span class="truncate text-xs text-ink-muted">
                        {formatCellValue(
                          column,
                          rowValue(props.row, column.id)
                        )}
                      </span>
                    }
                  >
                    <span class="flex min-w-0 flex-wrap gap-1">
                      <For
                        each={databaseCellValues(
                          rowValue(props.row, column.id),
                          column
                        )}
                      >
                        {(value) => (
                          <SelectPill label={String(value)} column={column} />
                        )}
                      </For>
                    </span>
                  </Show>
                </span>
              )}
            </For>
          </span>
        </Show>
      </button>
      <Show when={props.canEdit}>
        <div class="absolute top-3 right-2 flex items-start gap-0.5">
          <KanbanHandle label={`Drag ${title()}`}>
            <Button
              size="icon-xs"
              label={`Drag ${title()}`}
              tooltipDisabled
              class="touch-none opacity-60 group-hover:opacity-100 focus-visible:opacity-100"
              title="Drag to another group, or use the Move menu"
              tabindex={-1}
            >
              <GripIcon class="size-4" />
            </Button>
          </KanbanHandle>
          <Dropdown>
            <Dropdown.Trigger
              variant="ghost"
              size="icon-xs"
              class="size-5 rounded text-ink-muted"
              aria-label={`Move ${title()}`}
              data-kanban-no-drag
              title="Move to another group"
            >
              <DotsIcon class="size-4" />
            </Dropdown.Trigger>
            <Dropdown.Content class="min-w-44">
              <Dropdown.Group>
                <Dropdown.GroupLabel>Move to</Dropdown.GroupLabel>
                <For each={props.groups}>
                  {(group) => (
                    <Dropdown.Item
                      disabled={!canMoveTo(props.groupColumn, group.value)}
                      onSelect={() =>
                        props.onMove(props.row.rowId, group.value)
                      }
                    >
                      <span class="flex-1">
                        <SelectPill
                          label={group.label}
                          column={props.groupColumn}
                          empty={group.value === null}
                        />
                      </span>
                      <Show when={group.key === props.laneId}>
                        <CheckIcon class="size-3.5" />
                      </Show>
                    </Dropdown.Item>
                  )}
                </For>
              </Dropdown.Group>
            </Dropdown.Content>
          </Dropdown>
        </div>
      </Show>
    </KanbanCard>
  );
}

/** A card-shaped title field: Enter saves it, Escape or leaving it empty drops it. */
function NewBoardCard(props: {
  title: string;
  titlePlaceholder: string;
  canSetTitle: boolean;
  saving: boolean;
  ref: (element: HTMLElement) => void;
  onInput: (title: string) => void;
  onSubmit: (submission: DraftSubmission) => void;
  onCancel: (returnFocus: boolean) => void;
}) {
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      props.onCancel(true);
    } else if (event.key === 'Enter' && !event.isComposing) {
      event.preventDefault();
      props.onSubmit(event.shiftKey ? 'open' : 'next');
    }
  };
  return (
    <div class="rounded-lg border border-edge-muted bg-surface-3 p-3 shadow-sm">
      <Show
        when={!props.saving}
        fallback={
          <div role="status" aria-label="Saving new record">
            <p class="break-words text-[13px] font-medium leading-5 text-ink opacity-70">
              {props.title.trim() || 'Unnamed'}
            </p>
          </div>
        }
      >
        <Show
          when={props.canSetTitle}
          fallback={
            <button
              ref={props.ref}
              type="button"
              class="w-full text-left text-[13px] leading-5 text-ink-placeholder outline-none"
              onClick={() => props.onSubmit('next')}
              onKeyDown={(event) => {
                if (event.key === 'Escape') onKeyDown(event);
              }}
              onBlur={() => props.onCancel(false)}
            >
              Add record
            </button>
          }
        >
          <textarea
            ref={props.ref}
            rows={1}
            aria-label="New record title"
            aria-description="Enter to add, Shift+Enter to add and open, Escape to cancel"
            placeholder={`${props.titlePlaceholder}…`}
            value={props.title}
            maxlength={2000}
            class="block w-full resize-none bg-transparent text-[13px] font-medium leading-5 text-ink outline-none field-sizing-content placeholder:font-normal placeholder:text-ink-placeholder"
            onInput={(event) => props.onInput(event.currentTarget.value)}
            onKeyDown={onKeyDown}
            onBlur={() => {
              // Switching windows is not leaving the card.
              if (!document.hasFocus()) return;
              if (props.title.trim()) props.onSubmit('stay');
              else props.onCancel(false);
            }}
          />
        </Show>
      </Show>
    </div>
  );
}
