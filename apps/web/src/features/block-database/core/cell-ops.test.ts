import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import { err, ok } from 'neverthrow';
import { describe, expect, it } from 'vitest';
import { cellValue, mutationOp } from './cell-ops';

function column(
  dataType: ColumnDetail['definition']['definition']['data_type'],
  options: {
    multi?: boolean;
    entityType?: 'USER' | 'DOCUMENT';
    relation?: boolean;
  } = {}
): ColumnDetail {
  return {
    column: {
      id: 'column',
      table_id: 'table',
      property_definition_id: 'definition',
      position: 'a',
      config: options.relation
        ? { kind: 'link', database_id: 'db', table_id: 'related' }
        : null,
    },
    sql_name: '"Column"',
    writable: true,
    definition: {
      definition: {
        id: 'definition',
        owner: { scope: 'database', database_id: 'db' },
        display_name: 'Column',
        data_type: dataType,
        is_multi_select: options.multi ?? false,
        specific_entity_type: options.entityType ?? null,
        created_at: '',
        updated_at: '',
        is_system: false,
        is_metadata: false,
      },
      property_options: [],
    },
  };
}

describe('grid values as cell values', () => {
  it('names options by label, one for a single select and each for a multi select', () => {
    expect(cellValue(column('SELECT_STRING'), 'Going')).toEqual(
      ok({
        type: 'options',
        value: [{ label: 'Going' }],
      })
    );
    expect(
      cellValue(column('TAG', { multi: true }), '["Urgent","Backend"]')
    ).toEqual(
      ok({
        type: 'options',
        value: [{ label: 'Urgent' }, { label: 'Backend' }],
      })
    );
    expect(cellValue(column('SELECT_NUMBER', { multi: true }), '[]')).toEqual(
      ok({
        type: 'clear',
      })
    );
  });

  it('writes references with the kind the column points at, and relations as rows', () => {
    expect(
      cellValue(column('ENTITY', { entityType: 'DOCUMENT' }), 'document-1')
    ).toEqual(
      ok({
        type: 'entities',
        value: [{ entityType: 'DOCUMENT', entityId: 'document-1' }],
      })
    );
    expect(
      cellValue(
        column('ENTITY', { relation: true, multi: true }),
        '["row-1","row-2"]'
      )
    ).toEqual(ok({ type: 'rows', value: ['row-1', 'row-2'] }));
    expect(cellValue(column('ENTITY', { relation: true }), '[]')).toEqual(
      ok({
        type: 'clear',
      })
    );
  });

  it('keeps empty text but clears any other empty cell', () => {
    expect(cellValue(column('STRING'), '')).toEqual(
      ok({
        type: 'text',
        value: '',
      })
    );
    expect(cellValue(column('LINK'), '')).toEqual(ok({ type: 'clear' }));
    expect(cellValue(column('NUMBER'), '')).toEqual(ok({ type: 'clear' }));
    expect(cellValue(column('DATE'), null)).toEqual(ok({ type: 'clear' }));
  });

  it('writes checkboxes, numbers, links and dates as the values they are', () => {
    expect(cellValue(column('BOOLEAN'), 1)).toEqual(
      ok({
        type: 'boolean',
        value: true,
      })
    );
    expect(cellValue(column('BOOLEAN'), 'false')).toEqual(
      ok({
        type: 'boolean',
        value: false,
      })
    );
    expect(cellValue(column('NUMBER'), '12.5')).toEqual(
      ok({
        type: 'number',
        value: 12.5,
      })
    );
    expect(cellValue(column('LINK'), 'https://macro.com')).toEqual(
      ok({
        type: 'link',
        value: ['https://macro.com'],
      })
    );
    expect(cellValue(column('DATE'), '2026-09-30')).toEqual(
      ok({
        type: 'date',
        value: '2026-09-30T00:00:00Z',
      })
    );
    expect(cellValue(column('NUMBER'), 'lots')).toEqual(
      err({ kind: 'not-a-number' })
    );
  });
});

describe('grid values a column cannot take', () => {
  it('refuses references to rows outside a relation column', () => {
    const rows: ColumnDetail = {
      column: {
        id: 'column',
        table_id: 'table',
        property_definition_id: 'definition',
        position: 'a',
        config: null,
      },
      sql_name: '"Parties"',
      writable: true,
      definition: {
        definition: {
          id: 'definition',
          owner: { scope: 'database', database_id: 'db' },
          display_name: 'Parties',
          data_type: 'ENTITY',
          is_multi_select: false,
          specific_entity_type: 'DATABASE_ROW',
          created_at: '',
          updated_at: '',
          is_system: false,
          is_metadata: false,
        },
        property_options: [],
      },
    };

    expect(cellValue(rows, 'row-1')).toEqual(
      err({ kind: 'relation-as-entity' })
    );
  });

  it('refuses the whole op when one of its cells is refused', () => {
    expect(
      mutationOp(
        'table',
        { kind: 'create', values: { name: 'Ada', age: 'old' } },
        (columnId) =>
          columnId === 'name'
            ? ok(column('STRING'))
            : err({ kind: 'read-only-column' }),
        false
      )
    ).toEqual(err({ kind: 'read-only-column' }));
  });
});

describe('grid edits as ops', () => {
  it('deletes a record as one delete_rows op', () => {
    expect(
      mutationOp(
        'table',
        { kind: 'delete', rowId: 'row-1' },
        () => ok(column('STRING')),
        false
      )
    ).toEqual(ok({ kind: 'delete_rows', table: 'table', rows: ['row-1'] }));
  });

  it('creates a record with every value it starts with, letting new labels become options', () => {
    expect(
      mutationOp(
        'table',
        { kind: 'create', values: { status: 'New lane' } },
        () => ok(column('SELECT_STRING')),
        true
      )
    ).toEqual(
      ok({
        kind: 'insert_rows',
        table: 'table',
        rows: [
          [
            {
              column: 'status',
              value: { type: 'options', value: [{ label: 'New lane' }] },
            },
          ],
        ],
        createMissingOptions: true,
      })
    );
  });
});
