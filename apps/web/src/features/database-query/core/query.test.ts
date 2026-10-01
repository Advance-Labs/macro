import { showDatabaseSql } from '@core/constant/featureFlags';
import { afterEach, describe, expect, it } from 'vitest';
import {
  isScalarAnswer,
  parseQueryProposal,
  type QueryAnswer,
  type QuerySchema,
  queryErrorMessage,
  queryFailureDetail,
  queryStarters,
  unquoteIdentifier,
} from './query';

const schema: QuerySchema = {
  databaseId: 'db',
  name: 'Projects',
  tables: [
    {
      id: 'table',
      name: 'Projects',
      sqlName: '"my""projects"',
      columns: [
        {
          name: 'Status',
          sqlName: '"Status"',
          type: 'SelectString',
          options: ['In progress', 'Done'],
          multiple: false,
        },
      ],
    },
  ],
};
describe('unquoteIdentifier', () => {
  it('takes the table segment of a database-qualified name', () => {
    expect(unquoteIdentifier('"Untitled database"."Table 1"')).toBe('Table 1');
    expect(unquoteIdentifier('"my""db"."a""b"')).toBe('a"b');
    expect(unquoteIdentifier('"Status"')).toBe('Status');
  });
});

describe('database questions', () => {
  it('does not silently replace an unavailable selected table with the first table', () => {
    expect(queryStarters({ ...schema, focusTableId: 'removed-table' })).toEqual(
      []
    );
  });
  it('targets the active table without removing tables needed for cross-table questions', () => {
    const focused: QuerySchema = {
      ...schema,
      focusTableId: 'people',
      tables: [
        ...schema.tables,
        {
          id: 'people',
          name: 'People',
          sqlName: '"People"',
          columns: [],
        },
      ],
    };
    const starters = queryStarters(focused);
    expect(starters[0].prompt).toBe('How many records are in People?');
    expect(starters[0].sql).toBe('SELECT COUNT(*) FROM "People"');
    expect(starters[1].sql).toBe('SELECT * FROM "People" LIMIT 50');
    expect(focused.tables).toHaveLength(2);
  });

  it('uses pre-quoted SQL names verbatim, no aliases, and only groups scalar options', () => {
    expect(queryStarters(schema)[0].sql).toBe(
      'SELECT COUNT(*) FROM "my""projects"'
    );
    expect(queryStarters(schema)[2].sql).toBe(
      'SELECT "Status", COUNT(*) FROM "my""projects" GROUP BY "Status" ORDER BY COUNT(*) DESC'
    );
    expect(
      queryStarters({
        ...schema,
        tables: [
          {
            ...schema.tables[0],
            columns: [{ ...schema.tables[0].columns[0], multiple: true }],
          },
        ],
      })
    ).toHaveLength(2);
  });
  it('validates structured AI responses before review', () => {
    expect(
      parseQueryProposal({
        sql: '```sql\nSELECT COUNT(*) FROM projects\n```',
        explanation: 'Counts projects.',
      }).sql
    ).toBe('SELECT COUNT(*) FROM projects');
    expect(() =>
      parseQueryProposal({
        sql: 'DELETE FROM projects',
        explanation: 'Deletes projects.',
      })
    ).toThrow('Ask a question');
    expect(() =>
      parseQueryProposal({
        sql: 'WITH open AS (SELECT * FROM projects) SELECT COUNT(*) FROM open',
        explanation: 'Counts open projects.',
      })
    ).toThrow('Ask a question');
    expect(() => parseQueryProposal({ sql: 'SELECT 1' })).toThrow('incomplete');
  });
  it('only calls exactly one cell a scalar, including null', () => {
    const answer: QueryAnswer = {
      columns: [{ name: 'Answer', kind: 'number' }],
      rows: [[null]],
      rowIds: [],
      readTables: [],
      readDatabaseIds: [],
      truncatedTables: [],
    };
    expect(isScalarAnswer(answer)).toBe(true);
    expect(isScalarAnswer({ ...answer, rows: [] })).toBe(false);
    expect(isScalarAnswer({ ...answer, rows: [[null], [null]] })).toBe(false);
    expect(
      isScalarAnswer({
        ...answer,
        columns: [
          { name: 'Answer', kind: 'number' },
          { name: 'Other', kind: 'number' },
        ],
        rows: [[null, null]],
      })
    ).toBe(false);
  });
});

