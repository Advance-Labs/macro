import { describe, expect, it } from 'vitest';
import {
  rowsByIdStatement,
  selectAllStatement,
  sqlLiteral,
  tableRowsStatement,
  type ViewStatementColumn,
  viewSelectStatement,
} from './sql';

describe('database row SQL', () => {
  it('uses quoted names verbatim and doubles single quotes in literals', () => {
    expect(rowsByIdStatement('"Guest List"', ['row-2', "row'1"])).toBe(
      "SELECT * FROM \"Guest List\" WHERE row_id IN ('row-2', 'row''1')"
    );
    expect(selectAllStatement('"Guests"')).toBe('SELECT * FROM "Guests"');
    expect(tableRowsStatement('"Guests"')).toBe(
      'SELECT * FROM "Guests" ORDER BY row_position'
    );
    expect(sqlLiteral("O'Brien's; DROP TABLE people;")).toBe(
      "'O''Brien''s; DROP TABLE people;'"
    );
  });
  it('spells checkboxes and numbers in the dialect', () => {
    expect(sqlLiteral(true)).toBe('TRUE');
    expect(sqlLiteral(false)).toBe('FALSE');
    expect(sqlLiteral(12.5)).toBe('12.5');
  });
  it('rejects nonfinite numbers and empty names', () => {
    expect(() => sqlLiteral(Number.NaN)).toThrow();
    expect(() => sqlLiteral(Number.POSITIVE_INFINITY)).toThrow();
    expect(() => selectAllStatement('')).toThrow();
  });
});

const name: ViewStatementColumn = {
  sqlName: '"Name"',
  column: {
    id: 'name',
    name: 'Name',
    dataType: 'STRING',
    isMultiSelect: false,
    options: [],
    writable: true,
  },
};
const status: ViewStatementColumn = {
  sqlName: '"Status"',
  column: {
    id: 'status',
    name: 'Status',
    dataType: 'SELECT_STRING',
    isMultiSelect: false,
    options: ['To do', 'In progress', 'Done'],
    writable: true,
  },
};
const tags: ViewStatementColumn = {
  sqlName: '"Tags"',
  column: {
    id: 'tags',
    name: 'Tags',
    dataType: 'SELECT_STRING',
    isMultiSelect: true,
    options: ['Design', 'Design review', 'Launch'],
    writable: true,
  },
};
const amount: ViewStatementColumn = {
  sqlName: '"Amount"',
  column: {
    id: 'amount',
    name: 'Amount',
    dataType: 'NUMBER',
    isMultiSelect: false,
    options: [],
    writable: true,
  },
};
const due: ViewStatementColumn = {
  sqlName: '"Due"',
  column: {
    id: 'due',
    name: 'Due',
    dataType: 'DATE',
    isMultiSelect: false,
    options: [],
    writable: true,
  },
};
const done: ViewStatementColumn = {
  sqlName: '"Done"',
  column: {
    id: 'done',
    name: 'Done',
    dataType: 'BOOLEAN',
    isMultiSelect: false,
    options: [],
    writable: true,
  },
};
const customer: ViewStatementColumn = {
  sqlName: '"Customer"',
  column: {
    id: 'customer',
    name: 'Customer',
    dataType: 'STRING',
    isMultiSelect: true,
    options: [],
    writable: true,
    relation: { databaseId: 'crm', tableId: 'customers' },
  },
};

