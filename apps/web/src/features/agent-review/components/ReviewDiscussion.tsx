import { StaticMarkdown } from '@core/component/LexicalMarkdown/component/core/StaticMarkdown';
import { channelTheme } from '@core/component/LexicalMarkdown/theme';
import CheckIcon from '@phosphor/check.svg';
import SparkleIcon from '@phosphor/sparkle.svg';
import { Avatar, Button } from '@ui';
import { For, Show } from 'solid-js';
import type { ReviewThread } from '../core/model';
import { ReviewCommentComposer } from './ReviewCommentComposer';

export function ReviewDiscussion(props: {
  thread?: ReviewThread;
  readOnly?: boolean;
  sending?: boolean;
  locked?: boolean;
  note?: string;
  draft: string;
  composing: boolean;
  onDraft: (text: string) => void;
  onSend: () => void;
  onCancel: () => void;
  onResolve: () => void;
  onReply: () => void;
}) {
  return (
    <section
      class="mx-3 my-2 min-w-0 rounded-lg border border-edge-muted bg-panel px-3 py-2 font-sans text-[13px] leading-5 text-ink"
      aria-label={props.note ? 'Agent explanation' : 'Review thread'}
    >
      <Show when={props.note}>
        <div class="py-1">
          <div class="mb-1.5 flex items-center gap-2 text-xs text-ink-muted">
            <span class="grid size-5 place-items-center text-ink-muted">
              <SparkleIcon class="size-3" />
            </span>
            <span class="font-medium text-ink">Agent</span>
          </div>
          <StaticMarkdown
            autoLink
            markdown={props.note ?? ''}
            theme={channelTheme}
            target="internal"
            lazy={false}
          />
        </div>
      </Show>
      <For each={props.thread?.messages ?? []}>
        {(message) => (
          <div class="py-2">
            <div class="mb-1.5 flex items-center gap-2 text-xs text-ink-muted">
              <Avatar size="sm">
                <Avatar.Fallback>
                  <Show
                    when={message.author === 'Agent'}
                    fallback={message.author.slice(0, 1)}
                  >
                    <SparkleIcon class="size-3" />
                  </Show>
                </Avatar.Fallback>
              </Avatar>
              <span class="font-medium text-ink">{message.author}</span>
            </div>
            <StaticMarkdown
              autoLink
              markdown={message.body}
              theme={channelTheme}
              target="internal"
              lazy={false}
            />
            <Show when={message.delivery}>
              <p class="mt-2 text-[10px] text-ink-subtle">
                {message.delivery === 'pending'
                  ? 'Saved · waiting to reach the agent'
                  : message.delivery === 'queued'
                    ? 'Queued in this session'
                    : 'Delivery will retry'}
              </p>
            </Show>
          </div>
        )}
      </For>
      <Show
        when={props.composing}
        fallback={
          <div class="flex items-center justify-between border-t border-edge-muted/60 pt-1.5 mt-2">
            <Button
              variant="plain"
              size="xs"
              disabled={props.readOnly}
              onClick={props.onReply}
            >
              {props.note ? 'Ask about this' : 'Reply'}
            </Button>
            <Show when={props.thread}>
              <Button
                variant="plain"
                size="xs"
                disabled={props.readOnly || props.sending}
                onClick={props.onResolve}
              >
                <CheckIcon />
                {props.thread?.resolved ? 'Reopen' : 'Resolve'}
              </Button>
            </Show>
          </div>
        }
      >
        <ReviewCommentComposer
          draft={props.draft}
          readOnly={props.readOnly}
          sending={props.sending}
          locked={props.locked}
          onDraft={props.onDraft}
          onSend={props.onSend}
          onCancel={props.onCancel}
        />
      </Show>
    </section>
  );
}
