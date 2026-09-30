import type { DatabaseDetail } from '@service-storage/databases';
import { describe, expect, it } from 'vitest';
import { databaseSqlAnswer } from './answer';
import type { Catalog } from './protocol';

const timestamps = {
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

const party: DatabaseDetail = {
  database: {
    id: 'db-party',
    name: 'Party Planner',
    owner_id: 'macro|owner@databases.test',
    created_at: '2026-01-01T00:00:00Z',
    trashed_at: null,
  },
  grant: 'edit',
  tables: [
    {
      table: {
        id: 'table-guests',
        database_id: 'db-party',
        name: 'Guests',
        position: '000000000001',
        version: 7,
      },
      sql_name: '"Party Planner"."Guests"',
      columns: [
        {
          column: {
            id: 'column-name',
            table_id: 'table-guests',
            property_definition_id: 'def-name',
            position: '000000000001',
            config: null,
          },
          sql_name: '"Name"',
          writable: true,
          definition: {
            definition: {
              id: 'def-name',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'Name',
              data_type: 'STRING',
              is_multi_select: false,
              specific_entity_type: null,
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [],
          },
        },
        {
          column: {
            id: 'column-rsvp',
            table_id: 'table-guests',
            property_definition_id: 'def-rsvp',
            position: '000000000002',
            config: null,
          },
          sql_name: '"RSVP"',
          writable: true,
          definition: {
            definition: {
              id: 'def-rsvp',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'RSVP',
              data_type: 'SELECT_STRING',
              is_multi_select: false,
              specific_entity_type: null,
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [
              {
                id: 'option-yes',
                property_definition_id: 'def-rsvp',
                display_order: 0,
                value: { type: 'string', value: 'Yes' },
                ...timestamps,
              },
            ],
          },
        },
        {
          column: {
            id: 'column-diet',
            table_id: 'table-guests',
            property_definition_id: 'def-diet',
            position: '000000000003',
            config: null,
          },
          sql_name: '"Diet"',
          writable: true,
          definition: {
            definition: {
              id: 'def-diet',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'Diet',
              data_type: 'SELECT_STRING',
              is_multi_select: true,
              specific_entity_type: null,
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [
              {
                id: 'option-vegan',
                property_definition_id: 'def-diet',
                display_order: 0,
                value: { type: 'string', value: 'Vegan' },
                ...timestamps,
              },
              {
                id: 'option-nuts',
                property_definition_id: 'def-diet',
                display_order: 1,
                value: { type: 'string', value: 'No nuts' },
                ...timestamps,
              },
            ],
          },
        },
        {
          column: {
            id: 'column-parties',
            table_id: 'table-guests',
            property_definition_id: 'def-parties',
            position: '000000000004',
            config: {
              kind: 'link',
              database_id: 'db-party',
              table_id: 'table-parties',
            },
          },
          sql_name: '"Parties"',
          writable: true,
          definition: {
            definition: {
              id: 'def-parties',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'Parties',
              data_type: 'ENTITY',
              is_multi_select: true,
              specific_entity_type: 'DATABASE_ROW',
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [],
          },
        },
        {
          column: {
            id: 'column-host',
            table_id: 'table-guests',
            property_definition_id: 'def-host',
            position: '000000000005',
            config: null,
          },
          sql_name: '"Host"',
          writable: true,
          definition: {
            definition: {
              id: 'def-host',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'Host',
              data_type: 'ENTITY',
              is_multi_select: false,
              specific_entity_type: 'USER',
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [],
          },
        },
        {
          column: {
            id: 'column-arrives',
            table_id: 'table-guests',
            property_definition_id: 'def-arrives',
            position: '000000000006',
            config: null,
          },
          sql_name: '"Arrives"',
          writable: true,
          definition: {
            definition: {
              id: 'def-arrives',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'Arrives',
              data_type: 'DATE',
              is_multi_select: false,
              specific_entity_type: null,
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [],
          },
        },
        {
          column: {
            id: 'column-plus-one',
            table_id: 'table-guests',
            property_definition_id: 'def-plus-one',
            position: '000000000007',
            config: null,
          },
          sql_name: '"Plus one"',
          writable: true,
          definition: {
            definition: {
              id: 'def-plus-one',
              owner: { scope: 'database', database_id: 'db-party' },
              display_name: 'Plus one',
              data_type: 'BOOLEAN',
              is_multi_select: false,
              specific_entity_type: null,
              is_system: false,
              is_metadata: false,
              ...timestamps,
            },
            property_options: [],
          },
        },
      ],
    },
  ],
};

const catalog: Catalog = {
  tables: [
    {
      id: 'table-guests',
      database: 'Party Planner',
      name: 'Guests',
      source: 'database',
      columns: [
        { id: 'def-name', name: 'Name', kind: { kind: 'text' } },
        {
          id: 'def-rsvp',
          name: 'RSVP',
          kind: {
            kind: 'select',
            multi: false,
            options: [{ id: 'option-yes', label: 'Yes' }],
          },
        },
        {
          id: 'def-diet',
          name: 'Diet',
          kind: {
            kind: 'select',
            multi: true,
            options: [
              { id: 'option-vegan', label: 'Vegan' },
              { id: 'option-nuts', label: 'No nuts' },
            ],
          },
        },
        {
          id: 'def-parties',
          name: 'Parties',
          kind: { kind: 'entity', multi: true, target: 'DATABASE_ROW' },
        },
        {
          id: 'def-host',
          name: 'Host',
          kind: { kind: 'entity', multi: false, target: 'USER' },
        },
        { id: 'def-arrives', name: 'Arrives', kind: { kind: 'date' } },
        { id: 'def-plus-one', name: 'Plus one', kind: { kind: 'boolean' } },
      ],
    },
  ],
};

describe('databaseSqlAnswer', () => {
  it('spells a row-shaped result the way the exec API does', () => {
    const answer = databaseSqlAnswer(
      {
        columns: [
          { name: 'Name', column: 'def-name', kind: 'text' },
          { name: 'RSVP', column: 'def-rsvp', kind: 'select' },
          { name: 'Diet', column: 'def-diet', kind: 'select' },
          { name: 'Parties', column: 'def-parties', kind: 'entity' },
          { name: 'Host', column: 'def-host', kind: 'entity' },
          { name: 'Arrives', column: 'def-arrives', kind: 'date' },
          { name: 'Plus one', column: 'def-plus-one', kind: 'boolean' },
        ],
        rows: [
          [
            { type: 'text', value: 'Ada' },
            { type: 'options', value: ['option-yes'] },
            { type: 'options', value: ['option-vegan', 'option-nuts'] },
            { type: 'entities', value: ['row-party'] },
            { type: 'entities', value: ['macro|ada@databases.test'] },
            { type: 'date', value: '2026-06-01T18:30:00Z' },
            { type: 'bool', value: true },
          ],
          [
            { type: 'text', value: 'Grace' },
            null,
            null,
            null,
            null,
            null,
            { type: 'bool', value: false },
          ],
        ],
        rowIds: ['row-ada', 'row-grace'],
        readTables: ['table-guests'],
        truncated: false,
        insertedRowIds: [],
        changesApplied: 0,
        failures: [],
      },
      catalog,
      [party]
    );

    expect(answer).toEqual({
      results: [
        {
          columns: [
            { name: 'row_id', entity_type: null, origin: null },
            { name: 'Name', entity_type: null, origin: ['Guests', 'Name'] },
            { name: 'RSVP', entity_type: null, origin: ['Guests', 'RSVP'] },
            { name: 'Diet', entity_type: null, origin: ['Guests', 'Diet'] },
            {
              name: 'Parties',
              entity_type: 'database_row',
              origin: ['Guests', 'Parties'],
            },
            { name: 'Host', entity_type: 'user', origin: ['Guests', 'Host'] },
            {
              name: 'Arrives',
              entity_type: null,
              origin: ['Guests', 'Arrives'],
            },
            {
              name: 'Plus one',
              entity_type: null,
              origin: ['Guests', 'Plus one'],
            },
          ],
          rows: [
            [
              'row-ada',
              'Ada',
              'Yes',
              '["Vegan","No nuts"]',
              '["row-party"]',
              'macro|ada@databases.test',
              '2026-06-01T18:30:00+00:00',
              1,
            ],
            ['row-grace', 'Grace', null, null, null, null, null, 0],
          ],
        },
      ],
      read_tables: ['table-guests'],
      read_database_ids: ['db-party'],
      read_versions: { 'table-guests': 7 },
      truncated_tables: [],
    });
  });

  it('keeps an aggregate without row ids and names truncated tables', () => {
    const answer = databaseSqlAnswer(
      {
        columns: [
          { name: 'RSVP', column: 'def-rsvp', kind: 'select' },
          { name: 'COUNT(*)', kind: 'number' },
        ],
        rows: [
          [
            { type: 'options', value: ['option-yes'] },
            { type: 'number', value: 2 },
          ],
          [null, { type: 'number', value: 1 }],
        ],
        rowIds: [],
        readTables: ['table-guests'],
        truncated: true,
        insertedRowIds: [],
        changesApplied: 0,
        failures: [],
      },
      catalog,
      [party]
    );

    expect(answer).toEqual({
      results: [
        {
          columns: [
            { name: 'RSVP', entity_type: null, origin: ['Guests', 'RSVP'] },
            { name: 'COUNT(*)', entity_type: null, origin: null },
          ],
          rows: [
            ['Yes', 2],
            [null, 1],
          ],
        },
      ],
      read_tables: ['table-guests'],
      read_database_ids: ['db-party'],
      read_versions: { 'table-guests': 7 },
      truncated_tables: ['Guests'],
    });
  });

  it('answers an empty row read with its columns and row ids', () => {
    const answer = databaseSqlAnswer(
      {
        columns: [{ name: 'Name', column: 'def-name', kind: 'text' }],
        rows: [],
        rowIds: [],
        readTables: ['table-guests'],
        truncated: false,
        insertedRowIds: [],
        changesApplied: 0,
        failures: [],
      },
      catalog,
      [party]
    );

    expect(answer.results).toEqual([
      {
        columns: [
          { name: 'row_id', entity_type: null, origin: null },
          { name: 'Name', entity_type: null, origin: ['Guests', 'Name'] },
        ],
        rows: [],
      },
    ]);
  });
});
