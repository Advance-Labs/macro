import { describe, expect, it } from 'vitest';
import {
  boardMoveValue,
  type DatabaseCellValue,
  type DatabaseFilter,
  type DatabaseViewColumn,
  defaultDatabaseView,
  filterOperatorsFor,
  groupDatabaseRows,
  isBoardGroupColumn,
  isSavedDatabaseViewConfig,
  moveDatabaseViewColumn,
  orderDatabaseCards,
  orderDatabaseColumns,
  orderDatabaseGroups,
  placeDatabaseCard,
  reconcileDatabaseView,
} from './database-view';

const name: DatabaseViewColumn = {
  id: 'name',
  name: 'Name',
  dataType: 'STRING',
  isMultiSelect: false,
  options: [],
  writable: true,
};
const amount = { ...name, id: 'amount', name: 'Budget', dataType: 'NUMBER' };
const status = {
  ...name,
  id: 'status',
  name: 'Status',
  dataType: 'SELECT_STRING',
  options: ['Backlog', 'In progress', 'Done'],
};
type Row = Record<string, DatabaseCellValue>;
const getValue = (row: Row, id: string) => row[id] ?? null;
const filter = (
  operator: DatabaseFilter['operator'],
  value = ''
): DatabaseFilter => ({ id: 'filter', columnId: 'amount', operator, value });

