import { InteractionCard } from '@app/features/agent-interactions/components/InteractionCard';
import { StaticMarkdown } from '@core/component/LexicalMarkdown/component/core/StaticMarkdown';
import { toast } from '@core/component/Toast/Toast';
import { Button } from '@ui';
import { For, type ParentProps, Show } from 'solid-js';
import { useOptionalAgentDm } from './context';
import { useAgentDmControl } from './queries/controls';

/** A context boundary belongs to its first source message, including on reload. */
export function AgentDmContextBoundary(props: { messageId: string }) {
  const context = useOptionalAgentDm();
  const startsContext = () => {
    const data = context?.conversation();
    return data?.segments
      .slice(1)
      .some(
        (segment) =>
          data.turns.find((turn) => turn.sessionId === segment.sessionId)
            ?.sourceMessageId === props.messageId
      );
  };
  return (
    <Show when={startsContext()}>
      <div
        role="separator"
        class="my-5 flex items-center gap-3 text-xs text-ink-muted"
      >
        <span class="h-px flex-1 bg-edge-muted" />
        Fresh context · earlier messages kept
        <span class="h-px flex-1 bg-edge-muted" />
      </div>
    </Show>
  );
}

/** Durable per-message status, with an explicit retry of the exact failed attempt. */
export function AgentDmMessageStatus(props: { messageId: string }) {
  const context = useOptionalAgentDm();
  if (!context) return null;
  const turn = () =>
    context
      .conversation()
      ?.turns.find((turn) => turn.sourceMessageId === props.messageId);
  const retryable = () => {
    const data = context.conversation();
    return (
      data?.available &&
      data.segments.some(
        (segment) =>
          segment.isCurrent && segment.sessionId === turn()?.sessionId
      )
    );
  };
  const failed = () =>
    ['failed', 'stopped', 'interrupted'].includes(turn()?.state ?? '');
  const mutation = useAgentDmControl({
    onSuccess: context.refresh,
    onError: () =>
      toast.failure('Could not retry this message. Refresh and try again.'),
  });
  return (
    <Show when={turn()?.state === 'queued' || failed()}>
      <div
        class="ml-14 flex items-center gap-3 pb-2 text-xs text-ink-muted"
        role="status"
      >
        <span>
          {turn()?.state === 'queued'
            ? 'Queued'
            : turn()?.state === 'interrupted'
              ? 'Interrupted. Review completed actions before retrying.'
              : turn()?.state === 'stopped'
                ? 'Stopped'
                : 'Could not finish this message.'}
        </span>
        <Show when={failed() && retryable()}>
          <Button
            size="xs"
            variant="ghost"
            disabled={mutation.isPending}
            onClick={() => {
              const record = turn();
              const data = context.conversation();
              if (record && data)
                mutation.mutate({
                  type: 'retry',
                  channelId: data.channelId,
                  turn: record,
                });
            }}
          >
            Retry message
          </Button>
        </Show>
      </div>
    </Show>
  );
}

/** The normal message layout hosts streamed text and live decisions in place. */
export function AgentDmReplyContent(props: ParentProps<{ messageId: string }>) {
  const context = useOptionalAgentDm();
  if (!context) return props.children;
  const record = () =>
    context
      .conversation()
      ?.turns.find(
        (turn) =>
          turn.replyMessageId === props.messageId && turn.state === 'running'
      );
  const prompt = () =>
    context
      .messages()
      .find((message) => message.requestId === record()?.actionId);
  const reply = () =>
    context
      .messages()
      .find(
        (message) =>
          message.author.kind === 'agent' && message.turn === prompt()?.turn
      );
  const text = () =>
    reply()
      ?.parts.flatMap((part) => (part.kind === 'text' ? [part.text] : []))
      .join('\n\n') ?? '';
  const pending = () =>
    context.interactions
      .pending()
      .filter((request) => request.turn === prompt()?.turn);
  const keys = () =>
    pending().map((request) =>
      JSON.stringify([request.kind, request.requestId, request.turn])
    );
  const requestFor = (key: string) =>
    pending().find(
      (request) =>
        JSON.stringify([request.kind, request.requestId, request.turn]) === key
    );
  const workingTool = () =>
    reply()?.parts.findLast(
      (part) => part.kind === 'tool_use' && part.status === 'running'
    );
  const toolFor = (toolId: string | null) => {
    const part = reply()?.parts.find(
      (part) => part.kind === 'tool_use' && part.id === toolId
    );
    return part?.kind === 'tool_use' ? part : undefined;
  };
  const status = () =>
    pending().length
      ? 'Waiting for you'
      : workingTool()
        ? 'Using tools…'
        : text()
          ? 'Responding…'
          : 'Thinking…';
  return (
    <Show when={record()} fallback={props.children}>
      <div class="space-y-3">
        <Show when={text()}>
          <StaticMarkdown markdown={text()} />
        </Show>
        <div role="status" aria-live="polite" class="text-xs text-ink-muted">
          {status()}
        </div>
        <Show when={context.liveFailed()}>
          <div class="flex items-center gap-2 text-xs text-ink-muted">
            Live updates disconnected.
            <Button size="xs" variant="ghost" onClick={context.retryLive}>
              Reconnect
            </Button>
          </div>
        </Show>
        <For each={keys()}>
          {(key) => (
            <Show when={requestFor(key)}>
              {(request) => (
                <InteractionCard
                  request={request()}
                  controller={context.interactions}
                  tool={toolFor(request().toolCall)}
                />
              )}
            </Show>
          )}
        </For>
      </div>
    </Show>
  );
}
