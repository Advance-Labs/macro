import { describe, expect, it } from 'vitest';
import { toolAnswers } from './tool-answer';

describe('QueryDatabase tool results as answers', () => {
  it('reads each column’s kind off the tool’s entity type and values', () => {
    expect(
      toolAnswers({
        results: [
          {
            columns: [
              { name: 'Host', entityType: 'user' },
              { name: 'Guests', entityType: 'user' },
              { name: 'Budget' },
              { name: 'Starts' },
              { name: 'Venue' },
            ],
            rows: [
              [
                'macro|ada@x.test',
                '["macro|ada@x.test","macro|grace@x.test"]',
                1200,
                '2026-06-01T18:30:00+00:00',
                'Rooftop',
              ],
              [null, null, null, null, null],
            ],
          },
        ],
        changesApplied: 0,
        readVersions: [{ tableId: 'table-parties', version: 4 }],
        summary: 'Read 2 rows.',
      })
    ).toEqual([
      {
        columns: [
          {
            name: 'Host',
            kind: 'entity',
            source: {
              markdown: false,
              options: [],
              tag: false,
              target: 'USER',
              relatedTable: null,
            },
          },
          {
            name: 'Guests',
            kind: 'entity',
            source: {
              markdown: false,
              options: [],
              tag: false,
              target: 'USER',
              relatedTable: null,
            },
          },
          { name: 'Budget', kind: 'number' },
          { name: 'Starts', kind: 'date' },
          { name: 'Venue', kind: 'text' },
        ],
        rows: [
          [
            { type: 'entities', value: ['macro|ada@x.test'] },
            {
              type: 'entities',
              value: ['macro|ada@x.test', 'macro|grace@x.test'],
            },
            { type: 'number', value: 1200 },
            { type: 'date', value: '2026-06-01T18:30:00+00:00' },
            { type: 'text', value: 'Rooftop' },
          ],
          [null, null, null, null, null],
        ],
        rowIds: [],
        readTables: ['table-parties'],
        readDatabaseIds: [],
        truncatedTables: [],
      },
    ]);
  });

  it('moves a leading row_id column into the rows’ ids', () => {
    const [answer] = toolAnswers({
      results: [
        {
          columns: [{ name: 'row_id' }, { name: 'Name' }],
          rows: [['row-1', 'Halloween Bash']],
        },
      ],
      changesApplied: 0,
      readVersions: [],
      summary: '',
    });

    expect(answer.columns).toEqual([{ name: 'Name', kind: 'text' }]);
    expect(answer.rows).toEqual([[{ type: 'text', value: 'Halloween Bash' }]]);
    expect(answer.rowIds).toEqual(['row-1']);
  });

  it('keeps text that only looks like a number as text', () => {
    const [answer] = toolAnswers({
      results: [{ columns: [{ name: 'Zip' }], rows: [['02139'], [12]] }],
      changesApplied: 0,
      readVersions: [],
      summary: '',
    });

    expect(answer.columns).toEqual([{ name: 'Zip', kind: 'text' }]);
    expect(answer.rows).toEqual([
      [{ type: 'text', value: '02139' }],
      [{ type: 'text', value: '12' }],
    ]);
  });
});
