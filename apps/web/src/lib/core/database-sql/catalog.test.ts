import type { DatabaseDetail } from '@service-storage/databases';
import { describe, expect, it } from 'vitest';
import { databaseSqlCatalog } from './catalog';

const definition = (
  id: string,
  display_name: string,
  data_type: DatabaseDetail['tables'][number]['columns'][number]['definition']['definition']['data_type'],
  is_multi_select = false
) => ({
  id,
  owner: { scope: 'database' as const, database_id: 'db-crm' },
  display_name,
  data_type,
  is_multi_select,
  specific_entity_type: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  is_system: false,
  is_metadata: false,
});

const option = (
  id: string,
  display_order: number,
  value: { type: 'string'; value: string } | { type: 'number'; value: number }
) => ({
  id,
  property_definition_id: 'def-stage',
  display_order,
  value,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
});

const crm: DatabaseDetail = {
  database: {
    id: 'db-crm',
    name: 'CRM',
    owner_id: 'macro|owner@databases.test',
    created_at: '2026-01-01T00:00:00Z',
    trashed_at: null,
  },
  grant: 'edit',
  tables: [
    {
      table: {
        id: 'table-deals',
        database_id: 'db-crm',
        name: 'Deals',
        position: 'a',
        version: 3,
      },
      sql_name: '"Deals"',
      columns: [
        {
          column: {
            id: 'column-name',
            table_id: 'table-deals',
            property_definition_id: 'def-name',
            position: 'a',
            config: null,
            display_name: 'Deal name',
          },
          sql_name: '"Deal name"',
          definition: {
            definition: definition('def-name', 'Name', 'STRING'),
            property_options: [],
          },
          writable: true,
        },
        {
          column: {
            id: 'column-stage',
            table_id: 'table-deals',
            property_definition_id: 'def-stage',
            position: 'b',
            config: null,
          },
          sql_name: '"Stage"',
          definition: {
            definition: definition('def-stage', 'Stage', 'SELECT_STRING'),
            property_options: [
              option('option-won', 1, { type: 'string', value: 'Won' }),
              option('option-lead', 0, { type: 'string', value: 'Lead' }),
            ],
          },
          writable: true,
        },
        {
          column: {
            id: 'column-tier',
            table_id: 'table-deals',
            property_definition_id: 'def-tier',
            position: 'c',
            config: null,
          },
          sql_name: '"Tier"',
          definition: {
            definition: definition('def-tier', 'Tier', 'SELECT_NUMBER', true),
            property_options: [
              option('option-two', 0, { type: 'number', value: 2 }),
              option('option-half', 1, { type: 'number', value: 2.5 }),
            ],
          },
          writable: true,
        },
        {
          column: {
            id: 'column-contact',
            table_id: 'table-deals',
            property_definition_id: 'def-contact',
            position: 'd',
            config: {
              kind: 'link',
              database_id: 'db-crm',
              table_id: 'table-contacts',
            },
          },
          sql_name: '"Contact"',
          definition: {
            definition: definition('def-contact', 'Contact', 'ENTITY'),
            property_options: [],
          },
          writable: true,
        },
        {
          column: {
            id: 'column-contact-email',
            table_id: 'table-deals',
            property_definition_id: 'def-contact-email',
            position: 'e',
            config: {
              kind: 'lookup',
              via_column_id: 'column-contact',
              target: 'column-email',
            },
          },
          sql_name: '"Contact email"',
          definition: {
            definition: definition(
              'def-contact-email',
              'Contact email',
              'STRING'
            ),
            property_options: [],
          },
          writable: false,
        },
      ],
    },
    {
      table: {
        id: 'table-contacts',
        database_id: 'db-crm',
        name: 'Contacts',
        position: 'b',
        version: 1,
      },
      sql_name: '"Contacts"',
      columns: [
        {
          column: {
            id: 'column-email',
            table_id: 'table-contacts',
            property_definition_id: 'def-email',
            position: 'a',
            config: null,
          },
          sql_name: '"Email"',
          definition: {
            definition: definition('def-email', 'Email', 'LINK'),
            property_options: [],
          },
          writable: true,
        },
        {
          column: {
            id: 'column-owner',
            table_id: 'table-contacts',
            property_definition_id: 'def-owner',
            position: 'b',
            config: null,
          },
          sql_name: '"Owner"',
          definition: {
            definition: definition('def-owner', 'Owner', 'ENTITY', false),
            property_options: [],
          },
          writable: true,
        },
      ],
    },
  ],
};

describe('databaseSqlCatalog', () => {
  it('names tables and columns the way the server catalog does', () => {
    expect(databaseSqlCatalog([crm])).toEqual({
      tables: [
        {
          id: 'table-deals',
          database: 'CRM',
          name: 'Deals',
          source: 'database',
          columns: [
            { id: 'def-name', name: 'Deal name', kind: { kind: 'text' } },
            {
              id: 'def-stage',
              name: 'Stage',
              kind: {
                kind: 'select',
                multi: false,
                options: [
                  { id: 'option-lead', label: 'Lead' },
                  { id: 'option-won', label: 'Won' },
                ],
              },
            },
            {
              id: 'def-tier',
              name: 'Tier',
              kind: {
                kind: 'select',
                multi: true,
                options: [
                  { id: 'option-two', label: '2' },
                  { id: 'option-half', label: '2.5' },
                ],
              },
            },
            {
              id: 'def-contact',
              name: 'Contact',
              kind: { kind: 'entity', multi: true },
            },
          ],
        },
        {
          id: 'table-contacts',
          database: 'CRM',
          name: 'Contacts',
          source: 'database',
          columns: [
            { id: 'def-email', name: 'Email', kind: { kind: 'link' } },
            {
              id: 'def-owner',
              name: 'Owner',
              kind: { kind: 'entity', multi: false },
            },
          ],
        },
      ],
    });
  });

  it('lets the scoped database win a table name another database also uses', () => {
    const other: DatabaseDetail = {
      ...crm,
      database: { ...crm.database, id: 'db-other', name: 'crm' },
      tables: [
        {
          ...crm.tables[1],
          table: {
            ...crm.tables[1].table,
            id: 'table-other-contacts',
            database_id: 'db-other',
            name: 'contacts',
          },
        },
        {
          ...crm.tables[1],
          table: {
            ...crm.tables[1].table,
            id: 'table-other-leads',
            database_id: 'db-other',
            name: 'Leads',
          },
        },
      ],
    };

    expect(
      databaseSqlCatalog([crm, other], 'db-crm').tables.map((table) => table.id)
    ).toEqual(['table-deals', 'table-contacts', 'table-other-leads']);
  });
});
