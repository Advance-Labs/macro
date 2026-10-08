import type { AgentContextDecoratorProps } from '@macro-inc/lexical-core';
import CaretRight from '@phosphor/caret-right.svg';
import type { Component } from 'solid-js';

/**
 * A quiet disclosure for the channel context supplied to an agent. The toggle
 * sits in the prompt bubble's top padding, so a closed disclosure takes no room.
 */
export const AgentContext: Component<AgentContextDecoratorProps> = (props) => (
  <details class="group text-[10px] text-ink-extra-muted [&+*]:mt-0!">
    <summary class="absolute top-1 right-5 flex list-none select-none items-center gap-0.5 leading-none opacity-70 hover:opacity-100 [&::-webkit-details-marker]:hidden">
      <CaretRight class="size-2.5 transition-transform group-open:rotate-90" />
      Context
    </summary>
    <pre class="mt-2 mb-2 max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-edge-muted bg-panel p-2 font-mono text-[11px] text-ink-muted">
      {props.text}
    </pre>
  </details>
);
