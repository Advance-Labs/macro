import { describe, expect, it } from 'vitest';
import {
  deleteRowStatement,
  exportPageStatement,
  insertRowStatement,
  linkedFromStatement,
  rowsByIdStatement,
  selectAllStatement,
  sqlLiteral,
  updateCellStatement,
  type ViewStatementColumn,
  viewSelectStatement,
} from './sql';

describe('database row SQL', () => {
  it('uses quoted names verbatim and doubles single quotes in literals', () => {
    expect(
      updateCellStatement({
        tableSqlName: '"Guest List"',
        columnSqlName: '"Full "" Name"',
        rowId: "row'1",
        value: "O'Brien's; DROP TABLE people;",
      })
    ).toBe(
      "UPDATE \"Guest List\" SET \"Full \"\" Name\" = 'O''Brien''s; DROP TABLE people;' WHERE row_id = 'row''1'"
    );
    expect(deleteRowStatement({ tableSqlName: '"rows"', rowId: "a'b" })).toBe(
      "DELETE FROM \"rows\" WHERE row_id = 'a''b'"
    );
    expect(selectAllStatement('"Guests"')).toBe('SELECT * FROM "Guests"');
    expect(exportPageStatement('"Guests"', 5000, 5000)).toBe(
      'SELECT * FROM "Guests" LIMIT 5000 OFFSET 5000'
    );
  });
  it('writes checkboxes, lists, and NULL in the dialect', () => {
    expect(sqlLiteral(true)).toBe('TRUE');
    expect(sqlLiteral(false)).toBe('FALSE');
    expect(sqlLiteral(null)).toBe('NULL');
    expect(sqlLiteral(undefined)).toBe('NULL');
    expect(sqlLiteral(12.5)).toBe('12.5');
    expect(sqlLiteral(['Urgent', "Bob's"])).toBe("['Urgent', 'Bob''s']");
    expect(sqlLiteral([])).toBe('NULL');
    expect(
      updateCellStatement({
        tableSqlName: '"Tasks"',
        columnSqlName: '"Guests"',
        rowId: 'row-1',
        value: ['id-1', 'id-2'],
      })
    ).toBe(
      "UPDATE \"Tasks\" SET \"Guests\" = ['id-1', 'id-2'] WHERE row_id = 'row-1'"
    );
  });
  it('inserts board values without changing SQL types', () => {
    expect(
      insertRowStatement({
        tableSqlName: '"tasks"',
        values: {
          '"name"': "Sam's task",
          '"status"': 'Done',
          '"amount"': 0,
          '"due"': null,
          '"done"': true,
          '"tags"': ['A', 'B'],
        },
      })
    ).toBe(
      'INSERT INTO "tasks" ("name", "status", "amount", "due", "done", "tags") VALUES (\'Sam\'\'s task\', \'Done\', 0, NULL, TRUE, [\'A\', \'B\'])'
    );
    expect(insertRowStatement({ tableSqlName: '"tasks"', values: {} })).toBe(
      'INSERT INTO "tasks" DEFAULT VALUES'
    );
  });
  it('reads rows by id', () => {
    expect(rowsByIdStatement('"Tasks"', ['row-2', "row'1"])).toBe(
      "SELECT * FROM \"Tasks\" WHERE row_id IN ('row-2', 'row''1')"
    );
  });
  it('finds rows linked from another table with HAS', () => {
    expect(
      linkedFromStatement({
        tableSqlName: '"Events"',
        columnSqlName: '"Guests"',
        rowId: 'row-1',
      })
    ).toBe('SELECT row_id FROM "Events" WHERE "Guests" HAS \'row-1\'');
  });
  it('rejects nonfinite numbers and caller-supplied identities', () => {
    expect(() =>
      insertRowStatement({
        tableSqlName: '"tasks"',
        values: { '"amount"': Number.NaN },
      })
    ).toThrow();
    expect(() =>
      insertRowStatement({
        tableSqlName: '"tasks"',
        values: { '"amount"': Number.POSITIVE_INFINITY },
      })
    ).toThrow();
    expect(() =>
      insertRowStatement({
        tableSqlName: '"tasks"',
        values: { row_id: 'user-id' },
      })
    ).toThrow();
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
    ).toBe('SELECT * FROM "Tasks"');
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
      'SELECT * FROM "Tasks" WHERE ("Name" LIKE \'%DESIGN%\' OR "Tags" HAS \'Design\' OR "Tags" HAS \'Design review\')'
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
    ).toBe('SELECT * FROM "Tasks" WHERE row_id IS NULL');
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
      "SELECT * FROM \"Tasks\" WHERE \"Name\" LIKE '%50\\%\\_off\\\\Sam''s%' ESCAPE '\\'"
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
      'SELECT * FROM "Tasks" WHERE ("Name" LIKE \'Plan\' AND ("Name" != \'\' AND "Name" NOT LIKE \'Draft\') AND ("Name" != \'\' AND "Name" NOT LIKE \'%old%\') AND "Name" LIKE \'P%\')'
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
    ).toBe('SELECT * FROM "Tasks"');
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
      'SELECT * FROM "Tasks" WHERE (("Name" IS NULL OR "Name" = \'\') AND "Amount" IS NOT NULL)'
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
    ).toBe('SELECT * FROM "Tasks" WHERE ("Amount" >= 9 AND "Amount" != -1.5)');
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
      'SELECT * FROM "Tasks" WHERE (("Due" >= \'2026-09-30\' AND "Due" < \'2026-10-01\') AND ("Due" < \'2026-12-31\' OR "Due" >= \'2027-01-01\') AND "Due" >= \'2026-03-01\' AND "Due" < \'2026-10-02\')'
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
    ).toBe('SELECT * FROM "Tasks" WHERE "Done" = FALSE');
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
      'SELECT * FROM "Tasks" WHERE ("Status" IN (\'Done\') AND "Tags" HAS \'Launch\' AND "Tags" NOT HAS \'Design\')'
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
      'SELECT * FROM "Tasks" WHERE ("Stage" HAS \'Design, "review"\' AND row_id IS NOT NULL)'
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
    ).toBe('SELECT * FROM "Tasks" WHERE row_id IS NULL');
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
      'SELECT * FROM "Tasks" WHERE "Name" LIKE \'%plan%\' AND ("Status" IN (\'Done\') OR "Amount" > 100)'
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
      'SELECT * FROM "Tasks" WHERE ("Amount" > 100 OR row_id IS NOT NULL)'
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
      'SELECT * FROM "Tasks" WHERE ("Status" IN (\'Done\') AND "Amount" > 100)'
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
    ).toBe('SELECT * FROM "Tasks" WHERE "Customer" IS NOT NULL');
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
    ).toBe('SELECT * FROM "Tasks" ORDER BY "Due" DESC, "Name" ASC');
  });
});
