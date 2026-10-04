import { throwOnErr } from '@core/util/result';
import { controlAgentSession } from '@queries/agent-session/control';
import {
  type AgentDmTurn,
  retryAgentDm,
  startFreshAgentDm,
} from '@service-agent-harness/direct-messages';
import { useMutation } from '@tanstack/solid-query';

type DmControl =
  | { type: 'stop'; sessionId: string }
  | { type: 'retry'; channelId: string; turn: AgentDmTurn }
  | { type: 'fresh'; channelId: string; sessionId: string };

export function useAgentDmControl(callbacks: {
  onSuccess: () => void;
  onError: () => void;
}) {
  return useMutation(() => ({
    mutationFn: async (action: DmControl) => {
      switch (action.type) {
        case 'stop':
          await throwOnErr(() =>
            controlAgentSession(action.sessionId, { type: 'stop' })
          );
          break;
        case 'retry':
          await throwOnErr(() => retryAgentDm(action.channelId, action.turn));
          break;
        case 'fresh':
          await throwOnErr(() =>
            startFreshAgentDm(action.channelId, action.sessionId)
          );
          break;
      }
    },
    ...callbacks,
  }));
}
