import PlusIcon from '@phosphor/plus.svg';
import XIcon from '@phosphor/x.svg';
import { Button } from '@ui/components/Button';
import { Index, Show } from 'solid-js';
import {
  type DatabaseFilter,
  type DatabaseFilterConjunction,
  type DatabaseViewColumn,
  databaseFilterKind,
  filterNeedsValue,
  filterOperatorsFor,
} from '../core/database-view';
import { SelectPill } from './select-pill';
import { ViewSelect } from './view-select';

const CONJUNCTIONS: { value: DatabaseFilterConjunction; label: string }[] = [
  { value: 'and', label: 'And' },
  { value: 'or', label: 'Or' },
];

function FilterValue(props: {
  column: DatabaseViewColumn | undefined;
  value: string;
  onChange: (value: string) => void;
}) {
  const options = () =>
    props.column?.dataType === 'BOOLEAN'
      ? [
          { value: '1', label: 'Checked' },
          { value: '0', label: 'Unchecked' },
        ]
      : (props.column?.options.map((option) => ({
          value: String(option),
          label: String(option),
        })) ?? []);
  return (
    <Show
      when={options().length}
      fallback={
        <input
          aria-label="Filter value"
          type={
            props.column?.dataType === 'NUMBER'
              ? 'number'
              : props.column?.dataType === 'DATE'
                ? 'date'
                : 'text'
          }
          value={props.value}
          onInput={(event) => props.onChange(event.currentTarget.value)}
          placeholder="Enter a value…"
          class="h-8 min-w-28 flex-1 rounded-md border border-edge-muted bg-input px-2 text-xs outline-none placeholder:text-ink-placeholder focus:border-ink/50"
        />
      }
    >
      <ViewSelect
        label="Filter value"
        value={props.value}
        onChange={props.onChange}
        options={options()}
        placeholder="Choose"
        renderOption={
          props.column && databaseFilterKind(props.column) === 'select'
            ? (option) => (
                <SelectPill label={option.label} column={props.column} />
              )
            : undefined
        }
      />
    </Show>
  );
}

export function FilterPanel(props: {
  columns: DatabaseViewColumn[];
  filters: DatabaseFilter[];
  /** One conjunction joins every condition; mixing them per row would hide precedence. */
  conjunction: DatabaseFilterConjunction;
  onChange: (filters: DatabaseFilter[]) => void;
  onConjunctionChange: (conjunction: DatabaseFilterConjunction) => void;
}) {
  const patch = (id: string, change: Partial<DatabaseFilter>) =>
    props.onChange(
      props.filters.map((filter) =>
        filter.id === id ? { ...filter, ...change } : filter
      )
    );
  const add = () => {
    const column = props.columns[0];
    if (!column) return;
    props.onChange([
      ...props.filters,
      {
        id: crypto.randomUUID(),
        columnId: column.id,
        operator: filterOperatorsFor(column)[0].value,
        value: '',
      },
    ]);
  };
  return (
    <div class="w-118 max-w-full">
      <Show when={!props.filters.length}>
        <p class="mb-3 max-w-72 text-xs leading-relaxed text-ink-muted">
          Choose which records to show.
        </p>
      </Show>
      <div class="flex max-h-72 flex-col gap-2 overflow-auto">
        <Index each={props.filters}>
          {(filter, index) => {
            const column = () =>
              props.columns.find((item) => item.id === filter().columnId);
            return (
              <div class="flex flex-wrap items-center gap-1.5">
                <Show
                  when={index > 0}
                  fallback={
                    <span class="w-16 shrink-0 px-2 text-xs text-ink-muted">
                      Where
                    </span>
                  }
                >
                  <ViewSelect
                    label="Match conditions with"
                    value={props.conjunction}
                    class="w-16 shrink-0"
                    options={CONJUNCTIONS}
                    onChange={(value) => {
                      const next = CONJUNCTIONS.find(
                        (option) => option.value === value
                      );
                      if (next) props.onConjunctionChange(next.value);
                    }}
                  />
                </Show>
                <ViewSelect
                  label="Filter property"
                  value={filter().columnId}
                  class="w-28 min-w-0"
                  options={props.columns.map((item) => ({
                    value: item.id,
                    label: item.name,
                  }))}
                  onChange={(value) => {
                    const next = props.columns.find(
                      (item) => item.id === value
                    );
                    if (next)
                      patch(filter().id, {
                        columnId: next.id,
                        operator: filterOperatorsFor(next)[0].value,
                        value: '',
                      });
                  }}
                />
                <ViewSelect
                  label="Filter condition"
                  value={filter().operator}
                  class="w-32 min-w-0"
                  options={column() ? filterOperatorsFor(column()!) : []}
                  onChange={(value) => {
                    const current = column();
                    const next =
                      current &&
                      filterOperatorsFor(current).find(
                        (option) => option.value === value
                      );
                    if (next) patch(filter().id, { operator: next.value });
                  }}
                />
                <Show when={filterNeedsValue(filter().operator)}>
                  <FilterValue
                    column={column()}
                    value={filter().value}
                    onChange={(value) => patch(filter().id, { value })}
                  />
                </Show>
                <button
                  type="button"
                  aria-label="Remove filter"
                  class="rounded-md p-1.5 text-ink-muted hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50"
                  onClick={() =>
                    props.onChange(
                      props.filters.filter((item) => item.id !== filter().id)
                    )
                  }
                >
                  <XIcon class="size-3.5" />
                </button>
              </div>
            );
          }}
        </Index>
      </div>
      <div class="mt-3 flex items-center justify-between">
        <Button
          variant="ghost"
          size="sm"
          disabled={!props.columns.length}
          onClick={add}
        >
          <PlusIcon class="size-3.5" /> Add condition
        </Button>
        <Show when={props.filters.length}>
          <Button variant="ghost" size="sm" onClick={() => props.onChange([])}>
            Clear filters
          </Button>
        </Show>
      </div>
    </div>
  );
}
