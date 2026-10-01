import ArrowClockwiseIcon from '@phosphor/arrow-clockwise.svg';
import EyeSlashIcon from '@phosphor/eye-slash.svg';
import WarningIcon from '@phosphor/warning-circle.svg';
import XIcon from '@phosphor/x.svg';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import { until } from '@solid-primitives/promise';
import { Button } from '@ui/components/Button';
import { DeleteDialog } from '@ui/components/DeleteDialog';
import { Mutex } from 'async-mutex';
import { ok, okAsync } from 'neverthrow';
import {
  type Accessor,
  createMemo,
  createSignal,
  For,
  type JSX,
  onCleanup,
  Show,
  untrack,
} from 'solid-js';
import {
  GridCell,
  type GridCellEditorOptions,
  type GridCellProps,
} from '../component/GridCell';
import type { DatabaseBoardControls } from '../components/database-board';
import {
  type DatabaseCellFocus,
  type DatabaseCellPresence,
  DatabaseTable,
} from '../components/database-table';
import { filterConditionCount } from '../components/database-view-filters';
import type { PropertyCreatorVariant } from '../components/property-creator';
import { RecordPanel } from '../components/record-panel';
import type {
  DatabaseRowsSource,
  DatabaseWriteResult,
} from '../context/table-source';
import type { DatabaseColumnType } from '../core/column-inference';
import { reorderDatabaseColumns } from '../core/column-order';
import {
  columnSchemaMessage,
  type DatabaseColumnCastsSource,
  type DatabaseColumnTypeChange,
  type DatabaseSchemaChange,
} from '../core/column-schema';
import {
  type DatabaseCellValue,
  type DatabaseViewColumn,
  isBoardGroupColumn,
} from '../core/database-view';
import type { DatabasePropertyType } from '../core/property-creation';
import {
  canEditCell,
  type DatabaseRow,
  type DatabaseRowMutation,
  rowTitle,
  rowValue,
  titleColumn,
} from '../core/table';
import { withSort } from '../core/view-query';
import {
  layoutColumns,
  withLayoutColumn,
  withLayoutOrder,
  withoutColumn,
} from '../core/views';
import {
  databaseReadMessage,
  databaseWriteMessage,
} from '../core/write-failure';
import { createDraftRows } from '../primitives/draft-rows';
import {
  createTableController,
  type FailedWrite,
} from '../primitives/table-controller';
import type { ViewChange } from '../queries/views';
import { type BoardPositions, DatabaseBoardView } from './database-board-view';

const outcomeUnknown = (failure: FailedWrite) =>
  failure.failure.kind === 'outcome-unknown';

export type DatabaseTableActions = {
  createRecord: () => Promise<boolean>;
  focusFirstCell: () => Promise<void>;
  focusColumn: (columnId: string) => boolean;
  openRecord: (rowId: string) => void;
  pending: Accessor<boolean>;
};

/** How long a revealed row stays tinted. */
const HIGHLIGHT_MS = 1_600;

