/** Relabelling, recolouring and removing a select column's options, shown at once wherever its definition is bound. */
import { TAG_COLOR_OPTIONS } from '@property/tags/tagColors';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { PropertyOption } from '@service-storage/generated/schemas/propertyOption';
import type { ResultAsync } from 'neverthrow';
import type { OptionChange } from '../context/option-editing';
import type { DatabaseOpFailure } from '../core/write-failure';
import { applyOp, patchDetail } from './detail-cache';

type OptionTarget = {
  databaseId: string;
  tableId: string;
  columnId: string;
  optionId: string;
};

function definitionOf(
  detail: DatabaseDetail,
  { tableId, columnId }: OptionTarget
): string | undefined {
  return detail.tables
    .find((table) => table.table.id === tableId)
    ?.columns.find((column) => column.column.id === columnId)?.definition
    .definition.id;
}

/** Every column bound to the target's definition, with its options changed. */
function patchOptions(
  target: OptionTarget,
  change: (options: PropertyOption[]) => PropertyOption[]
) {
  patchDetail(target.databaseId, (detail) => {
    const definition = definitionOf(detail, target);
    return {
      ...detail,
      tables: detail.tables.map((table) => ({
        ...table,
        columns: table.columns.map((column) =>
          column.definition.definition.id === definition
            ? {
                ...column,
                definition: {
                  ...column.definition,
                  property_options: change(column.definition.property_options),
                },
              }
            : column
        ),
      })),
    };
  });
}

function changed(option: PropertyOption, change: OptionChange): PropertyOption {
  const color =
    change.color === undefined
      ? option.color
      : (TAG_COLOR_OPTIONS.find((entry) => entry.value === change.color)
          ?.color ?? option.color);
  if (change.label === undefined) return { ...option, color };
  const value =
    option.value.type === 'number'
      ? { type: option.value.type, value: Number(change.label) }
      : { type: option.value.type, value: change.label };
  return { ...option, color, value };
}

export function updateDatabaseOption(
  target: OptionTarget,
  change: OptionChange
): ResultAsync<void, DatabaseOpFailure> {
  patchOptions(target, (options) =>
    options.map((option) =>
      option.id === target.optionId ? changed(option, change) : option
    )
  );
  return applyOp(
    target.databaseId,
    target.tableId,
    {
      kind: 'update_option',
      table: target.tableId,
      column: target.columnId,
      option: target.optionId,
      ...change,
    },
    'option_changed'
  ).map(() => undefined);
}

/** Remove an option; the cells holding it are emptied of it on the server. */
export function deleteDatabaseOption(
  target: OptionTarget
): ResultAsync<void, DatabaseOpFailure> {
  patchOptions(target, (options) =>
    options.filter((option) => option.id !== target.optionId)
  );
  return applyOp(
    target.databaseId,
    target.tableId,
    {
      kind: 'delete_option',
      table: target.tableId,
      column: target.columnId,
      option: target.optionId,
    },
    'option_changed'
  ).map(() => undefined);
}
