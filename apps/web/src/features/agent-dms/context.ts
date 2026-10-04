import type { InteractionController } from '@app/features/agent-interactions/context/interaction';
import type { FoldedMessage } from '@service-agent-fold/generated/types';
import type { AgentDmConversationResponse } from '@service-agent-harness/direct-messages';
import { type Accessor, createContext, useContext } from 'solid-js';

export type AgentDmContextValue = {
  conversation: Accessor<AgentDmConversationResponse | undefined>;
  messages: Accessor<FoldedMessage[]>;
  interactions: InteractionController;
  liveFailed: Accessor<boolean>;
  retryLive: () => void;
  refresh: () => void;
};

export const AgentDmContext = createContext<AgentDmContextValue>();
export const useOptionalAgentDm = () => useContext(AgentDmContext);
