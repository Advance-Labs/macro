import CaretRightIcon from '@phosphor/caret-right.svg';
import CheckIcon from '@phosphor/check.svg';
import { Dropdown } from '@ui/components/Dropdown';
import { type Accessor, createSignal, For, type JSX, Show } from 'solid-js';
import {
  castFor,
  type DatabaseColumnCast,
  type DatabaseColumnCasts,
  type DatabaseColumnTypeChange,
} from '../core/column-schema';
import type { DatabaseViewColumn } from '../core/database-view';
import { PropertyIcon } from './property-icon';

const types: { label: string; change: DatabaseColumnTypeChange }[] = [
  { label: 'Text', change: { dataType: 'STRING' } },
  { label: 'Number', change: { dataType: 'NUMBER' } },
  { label: 'Select', change: { dataType: 'SELECT_STRING' } },
  {
    label: 'Multi-select',
    change: { dataType: 'SELECT_STRING', isMultiSelect: true },
  },
  { label: 'Date', change: { dataType: 'DATE' } },
  { label: 'Checkbox', change: { dataType: 'BOOLEAN' } },
  { label: 'URL', change: { dataType: 'LINK' } },
  {
    label: 'People',
    change: { dataType: 'ENTITY', specificEntityType: 'USER' },
  },
  {
    label: 'Documents',
    change: { dataType: 'ENTITY', specificEntityType: 'DOCUMENT' },
  },
  {
    label: 'Tasks',
    change: { dataType: 'ENTITY', specificEntityType: 'TASK' },
  },
];

/** A checked choice some values would not survive, awaiting confirmation. */
export type DatabaseColumnClearingChoice = {
  label: string;
  change: DatabaseColumnTypeChange;
  cast: Extract<DatabaseColumnCast, { verdict: 'checked' }>;
};

export function ColumnTypeMenu(props: {
  column: DatabaseViewColumn;
  tables?: { id: string; name: string }[];
  /** The dry run, read while the submenu is open. */
  loadCasts?: (
    open: Accessor<boolean>
  ) => Accessor<DatabaseColumnCasts> | undefined;
  onChange: (change: DatabaseColumnTypeChange) => void;
  onConfirmClearing: (choice: DatabaseColumnClearingChoice) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const casts = props.loadCasts?.(open);
  const castOf = (change: DatabaseColumnTypeChange) =>
    casts ? castFor(casts(), change) : undefined;
  const selected = (change: DatabaseColumnTypeChange) =>
    !props.column.relation &&
    props.column.dataType === change.dataType &&
    props.column.isMultiSelect === !!change.isMultiSelect &&
    (props.column.specificEntityType ?? undefined) ===
      change.specificEntityType;
  /** A type no value converts to is left out; until the dry run answers, nothing is listed. */
  const offered = (change: DatabaseColumnTypeChange) =>
    castOf(change)?.verdict !== 'never';
  const checking = () => casts?.().status === 'loading';
  const offeredTables = () =>
    (props.tables ?? []).filter((table) =>
      offered({
        dataType: 'ENTITY',
        isMultiSelect: true,
        linkToTableId: table.id,
      })
    );
  const choose = (label: string, change: DatabaseColumnTypeChange) => {
    const cast = castOf(change);
    if (cast?.verdict === 'checked' && cast.failures > 0)
      props.onConfirmClearing({ label, change, cast });
    else props.onChange(change);
  };
  return (
    <Dropdown.Sub open={open()} onOpenChange={setOpen}>
      <Dropdown.SubTrigger>
        <PropertyIcon
          type={props.column.dataType}
          entityType={props.column.specificEntityType}
          relation={!!props.column.relation}
        />
        <span class="flex-1">Change type</span>
        <CaretRightIcon class="size-3" />
      </Dropdown.SubTrigger>
      <Dropdown.SubContent class="w-60 max-h-[min(28rem,80vh)] overflow-y-auto">
        <Show when={checking()}>
          <Dropdown.Item disabled>
            <span class="text-xs text-ink-muted">Checking values…</span>
          </Dropdown.Item>
        </Show>
        <Show when={!checking()}>
          <Dropdown.Group>
            <For each={types.filter((type) => offered(type.change))}>
              {(type) => (
                <TypeItem
                  label={type.label}
                  cast={castOf(type.change)}
                  icon={
                    <PropertyIcon
                      type={type.change.dataType}
                      entityType={type.change.specificEntityType}
                    />
                  }
                  selected={selected(type.change)}
                  onSelect={() => choose(type.label, type.change)}
                />
              )}
            </For>
          </Dropdown.Group>
          <Show when={offeredTables().length}>
            <Dropdown.Group>
              <Dropdown.GroupLabel>Related table</Dropdown.GroupLabel>
              <For each={offeredTables()}>
                {(table) => {
                  const change: DatabaseColumnTypeChange = {
                    dataType: 'ENTITY',
                    isMultiSelect: true,
                    linkToTableId: table.id,
                  };
                  return (
                    <TypeItem
                      label={table.name}
                      cast={castOf(change)}
                      icon={<PropertyIcon type="ENTITY" relation />}
                      selected={props.column.relation?.tableId === table.id}
                      onSelect={() => choose(table.name, change)}
                    />
                  );
                }}
              </For>
            </Dropdown.Group>
          </Show>
        </Show>
      </Dropdown.SubContent>
    </Dropdown.Sub>
  );
}

/** One offered type; a checked one says what converting would clear. */
function TypeItem(props: {
  label: string;
  cast: DatabaseColumnCast | undefined;
  icon: JSX.Element;
  selected: boolean;
  onSelect: () => void;
}) {
  const description = () => {
    const cast = props.cast;
    return cast?.verdict === 'checked' ? cast.summary : undefined;
  };
  return (
    <Dropdown.Item onSelect={props.onSelect}>
      {props.icon}
      <span class="flex min-w-0 flex-1 flex-col">
        <Dropdown.ItemLabel class="truncate">{props.label}</Dropdown.ItemLabel>
        <Show when={description()}>
          {(text) => (
            <Dropdown.ItemDescription class="text-xs text-ink-muted">
              {text()}
            </Dropdown.ItemDescription>
          )}
        </Show>
      </span>
      <Show when={props.selected}>
        <CheckIcon class="size-3.5" />
      </Show>
    </Dropdown.Item>
  );
}
