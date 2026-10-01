/**
 * The databases the viewer has open, as the schema the engine builds its
 * catalog from (`database_sql::catalog::build`, exposed by the wasm module as
 * `buildCatalog`). The server maps its own entries onto the same schema, so a
 * statement names the same tables and columns in the browser as it does for
 * agents.
 */

import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { Schema } from './generated/types';

export function databaseSqlSchema(
  databases: readonly DatabaseDetail[]
): Schema {
  return {
    databases: databases.map(({ database, tables }) => ({
      id: database.id,
      name: database.name,
      tables: tables.map(({ table, columns }) => ({
        id: table.id,
        name: table.name,
        columns: columns
          // Lookups are derived and have no cells.
          .filter(({ column }) => column.config?.kind !== 'lookup')
          .map(({ column, definition }) => ({
            id: column.id,
            definition: definition.definition.id,
            name: column.display_name ?? definition.definition.display_name,
            property: {
              dataType: definition.definition.data_type,
              multi: definition.definition.is_multi_select,
              entityType: definition.definition.specific_entity_type ?? null,
              relation: column.config?.kind === 'link',
            },
            options: definition.property_options.map((option) => ({
              id: option.id,
              value: option.value,
              order: option.display_order,
            })),
          })),
      })),
    })),
    platform: [],
  };
}
