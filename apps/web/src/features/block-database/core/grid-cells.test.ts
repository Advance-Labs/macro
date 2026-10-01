import { describe, expect, it } from 'vitest';
import { gridRows, UNAVAILABLE_OPTION } from './grid-cells';

describe('engine cells as grid values', () => {
  it('shows an option the catalog lacks instead of dropping it', () => {
    expect(
      gridRows(
        {
          columns: [
            {
              name: 'Tags',
              column: 'definition',
              kind: 'select',
            },
          ],
          rows: [[{ type: 'options', value: ['urgent', 'added-elsewhere'] }]],
          rowIds: ['row-1'],
          readTables: ['table'],
          truncated: false,
          insertedRowIds: [],
          changesApplied: 0,
        },
        {
          tables: [
            {
              id: 'table',
              databaseId: 'db',
              database: 'Projects',
              name: 'Tasks',
              columns: [
                {
                  id: 'definition',
                  placement: 'column',
                  name: 'Tags',
                  kind: {
                    kind: 'select',
                    multi: true,
                    options: [{ id: 'urgent', label: 'Urgent' }],
                  },
                },
              ],
            },
          ],
        },
        [
          {
            shared_outside_database: false,
            column: {
              id: 'column',
              table_id: 'table',
              property_definition_id: 'definition',
              position: 'a',
              config: null,
              display_name: null,
              infer_type: false,
            },
            sql_name: '"Tags"',
            writable: true,
            definition: {
              definition: {
                id: 'definition',
                owner: { scope: 'database', database_id: 'db' },
                display_name: 'Tags',
                data_type: 'TAG',
                is_multi_select: true,
                specific_entity_type: null,
                created_at: '',
                updated_at: '',
                is_system: false,
                is_metadata: false,
              },
              property_options: [],
            },
          },
        ]
      )
    ).toEqual([
      {
        rowId: 'row-1',
        cells: { column: JSON.stringify(['Urgent', UNAVAILABLE_OPTION]) },
      },
    ]);
  });
});
