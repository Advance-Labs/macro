import { cn } from '@ui';
import { For } from 'solid-js';
import { KEYPAD_KEYS } from '../core/phone-call';

/** A telephone keypad: digits with their letters, plus `*` and `#`. */
export function Dialpad(props: {
  onKey: (key: string) => void;
  disabled?: boolean;
  class?: string;
}) {
  return (
    <div
      role="group"
      aria-label="Keypad"
      class={cn('grid grid-cols-3 gap-2', props.class)}
    >
      <For each={KEYPAD_KEYS}>
        {(entry) => (
          <button
            type="button"
            disabled={props.disabled}
            aria-label={
              entry.letters ? `${entry.key} ${entry.letters}` : entry.key
            }
            onClick={() => props.onKey(entry.key)}
            class="flex h-12 flex-col items-center justify-center rounded-xl border border-edge-muted bg-control text-ink outline-none transition-colors duration-120 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-edge-focus not-touch:not-disabled:hover:overlay-hover not-touch:not-disabled:active:overlay-active disabled:opacity-50 motion-reduce:transition-none"
          >
            <span class="text-lg font-medium leading-none tabular-nums">
              {entry.key}
            </span>
            <span
              aria-hidden="true"
              class="mt-0.5 h-3 text-[10px] font-semibold tracking-wider text-ink-muted"
            >
              {entry.letters}
            </span>
          </button>
        )}
      </For>
    </div>
  );
}
