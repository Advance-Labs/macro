import { describe, expect, it } from 'vitest';
import {
  rowsByIdStatement,
  selectAllStatement,
  sqlLiteral,
  tableRowsStatement,
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
