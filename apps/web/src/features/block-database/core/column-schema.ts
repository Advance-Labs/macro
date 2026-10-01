import type { ResultError } from '@core/util/result';
import type { DatabaseSchemaErrorCode } from '@service-storage/databases';
import type { ResultAsync } from 'neverthrow';
import type { Accessor } from 'solid-js';
import { match } from 'ts-pattern';
import type { DatabaseEntityType } from './column-inference';

/** What the tabs say when a table rename was refused. */
export function tableRenameMessage(
  errors: readonly ResultError<DatabaseSchemaErrorCode>[]
): string {
  const error = errors[0];
  return error?.code === 'INVALID_SCHEMA'
    ? error.message
    : 'Could not rename this table. Its name may have changed. Check your connection, or reopen Rename table and try again.';
}

/** What the tabs say when a new tab order was refused. */
export function tableOrderMessage(
  errors: readonly ResultError<DatabaseSchemaErrorCode>[]
): string {
  return match(errors[0]?.code)
    .with(
      'NETWORK_ERROR',
      () => 'Could not move this table. Check your connection and try again.'
    )
    .otherwise(
      () => 'Could not move this table. The tables may have changed; try again.'
    );
}

/** A column change the service applies or refuses. */
export type DatabaseSchemaChange<Value = void> = ResultAsync<
  Value,
  ResultError<DatabaseSchemaErrorCode>[]
>;

/** What the grid says when the service refused a schema change. */
export function columnSchemaMessage(
  errors: readonly ResultError<DatabaseSchemaErrorCode>[]
): string {
  const error = errors[0];
  return match(error?.code)
    .with('INVALID_SCHEMA', () => error?.message ?? '')
    .with('CONFLICT', () => 'This table changed. Refresh and try again.')
    .with('FORBIDDEN', () => 'You can’t change this table.')
    .with('NOT_FOUND', 'GONE', () => 'This table is no longer available.')
    .with(
      'NETWORK_ERROR',
      () => 'Your change could not be sent. Check your connection.'
    )
    .otherwise(() => 'This column could not be updated. Try again.');
}

/** Explicit changes are validated against every stored value by the server. */
export type DatabaseColumnTypeChange = {
  dataType:
    | 'STRING'
    | 'NUMBER'
    | 'BOOLEAN'
    | 'DATE'
    | 'SELECT_STRING'
    | 'LINK'
    | 'ENTITY';
  isMultiSelect?: boolean;
  specificEntityType?: DatabaseEntityType;
  linkToTableId?: string;
  /** Empty the values that do not fit instead of refusing the change. */
  clearInvalid?: boolean;
};

/** What changing a column to one type would do to its values. */
export type DatabaseColumnCast =
  | { verdict: 'safe' }
  | {
      verdict: 'checked';
      /** Cells whose value would not convert. */
      failures: number;
      /** What is wrong with them, e.g. `3 values aren't numbers`. */
      summary: string | undefined;
      examples: string[];
    }
  | { verdict: 'never'; reason: string };

/** A type the menu offers; `relation` stands for every related table. */
export type DatabaseColumnCastTarget = {
  dataType: string;
  isMultiSelect: boolean;
  specificEntityType?: DatabaseEntityType;
  relation: boolean;
};

export type DatabaseColumnCasts =
  | { status: 'loading' }
  | { status: 'error' }
  | {
      status: 'ready';
      casts: { target: DatabaseColumnCastTarget; cast: DatabaseColumnCast }[];
    };

/** A column's dry run, read while `open` holds. */
export type DatabaseColumnCastsSource = (
  columnId: string,
  open: Accessor<boolean>
) => Accessor<DatabaseColumnCasts>;

/** The dry run's answer for one menu choice, once it has one. */
export function castFor(
  casts: DatabaseColumnCasts,
  change: DatabaseColumnTypeChange
): DatabaseColumnCast | undefined {
  if (casts.status !== 'ready') return undefined;
  const relation = !!change.linkToTableId;
  return casts.casts.find(
    ({ target }) =>
      target.relation === relation &&
      (relation ||
        (target.dataType === change.dataType &&
          target.isMultiSelect === !!change.isMultiSelect &&
          target.specificEntityType === change.specificEntityType))
  )?.cast;
}
