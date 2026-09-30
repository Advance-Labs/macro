import type { DatabaseColumnDetail } from '@service-storage/databases';
import { describe, expect, it } from 'vitest';
import { cellValue, mutationOp } from './cell-ops';

function column(
  dataType: DatabaseColumnDetail['definition']['definition']['data_type'],
  options: {
    multi?: boolean;
    entityType?: 'USER' | 'DOCUMENT';
    relation?: boolean;
  } = {}
): DatabaseColumnDetail {
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
    expect(cellValue(column('SELECT_STRING'), 'Going')).toEqual({
      type: 'options',
      value: [{ label: 'Going' }],
    });
    expect(
      cellValue(column('TAG', { multi: true }), '["Urgent","Backend"]')
    ).toEqual({
      type: 'options',
      value: [{ label: 'Urgent' }, { label: 'Backend' }],
    });
    expect(cellValue(column('SELECT_NUMBER', { multi: true }), '[]')).toEqual({
      type: 'clear',
    });
  });

  it('writes references with the kind the column points at, and relations as rows', () => {
    expect(
      cellValue(column('ENTITY', { entityType: 'DOCUMENT' }), 'document-1')
    ).toEqual({
      type: 'entities',
      value: [{ entityType: 'DOCUMENT', entityId: 'document-1' }],
    });
    expect(
      cellValue(
        column('ENTITY', { relation: true, multi: true }),
        '["row-1","row-2"]'
      )
    ).toEqual({ type: 'rows', value: ['row-1', 'row-2'] });
    expect(cellValue(column('ENTITY', { relation: true }), '[]')).toEqual({
      type: 'clear',
    });
  });

  it('keeps empty text but clears any other empty cell', () => {
    expect(cellValue(column('STRING'), '')).toEqual({
      type: 'text',
      value: '',
    });
    expect(cellValue(column('LINK'), '')).toEqual({ type: 'clear' });
    expect(cellValue(column('NUMBER'), '')).toEqual({ type: 'clear' });
    expect(cellValue(column('DATE'), null)).toEqual({ type: 'clear' });
  });

  it('writes checkboxes, numbers, links and dates as the values they are', () => {
    expect(cellValue(column('BOOLEAN'), 1)).toEqual({
      type: 'boolean',
      value: true,
    });
    expect(cellValue(column('BOOLEAN'), 'false')).toEqual({
      type: 'boolean',
      value: false,
    });
    expect(cellValue(column('NUMBER'), '12.5')).toEqual({
      type: 'number',
      value: 12.5,
    });
    expect(cellValue(column('LINK'), 'https://macro.com')).toEqual({
      type: 'link',
      value: ['https://macro.com'],
    });
    expect(cellValue(column('DATE'), '2026-09-30')).toEqual({
      type: 'date',
      value: '2026-09-30T00:00:00Z',
    });
    expect(() => cellValue(column('NUMBER'), 'lots')).toThrow(
      'expects a number'
    );
  });
});

describe('grid edits as ops', () => {
  it('deletes a record as one delete_rows op', () => {
    expect(
      mutationOp(
        'table',
        { kind: 'delete', rowId: 'row-1' },
        () => column('STRING'),
        false
      )
    ).toEqual({ kind: 'delete_rows', table: 'table', rows: ['row-1'] });
  });

  it('creates a record with every value it starts with, letting new labels become options', () => {
    expect(
      mutationOp(
        'table',
        { kind: 'create', values: { status: 'New lane' } },
        () => column('SELECT_STRING'),
        true
      )
    ).toEqual({
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
    });
  });
});
