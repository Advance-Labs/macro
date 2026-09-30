import { showDatabaseSql } from '@core/constant/featureFlags';
import { afterEach, describe, expect, it } from 'vitest';
import {
  isScalarAnswer,
  parseQueryProposal,
  type QuerySchema,
  queryErrorMessage,
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
    const answer = {
      results: [
        { columns: [{ name: 'Answer', entity_type: null }], rows: [[null]] },
      ],
      read_tables: [],
      read_versions: {},
      truncated_tables: [],
    };
    expect(isScalarAnswer(answer)).toBe(true);
    expect(
      isScalarAnswer({
        ...answer,
        results: [{ ...answer.results[0], rows: [] }],
      })
    ).toBe(false);
    expect(
      isScalarAnswer({
        ...answer,
        results: [...answer.results, ...answer.results],
      })
    ).toBe(false);
  });
});

describe('queryErrorMessage', () => {
  it('explains that questions only read when the browser engine refuses a write', () => {
    expect(
      queryErrorMessage(
        new Error('the engine runs SELECT statements; writes go through run()')
      )
    ).toBe(
      'Questions can only read data you have access to. Edit records in the table or board.'
    );
  });
});

describe('queryErrorMessage with SQL hidden', () => {
  afterEach(() => {
    showDatabaseSql.enabled = false;
  });
  const plain = (message: string) => queryErrorMessage(new Error(message));

  it('names what changed instead of quoting the engine', () => {
    expect(
      plain('unknown column "Price" in "Shop"."Items" — did you mean "Prices"?')
    ).toBe(
      "This answer couldn't be computed: the column Price no longer exists."
    );
    expect(plain('unknown table "Shop"."Old Items"')).toBe(
      "This answer couldn't be computed: the table Old Items no longer exists."
    );
    expect(
      plain(
        'table "Guests" exists in Party and Offsite — qualify it as Party.Guests or Offsite.Guests'
      )
    ).toBe(
      "This answer couldn't be computed: more than one database has a table named Guests."
    );
    expect(
      plain('"Maybe" is not an option of "Status" (Going, Declined)')
    ).toBe(
      "This answer couldn't be computed: Maybe is not an option of Status."
    );
    expect(plain('"Price" is a number column; compare it to a number')).toBe(
      "This answer couldn't be computed: Price holds number values, which don't fit this question."
    );
    expect(plain('"Tags" holds several values; use HAS instead of =')).toBe(
      "This answer couldn't be computed: Tags can't be used that way."
    );
  });

  it('turns anything else that quotes a statement into a plain line', () => {
    expect(plain('expected FROM, found end of input at 14..14')).toBe(
      "This answer couldn't be computed. Try asking again."
    );
    expect(plain('"Name" must appear in GROUP BY or inside an aggregate')).toBe(
      "This answer couldn't be computed. Try asking again."
    );
    expect(plain('Query budget exceeded')).toBe(
      'This question needs less data. Try a narrower question.'
    );
  });

  it('keeps authored messages and the raw engine text when SQL is shown', () => {
    expect(
      plain('Choose an available table before asking this question.')
    ).toBe('Choose an available table before asking this question.');
    showDatabaseSql.enabled = true;
    expect(plain('expected FROM, found end of input at 14..14')).toBe(
      'expected FROM, found end of input at 14..14'
    );
  });
});
