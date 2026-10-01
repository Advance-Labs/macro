import { ContextMenuContent, MenuItem } from '@core/component/ContextMenu';
import { ContextMenu } from '@kobalte/core/context-menu';
import FunnelIcon from '@phosphor/funnel.svg';
import KanbanIcon from '@phosphor/kanban.svg';
import MagnifyingGlassIcon from '@phosphor/magnifying-glass.svg';
import PlusIcon from '@phosphor/plus.svg';
import SlidersHorizontalIcon from '@phosphor/sliders-horizontal.svg';
import SortAscendingIcon from '@phosphor/sort-ascending.svg';
import TableIcon from '@phosphor/table.svg';
import XIcon from '@phosphor/x.svg';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import { Key } from '@solid-primitives/keyed';
import {
  closestCenter,
  createSortable,
  DragDropProvider,
  DragDropSensors,
  SortableProvider,
} from '@thisbeyond/solid-dnd';
import { Button } from '@ui/components/Button';
import { DeleteDialog } from '@ui/components/DeleteDialog';
import { InputGroup } from '@ui/components/InputGroup';
import { ToggleSwitch } from '@ui/components/ToggleSwitch';
import { Tooltip } from '@ui/components/Tooltip';
import type { ResultAsync } from 'neverthrow';
import { createSignal, For, Index, type JSX, Show } from 'solid-js';
import type { DatabaseViewColumn } from '../core/database-view';
import { withSort } from '../core/view-query';
import {
  boardGroupColumns,
  boardLayout,
  laneLabel,
  layoutColumns,
  movedViewOrder,
  withLaneHidden,
  withLayoutColumn,
} from '../core/views';
import {
  type DatabaseOpFailure,
  databaseOpMessage,
} from '../core/write-failure';
import type { ViewChange } from '../queries/views';
import { FilterPanel, filterConditionCount } from './database-view-filters';
import { createInlineRename } from './inline-rename';
import { type NewView, NewViewDialog } from './new-view-dialog';
import { ToolbarPopover } from './view-control-popover';
import { ViewSelect } from './view-select';

type DatabaseToolbarProps = {
  columns: DatabaseViewColumn[];
  /** The table's stored views, in order. */
  views: DatabaseView[];
  /** The view on screen: a stored one, or the table's own All records. */
  view: DatabaseView;
  selectedViewId?: string;
  canEdit: boolean;
  search: string;
  onSearchChange: (search: string) => void;
  onSelectView: (id?: string) => void;
  onChangeView: (change: ViewChange) => void;
  onCreateView: (view: NewView) => ResultAsync<void, DatabaseOpFailure>;
  onRenameView: (
    view: DatabaseView,
    name: string
  ) => ResultAsync<void, DatabaseOpFailure>;
  onDeleteView: (view: DatabaseView) => ResultAsync<void, DatabaseOpFailure>;
  /** Every stored view of the table, in its new order. */
  onReorderViews: (order: string[]) => void;
  addColumn?: JSX.Element;
  onCreateRecord?: () => void;
  canCreateRecord?: boolean;
  creating?: boolean;
};

