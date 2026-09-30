import type { Accessor } from 'solid-js';
import type { DatabaseEntityType } from './column-inference';

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