export function DatabaseTableView(props: {
  name: string;
  source: DatabaseRowsSource;
  canEdit: boolean;
  /** The view on screen: a stored one, or the table's own All records. */
  view: DatabaseView;
  /** Whether the view is stored, so changing it changes it for everyone. */
  stored: boolean;
  search: string;
  onViewChange?: (change: ViewChange) => void;
  /** Clear the search and the view's filter. */
  onClearConstraints?: () => void;
  /** A board's card places; without them a board view cannot show. */
  boardPositions?: BoardPositions;
  renderTextEditor?: GridCellProps['renderTextEditor'];
  renderTextValue?: GridCellProps['renderTextValue'];
  renderMentionPicker?: GridCellProps['renderMentionPicker'];
  renderMentionValue?: GridCellProps['renderMentionValue'];
  renderRelationCell?: (props: GridCellProps) => JSX.Element;
  relationTables?: { id: string; name: string }[];
  columnCasts?: DatabaseColumnCastsSource;
  onCellFocus?: (cell: DatabaseCellFocus | undefined) => void;
  remoteUsers?: DatabaseCellPresence[];
  onChangeColumnType?: (
    columnId: string,
    change: DatabaseColumnTypeChange
  ) => DatabaseSchemaChange;
  onDeleteColumn?: (columnId: string) => DatabaseSchemaChange;
  onReorderColumns?: (columnIds: string[]) => DatabaseSchemaChange;
  onRenameColumn?: (
    columnId: string,
    name: string,
    previousName: string
  ) => DatabaseSchemaChange;
  renderToolbar?: (actions: DatabaseTableActions) => JSX.Element;
  /** Create a new column at the end of the table and return its id. */
  createColumn?: () => DatabaseSchemaChange<string>;
  addColumn: (
    label?: string,
    initialType?: DatabasePropertyType,
    variant?: PropertyCreatorVariant,
    onCreated?: (columnId: string) => boolean
  ) => JSX.Element;
}) {
  const controller = createTableController(props.source, recordSaved);
  const draftRows = createDraftRows(controller);
  const [editColumn, setEditColumn] = createSignal<string>();
  const [schemaError, setSchemaError] = createSignal('');
  const columnOrderMutex = new Mutex();
  let columnOrderSequence = 0;
  let pendingColumnOrders = 0;
  let confirmedColumnOrder: string[] = [];
  let boardControls: DatabaseBoardControls | undefined;
  const [selectedId, setSelectedId] = createSignal<string>();
  const [editCell, setEditCell] = createSignal<{
    rowId: string;
    columnId: string;
  }>();
  const [deleteTarget, setDeleteTarget] = createSignal<{
    rowId: string;
    name: string;
  }>();
  const deletionMutations = new Map<
    string,
    Extract<DatabaseRowMutation, { kind: 'delete' }>
  >();
  const duplicateIntents = new Map<string, string>();
  let duplicateSequence = 0;
  const deletionError = () => {
    const failed = controller.failure();
    return failed?.mutation.kind === 'delete' &&
      failed.mutation.rowId === deleteTarget()?.rowId
      ? 'Could not delete this record. Try again.'
      : undefined;
  };
  const [hiddenSavedRecord, setHiddenSavedRecord] = createSignal<{
    rowId: string;
    created: boolean;
    noEditableColumns?: boolean;
  }>();
  let returnFocus: HTMLElement | undefined;
  let deleteReturnFocus: HTMLElement | undefined;
  let disposed = false;
  let firstCellRequest: Promise<void> | undefined;
  let cancelFirstCellWait: (() => void) | undefined;
  onCleanup(() => {
    disposed = true;
    cancelFirstCellWait?.();
  });
  const columns = () => props.source.columns();
  const layout = () => layoutColumns(props.view.layout, columns());
  const visibleColumns = () =>
    layout()
      .filter((entry) => !entry.hidden)
      .map((entry) => entry.column);
  const boardLayout = () =>
    props.view.layout.kind === 'board' ? props.view.layout : undefined;
  // The engine searched, filtered and sorted these for the view.
  const rows = controller.rows;
  const groupColumn = () =>
    columns().find(
      (column) =>
        column.id === boardLayout()?.groupBy && isBoardGroupColumn(column)
    );
  const selected = () =>
    controller.knownRows().find((row) => row.rowId === selectedId());
  const selectedPosition = () =>
    rows().findIndex((row) => row.rowId === selectedId());
  const filtered = () => filterConditionCount(props.view.query.filter) > 0;
  const searched = () => props.search.trim() !== '';
  const constrained = () => searched() || filtered();
  const constraints = () =>
    searched() ? (filtered() ? 'search and filters' : 'search') : 'filters';
  const hiddenRecord = () => {
    const saved = hiddenSavedRecord();
    return saved &&
      (saved.noEditableColumns ||
        !rows().some((row) => row.rowId === saved.rowId))
      ? controller.knownRows().find((row) => row.rowId === saved.rowId)
      : undefined;
  };
  // The row whose cell is open for typing, so a refresh cannot pull it away.
  const [editingRowId, setEditingRowId] = createSignal<string>();
  // Records being looked at or typed into stay readable when they leave the view.
  props.source.retain(() =>
    [
      selectedId(),
      hiddenSavedRecord()?.rowId,
      editingRowId(),
      ...draftRows.serverIds(),
    ].filter((rowId): rowId is string => rowId !== undefined)
  );
  /**
   * The grid's rows. Someone else's edit can make the row being typed in stop
   * matching the view; it keeps its place until the edit ends.
   */
  let shownBefore: DatabaseRow[] = [];
  const gridRows = createMemo(() => {
    const current =
      props.canEdit && visibleColumns().some(canEditCell)
        ? draftRows.project(rows())
        : rows();
    const held = editingRowId();
    const index = shownBefore.findIndex((row) => row.rowId === held);
    const shown =
      held === undefined ||
      index < 0 ||
      current.some((row) => row.rowId === held)
        ? current
        : [
            ...current.slice(0, index),
            controller.knownRows().find((row) => row.rowId === held) ??
              shownBefore[index],
            ...current.slice(index),
          ];
    shownBefore = shown;
    return shown;
  });
  function recordSaved(
    mutation: DatabaseRowMutation,
    result: DatabaseWriteResult
  ) {
    const rowId =
      mutation.kind === 'create' ? result.insertedRowIds[0] : mutation.rowId;
    if (!rowId) return;
    if (mutation.kind === 'delete') {
      deletionMutations.delete(rowId);
      if (deleteTarget()?.rowId === rowId) setDeleteTarget(undefined);
      if (selectedId() === rowId) setSelectedId(undefined);
    }
    if (mutation.kind === 'create') {
      for (const [rowId, intent] of duplicateIntents) {
        if (controller.createComplete(intent)) duplicateIntents.delete(rowId);
      }
    }
    if (
      mutation.kind !== 'delete' &&
      constrained() &&
      !rows().some((row) => row.rowId === rowId)
    ) {
      setHiddenSavedRecord({ rowId, created: mutation.kind === 'create' });
    } else if (hiddenSavedRecord()?.rowId === rowId) {
      setHiddenSavedRecord(undefined);
    }
  }

  const actualRowId = (rowId: string) =>
    draftRows.has(rowId) ? draftRows.serverId(rowId) : rowId;
  async function retry() {
    const failure = controller.failure();
    if (failure?.failure.kind === 'outcome-unknown') {
      await controller.refresh();
      return;
    }
    const local = failure
      ? draftRows.retryTarget(failure)
      : draftRows.error()?.id;
    if (local) await draftRows.retry(local);
    else await controller.retry();
  }
  function dismissSaveFailure() {
    const failure = controller.failure();
    if (failure?.failure.kind === 'outcome-unknown' && failure.createIntentId)
      draftRows.discardUncertain(failure.createIntentId);
    controller.dismissFailure();
  }
  function focusBlankRow() {
    const field = visibleColumns().find(canEditCell);
    if (!props.canEdit || !field) return false;
    setEditCell({ rowId: draftRows.blankId(), columnId: field.id });
    return true;
  }
  function focusColumn(columnId: string) {
    if (!props.canEdit || props.view.layout.kind !== 'table') return false;
    // A create response can arrive before the reactive schema mounts its header.
    // DatabaseTable keeps the request until that header registers itself.
    setEditColumn(undefined);
    setEditColumn(columnId);
    return true;
  }
  function open(rowId: string) {
    const actualId = actualRowId(rowId);
    if (!actualId) return;
    rowId = actualId;
    if (document.activeElement instanceof HTMLElement)
      returnFocus = document.activeElement;
    setSelectedId(rowId);
  }
  const [highlightedRowId, setHighlightedRowId] = createSignal<string>();
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(highlightTimer));
  /**
   * Show a record arrived at from elsewhere where it sits in the table. A
   * record this view does not show (filtered out, or a board) opens instead.
   */
  function reveal(rowId: string) {
    if (
      props.view.layout.kind !== 'table' ||
      !rows().some((row) => row.rowId === rowId)
    ) {
      open(rowId);
      return;
    }
    clearTimeout(highlightTimer);
    setHighlightedRowId(rowId);
    highlightTimer = setTimeout(
      () => setHighlightedRowId(undefined),
      HIGHLIGHT_MS
    );
  }
  function editCreatedRow(rowId: string) {
    if (props.view.layout.kind !== 'table') {
      open(rowId);
      return;
    }
    const title = titleColumn(columns());
    const editable = visibleColumns().filter(canEditCell);
    const field =
      editable.find((column) => column.id === title?.id) ?? editable[0];
    if (field && rows().some((row) => row.rowId === rowId))
      setEditCell({ rowId, columnId: field.id });
    else if (!field)
      setHiddenSavedRecord({ rowId, created: true, noEditableColumns: true });
  }
  async function duplicateRow(rowId: string) {
    const actualId = actualRowId(rowId);
    if (!props.canEdit || !actualId) return false;
    rowId = actualId;
    const row = controller.knownRows().find((row) => row.rowId === rowId);
    if (!row) return false;
    const intent =
      duplicateIntents.get(rowId) ??
      `duplicate:${rowId}:${++duplicateSequence}`;
    duplicateIntents.set(rowId, intent);
    const values = Object.fromEntries(
      columns()
        .filter((column) => column.writable)
        .map((column) => [column.id, rowValue(row, column.id)])
    );
    const result = await controller.save(
      { kind: 'create', values },
      'duplicate record',
      undefined,
      intent
    );
    if (result.isErr()) return false;
    const createdId = result.value.insertedRowIds[0];
    if (createdId) editCreatedRow(createdId);
    return true;
  }
  function requestDelete(rowId: string) {
    const actualId = actualRowId(rowId);
    if (!props.canEdit || controller.pending() || !actualId) return;
    rowId = actualId;
    const row = controller.knownRows().find((row) => row.rowId === rowId);
    if (!row) return;
    if (document.activeElement instanceof HTMLElement)
      deleteReturnFocus = document.activeElement;
    setDeleteTarget({ rowId, name: rowTitle(row, columns()) });
  }
  async function deleteRow(rowId: string) {
    if (!props.canEdit || controller.pending()) return false;
    const mutation =
      deletionMutations.get(rowId) ?? ({ kind: 'delete', rowId } as const);
    deletionMutations.set(rowId, mutation);
    return (await controller.save(mutation, 'delete record')).isOk();
  }
  function navigate(delta: number) {
    const row = rows()[selectedPosition() + delta];
    if (row) setSelectedId(row.rowId);
  }
  async function writeCell(
    row: DatabaseRow,
    column: DatabaseViewColumn,
    value: DatabaseCellValue,
    option?: string,
    columnType?: DatabaseColumnType
  ) {
    if (!props.canEdit || !column.writable) return false;
    if (option === undefined && rowValue(row, column.id) === value) return true;
    const saved = await controller.save(
      {
        kind: 'cell',
        rowId: row.rowId,
        columnId: column.id,
        value,
        ...(columnType ? { columnTypes: { [column.id]: columnType } } : {}),
      },
      column.name,
      option
    );
    return saved.isOk();
  }
  function renderCell(
    row: Accessor<DatabaseRow>,
    column: Accessor<DatabaseViewColumn>,
    options?: GridCellEditorOptions
  ) {
    const write = (value: DatabaseCellValue) =>
      draftRows.has(row().rowId)
        ? draftRows.write(row().rowId, column().id, value)
        : writeCell(row(), column(), value);
    return (
      <Show
        when={column().relation && props.renderRelationCell}
        fallback={
          <GridCell
            {...options}
            column={column()}
            emptyLabel={
              column().id === titleColumn(columns())?.id ? 'Unnamed' : undefined
            }
            value={rowValue(row(), column().id)}
            canEdit={props.canEdit}
            renderTextEditor={props.renderTextEditor}
            renderTextValue={props.renderTextValue}
            renderMentionPicker={props.renderMentionPicker}
            renderMentionValue={props.renderMentionValue}
            onMention={(mention) => {
              const type = {
                dataType: 'ENTITY',
                entityType: mention.entityType,
              } as const;
              return draftRows.has(row().rowId)
                ? draftRows.write(
                    row().rowId,
                    column().id,
                    mention.id,
                    undefined,
                    type
                  )
                : writeCell(row(), column(), mention.id, undefined, type);
            }}
            onWrite={write}
            onAddOption={(label, value) =>
              draftRows.has(row().rowId)
                ? draftRows.write(
                    row().rowId,
                    column().id,
                    value ?? label,
                    label
                  )
                : writeCell(row(), column(), value ?? label, label)
            }
          />
        }
      >
        {(render) =>
          render()({
            ...options,
            get column() {
              return column();
            },
            get value() {
              return rowValue(row(), column().id);
            },
            get canEdit() {
              return props.canEdit;
            },
            onWrite: write,
            onAddOption: async () => false,
          })
        }
      </Show>
    );
  }
  /** A new record, in the board lane of `lane` (an option id, or none) when given. */
  async function createRow(
    lane?: string | null,
    title = '',
    openAfterCreate = true,
    createIntentId?: string
  ) {
    if (!props.canEdit) return false;
    const group = groupColumn();
    const titleField = titleColumn(columns());
    const values: Record<string, DatabaseCellValue> = {};
    if (lane && group?.writable)
      values[group.id] =
        group.options.find((option) => option.id === lane)?.label ?? null;
    if (title && titleField?.writable) values[titleField.id] = title;
    const saved = await controller.save(
      { kind: 'create', values },
      'new record',
      undefined,
      createIntentId
    );
    if (saved.isErr()) return false;
    const rowId = saved.value.insertedRowIds[0];
    if (rowId && openAfterCreate) editCreatedRow(rowId);
    return true;
  }
  function focusFirstCell(): Promise<void> {
    if (firstCellRequest) return firstCellRequest;
    firstCellRequest = (async () => {
      if (props.source.loading()) {
        const ready = until(() => !props.source.loading());
        cancelFirstCellWait = ready.dispose;
        await ready.catch(() => undefined);
        cancelFirstCellWait = undefined;
      }
      if (
        disposed ||
        !props.canEdit ||
        props.source.error() ||
        !props.source.snapshot()
      )
        return;
      const field = visibleColumns().find(canEditCell);
      if (!field) return;
      const row =
        props.view.layout.kind === 'table'
          ? draftRows.project(rows())[0]
          : rows()[0];
      if (row) {
        if (props.view.layout.kind === 'table')
          setEditCell({ rowId: row.rowId, columnId: field.id });
        else open(row.rowId);
      } else if (props.view.layout.kind === 'table') {
        focusBlankRow();
      }
    })().finally(() => {
      firstCellRequest = undefined;
    });
    return firstCellRequest;
  }
  function sort(columnId: string, direction: 'asc' | 'desc' | null) {
    props.onViewChange?.({
      query: {
        ...props.view.query,
        sort: withSort(
          props.view.query.sort ?? [],
          columnId,
          direction === null
            ? null
            : direction === 'asc'
              ? 'ascending'
              : 'descending'
        ),
      },
    });
  }
  function hideColumn(columnId: string) {
    props.onViewChange?.({
      layout: withLayoutColumn(props.view.layout, columns(), columnId, {
        hidden: true,
      }),
    });
  }
  function resizeColumn(columnId: string, width: number) {
    props.onViewChange?.({
      layout: withLayoutColumn(props.view.layout, columns(), columnId, {
        width,
      }),
    });
  }
  const columnOrder = () => layout().map((entry) => entry.column.id);
  /**
   * A drop moves the header at once. A stored view keeps its own column
   * order; All records follows the table's, which the drop then saves.
   */
  async function reorderColumn(
    columnId: string,
    targetId: string,
    edge: 'before' | 'after'
  ) {
    const order = columnOrder();
    const nextOrder = reorderDatabaseColumns(
      order,
      layout()
        .filter((entry) => entry.hidden)
        .map((entry) => entry.column.id),
      columnId,
      targetId,
      edge
    );
    if (!nextOrder) return;
    setSchemaError('');
    const showOrder = (ids: readonly string[]) =>
      props.onViewChange?.({
        layout: withLayoutOrder(props.view.layout, columns(), ids),
      });
    showOrder(nextOrder);
    const persist =
      props.canEdit && !props.stored ? props.onReorderColumns : undefined;
    if (!persist) return;
    const sequence = ++columnOrderSequence;
    if (pendingColumnOrders++ === 0) confirmedColumnOrder = order;
    try {
      // Each request reads the schema version refreshed by the previous write.
      // Queue complete orders so a later drop includes all optimistic moves.
      const persisted = await columnOrderMutex.runExclusive(async () => {
        const result = await persist(nextOrder);
        if (result.isOk()) confirmedColumnOrder = nextOrder;
        return result;
      });
      if (disposed || sequence !== columnOrderSequence) return;
      if (persisted.isOk()) {
        setSchemaError('');
        return;
      }
      const currentOrder = columnOrder();
      // A view selection made during the write owns its own column layout.
      if (
        currentOrder.length === nextOrder.length &&
        currentOrder.every((id, index) => id === nextOrder[index])
      )
        showOrder(confirmedColumnOrder);
      setSchemaError(columnSchemaMessage(persisted.error));
    } finally {
      pendingColumnOrders--;
    }
  }
  async function insertColumn(targetId: string, side: 'left' | 'right') {
    const create = props.createColumn;
    if (!create) return;
    setSchemaError('');
    const result = await create();
    if (disposed) return;
    if (result.isErr()) {
      setSchemaError(columnSchemaMessage(result.error));
      return;
    }
    const created = result.value;
    // The new column arrives with the refreshed schema, at the table's end.
    await until(() => columns().some((column) => column.id === created));
    await reorderColumn(
      created,
      targetId,
      side === 'left' ? 'before' : 'after'
    );
    focusColumn(created);
  }
  function moveColumn(columnId: string, direction: 'left' | 'right') {
    const columns = visibleColumns();
    const target =
      columns[
        columns.findIndex((column) => column.id === columnId) +
          (direction === 'left' ? -1 : 1)
      ];
    if (target)
      void reorderColumn(
        columnId,
        target.id,
        direction === 'left' ? 'before' : 'after'
      );
  }

  return (
    <>
      {/* Rendered once: the toolbar's own props keep it current, and a rerun would close its open popovers. */}
      {untrack(() =>
        props.renderToolbar?.({
          createRecord: async () =>
            props.view.layout.kind === 'table'
              ? focusBlankRow()
              : boardControls?.addCard() || createRow(),
          focusFirstCell,
          focusColumn,
          openRecord: reveal,
          pending: controller.pending,
        })
      )}
      <div class="relative flex min-h-0 flex-1 flex-col overflow-hidden">
        <Show when={schemaError()}>
          <p
            role="alert"
            class="border-b border-edge-muted px-5 py-2 text-xs text-failure-ink"
          >
            {schemaError()}
          </p>
        </Show>
        <Show when={controller.failure()}>
          {(failure) => (
            <div
              role="alert"
              class="flex shrink-0 items-start gap-2 border-b border-warning/20 bg-warning/5 px-5 py-3 text-xs"
            >
              <WarningIcon class="mt-0.5 size-4 shrink-0 text-warning-ink" />
              <div class="min-w-0 flex-1">
                <p class="font-medium text-ink">
                  {outcomeUnknown(failure())
                    ? 'This row may already be saved.'
                    : `Could not save ${failure().label}.`}
                </p>
                <p class="mt-1 text-ink-muted">
                  {outcomeUnknown(failure())
                    ? 'Check the latest rows against your draft, then discard the draft. Refreshing will not submit it again.'
                    : databaseWriteMessage(failure().failure)}
                </p>
              </div>
              <Button
                size="xs"
                class="shrink-0"
                disabled={controller.pending()}
                onClick={() => void retry()}
              >
                {outcomeUnknown(failure()) ? 'Refresh' : 'Retry'}
              </Button>
              <Show
                when={
                  outcomeUnknown(failure()) &&
                  failure().createIntentId &&
                  draftRows.has(failure().createIntentId!)
                }
              >
                <Button
                  size="xs"
                  class="shrink-0"
                  disabled={controller.pending()}
                  onClick={dismissSaveFailure}
                >
                  Discard draft
                </Button>
              </Show>
              <Button
                size="icon-xs"
                label="Dismiss save error"
                tooltipDisabled
                onClick={controller.dismissFailure}
              >
                <XIcon class="size-3.5" />
              </Button>
            </div>
          )}
        </Show>
        <Show
          when={
            controller.refreshWarning() ||
            (props.source.error() && props.source.snapshot())
          }
        >
          <div
            role="status"
            class="flex shrink-0 items-center gap-2 border-b border-edge-muted px-5 py-2 text-xs text-ink-muted"
          >
            The latest data could not be refreshed.
            <Button
              variant="plain"
              size="xs"
              class="text-accent"
              onClick={() => void controller.refresh()}
            >
              Refresh
            </Button>
          </div>
        </Show>
        <Show when={hiddenRecord()}>
          {(row) => (
            <div
              role="status"
              class="flex shrink-0 items-start gap-2 border-b border-edge-muted bg-accent/5 px-5 py-3 text-xs"
            >
              <EyeSlashIcon class="mt-0.5 size-4 shrink-0 text-ink-muted" />
              <div class="min-w-0 flex-1">
                <p class="font-medium text-ink">
                  {hiddenSavedRecord()?.created
                    ? hiddenSavedRecord()?.noEditableColumns
                      ? 'Record created'
                      : 'Record created outside this view'
                    : 'Record saved outside this view'}
                </p>
                <p class="mt-1 text-ink-muted">
                  {hiddenSavedRecord()?.noEditableColumns
                    ? 'This view has no editable columns. Open the record to see its details.'
                    : `“${rowTitle(row(), columns())}” doesn’t match your ${constraints()}.`}
                </p>
              </div>
              <Button
                size="xs"
                class="shrink-0 text-accent"
                onClick={() => open(row().rowId)}
              >
                Open record
              </Button>
              <Button
                size="icon-xs"
                label="Dismiss record notice"
                tooltipDisabled
                onClick={() => setHiddenSavedRecord(undefined)}
              >
                <XIcon class="size-3.5" />
              </Button>
            </div>
          )}
        </Show>
        <Show when={!controller.failure() ? draftRows.error() : undefined}>
          {(failure) => (
            <div
              role="alert"
              class="flex items-center gap-3 border-b border-edge-muted px-4 py-2 text-xs text-ink-muted"
            >
              <span class="flex-1">
                {draftRows.isUncertain(failure().id)
                  ? 'This row may already be saved. Check the latest rows, then discard this draft.'
                  : failure().error}
              </span>
              <Button
                size="xs"
                disabled={controller.pending()}
                onClick={() =>
                  void (draftRows.isUncertain(failure().id)
                    ? controller.refresh()
                    : retry())
                }
              >
                {draftRows.isUncertain(failure().id) ? 'Refresh' : 'Retry'}
              </Button>
              <Show when={draftRows.isUncertain(failure().id)}>
                <Button
                  size="xs"
                  disabled={controller.pending()}
                  onClick={() => draftRows.discardUncertain(failure().id)}
                >
                  Discard draft
                </Button>
              </Show>
            </div>
          )}
        </Show>
        <Show when={!props.source.loading()} fallback={<TableSkeleton />}>
          <Show
            when={props.source.snapshot()}
            fallback={
              <div class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
                <WarningIcon class="size-7 text-ink-muted" />
                <p class="text-sm font-medium">
                  This table could not be loaded
                </p>
                <p class="max-w-96 text-xs text-ink-muted">
                  <Show
                    when={props.source.error()}
                    fallback="Try refreshing the table."
                  >
                    {(failure) => databaseReadMessage(failure())}
                  </Show>
                </p>
                <Button
                  size="sm"
                  class="gap-2"
                  onClick={() => void controller.refresh()}
                >
                  <ArrowClockwiseIcon class="size-3.5" />
                  Try again
                </Button>
              </div>
            }
          >
            <Show
              when={props.view.layout.kind === 'board'}
              fallback={
                <DatabaseTable
                  name={props.name}
                  rows={gridRows()}
                  isUnsavedRow={draftRows.isUnsaved}
                  onRowFocus={draftRows.setActive}
                  onCellFocus={(cell) => {
                    setEditingRowId(cell?.editing ? cell.rowId : undefined);
                    props.onCellFocus?.(cell);
                  }}
                  remoteUsers={props.remoteUsers}
                  highlightRowId={highlightedRowId()}
                  columns={visibleColumns()}
                  sort={props.view.query.sort ?? []}
                  widths={Object.fromEntries(
                    layout().map((entry) => [entry.column.id, entry.width])
                  )}
                  onResizeColumn={props.onViewChange ? resizeColumn : undefined}
                  canEdit={props.canEdit}
                  canCreateRecord={columns().length > 0}
                  pending={controller.pending()}
                  addColumn={props.addColumn(
                    columns().length ? undefined : 'Add first column',
                    undefined,
                    undefined,
                    focusColumn
                  )}
                  renderCell={renderCell}
                  editCell={editCell()}
                  titleColumnId={titleColumn(columns())?.id}
                  getRowTitle={(row) => rowTitle(row, columns())}
                  onOpen={open}
                  onCreate={() => void createRow()}
                  onDuplicate={duplicateRow}
                  onRequestDelete={requestDelete}
                  editColumn={editColumn()}
                  relationTables={props.relationTables}
                  columnCasts={props.columnCasts}
                  onChangeColumnType={props.onChangeColumnType}
                  onDeleteColumn={
                    props.onDeleteColumn
                      ? (columnId) =>
                          (
                            props.onDeleteColumn?.(columnId) ??
                            okAsync(undefined)
                          ).map(() => {
                            // The server takes a column out of the views it stores.
                            if (props.stored) return;
                            const { query, layout } = withoutColumn(
                              props.view,
                              columnId
                            );
                            props.onViewChange?.({ query, layout });
                          })
                      : undefined
                  }
                  onReorderColumn={reorderColumn}
                  onRenameColumn={props.onRenameColumn}
                  onSort={sort}
                  onHide={props.onViewChange ? hideColumn : undefined}
                  onMove={props.onViewChange ? moveColumn : undefined}
                  onInsertColumn={
                    props.canEdit && props.createColumn
                      ? (columnId, side) => void insertColumn(columnId, side)
                      : undefined
                  }
                  emptyState={
                    <Show
                      when={
                        rows().length === 0 &&
                        (constrained() || columns().length === 0)
                      }
                    >
                      <div class="py-5 pr-4 pl-14">
                        <p class="max-w-80 text-sm text-ink-muted">
                          {constrained()
                            ? 'No records match this view.'
                            : 'Add a column to get started.'}
                        </p>
                        <Show when={constrained()}>
                          <Button
                            variant="plain"
                            size="xs"
                            class="mt-2 text-accent"
                            onClick={() => props.onClearConstraints?.()}
                          >
                            Clear filters
                          </Button>
                        </Show>
                      </div>
                    </Show>
                  }
                />
              }
            >
              <Show
                when={
                  boardLayout() && groupColumn() && props.boardPositions
                    ? {
                        layout: boardLayout()!,
                        group: groupColumn()!,
                        positions: props.boardPositions,
                      }
                    : undefined
                }
                fallback={
                  <div class="flex flex-1 flex-col items-start px-5 py-8">
                    <p class="text-sm text-ink-muted">
                      Choose a single Select column to group cards.
                    </p>
                    <Button
                      size="sm"
                      class="mt-3"
                      onClick={() =>
                        props.onViewChange?.({
                          layout: { kind: 'table', columns: [] },
                        })
                      }
                    >
                      Open table
                    </Button>
                  </div>
                }
              >
                {(board) => (
                  <DatabaseBoardView
                    view={props.view}
                    layout={board().layout}
                    source={props.source}
                    rows={rows()}
                    columns={columns()}
                    groupColumn={board().group}
                    positions={board().positions}
                    canEdit={props.canEdit && board().group.writable}
                    onViewChange={props.onViewChange}
                    renderTextValue={props.renderTextValue}
                    rowPending={controller.rowPending}
                    createPending={controller.createPending}
                    createComplete={controller.createComplete}
                    onOpen={open}
                    onCreate={(lane, title, intentId, options) =>
                      createRow(lane, title, options?.open ?? false, intentId)
                    }
                    controlsRef={(controls) => {
                      boardControls = controls;
                    }}
                    onAddGroup={(label) => {
                      const column = board().group;
                      return props.canEdit && column.writable
                        ? controller.addGroup(column.id, label)
                        : Promise.resolve(ok(undefined));
                    }}
                  />
                )}
              </Show>
            </Show>
          </Show>
        </Show>
        <Show when={selected()}>
          {(row) => (
            <RecordPanel
              row={row()}
              tableName={props.name}
              columns={columns()}
              canEdit={props.canEdit}
              pending={controller.pending()}
              outsideViewReason={
                selectedPosition() < 0 && constrained()
                  ? `This record doesn’t match your ${constraints()}. You can ${props.canEdit ? 'keep editing' : 'view'} it here.`
                  : undefined
              }
              saveError={
                controller.failure()
                  ? `Your change to ${controller.failure()!.label} was not saved.`
                  : undefined
              }
              onRetry={() => void retry()}
              position={selectedPosition()}
              total={rows().length}
              renderCell={renderCell}
              onClose={() => setSelectedId(undefined)}
              onNavigate={navigate}
              returnFocus={returnFocus}
              onRequestDelete={() => requestDelete(row().rowId)}
            />
          )}
        </Show>
        <DeleteDialog
          open={!!deleteTarget()}
          onOpenChange={(open) => {
            if (!open) setDeleteTarget(undefined);
          }}
          title="Delete record?"
          pending={controller.pending()}
          onDelete={() => {
            const target = deleteTarget();
            if (target) void deleteRow(target.rowId);
          }}
          onCloseAutoFocus={(event) => {
            const target = [deleteReturnFocus, returnFocus].find(
              (element) => element?.isConnected
            );
            if (!target) return;
            event.preventDefault();
            target.focus();
          }}
          body={
            <>
              <p class="break-words">
                “{deleteTarget()?.name}” will be deleted.
              </p>
              <Show when={deletionError()}>
                <p role="alert" class="mt-2 text-failure-ink">
                  {deletionError()}
                </p>
              </Show>
            </>
          }
        />
      </div>
    </>
  );
}

function TableSkeleton() {
  return (
    <div
      class="flex-1 p-5"
      role="status"
      aria-label="Loading records"
      aria-busy="true"
    >
      <div class="mb-4 h-8 animate-pulse rounded bg-hover" />
      <For each={[0, 1, 2, 3, 4, 5]}>
        {() => <div class="mb-2 h-9 animate-pulse rounded bg-hover/60" />}
      </For>
    </div>
  );
}