describe('queryErrorMessage', () => {
  it('explains that questions only read when a write is refused', () => {
    expect(queryErrorMessage({ kind: 'read-only' })).toBe(
      'Questions only read your data. Ask a question about it above.'
    );
    expect(
      queryErrorMessage({
        kind: 'question',
        error: { code: 'READ_ONLY', message: 'writes are not allowed' },
      })
    ).toBe(
      'Questions can only read data you have access to. Edit records in the table or board.'
    );
  });

  it('words the service failures it knows by code', () => {
    expect(
      queryErrorMessage({
        kind: 'question',
        error: { code: 'NOT_FOUND', message: '' },
      })
    ).toBe('This saved question no longer exists.');
    expect(
      queryErrorMessage({
        kind: 'question',
        error: { code: 'BUDGET_EXCEEDED', message: 'Query budget exceeded' },
      })
    ).toBe('This question needs less data. Try a narrower question.');
    expect(
      queryErrorMessage({
        kind: 'databases',
        error: { code: 'GONE', message: '' },
      })
    ).toBe(
      'This table is no longer available. Choose a database and update the question.'
    );
    expect(
      queryErrorMessage({ kind: 'fetch', message: 'Failed to fetch' })
    ).toBe('Your data could not be reached. Check your connection.');
    expect(queryErrorMessage({ kind: 'table-unavailable' })).toBe(
      'Choose an available table before asking this question.'
    );
  });
});

describe('queryErrorMessage with SQL hidden', () => {
  afterEach(() => {
    showDatabaseSql.enabled = false;
  });

  it('turns the engine’s words, which quote the statement, into a plain line', () => {
    expect(
      queryErrorMessage({
        kind: 'engine',
        message: 'expected FROM, found end of input at 14..14',
      })
    ).toBe("This answer couldn't be computed. Try asking again.");
    expect(
      queryErrorMessage({
        kind: 'question',
        error: {
          code: 'INVALID_QUERY',
          message: '"Name" must appear in GROUP BY or inside an aggregate',
        },
      })
    ).toBe("This answer couldn't be computed. Try asking again.");
  });

  it('keeps authored messages and the raw engine text when SQL is shown', () => {
    expect(
      queryErrorMessage({
        kind: 'generation',
        message: 'Try a question about the properties in this database.',
      })
    ).toBe('Try a question about the properties in this database.');
    showDatabaseSql.enabled = true;
    expect(
      queryErrorMessage({
        kind: 'engine',
        message: 'expected FROM, found end of input at 14..14',
      })
    ).toBe('expected FROM, found end of input at 14..14');
    expect(
      queryErrorMessage({
        kind: 'question',
        error: { code: 'BUDGET_EXCEEDED', message: 'Query budget exceeded' },
      })
    ).toBe(
      'This question needs less data. Try a narrower question or add a LIMIT in SQL.'
    );
  });
});

describe('queryFailureDetail', () => {
  it('is the engine’s or the service’s own words', () => {
    expect(
      queryFailureDetail({ kind: 'engine', message: 'unknown table "Old"' })
    ).toBe('unknown table "Old"');
    expect(
      queryFailureDetail({
        kind: 'question',
        error: { code: 'INVALID_QUERY', message: 'bad statement' },
      })
    ).toBe('bad statement');
    expect(queryFailureDetail({ kind: 'read-only' })).toBeUndefined();
  });
});
