import type { Accessor } from 'solid-js';
import type { DatabaseViewColumn } from '../core/database-view';
import type { DatabaseRow, DatabaseRowMutation } from '../core/table';

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
  error: Accessor<Error | undefined>;
  refresh(): Promise<void>;
  write(
    mutation: DatabaseRowMutation,
    version: number | undefined
  ): Promise<DatabaseWriteResult>;
  addOption(columnId: string, label: string): Promise<void>;
  /** Keep reading these rows by id, whether or not the view shows them. */
  retain(rowIds: Accessor<readonly string[]>): void;
};

/** A create request may have committed before its response was lost. */
export class DatabaseWriteOutcomeUnknown extends Error {}