describe('view statements', () => {
  it('reads the whole table when the view has no search, filters or sorts', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name, status],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe('SELECT * FROM "Tasks" ORDER BY row_position');
  });

  it('searches text columns and matching select options case-insensitively, skipping numbers and dates', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name, status, tags, amount, due],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '  DESIGN ',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Name" LIKE \'%DESIGN%\' OR "Tags" HAS \'Design\' OR "Tags" HAS \'Design review\') ORDER BY row_position'
    );
  });

  it('matches no rows when a search can reach no column', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [status, amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: 'launch',
        },
      })
    ).toBe('SELECT * FROM "Tasks" WHERE row_id IS NULL ORDER BY row_position');
  });

  it('escapes LIKE wildcards and quotes in a contains filter', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            {
              id: 'filter',
              columnId: 'name',
              operator: 'contains',
              value: "50%_off\\Sam's",
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      "SELECT * FROM \"Tasks\" WHERE \"Name\" LIKE '%50\\%\\_off\\\\Sam''s%' ESCAPE '\\' ORDER BY row_position"
    );
  });

  it('compares text equality without case and keeps empty cells out of negative text filters', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'name', operator: 'equals', value: 'Plan' },
            {
              id: 'b',
              columnId: 'name',
              operator: 'not_equals',
              value: 'Draft',
            },
            {
              id: 'c',
              columnId: 'name',
              operator: 'not_contains',
              value: 'old',
            },
            {
              id: 'd',
              columnId: 'name',
              operator: 'starts_with',
              value: 'P',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Name" LIKE \'Plan\' AND ("Name" != \'\' AND "Name" NOT LIKE \'Draft\') AND ("Name" != \'\' AND "Name" NOT LIKE \'%old%\') AND "Name" LIKE \'P%\') ORDER BY row_position'
    );
  });

  it('ignores unfinished filters and filters on missing columns', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name, amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'name', operator: 'contains', value: '  ' },
            { id: 'b', columnId: 'amount', operator: 'gt', value: 'many' },
            { id: 'c', columnId: 'gone', operator: 'equals', value: 'x' },
          ],
          filterConjunction: 'or',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe('SELECT * FROM "Tasks" ORDER BY row_position');
  });

  it('treats null and empty text as empty', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name, amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'name', operator: 'is_empty', value: '' },
            {
              id: 'b',
              columnId: 'amount',
              operator: 'is_not_empty',
              value: '',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE (("Name" IS NULL OR "Name" = \'\') AND "Amount" IS NOT NULL) ORDER BY row_position'
    );
  });

  it('compares numbers numerically', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'amount', operator: 'gte', value: ' 9 ' },
            {
              id: 'b',
              columnId: 'amount',
              operator: 'not_equals',
              value: '-1.5',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Amount" >= 9 AND "Amount" != -1.5) ORDER BY row_position'
    );
  });

  it('compares dates by calendar day', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [due],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            {
              id: 'a',
              columnId: 'due',
              operator: 'equals',
              value: '2026-09-30',
            },
            {
              id: 'b',
              columnId: 'due',
              operator: 'not_equals',
              value: '2026-12-31',
            },
            { id: 'c', columnId: 'due', operator: 'gt', value: '2026-02-28' },
            { id: 'd', columnId: 'due', operator: 'lte', value: '2026-10-01' },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE (("Due" >= \'2026-09-30\' AND "Due" < \'2026-10-01\') AND ("Due" < \'2026-12-31\' OR "Due" >= \'2027-01-01\') AND "Due" >= \'2026-03-01\' AND "Due" < \'2026-10-02\') ORDER BY row_position'
    );
  });

  it('matches a checkbox as TRUE or FALSE', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [done],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'done', operator: 'equals', value: '0' },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe('SELECT * FROM "Tasks" WHERE "Done" = FALSE ORDER BY row_position');
  });

  it('matches single selects by option and multi-selects by membership', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [status, tags],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'status', operator: 'equals', value: 'done' },
            { id: 'b', columnId: 'tags', operator: 'equals', value: 'Launch' },
            {
              id: 'c',
              columnId: 'tags',
              operator: 'not_equals',
              value: 'Design',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Status" IN (\'Done\') AND "Tags" HAS \'Launch\' AND "Tags" NOT HAS \'Design\') ORDER BY row_position'
    );
  });

  it('matches whole option labels, quotes and commas included, never part of one', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [
          {
            sqlName: '"Stage"',
            column: {
              id: 'stage',
              name: 'Stage',
              dataType: 'SELECT_STRING',
              isMultiSelect: true,
              options: ['Design, "review"', 'In progress'],
              writable: true,
            },
          },
        ],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            {
              id: 'a',
              columnId: 'stage',
              operator: 'equals',
              value: 'design, "REVIEW"',
            },
            {
              id: 'b',
              columnId: 'stage',
              operator: 'not_equals',
              value: 'progress',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Stage" HAS \'Design, "review"\' AND row_id IS NOT NULL) ORDER BY row_position'
    );
  });

  it('matches nothing for a select value that is no longer an option', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [status],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            {
              id: 'a',
              columnId: 'status',
              operator: 'equals',
              value: 'Archived',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe('SELECT * FROM "Tasks" WHERE row_id IS NULL ORDER BY row_position');
  });

  it('joins filters with OR and still requires the search', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name, status, amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'status', operator: 'equals', value: 'Done' },
            { id: 'b', columnId: 'amount', operator: 'gt', value: '100' },
          ],
          filterConjunction: 'or',
          sorts: [],
          hiddenColumns: [],
          search: 'plan',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE "Name" LIKE \'%plan%\' AND ("Status" IN (\'Done\') OR "Amount" > 100) ORDER BY row_position'
    );
  });

  it('keeps an OR condition that holds for every row', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [tags, amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'amount', operator: 'gt', value: '100' },
            {
              id: 'b',
              columnId: 'tags',
              operator: 'not_equals',
              value: 'Archived',
            },
          ],
          filterConjunction: 'or',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Amount" > 100 OR row_id IS NOT NULL) ORDER BY row_position'
    );
  });

  it('reads a saved view without a conjunction as AND', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [status, amount],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            { id: 'a', columnId: 'status', operator: 'equals', value: 'Done' },
            { id: 'b', columnId: 'amount', operator: 'gt', value: '100' },
          ],
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE ("Status" IN (\'Done\') AND "Amount" > 100) ORDER BY row_position'
    );
  });

  it('filters relations only by whether they are empty', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [customer],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [
            {
              id: 'a',
              columnId: 'customer',
              operator: 'contains',
              value: 'Acme',
            },
            {
              id: 'b',
              columnId: 'customer',
              operator: 'is_not_empty',
              value: '',
            },
          ],
          filterConjunction: 'and',
          sorts: [],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" WHERE "Customer" IS NOT NULL ORDER BY row_position'
    );
  });

  it('orders by each sort in turn, skipping relations', () => {
    expect(
      viewSelectStatement({
        tableSqlName: '"Tasks"',
        columns: [name, due, customer],
        view: {
          layout: 'table',
          groupBy: null,
          filters: [],
          filterConjunction: 'and',
          sorts: [
            { columnId: 'due', direction: 'desc' },
            { columnId: 'customer', direction: 'asc' },
            { columnId: 'name', direction: 'asc' },
          ],
          hiddenColumns: [],
          search: '',
        },
      })
    ).toBe(
      'SELECT * FROM "Tasks" ORDER BY "Due" DESC, "Name" ASC, row_position'
    );
  });
});
