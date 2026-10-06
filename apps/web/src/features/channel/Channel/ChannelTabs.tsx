import { cn } from '@ui';
import type { ParentProps } from 'solid-js';

type ChannelTabLayoutProps = ParentProps<{ class?: string }>;

/**
 * Gives channel chrome its intrinsic height and measures the active tab pane
 * against the space left over. Keep every host on this layout boundary: a
 * pane with `h-full` must fill the remainder, not the pre-chrome container.
 */
export function ChannelTabLayout(props: ChannelTabLayoutProps) {
  return (
    <div class={cn('flex min-h-0 flex-col', props.class)}>
      <div class="flex min-h-0 flex-1 flex-col" data-channel-tab-content>
        {props.children}
      </div>
    </div>
  );
}
