import type { ResultError } from '@core/util/result';
import type { DatabaseSchemaErrorCode } from '@service-storage/databases';
import type { ResultAsync } from 'neverthrow';
import type { Accessor } from 'solid-js';
import type { DatabaseViewColumn } from '../core/database-view';
import type { DatabaseRow, DatabaseRowMutation } from '../core/table';
import type {
  DatabaseReadFailure,
  DatabaseWriteFailure,
} from '../core/write-failure';

export type DatabaseRowsSnapshot = {
  /** The rows the view's statement returned, in its order. */
  rows: DatabaseRow[];
  /** Rows the view holds on to by id, such as an open record, in or out of the view. */
  retained: DatabaseRow[];
  version: number | undefined;
};
export type DatabaseWriteResult = {
  insertedRowIds: string[];
  version: number | undefined;
};

/** Rows are undefined before the first successful read; background errors may coexist with them. */
export type DatabaseRowsSource = {
  columns: Accessor<DatabaseViewColumn[]>;
  snapshot: Accessor<DatabaseRowsSnapshot | undefined>;
  loading: Accessor<boolean>;
  refreshing: Accessor<boolean>;
  error: Accessor<DatabaseReadFailure | undefined>;
  refresh(): ResultAsync<void, DatabaseReadFailure>;
  /** `createOptions` lets labels a column lacks become new options. */
  write(
    mutation: DatabaseRowMutation,
    version: number | undefined,
    createOptions: boolean
  ): ResultAsync<DatabaseWriteResult, DatabaseWriteFailure>;
  addOption(
    columnId: string,
    label: string
  ): ResultAsync<void, ResultError<DatabaseSchemaErrorCode>[]>;
  /** Keep reading these rows by id, whether or not the view shows them. */
  retain(rowIds: Accessor<readonly string[]>): void;
};