describe('database views', () => {
  it('names membership operators for multi-selects and offers only emptiness for references', () => {
    const tags = { ...status, isMultiSelect: true };
    expect(filterOperatorsFor(tags).map((operator) => operator.label)).toEqual([
      'contains',
      'does not contain',
      'is empty',
      'is not empty',
    ]);
    expect(
      filterOperatorsFor({ ...name, dataType: 'ENTITY' }).map(
        (operator) => operator.value
      )
    ).toEqual(['is_empty', 'is_not_empty']);
    expect(
      filterOperatorsFor({
        ...name,
        relation: { databaseId: 'db', tableId: 'customers' },
      }).map((operator) => operator.value)
    ).toEqual(['is_empty', 'is_not_empty']);
  });

  it('keeps unused select options and legacy values in separate board groups', () => {
    const rows: Row[] = [
      { status: 'Done' },
      { status: 'Legacy' },
      { status: null },
    ];
    const groups = groupDatabaseRows(rows, status, getValue);
    expect(groups.map((group) => group.value)).toEqual([
      'Backlog',
      'Done',
      'In progress',
      'Legacy',
      null,
    ]);
    expect(groups.find((group) => group.value === 'Backlog')?.rows).toEqual([]);
    expect(groups.find((group) => group.value === null)?.rows).toEqual([
      rows[2],
    ]);
  });

  it('keeps numeric select labels as strings and boolean writes as integers', () => {
    const groups = groupDatabaseRows(
      [{ status: '10' }],
      { ...status, dataType: 'SELECT_NUMBER', options: [2, 10] },
      getValue
    );
    expect(groups.map((group) => group.value)).toEqual(['2', '10', null]);
    expect(groups[1].rows).toHaveLength(1);
    const checked = groupDatabaseRows(
      [{ status: 0 }, { status: 1 }, { status: null }],
      { ...status, dataType: 'BOOLEAN' },
      getValue
    );
    expect(checked.map((group) => group.value)).toEqual([1, 0, null]);
    expect(checked.map((group) => group.rows.length)).toEqual([1, 1, 1]);
  });

  it('does not collapse an empty lane into an option literally named empty', () => {
    const groups = groupDatabaseRows(
      [{ status: 'empty' }, { status: null }],
      { ...status, options: ['empty'] },
      getValue
    );
    expect(new Set(groups.map((group) => group.key)).size).toBe(2);
  });

  it('offers scalar and multi-select categorical fields for grouping', () => {
    expect(isBoardGroupColumn(status)).toBe(true);
    expect(isBoardGroupColumn({ ...status, writable: false })).toBe(true);
    expect(isBoardGroupColumn({ ...status, isMultiSelect: true })).toBe(true);
    expect(isBoardGroupColumn(name)).toBe(false);
  });

  it('reads a saved view without a filter conjunction as matching every filter', () => {
    const saved = {
      kind: 'database-view',
      version: 1,
      databaseId: 'db',
      tableId: 'table',
      view: {
        layout: 'table',
        groupBy: null,
        filters: [],
        sorts: [],
        hiddenColumns: [],
        search: '',
      },
    };
    expect(isSavedDatabaseViewConfig(saved)).toBe(true);
    expect(
      reconcileDatabaseView(defaultDatabaseView(), [name]).filterConjunction
    ).toBe('and');
    expect(
      reconcileDatabaseView({ ...saved.view, layout: 'table' }, [name])
        .filterConjunction
    ).toBe('and');
    expect(
      isSavedDatabaseViewConfig({
        ...saved,
        view: { ...saved.view, filterConjunction: 'xor' },
      })
    ).toBe(false);
  });

  it('rejects unrelated and malformed saved views', () => {
    const saved = {
      kind: 'database-view',
      version: 1,
      databaseId: 'db',
      tableId: 'table',
      view: defaultDatabaseView(),
    };
    expect(isSavedDatabaseViewConfig(saved)).toBe(true);
    expect(isSavedDatabaseViewConfig({ ...saved, kind: 'crm' })).toBe(false);
    expect(isSavedDatabaseViewConfig({ ...saved, version: 2 })).toBe(false);
    expect(
      isSavedDatabaseViewConfig({
        ...saved,
        view: { ...saved.view, filters: [{ operator: 'sql', value: 'DROP' }] },
      })
    ).toBe(false);
    expect(
      isSavedDatabaseViewConfig({
        ...saved,
        view: { ...saved.view, hiddenColumns: [2] },
      })
    ).toBe(false);
    expect(isSavedDatabaseViewConfig(null)).toBe(false);
    expect(
      isSavedDatabaseViewConfig({
        ...saved,
        view: { ...saved.view, columnOrder: ['name', 7] },
      })
    ).toBe(false);
  });

  it('drops missing columns from older views and chooses a valid board group', () => {
    const result = reconcileDatabaseView(
      {
        ...defaultDatabaseView(),
        layout: 'board',
        groupBy: 'deleted',
        hiddenColumns: ['deleted', 'name'],
        filters: [{ ...filter('equals', '1'), columnId: 'deleted' }],
        sorts: [{ columnId: 'deleted', direction: 'asc' }],
      },
      [name, status]
    );
    expect(result.groupBy).toBe('status');
    expect(result.hiddenColumns).toEqual(['name']);
    expect(result.filters).toEqual([]);
    expect(result.sorts).toEqual([]);
  });

  it('keeps legacy schema order and appends new columns after a saved custom order', () => {
    const columns = [name, amount, status];
    expect(orderDatabaseColumns(columns)).toEqual(columns);
    expect(
      orderDatabaseColumns(columns, ['status', 'deleted', 'status', 'name'])
    ).toEqual([status, name, amount]);
    expect(columns).toEqual([name, amount, status]);
  });

  it('moves across hidden columns without changing their position or other settings', () => {
    const view = {
      ...defaultDatabaseView(),
      hiddenColumns: ['amount'],
      search: 'launch',
    };
    const moved = moveDatabaseViewColumn(
      view,
      [name, amount, status],
      'status',
      'left'
    );
    expect(moved.columnOrder).toEqual(['status', 'amount', 'name']);
    expect(moved.hiddenColumns).toEqual(['amount']);
    expect(moved.search).toBe('launch');
    expect(view.columnOrder).toBeUndefined();
    const restored = moveDatabaseViewColumn(
      moved,
      [name, amount, status],
      'status',
      'right'
    );
    expect(restored.columnOrder).toBeUndefined();
  });

  it('does not move an edge, hidden, or missing column', () => {
    const view = { ...defaultDatabaseView(), hiddenColumns: ['amount'] };
    for (const [id, direction] of [
      ['name', 'left'],
      ['status', 'right'],
      ['amount', 'left'],
      ['missing', 'right'],
    ] as const) {
      expect(
        moveDatabaseViewColumn(view, [name, amount, status], id, direction)
      ).toBe(view);
    }
  });

  it('reconciles deleted and duplicate order ids without hiding newly added columns', () => {
    const view = reconcileDatabaseView(
      {
        ...defaultDatabaseView(),
        columnOrder: ['deleted', 'status', 'status', 'name'],
        hiddenColumns: ['deleted', 'name'],
      },
      [name, amount, status]
    );
    expect(view.columnOrder).toEqual(['status', 'name', 'amount']);
    expect(view.hiddenColumns).toEqual(['name']);
    expect(
      orderDatabaseColumns([name, amount, status], view.columnOrder).filter(
        (column) => !view.hiddenColumns.includes(column.id)
      )
    ).toEqual([status, amount]);
    expect(
      reconcileDatabaseView(view, [name, amount]).columnOrder
    ).toBeUndefined();
  });
});

