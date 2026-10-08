import type { AgentContextDecoratorProps } from '@macro-inc/lexical-core';
import Eye from '@phosphor/eye.svg';
import { type Component, createMemo, createSignal, For, Show } from 'solid-js';
import { parseContext, summaryOf } from './agent-context-tree';
import { AgentContextElement } from './AgentContextElement';

/** What the agent was given beside a prompt: a chip naming it, opening to the context itself. */
export const AgentContext: Component<AgentContextDecoratorProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [raw, setRaw] = createSignal(false);
  const elements = createMemo(() => parseContext(props.text));
  const label = () => {
    const parsed = elements();
    return parsed ? summaryOf(parsed) : 'Context';
  };

  return (
    <div class="my-1 flex flex-col gap-1.5 text-xs text-ink-muted">
      <button
        type="button"
        aria-expanded={open()}
        onClick={() => setOpen(!open())}
        class="flex w-fit select-none items-center gap-1.5 rounded-full border border-edge-muted bg-hover px-2 py-0.5 hover:text-ink"
      >
        <Eye class="size-3.5" />
        <span>{label()}</span>
      </button>
      <Show when={open()}>
        <div class="flex max-h-96 flex-col gap-2 overflow-auto rounded-md border border-edge-muted bg-panel p-3">
          <div class="flex justify-end">
            <button
              type="button"
              onClick={() => setRaw(!raw())}
              class="text-[11px] text-ink-extra-muted hover:text-ink"
            >
              {raw() ? 'Structured' : 'Raw'}
            </button>
          </div>
          <Show
            when={!raw() && elements()}
            fallback={
              <pre class="whitespace-pre-wrap font-mono text-[11px] text-ink-muted">
                {props.text}
              </pre>
            }
          >
            {(parsed) => (
              <div class="flex flex-col gap-3">
                <For each={parsed()}>
                  {(element) => <AgentContextElement element={element} />}
                </For>
              </div>
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
};
