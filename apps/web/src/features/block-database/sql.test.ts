import { describe, expect, it } from 'vitest';
import {
  deleteRowStatement,
  exportPageStatement,
  insertRowStatement,
  linkedFromStatement,
  selectAllStatement,
  sqlLiteral,
  updateCellStatement,
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
