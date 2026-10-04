import type { PendingInteraction } from '@service-agent-fold/generated/types';
import type { AgentDmConversationResponse } from '@service-agent-harness/direct-messages';
import { cleanup, render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';

vi.mock(
  '@core/component/LexicalMarkdown/component/core/StaticMarkdown',
  () => ({
    StaticMarkdown: (props: { markdown: string }) => <p>{props.markdown}</p>,
  })
);
vi.mock('@app/features/agent-interactions/components/InteractionCard', () => ({
  InteractionCard: (props: { request: PendingInteraction }) => (
    <section aria-label="Permission request">{props.request.requestId}</section>
  ),
}));
vi.mock('./queries/controls', () => ({ useAgentDmControl: vi.fn() }));
vi.mock('@core/component/Toast/Toast', () => ({ toast: {} }));
vi.mock('@ui', () => ({ Button: () => null }));

import {
  AgentDmContextBoundary,
  AgentDmReplyContent,
} from './agent-dm-message';
import { AgentDmContext, type AgentDmContextValue } from './context';

afterEach(cleanup);

function conversation(): AgentDmConversationResponse {
  return {
    channelId: 'channel',
    botId: 'bot',
    name: 'Researcher',
    avatarUrl: null,
    available: true,
    settingsChanged: false,
    segments: [
      { sessionId: 'old', isCurrent: false, createdAt: '2026-10-01T00:00:00Z' },
      { sessionId: 'new', isCurrent: true, createdAt: '2026-10-01T01:00:00Z' },
    ],
    turns: [
      {
        sessionId: 'new',
        sourceMessageId: 'source',
        replyMessageId: 'reply',
        actionId: 'attempt',
        state: 'running',
        createdAt: '2026-10-01T01:00:00Z',
      },
    ],
  };
}

function context(data: () => AgentDmConversationResponse): AgentDmContextValue {
  return {
    conversation: data,
    messages: () => [
      {
        agentSessionId: 'new',
        turn: 4,
        author: { kind: 'user', userId: 'user' },
        requestId: 'attempt',
        parts: [{ kind: 'text', text: 'Question' }],
        stop: null,
        pending: false,
      },
      {
        agentSessionId: 'new',
        turn: 4,
        author: { kind: 'agent' },
        requestId: null,
        parts: [
          { kind: 'thought', text: 'Private reasoning' },
          { kind: 'text', text: 'Current answer' },
        ],
        stop: null,
        pending: false,
      },
      {
        agentSessionId: 'new',
        turn: 3,
        author: { kind: 'agent' },
        requestId: null,
        parts: [{ kind: 'text', text: 'Previous answer' }],
        stop: null,
        pending: false,
      },
    ],
    interactions: {
      pending: () => [
        {
          kind: 'permission',
          turn: 4,
          requestId: 'current-approval',
          toolCall: 'tool',
          options: [],
        },
        {
          kind: 'permission',
          turn: 3,
          requestId: 'old-approval',
          toolCall: 'old-tool',
          options: [],
        },
      ],
      canAnswer: () => true,
      answering: () => false,
      respond: vi.fn(),
    },
    liveFailed: () => false,
    retryLive: vi.fn(),
    refresh: vi.fn(),
  };
}

it('correlates live text and approvals to the reply’s exact attempt, then restores the persisted reply', () => {
  const [data, setData] = createSignal(conversation());
  const view = render(() => (
    <AgentDmContext.Provider value={context(data)}>
      <AgentDmReplyContent messageId="reply">
        <p>Persisted answer</p>
      </AgentDmReplyContent>
      <AgentDmReplyContent messageId="different-reply">
        <p>Other reply</p>
      </AgentDmReplyContent>
    </AgentDmContext.Provider>
  ));
  expect(view.getByText('Current answer')).toBeTruthy();
  expect(
    view.getByRole('region', { name: 'Permission request' }).textContent
  ).toBe('current-approval');
  expect(view.getByText('Other reply')).toBeTruthy();
  for (const text of [
    'Previous answer',
    'Private reasoning',
    'old-approval',
    'Persisted answer',
  ]) {
    expect(view.queryByText(text)).toBeNull();
  }
  setData((current) => ({
    ...current,
    turns: current.turns.map((turn) => ({ ...turn, state: 'succeeded' })),
  }));
  expect(view.getByText('Persisted answer')).toBeTruthy();
  expect(view.queryByRole('region', { name: 'Permission request' })).toBeNull();
});

it('places a fresh-context divider only before the segment’s first source message', () => {
  const view = render(() => (
    <AgentDmContext.Provider value={context(conversation)}>
      <AgentDmContextBoundary messageId="source" />
      <AgentDmContextBoundary messageId="reply" />
    </AgentDmContext.Provider>
  ));
  expect(view.getAllByRole('separator')).toHaveLength(1);
});
