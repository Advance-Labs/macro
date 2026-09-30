import { Select } from '@ui/components/Select';
import { createMemo, type JSX } from 'solid-js';

type ViewSelectOption = { value: string; label: string };

/** Compact app-native choice control for view configuration. */
export function ViewSelect(props: {
  label: string;
  value: string;
  options: ViewSelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  class?: string;
  /** Draws an option in the list and in the trigger; its label by default. */
  renderOption?: (option: ViewSelectOption) => JSX.Element;
}) {
  const options = createMemo(() => props.options);
  const render = (option: ViewSelectOption) =>
    props.renderOption ? props.renderOption(option) : option.label;
  return (
    <Select<ViewSelectOption>
      options={options()}
      optionValue="value"
      optionTextValue="label"
      value={options().find((option) => option.value === props.value)}
      onChange={(option) => {
        if (option && option.value !== props.value)
          props.onChange(option.value);
      }}
      placeholder={props.placeholder ?? 'Choose…'}
      disabled={props.disabled}
      class={props.class ?? 'min-w-0 flex-1'}
      itemComponent={(item) => (
        <Select.Item item={item.item}>
          <Select.ItemLabel>{render(item.item.rawValue)}</Select.ItemLabel>
          <Select.ItemIndicator />
        </Select.Item>
      )}
    >
      <Select.Trigger
        aria-label={props.label}
        class="h-8 rounded-md border border-edge-muted bg-input px-2 text-xs outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ink/50"
      >
        <Select.Value<ViewSelectOption>>
          {(state) => render(state.selectedOption())}
        </Select.Value>
        <Select.Icon />
      </Select.Trigger>
      <Select.Content portalScope="local" class="max-h-64 min-w-36 max-w-72">
        <Select.Listbox />
      </Select.Content>
    </Select>
  );
}
