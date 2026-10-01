/** Adding columns and options under ids minted here, refreshing the cached schema once they land. */
import type { OpColumnKind } from '@core/database-sql/generated/types';
import {
  applyDatabaseOps,
  invalidateDatabase,
} from '@queries/storage/databases';
import { v7 as uuidv7 } from 'uuid';
import type { DatabaseSchemaChange } from '../core/column-schema';

/**
 * Add a column at the table's end and return its id. A select column's
 * options go in the same op, each under its own id.
 */
export function createDatabaseColumn(params: {
  databaseId: string;
  tableId: string;
  name: string;
  type: OpColumnKind;
  options?: string[];
  /** Let the column's first value settle its type: only for plain text. */
  inferType?: boolean;
}): DatabaseSchemaChange<string> {
  const id = uuidv7();
  return applyDatabaseOps(params.databaseId, [
    {
      kind: 'create_column',
      table: params.tableId,
      id,
      definition: {
        source: 'new',
        name: params.name,
        type: params.type,
        ...(params.options && {
          options: params.options.map((label) => ({ id: uuidv7(), label })),
        }),
        ...(params.inferType && { inferType: true }),
      },
    },
  ]).map(async () => {
    await invalidateDatabase(params.databaseId);
    return id;
  });
}

/** Add select options to a column; labels it already has are left out. */
export function addDatabaseColumnOptions(params: {
  databaseId: string;
  tableId: string;
  columnId: string;
  labels: string[];
}): DatabaseSchemaChange {
  return applyDatabaseOps(params.databaseId, [
    {
      kind: 'add_options',
      table: params.tableId,
      column: params.columnId,
      options: params.labels.map((label) => ({ id: uuidv7(), label })),
    },
  ]).map(async () => {
    await invalidateDatabase(params.databaseId);
  });
}
