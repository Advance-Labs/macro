/**
 * The engine's catalog from the databases the viewer has open, built the way
 * the server builds it (`crates/databases/src/domain/catalog.rs`
 * `engine_catalog`), so a statement names the same tables and columns in the
 * browser as it does for agents.
 */

import type {
  DatabaseColumnDetail,
  DatabaseDetail,
} from '@service-storage/databases';
import { match } from 'ts-pattern';
import type { Catalog, CatalogTable, ColumnKind } from './protocol';

export function databaseSqlCatalog(
  databases: readonly DatabaseDetail[],
  scope?: string
): Catalog {
  const entries = databases.flatMap(({ database, tables }) =>
    tables.map(({ table, columns }) => ({ database, table, columns }))
  );
  // A table of another database named like one of the scoped database's
  // would make the statement ambiguous; the scoped one wins.
  const scoped = new Set(
    entries
      .filter((entry) => entry.database.id === scope)
      .map(({ database, table }) => qualifiedName(database.name, table.name))
  );
  return {
    tables: entries
      .filter(
        ({ database, table }) =>
          database.id === scope ||
          !scoped.has(qualifiedName(database.name, table.name))
      )
      .map(
        ({ database, table, columns }): CatalogTable => ({
          id: table.id,
          database: database.name,
          name: table.name,
          source: 'database',
          columns: columns
            // Lookups are derived and have no cells.
            .filter((column) => column.column.config?.kind !== 'lookup')
            .map((column) => ({
              id: column.definition.definition.id,
              name:
                column.column.display_name ??
                column.definition.definition.display_name,
              kind: columnKind(column),
            })),
        })
      ),
  };
}

function qualifiedName(database: string, table: string): string {
  return `${database.toLowerCase()}.${table.toLowerCase()}`;
}

function columnKind({ column, definition }: DatabaseColumnDetail): ColumnKind {
  if (column.config?.kind === 'link')
    return { kind: 'entity', multi: true, target: 'DATABASE_ROW' };
  const multi = definition.definition.is_multi_select;
  return match(definition.definition.data_type)
    .returnType<ColumnKind>()
    .with('STRING', () => ({ kind: 'text' }))
    .with('NUMBER', () => ({ kind: 'number' }))
    .with('BOOLEAN', () => ({ kind: 'boolean' }))
    .with('DATE', () => ({ kind: 'date' }))
    .with('LINK', () => ({ kind: 'link' }))
    .with('SELECT_STRING', 'SELECT_NUMBER', 'TAG', () => ({
      kind: 'select',
      multi,
      options: [...definition.property_options]
        .sort((left, right) => left.display_order - right.display_order)
        .map((option) => ({
          id: option.id,
          label: match(option.value)
            .with({ type: 'string' }, ({ value }) => value)
            .with({ type: 'number' }, ({ value }) => numberLabel(value))
            .exhaustive(),
        })),
    }))
    .with('ENTITY', () => ({
      kind: 'entity',
      multi,
      target: definition.definition.specific_entity_type ?? 'USER',
    }))
    .exhaustive();
}

/** A number the way the server labels it: no trailing `.0` on whole numbers. */
function numberLabel(value: number): string {
  return Number.isInteger(value) && Math.abs(value) < 1e15
    ? value.toFixed(0)
    : String(value);
}
