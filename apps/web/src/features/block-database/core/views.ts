/** A table's typed views as the grid and board draw them, and the layouts new views start with. */
import type { FilterNode } from '@core/database-sql/generated/types';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import type { ViewColumn } from '@service-storage/generated/schemas/viewColumn';
import type { ViewLayout } from '@service-storage/generated/schemas/viewLayout';
import { err, ok, type Result } from 'neverthrow';
import { type DatabaseViewColumn, isBoardGroupColumn } from './database-view';
import { moveBeside } from './move-beside';
import { titleColumn } from './table';

/** A table layout's column, with how it shows. */
type LayoutColumn = {
  column: DatabaseViewColumn;
  width: number | null;
  hidden: boolean;
};

/**
 * Every column in display order: the layout's listed ones first, then the
 * rest in the table's order. A board lists none.
 */
export function layoutColumns(
  layout: ViewLayout,
  columns: readonly DatabaseViewColumn[]
): LayoutColumn[] {
  const listed = layout.kind === 'table' ? layout.columns : [];
  const byId = new Map(columns.map((column) => [column.id, column]));
  const shown = listed.flatMap((entry) => {
    const column = byId.get(entry.column);
    return column
      ? [{ column, width: entry.width ?? null, hidden: !!entry.hidden }]
      : [];
  });
  const named = new Set(shown.map((entry) => entry.column.id));
  return [
    ...shown,
    ...columns
      .filter((column) => !named.has(column.id))
      .map((column) => ({ column, width: null, hidden: false })),
  ];
}

function tableLayout(entries: readonly LayoutColumn[]): ViewLayout {
  return {
    kind: 'table',
    columns: entries.map(
      (entry): ViewColumn => ({
        column: entry.column.id,
        width: entry.width,
        hidden: entry.hidden,
      })
    ),
  };
}

/** The table layout with one column's width or visibility changed. */
export function withLayoutColumn(
  layout: ViewLayout,
  columns: readonly DatabaseViewColumn[],
  columnId: string,
  change: Partial<Pick<LayoutColumn, 'width' | 'hidden'>>
): ViewLayout {
  return tableLayout(
    layoutColumns(layout, columns).map((entry) =>
      entry.column.id === columnId ? { ...entry, ...change } : entry
    )
  );
}

/** The table layout with its columns in `order`; columns it leaves out keep their places after. */
export function withLayoutOrder(
  layout: ViewLayout,
  columns: readonly DatabaseViewColumn[],
  order: readonly string[]
): ViewLayout {
  const entries = layoutColumns(layout, columns);
  const byId = new Map(entries.map((entry) => [entry.column.id, entry]));
  const ordered = order.flatMap((id) => {
    const entry = byId.get(id);
    return entry ? [entry] : [];
  });
  const placed = new Set(ordered.map((entry) => entry.column.id));
  return tableLayout([
    ...ordered,
    ...entries.filter((entry) => !placed.has(entry.column.id)),
  ]);
}

/**
 * The view of every row in the table's order, which a table always has:
 * not stored, so changing it changes only what this viewer sees.
 */
export function allRecordsView(table: {
  id: string;
  database_id: string;
}): DatabaseView {
  const epoch = new Date(0).toISOString();
  return {
    id: table.id,
    databaseId: table.database_id,
    tableId: table.id,
    name: 'All records',
    position: '',
    query: { filter: null, sort: [] },
    layout: { kind: 'table', columns: [] },
    createdAt: epoch,
    updatedAt: epoch,
  };
}

/** The single selects a board can group by. */
export function boardGroupColumns(
  columns: readonly DatabaseViewColumn[]
): DatabaseViewColumn[] {
  return columns.filter(isBoardGroupColumn);
}

/** A new board grouped by `groupBy`, its cards showing the first few other fields. */
export function boardLayout(
  groupBy: string,
  columns: readonly DatabaseViewColumn[]
): ViewLayout {
  const title = titleColumn(columns)?.id;
  return {
    kind: 'board',
    groupBy,
    lanes: [],
    cardFields: columns
      .filter((column) => column.id !== groupBy && column.id !== title)
      .slice(0, 3)
      .map((column) => column.id),
    hideEmptyLanes: false,
  };
}

/** What a board lane is called: its option's label, or `No <column>` for cards without one. */
export function laneLabel(
  groupColumn: DatabaseViewColumn,
  option: string | null
): string {
  return (
    groupColumn.options.find((entry) => entry.id === option)?.label ??
    `No ${groupColumn.name.toLocaleLowerCase()}`
  );
}

type BoardLayout = Extract<ViewLayout, { kind: 'board' }>;

/** The board with its lanes in `order`, keeping each lane's hidden flag. */
export function withLaneOrder(
  layout: BoardLayout,
  order: readonly (string | null)[]
): BoardLayout {
  return {
    ...layout,
    lanes: order.map((option) => ({
      option,
      hidden: layout.lanes.some(
        (lane) => lane.option === option && lane.hidden
      ),
    })),
  };
}

/** The board with one lane hidden or shown; the lanes it lists keep their order. */
export function withLaneHidden(
  layout: BoardLayout,
  option: string | null,
  hidden: boolean
): BoardLayout {
  const listed = layout.lanes.some((lane) => lane.option === option);
  return {
    ...layout,
    lanes: listed
      ? layout.lanes.map((lane) =>
          lane.option === option ? { ...lane, hidden } : lane
        )
      : [...layout.lanes, { option, hidden }],
  };
}

function withoutColumnNode(
  node: FilterNode,
  columnId: string
): FilterNode | undefined {
  if (node.kind === 'condition')
    return node.column === columnId ? undefined : node;
  const conditions = node.conditions.flatMap((child) => {
    const kept = withoutColumnNode(child, columnId);
    return kept ? [kept] : [];
  });
  return conditions.length ? { ...node, conditions } : undefined;
}

/**
 * The view without a removed column: its conditions, sort key and layout
 * entry go. A board grouped by it has nothing else to group by, so the
 * column cannot be removed from it, as the service refuses too.
 */
export function withoutColumn(
  view: DatabaseView,
  columnId: string
): Result<DatabaseView, { kind: 'board-groups-by-column' }> {
  if (view.layout.kind === 'board' && view.layout.groupBy === columnId)
    return err({ kind: 'board-groups-by-column' });
  const filter = view.query.filter;
  const kept = filter
    ? withoutColumnNode({ kind: 'group', ...filter }, columnId)
    : undefined;
  return ok({
    ...view,
    query: {
      filter:
        kept?.kind === 'group'
          ? { conjunction: kept.conjunction, conditions: kept.conditions }
          : null,
      sort: (view.query.sort ?? []).filter((key) => key.column !== columnId),
    },
    layout:
      view.layout.kind === 'table'
        ? {
            ...view.layout,
            columns: view.layout.columns.filter(
              (entry) => entry.column !== columnId
            ),
          }
        : {
            ...view.layout,
            cardFields: view.layout.cardFields.filter((id) => id !== columnId),
          },
  });
}

/** The view order with `id` dropped onto `target`'s place; unchanged when either is unknown or they are the same. */
export function movedViewOrder(
  order: readonly string[],
  id: string,
  target: string
): string[] {
  const edge = order.indexOf(id) < order.indexOf(target) ? 'after' : 'before';
  return moveBeside(order, id, target, edge) ?? [...order];
}
