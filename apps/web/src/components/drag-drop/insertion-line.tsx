import { type JSX, splitProps } from 'solid-js';
import type { HorizontalReorderDrop } from './create-horizontal-reorder';

/** Where a horizontal reorder lands; place it in the row's positioned container. */
export function InsertionLine(
  props: JSX.HTMLAttributes<HTMLDivElement> & {
    drop: HorizontalReorderDrop;
    /** The line's vertical extent, e.g. `inset-y-0`. */
    class: string;
  }
) {
  const [local, rest] = splitProps(props, ['drop', 'class']);
  return (
    <div
      {...rest}
      aria-hidden="true"
      data-drop-target={local.drop.targetId}
      data-drop-edge={local.drop.edge}
      class={`pointer-events-none absolute z-2 w-0.5 -translate-x-1/2 bg-accent ${local.class}`}
      style={{ left: `${local.drop.left}px` }}
    />
  );
}