describe('multi-select boards', () => {
  it('shows a tagged record once per selected group and empty records only in the empty lane', () => {
    const tags = { ...status, isMultiSelect: true };
    const tagged = { status: '["Done","Backlog","Done"]' };
    const empty = { status: '[]' };
    const groups = groupDatabaseRows([tagged, empty], tags, getValue);
    expect(groups.map((group) => [group.label, group.rows])).toEqual([
      ['Backlog', [tagged]],
      ['Done', [tagged]],
      ['In progress', []],
      ['No status', [empty]],
    ]);
  });

  it('moves one tag occurrence while preserving unrelated tags and never duplicates the target', () => {
    const tags = { ...status, isMultiSelect: true };
    expect(
      boardMoveValue(tags, '["Done","Backlog"]', 'In progress', 'Done')
    ).toBe('["Backlog","In progress"]');
    expect(boardMoveValue(tags, '["Done","Backlog"]', 'Backlog', 'Done')).toBe(
      '["Backlog"]'
    );
    expect(boardMoveValue(tags, '["Done","Backlog"]', null, 'Done')).toBe('[]');
    expect(boardMoveValue(status, 'Done', 'Backlog', 'Done')).toBe('Backlog');
  });

  it('restores custom lane order, appends new groups alphabetically, and ignores deleted or repeated keys', () => {
    const groups = groupDatabaseRows([], status, getValue);
    const ordered = orderDatabaseGroups(groups, [
      'value:"Done"',
      'deleted',
      'value:"Done"',
      'empty',
    ]);
    expect(ordered.map((group) => group.label)).toEqual([
      'Done',
      'No status',
      'Backlog',
      'In progress',
    ]);
  });

  it('validates optional per-lane card order while accepting older saved views', () => {
    const saved = {
      kind: 'database-view',
      version: 1,
      databaseId: 'db',
      tableId: 'table',
      view: {
        ...defaultDatabaseView(),
        cardOrder: { 'value:"Done"': ['second', 'first'], empty: [] },
      },
    };
    expect(isSavedDatabaseViewConfig(saved)).toBe(true);
    for (const cardOrder of [
      null,
      [],
      { empty: 'row' },
      { empty: [12] },
      { empty: {} },
    ]) {
      expect(
        isSavedDatabaseViewConfig({
          ...saved,
          view: { ...saved.view, cardOrder },
        })
      ).toBe(false);
    }
  });

  it('restores separate lane positions for a multi-select record without duplicating or losing cards', () => {
    const first = { id: 'first' },
      second = { id: 'second' },
      third = { id: 'third' };
    const rows = [first, second, third];
    expect(
      orderDatabaseCards(
        rows,
        ['deleted', 'third', 'third', 'first'],
        (row) => row.id
      )
    ).toEqual([third, first, second]);
    expect(
      orderDatabaseCards([first, third], ['first', 'third'], (row) => row.id)
    ).toEqual([first, third]);
    expect(orderDatabaseCards(rows, undefined, (row) => row.id)).toEqual(rows);
    expect(rows).toEqual([first, second, third]);
  });

  it('moves cards to precise visible gaps in both directions, including first and last', () => {
    const order = ['first', 'second', 'third'];
    expect(placeDatabaseCard(order, order, 'third', 'first')).toEqual([
      'third',
      'first',
      'second',
    ]);
    expect(placeDatabaseCard(order, order, 'first', 'third')).toEqual([
      'second',
      'first',
      'third',
    ]);
    expect(placeDatabaseCard(order, order, 'first')).toEqual([
      'second',
      'third',
      'first',
    ]);
    expect(placeDatabaseCard(order, order, 'first', 'second')).toEqual(order);
    expect(placeDatabaseCard(order, order, 'first', 'missing')).toEqual(order);
    expect(order).toEqual(['first', 'second', 'third']);
  });

  it('retains filtered card slots when reordering or inserting from another lane', () => {
    const order = [
      'hidden-first',
      'first',
      'hidden-middle',
      'second',
      'third',
      'hidden-last',
    ];
    const visible = ['first', 'second', 'third'];
    expect(placeDatabaseCard(order, visible, 'third', 'first')).toEqual([
      'hidden-first',
      'third',
      'hidden-middle',
      'first',
      'second',
      'hidden-last',
    ]);
    expect(placeDatabaseCard(order, visible, 'incoming', 'second')).toEqual([
      'hidden-first',
      'first',
      'hidden-middle',
      'incoming',
      'second',
      'hidden-last',
      'third',
    ]);
    expect(placeDatabaseCard([], [], 'incoming')).toEqual(['incoming']);
  });

  it('includes new visible cards omitted from saved order and drops duplicate ids', () => {
    expect(
      placeDatabaseCard(
        ['first', 'first'],
        ['first', 'new', 'third'],
        'third',
        'new'
      )
    ).toEqual(['first', 'third', 'new']);
  });

  it('clears manual card positions if schema changes require a different grouping column', () => {
    const view = {
      ...defaultDatabaseView(),
      layout: 'board' as const,
      groupBy: 'removed',
      cardOrder: { empty: ['row'] },
    };
    expect(reconcileDatabaseView(view, [status]).cardOrder).toBeUndefined();
    expect(
      reconcileDatabaseView({ ...view, groupBy: status.id }, [status]).cardOrder
    ).toEqual(view.cardOrder);
  });
});
