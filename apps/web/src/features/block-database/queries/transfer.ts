import { databaseSqlAnswer } from '@core/database-sql/answer';
import { databaseSqlCatalog } from '@core/database-sql/catalog';
import { readDatabaseSql } from '@queries/database-sql/create-database-sql-query';
import { invalidateDatabase } from '@queries/storage/databases';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseDetail,
  DatabaseTableDetail,
  ImportDatabaseTableRequest,
  SqlValue,
} from '@service-storage/databases';
import { encodeDatabaseCsv } from '../core/csv';
import { resultColumnName, tableRowsStatement } from '../sql';

/** Request IDs survive a transport error; retrying resolves the original import. */
export async function importDatabaseTable(
  databaseId: string,
  request: ImportDatabaseTableRequest
) {
  const result = await storageServiceClient.databases.importTable({
    id: databaseId,
    request,
  });
  if (result.isErr())
    throw Object.assign(
      new Error(
        result.error[0]?.message ?? 'Could not import this CSV. Try again.'
      ),
      { code: result.error[0]?.code }
    );
  // A cache refresh failure must not turn a committed import into a failed one.
  await invalidateDatabase(databaseId);
  return result.value;
}

/** Never silently export a partial read. */
export async function exportDatabaseTableCsv(
  database: DatabaseDetail,
  table: DatabaseTableDetail
): Promise<Blob> {
  const columns = table.columns.filter(
    (column) => column.column.config?.kind !== 'lookup'
  );
  const catalog = databaseSqlCatalog(
    [{ ...database, tables: [table] }],
    database.database.id
  );
  const outcome = await readDatabaseSql({
    catalog,
    sql: tableRowsStatement(table.sql_name),
  });
  if (outcome.truncated)
    throw new Error('This table is too large for CSV export.');
  const result = databaseSqlAnswer(outcome, catalog, [database]).results[0];
  if (!result) throw new Error('The table could not be exported.');
  const indexes = columns.map((column) =>
    result.columns.findIndex((field) => field.name === resultColumnName(column))
  );
  const rows: SqlValue[][] = result.rows.map((row) =>
    indexes.map((index) => row[index])
  );
  return new Blob(
    [
      encodeDatabaseCsv(
        columns.map(
          (column) =>
            column.column.display_name ??
            column.definition.definition.display_name
        ),
        rows
      ),
    ],
    { type: 'text/csv;charset=utf-8' }
  );
}
