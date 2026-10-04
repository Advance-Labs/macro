import { createInteractionController } from '@app/features/agent-interactions/primitives/create-interaction-controller';
import { toast } from '@core/component/Toast/Toast';
import { type ParentProps, Show } from 'solid-js';
import { AgentDmContext } from './context';
import { useAgentDmConversation } from './queries/conversation';
import { createDmLiveSession } from './queries/live-session';

/** Ordinary channels never subscribe to agent runtime state. */
export function AgentDmProvider(
  props: ParentProps<{ channelId: string; isAgentDm: boolean }>
) {
  return (
    <Show when={props.isAgentDm} fallback={props.children}>
      <ConversationProvider channelId={props.channelId}>
        {props.children}
      </ConversationProvider>
    </Show>
  );
}

function ConversationProvider(props: ParentProps<{ channelId: string }>) {
  const query = useAgentDmConversation(() => props.channelId);
  const data = () => (query.isSuccess ? query.data : undefined);
  const current = () =>
    data()?.segments.find((segment) => segment.isCurrent)?.sessionId;
  const sessionId = () =>
    data()?.turns.some(
      (turn) => turn.sessionId === current() && turn.replyMessageId
    )
      ? current()
      : undefined;
  const live = createDmLiveSession(sessionId);
  const interactions = createInteractionController({
    sessionId,
    pending: () => live.metadata()?.pendingInteractions ?? [],
    canEdit: () => data()?.available === true,
    issue: live.issue,
    onFailure: (message) => toast.failure(message),
  });
  return (
    <AgentDmContext.Provider
      value={{
        conversation: data,
        messages: live.messages,
        interactions,
        liveFailed: live.failed,
        retryLive: live.retry,
        refresh: () => void query.refetch(),
      }}
    >
      {props.children}
    </AgentDmContext.Provider>
  );
}