/** View controls contain no data fetching or mutation implementation. */
export function DatabaseToolbar(props: DatabaseToolbarProps) {
  const [creating, setCreating] = createSignal<HTMLElement>();
  const [deleting, setDeleting] = createSignal<{ view: DatabaseView }>();
  const [searchOpen, setSearchOpen] = createSignal(false);
  let searchButton: HTMLButtonElement | undefined;
  let searchInput: HTMLInputElement | undefined;
  let renameInput: HTMLInputElement | undefined;
  let allRecordsButton: HTMLButtonElement | undefined;
  let viewRail: HTMLDivElement | undefined;
  /** Viewers change only what All records shows them; stored views are everyone's. */
  const canChangeView = () => !props.selectedViewId || props.canEdit;
  const layout = () => props.view.layout;
  const board = () => {
    const current = layout();
    return current.kind === 'board' ? current : undefined;
  };
  const sort = () => props.view.query.sort ?? [];
  const viewRename = createInlineRename({
    name: (view: DatabaseView) => view.name,
    rename: (view, name) => props.onRenameView(view, name),
    failureMessage: (failure: DatabaseOpFailure) =>
      databaseOpMessage(failure, 'this view'),
    emptyName: { message: 'Enter a view name.', onBlur: 'keep-editing' },
    input: () => renameInput,
    // The tab is drawn again when the rename input goes, so focus finds it by its view.
    restoreFocus: (view) =>
      viewRail
        ?.querySelector<HTMLElement>(`[data-view-id="${view.id}"]`)
        ?.focus(),
  });
  const viewError = viewRename.error;
  const setViewError = viewRename.setError;
  function reorder(id: string, targetId: string) {
    const order = props.views.map((view) => view.id);
    const moved = movedViewOrder(order, id, targetId);
    if (moved.some((view, index) => view !== order[index]))
      props.onReorderViews(moved);
  }
  const groupColumn = () =>
    props.columns.find((column) => column.id === board()?.groupBy);
  const hiddenLanes = () => {
    const column = groupColumn();
    if (!column) return [];
    return (board()?.lanes ?? [])
      .filter((lane) => lane.hidden)
      .map((lane) => ({
        option: lane.option,
        label: laneLabel(column, lane.option),
      }));
  };
  return (
    <div
      class="@container/view-toolbar shrink-0 border-b border-edge-muted bg-canvas-base [&_button:focus-visible]:ring-2 [&_button:focus-visible]:ring-ink/50"
      data-database-toolbar
    >
      <div class="flex items-center gap-2 px-3 py-1.5 @max-[520px]/view-toolbar:flex-wrap @min-[640px]/view-toolbar:px-4">
        <div class="flex min-w-0 flex-1 items-center gap-0.5 @max-[520px]/view-toolbar:basis-full">
          <span class="mr-1.5 shrink-0 text-[10px] text-ink-placeholder @max-[640px]/view-toolbar:sr-only">
            Views
          </span>
          <div
            ref={viewRail}
            class="flex min-w-0 items-center gap-0.5 overflow-x-auto"
            aria-label="Views"
          >
            <button
              ref={allRecordsButton}
              type="button"
              aria-pressed={!props.selectedViewId}
              onClick={() => props.onSelectView()}
              class="flex h-8 max-w-40 shrink-0 items-center gap-1.5 rounded-md px-2 text-xs text-ink-muted outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50"
              classList={{
                'bg-hover font-medium text-ink': !props.selectedViewId,
              }}
            >
              <TableIcon class="size-3.5" />
              <span class="truncate">All records</span>
            </button>
            <DragDropProvider
              collisionDetector={closestCenter}
              onDragEnd={({ draggable, droppable }) => {
                if (droppable)
                  reorder(String(draggable.id), String(droppable.id));
              }}
            >
              <DragDropSensors />
              <SortableProvider ids={props.views.map((view) => view.id)}>
                <Key each={props.views} by="id">
                  {(view) => (
                    <ViewTab
                      view={view()}
                      selected={props.selectedViewId === view().id}
                      canEdit={props.canEdit}
                      renaming={viewRename.target()?.id === view().id}
                      renameDraft={viewRename.draft()}
                      renamePending={viewRename.pending()}
                      renameInput={(element) => {
                        renameInput = element;
                      }}
                      onRenameInput={viewRename.setDraft}
                      onRenameSave={(restoreFocus) =>
                        void viewRename.save(restoreFocus)
                      }
                      onRenameCancel={() => viewRename.cancel(true)}
                      onSelect={() => props.onSelectView(view().id)}
                      onRename={() => {
                        if (props.canEdit) viewRename.begin(view());
                      }}
                      onDelete={() => {
                        setViewError('');
                        setDeleting({ view: view() });
                      }}
                    />
                  )}
                </Key>
              </SortableProvider>
            </DragDropProvider>
          </div>
          <Show when={props.canEdit}>
            <Button
              size="icon-sm"
              label="New view"
              class="shrink-0"
              onClick={(event) => setCreating(event.currentTarget)}
            >
              <PlusIcon class="size-3.5" />
            </Button>
          </Show>
        </div>
        <div class="flex shrink-0 items-center gap-0.5 @max-[520px]/view-toolbar:w-full @max-[520px]/view-toolbar:justify-end">
          <Show when={canChangeView()}>
            <ToolbarPopover
              label="Filter"
              compact
              count={filterConditionCount(props.view.query.filter)}
              icon={<FunnelIcon class="size-3.5" />}
            >
              <FilterPanel
                columns={props.columns}
                filter={props.view.query.filter}
                onChange={(filter) =>
                  props.onChangeView({
                    query: { ...props.view.query, filter },
                  })
                }
              />
            </ToolbarPopover>
            <ToolbarPopover
              label="Sort"
              compact
              count={sort().length}
              icon={<SortAscendingIcon class="size-3.5" />}
            >
              <SortPanel
                columns={props.columns}
                view={props.view}
                onChange={props.onChangeView}
              />
            </ToolbarPopover>
          </Show>
          <Show
            when={searchOpen() || props.search}
            fallback={
              <button
                ref={searchButton}
                type="button"
                aria-label="Search"
                title="Search records"
                onClick={() => setSearchOpen(true)}
                class="flex size-8 shrink-0 items-center justify-center rounded-md text-ink-muted outline-none hover:bg-hover hover:text-ink focus-visible:ring-2 focus-visible:ring-ink/50"
              >
                <MagnifyingGlassIcon class="size-3.5" />
              </button>
            }
          >
            <InputGroup
              size="sm"
              class="w-32 @min-[640px]/view-toolbar:w-44"
              onFocusOut={(event) => {
                const next = event.relatedTarget;
                if (next instanceof Node && event.currentTarget.contains(next))
                  return;
                if (!props.search) setSearchOpen(false);
              }}
            >
              <InputGroup.Addon align="inline-start">
                <MagnifyingGlassIcon class="size-3.5" />
              </InputGroup.Addon>
              <InputGroup.Input
                ref={(element) => {
                  searchInput = element;
                  if (searchOpen()) queueMicrotask(() => element.focus());
                }}
                type="search"
                aria-label="Search records"
                placeholder="Search…"
                value={props.search}
                onInput={(event) =>
                  props.onSearchChange(event.currentTarget.value)
                }
                onKeyDown={(event) => {
                  if (event.key !== 'Escape') return;
                  event.preventDefault();
                  event.stopPropagation();
                  props.onSearchChange('');
                  setSearchOpen(false);
                  queueMicrotask(() => searchButton?.focus());
                }}
                class="text-xs"
              />
              <InputGroup.Addon align="inline-end">
                <InputGroup.Button
                  size="icon-xs"
                  label={props.search ? 'Clear search' : 'Close search'}
                  tooltipDisabled
                  onClick={() => {
                    if (props.search) {
                      setSearchOpen(true);
                      props.onSearchChange('');
                      searchInput?.focus();
                    } else {
                      setSearchOpen(false);
                      queueMicrotask(() => searchButton?.focus());
                    }
                  }}
                >
                  <XIcon class="size-3" />
                </InputGroup.Button>
              </InputGroup.Addon>
            </InputGroup>
          </Show>
          <Show when={canChangeView()}>
            <ToolbarPopover
              label="View settings"
              compact
              icon={<SlidersHorizontalIcon class="size-3.5" />}
            >
              <div class="w-64 max-w-full">
                <Show when={props.selectedViewId}>
                  <p class="mb-2 text-xs font-medium">Layout</p>
                  <div
                    class="flex items-center gap-1 rounded-lg border border-edge-muted p-1"
                    aria-label="View layout"
                  >
                    <LayoutButton
                      label="Table"
                      icon={<TableIcon class="size-4" />}
                      pressed={layout().kind === 'table'}
                      onClick={() => {
                        if (layout().kind !== 'table')
                          props.onChangeView({
                            layout: { kind: 'table', columns: [] },
                          });
                      }}
                    />
                    <LayoutButton
                      label="Board"
                      icon={<KanbanIcon class="size-4" />}
                      pressed={layout().kind === 'board'}
                      disabled={!boardGroupColumns(props.columns).length}
                      onClick={() => {
                        const [first] = boardGroupColumns(props.columns);
                        if (first && layout().kind !== 'board')
                          props.onChangeView({
                            layout: boardLayout(first.id, props.columns),
                          });
                      }}
                    />
                  </div>
                </Show>
                <Show when={board()}>
                  {(current) => (
                    <>
                      <label class="mt-3 flex items-center justify-between gap-3 text-xs text-ink-muted">
                        Group by
                        <ViewSelect
                          label="Group board by"
                          value={current().groupBy}
                          options={boardGroupColumns(props.columns).map(
                            (column) => ({
                              value: column.id,
                              label: column.name,
                            })
                          )}
                          onChange={(groupBy) =>
                            props.onChangeView({
                              layout: { ...current(), groupBy, lanes: [] },
                            })
                          }
                        />
                      </label>
                      <ToggleSwitch
                        class="mt-2 flex w-full flex-row-reverse justify-between gap-2.5 rounded-lg px-2 py-2 text-xs hover:bg-hover"
                        label="Hide empty lanes"
                        checked={current().hideEmptyLanes}
                        onChange={(hideEmptyLanes) =>
                          props.onChangeView({
                            layout: { ...current(), hideEmptyLanes },
                          })
                        }
                      />
                      <Show when={hiddenLanes().length}>
                        <p class="mt-2 px-2 text-xs font-medium">
                          Hidden lanes
                        </p>
                        <For each={hiddenLanes()}>
                          {(lane) => (
                            <div class="flex items-center justify-between px-2 py-1 text-xs">
                              <span class="truncate">{lane.label}</span>
                              <Button
                                size="xs"
                                onClick={() =>
                                  props.onChangeView({
                                    layout: withLaneHidden(
                                      current(),
                                      lane.option,
                                      false
                                    ),
                                  })
                                }
                              >
                                Show
                              </Button>
                            </div>
                          )}
                        </For>
                      </Show>
                    </>
                  )}
                </Show>
              </div>
              <div class="mt-3 max-h-60 w-64 max-w-full overflow-auto border-t border-edge-muted pt-3">
                <p class="px-2 pb-2 text-xs font-medium text-ink">
                  {board() ? 'Card fields' : 'Columns'}
                </p>
                <Show
                  when={board()}
                  fallback={
                    <For each={layoutColumns(layout(), props.columns)}>
                      {(entry) => (
                        <ColumnSwitch
                          name={entry.column.name}
                          checked={!entry.hidden}
                          onChange={(visible) =>
                            props.onChangeView({
                              layout: withLayoutColumn(
                                layout(),
                                props.columns,
                                entry.column.id,
                                { hidden: !visible }
                              ),
                            })
                          }
                        />
                      )}
                    </For>
                  }
                >
                  {(current) => (
                    <For
                      each={props.columns.filter(
                        (column) => column.id !== current().groupBy
                      )}
                    >
                      {(column) => (
                        <ColumnSwitch
                          name={column.name}
                          checked={current().cardFields.includes(column.id)}
                          onChange={(shown) =>
                            props.onChangeView({
                              layout: {
                                ...current(),
                                cardFields: shown
                                  ? [...current().cardFields, column.id]
                                  : current().cardFields.filter(
                                      (id) => id !== column.id
                                    ),
                              },
                            })
                          }
                        />
                      )}
                    </For>
                  )}
                </Show>
                <Show when={!props.columns.length}>
                  <p class="text-xs text-ink-muted">
                    Add a column to get started.
                  </p>
                </Show>
              </div>
              <Show
                when={layoutColumns(layout(), props.columns).some(
                  (entry) => entry.hidden
                )}
              >
                <Button
                  size="sm"
                  variant="ghost"
                  class="mt-2"
                  onClick={() => {
                    const current = layout();
                    if (current.kind !== 'table') return;
                    props.onChangeView({
                      layout: {
                        ...current,
                        columns: current.columns.map((entry) => ({
                          ...entry,
                          hidden: false,
                        })),
                      },
                    });
                  }}
                >
                  Show all columns
                </Button>
              </Show>
              <Show when={props.addColumn}>
                <div class="mt-2 border-t border-edge-muted pt-3">
                  {props.addColumn}
                </div>
              </Show>
            </ToolbarPopover>
          </Show>
          <div class="ml-1 flex shrink-0 items-center">
            <Show when={props.onCreateRecord && board()}>
              <Button
                size="sm"
                variant="outline"
                class="h-8 gap-1.5 px-2.5 text-xs"
                aria-label={props.creating ? 'Saving record' : 'New record'}
                disabled={!props.canCreateRecord || props.creating}
                onClick={() => props.onCreateRecord?.()}
              >
                <PlusIcon class="size-3.5" />
                <Show when={!props.creating} fallback="Saving…">
                  New
                </Show>
              </Button>
            </Show>
          </div>
        </div>
      </div>
      <Show when={viewError()}>
        <p role="alert" class="px-4 pb-2 text-xs text-failure">
          {viewError()}
        </p>
      </Show>
      <Show when={creating()} keyed>
        {(origin) => (
          <NewViewDialog
            initialName="Table view"
            columns={props.columns}
            returnFocus={origin}
            returnFocusFallback={allRecordsButton}
            onClose={() => setCreating(undefined)}
            onSubmit={props.onCreateView}
          />
        )}
      </Show>
      <DeleteDialog
        open={!!deleting()}
        onOpenChange={(open) => {
          if (!open) setDeleting(undefined);
        }}
        title="Delete view?"
        deleteLabel="Delete view"
        onDelete={() => {
          const target = deleting();
          if (!target) return;
          setDeleting(undefined);
          void props
            .onDeleteView(target.view)
            .mapErr((failure) =>
              setViewError(databaseOpMessage(failure, 'this view'))
            );
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          allRecordsButton?.focus();
        }}
        body={
          <p class="break-words">
            “{deleting()?.view.name}” will be deleted for everyone. The table
            and its records stay.
          </p>
        }
      />
    </div>
  );
}

