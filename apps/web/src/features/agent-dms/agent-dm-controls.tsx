import { toast } from '@core/component/Toast/Toast';
import type { AgentDmConversationResponse } from '@service-agent-harness/direct-messages';
import { Show } from 'solid-js';
import { ContextControls } from './components/context-controls';
import { ConversationActivity } from './components/conversation-activity';
import { useOptionalAgentDm } from './context';
import { useAgentDmControl } from './queries/controls';

/** Session controls for the current segment, independent of the agent block. */
export function AgentDmControls(props: {
  conversation: AgentDmConversationResponse;
  onChanged: () => void;
}) {
  const context = useOptionalAgentDm();
  const current = () =>
    props.conversation.segments?.find((segment) => segment.isCurrent)
      ?.sessionId;
  const turns = () =>
    (props.conversation.turns ?? []).filter(
      (turn) => turn.sessionId === current()
    );
  const running = () => turns().find((turn) => turn.state === 'running');
  const queued = () => turns().filter((turn) => turn.state === 'queued').length;
  const failed = () =>
    turns().findLast(
      (turn) =>
        turn.state === 'failed' ||
        turn.state === 'stopped' ||
        turn.state === 'interrupted'
    );
  const mutation = useAgentDmControl({
    onSuccess: props.onChanged,
    onError: () =>
      toast.failure(
        'Could not update this conversation. Refresh and try again.'
      ),
  });
  const failure = () => {
    const state = failed()?.state;
    return state === 'failed' || state === 'stopped' || state === 'interrupted'
      ? state
      : undefined;
  };
  return (
    <>
      <Show when={current()}>
        {(session) => (
          <ContextControls
            available={props.conversation.available}
            settingsChanged={props.conversation.settingsChanged}
            busy={!!running()}
            pending={mutation.isPending}
            onStartFresh={() =>
              mutation.mutateAsync({
                type: 'fresh',
                channelId: props.conversation.channelId,
                sessionId: session(),
              })
            }
          />
        )}
      </Show>
      <ConversationActivity
        running={!!running()}
        queued={queued()}
        failed={context ? undefined : failure()}
        canRetry={props.conversation.available}
        pending={mutation.isPending}
        onStop={() => {
          const turn = running();
          if (turn)
            mutation.mutate({ type: 'stop', sessionId: turn.sessionId });
        }}
        onRetry={() => {
          const turn = failed();
          if (turn)
            mutation.mutate({
              type: 'retry',
              channelId: props.conversation.channelId,
              turn,
            });
        }}
      />
    </>
  );
}
