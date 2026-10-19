import { type JSX, Show } from 'solid-js';
import { useSidebarPrefs } from './use-sidebar-prefs';

/**
 * Geometry shared by every rail button, collapsed or popped out.
 *
 * Collapsed, a 40px square with its label in the tooltip. Expanded, a
 * full-width row with the glyph leading a visible label, so the tooltip is
 * redundant and switched off. Expanded rows use the `md` size, not `icon-md`:
 * that size's `size-8 aspect-square` would outlast a `w-full` override.
 */
export function useRailButton() {
  const { expanded } = useSidebarPrefs();
  return {
    expanded,
    size: (): 'md' | 'icon-md' => (expanded() ? 'md' : 'icon-md'),
    class: () =>
      expanded() ? 'h-10 w-full justify-start gap-3 px-2.5' : 'size-10',
    /** For the context-menu wrappers around a rail button. */
    triggerClass: () => (expanded() ? 'h-10 w-full' : 'size-10'),
    buttonProps: () => ({
      fullWidth: expanded(),
      tooltipDisabled: expanded(),
    }),
  };
}

/** A rail button's label, shown only while the rail is expanded. */
export const RailLabel = (props: { children: JSX.Element }) => {
  const { expanded } = useSidebarPrefs();
  return (
    <Show when={expanded()}>
      <span class="pointer-events-none min-w-0 flex-1 truncate text-left">
        {props.children}
      </span>
    </Show>
  );
};
