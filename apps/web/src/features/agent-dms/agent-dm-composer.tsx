import { type ParentProps, Show } from 'solid-js';
import { AgentDmControls } from './agent-dm-controls';
import { ConversationNotice } from './components/conversation-notice';
import { useAgentDmConversation } from './queries/conversation';

/** App-facing composition; mount only after the channel identifies an agent DM. */
export function AgentDmComposer(
  props: ParentProps<{ channelId: string; isAgentDm: boolean }>
) {
  return (
    <Show when={props.isAgentDm} fallback={props.children}>
      <AgentDmComposerContent channelId={props.channelId}>
        {props.children}
      </AgentDmComposerContent>
    </Show>
  );
}

function AgentDmComposerContent(props: ParentProps<{ channelId: string }>) {
  const conversation = useAgentDmConversation(() => props.channelId);
  const available = () =>
    conversation.isSuccess && conversation.data?.available;
  const notice = () =>
    conversation.isError
      ? 'error'
      : conversation.isSuccess
        ? 'unavailable'
        : 'loading';
  return (
    <div class="flex w-full min-w-0 flex-col">
      <Show when={conversation.isSuccess && conversation.data}>
        {(data) => (
          <AgentDmControls
            conversation={data()}
            onChanged={() => void conversation.refetch()}
          />
        )}
      </Show>
      <Show
        when={available()}
        fallback={
          <ConversationNotice
            state={notice()}
            onRetry={() => void conversation.refetch()}
          />
        }
      >
        {props.children}
      </Show>
    </div>
  );
}