function ViewTab(props: {
  view: DatabaseView;
  selected: boolean;
  canEdit: boolean;
  renaming: boolean;
  renameDraft: string;
  renamePending: boolean;
  renameInput: (element: HTMLInputElement) => void;
  onRenameInput: (name: string) => void;
  onRenameSave: (restoreFocus: boolean) => void;
  onRenameCancel: () => void;
  onSelect: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const sortable = createSortable(props.view.id);
  return (
    <div
      ref={sortable.ref}
      class="shrink-0"
      classList={{ 'opacity-40': sortable.isActiveDraggable }}
      style={
        sortable.transform.x
          ? { transform: `translateX(${sortable.transform.x}px)` }
          : undefined
      }
    >
      <Show
        when={!props.renaming}
        fallback={
          <form
            onSubmit={(event) => {
              event.preventDefault();
              props.onRenameSave(true);
            }}
          >
            <input
              ref={props.renameInput}
              aria-label="View name"
              maxlength={100}
              value={props.renameDraft}
              readOnly={props.renamePending}
              class="h-8 w-36 rounded-md border border-ink/40 bg-input px-2 text-xs text-ink outline-none focus:ring-2 focus:ring-ink/10"
              onInput={(event) =>
                props.onRenameInput(event.currentTarget.value)
              }
              onBlur={() => props.onRenameSave(false)}
              onKeyDown={(event) => {
                if (event.key !== 'Escape') return;
                event.preventDefault();
                event.stopPropagation();
                props.onRenameCancel();
              }}
            />
          </form>
        }
      >
        <ContextMenu>
          <Tooltip label={props.view.name}>
            <ContextMenu.Trigger
              as="button"
              type="button"
              {...(props.canEdit ? sortable.dragActivators : {})}
              data-view-id={props.view.id}
              aria-pressed={props.selected}
              aria-keyshortcuts={props.canEdit ? 'F2 Shift+F10' : undefined}
              onClick={props.onSelect}
              onDblClick={(event: MouseEvent) => {
                event.preventDefault();
                props.onRename();
              }}
              onKeyDown={(event: KeyboardEvent) => {
                if (event.key === 'F2') {
                  event.preventDefault();
                  props.onRename();
                }
              }}
              class="flex h-8 max-w-40 items-center gap-1.5 rounded-md px-2 text-xs text-ink-muted outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50"
              classList={{ 'bg-hover font-medium text-ink': props.selected }}
            >
              <Show
                when={props.view.layout.kind === 'board'}
                fallback={<TableIcon class="size-3.5 shrink-0" />}
              >
                <KanbanIcon class="size-3.5 shrink-0" />
              </Show>
              <span class="truncate">{props.view.name}</span>
            </ContextMenu.Trigger>
          </Tooltip>
          <Show when={props.canEdit}>
            <ContextMenu.Portal>
              <ContextMenuContent class="min-w-48">
                <MenuItem
                  text="Rename view"
                  closeOnSelect
                  shortcut="F2"
                  onClick={props.onRename}
                />
                <MenuItem
                  text="Delete view"
                  closeOnSelect
                  onClick={props.onDelete}
                />
              </ContextMenuContent>
            </ContextMenu.Portal>
          </Show>
        </ContextMenu>
      </Show>
    </div>
  );
}

function LayoutButton(props: {
  label: string;
  icon: JSX.Element;
  pressed: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={props.pressed}
      disabled={props.disabled}
      onClick={props.onClick}
      class="flex h-9 flex-1 items-center justify-center gap-2 rounded-md text-xs text-ink-muted outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50 disabled:opacity-50"
      classList={{ 'bg-hover font-medium text-ink': props.pressed }}
    >
      {props.icon}
      {props.label}
    </button>
  );
}

function ColumnSwitch(props: {
  name: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <ToggleSwitch
      class="flex w-full flex-row-reverse justify-between gap-2.5 rounded-lg px-2 py-2 text-xs hover:bg-hover"
      labelClass="min-w-0 flex-1 truncate"
      label={<span title={props.name}>{props.name}</span>}
      checked={props.checked}
      onChange={props.onChange}
    />
  );
}

function SortPanel(props: {
  columns: DatabaseViewColumn[];
  view: DatabaseView;
  onChange: (change: ViewChange) => void;
}) {
  const sort = () => props.view.query.sort ?? [];
  const setSort = (next: NonNullable<DatabaseView['query']['sort']>) =>
    props.onChange({ query: { ...props.view.query, sort: next } });
  return (
    <div class="w-76 max-w-full">
      <Show when={!sort().length}>
        <p class="mb-3 text-xs text-ink-muted">
          Choose the order records appear in.
        </p>
      </Show>
      <div class="flex flex-col gap-2">
        <Index each={sort()}>
          {(key, index) => (
            <div class="flex gap-1.5">
              <ViewSelect
                label="Sort property"
                value={key().column}
                options={props.columns
                  .filter(
                    (column) =>
                      !column.relation &&
                      (column.id === key().column ||
                        !sort().some((item) => item.column === column.id))
                  )
                  .map((column) => ({ value: column.id, label: column.name }))}
                onChange={(column) =>
                  setSort(
                    sort().map((item, position) =>
                      position === index ? { ...item, column } : item
                    )
                  )
                }
              />
              <ViewSelect
                label="Sort direction"
                value={key().direction}
                class="w-28"
                options={[
                  { value: 'ascending', label: 'Ascending' },
                  { value: 'descending', label: 'Descending' },
                ]}
                onChange={(direction) =>
                  setSort(
                    sort().map((item, position) =>
                      position === index
                        ? {
                            ...item,
                            direction,
                          }
                        : item
                    )
                  )
                }
              />
              <Button
                size="icon-sm"
                label="Remove sort"
                tooltipDisabled
                onClick={() => setSort(withSort(sort(), key().column, null))}
              >
                <XIcon class="size-3.5" />
              </Button>
            </div>
          )}
        </Index>
      </div>
      <Button
        size="sm"
        variant="ghost"
        class="mt-3"
        disabled={
          !props.columns.some(
            (column) =>
              !column.relation &&
              !sort().some((key) => key.column === column.id)
          )
        }
        onClick={() => {
          const column = props.columns.find(
            (item) =>
              !item.relation && !sort().some((key) => key.column === item.id)
          );
          if (column)
            setSort([...sort(), { column: column.id, direction: 'ascending' }]);
        }}
      >
        <PlusIcon class="size-3.5" /> Add sort
      </Button>
    </div>
  );
}
