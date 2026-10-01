import type { DatabaseSqlFailure } from '@core/database-sql/driver';
import type { ResultError } from '@core/util/result';
import type {
  DatabaseOpsError,
  DatabaseSchemaErrorCode,
} from '@service-storage/databases';
import { match } from 'ts-pattern';

/** A grid value the column it is written to cannot take. */
export type DatabaseCellFailure =
  | { kind: 'read-only-column' }
  | { kind: 'not-a-number' }
  /** A relation's cells are rows, edited as a relation. */
  | { kind: 'relation-as-entity' };

/** Why a grid write did not land, or may not have. */
export type DatabaseWriteFailure =
  | DatabaseCellFailure
  /** A new column's first value needs a fresh read of the table. */
  | { kind: 'needs-refresh' }
  /** A mention was picked for a column of another type. */
  | { kind: 'type-mismatch' }
  /** Settling a new column's type from its first value was refused. */
  | { kind: 'type-refused'; error: ResultError<DatabaseSchemaErrorCode> }
  /** The service refused the write, or never answered it. */
  | { kind: 'ops'; error: DatabaseOpsError }
  /** A new row may have been saved before its answer was lost. */
  | { kind: 'outcome-unknown' }
  /** The table's schema could not be read again. */
  | { kind: 'table-unavailable' }
  /** The service answered the write with something other than rows. */
  | { kind: 'unexpected-result' };

/** Why the table's rows could not be read again. */
export type DatabaseReadFailure =
  | DatabaseSqlFailure
  | { kind: 'table-unavailable' };

/** What the grid says about a write that did not land. */
export function databaseWriteMessage(failure: DatabaseWriteFailure): string {
  return match(failure)
    .returnType<string>()
    .with({ kind: 'read-only-column' }, () => 'This property is read-only.')
    .with(
      { kind: 'not-a-number' },
      () =>
        'This column expects a number. Your entry is kept so you can correct it.'
    )
    .with(
      { kind: 'relation-as-entity' },
      () => 'This column holds related records; edit it as a relation.'
    )
    .with(
      { kind: 'needs-refresh' },
      () => 'Refresh this table before entering its first value.'
    )
    .with(
      { kind: 'type-mismatch' },
      () =>
        'This column has a different type. Choose a matching mention or add a new column.'
    )
    .with({ kind: 'type-refused' }, ({ error }) =>
      error.code === 'INVALID_SCHEMA'
        ? error.message
        : 'Could not set the column type. Your entry is kept.'
    )
    .with({ kind: 'ops' }, ({ error }) =>
      match(error.code)
        .with('INVALID_OP', () => error.message)
        .with('FORBIDDEN', () => 'You can’t edit this table.')
        .with('CONFLICT', () => 'This table changed. Refresh and try again.')
        .with('NOT_FOUND', 'GONE', () => 'This table is no longer available.')
        .with(
          'NETWORK_ERROR',
          () => 'Your change could not be sent. Check your connection.'
        )
        .otherwise(() => 'The database could not apply that change.')
    )
    .with(
      { kind: 'outcome-unknown' },
      () =>
        'This row may already be saved. Check the latest rows before creating it again. Your draft is kept here.'
    )
    .with(
      { kind: 'table-unavailable' },
      () => 'This table is no longer available.'
    )
    .with(
      { kind: 'unexpected-result' },
      () => 'The database answered the edit with something else.'
    )
    .exhaustive();
}

/** What the grid says about rows it could not read. */
export function databaseReadMessage(failure: DatabaseReadFailure): string {
  return match(failure)
    .returnType<string>()
    .with({ kind: 'engine' }, ({ message }) => message)
    .with(
      { kind: 'fetch' },
      () => 'The rows could not be loaded. Check your connection.'
    )
    .with(
      { kind: 'ops' },
      { kind: 'read-only' },
      () => 'Try refreshing the table.'
    )
    .with(
      { kind: 'table-unavailable' },
      () => 'This table is no longer available.'
    )
    .exhaustive();
}

/** Why renaming or deleting a database did not land. */
export type DatabaseEntityFailure =
  | { kind: 'empty-name' }
  /** The service refused, in its own words. */
  | { kind: 'refused'; message: string }
  | { kind: 'unreachable' };

export function databaseEntityMessage(
  failure: DatabaseEntityFailure,
  action: 'rename' | 'delete'
): string {
  return match(failure)
    .with({ kind: 'empty-name' }, () => 'Give your database a name.')
    .with({ kind: 'refused' }, ({ message }) => message)
    .with({ kind: 'unreachable' }, () =>
      action === 'rename'
        ? 'Could not rename this database.'
        : 'Could not delete this database. Try again.'
    )
    .exhaustive();
}
