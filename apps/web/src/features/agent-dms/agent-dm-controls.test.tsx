import type {
  AgentDmConversationResponse,
  AgentDmTurn,
} from '@service-agent-harness/direct-messages';
import { cleanup, fireEvent, render, waitFor } from '@solidjs/testing-library';
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query';
import { ok } from 'neverthrow';
import type { JSX } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  stop: vi.fn(),
  retry: vi.fn(),
  failure: vi.fn(),
  fresh: vi.fn(),
}));
vi.mock('@service-agent-harness/client', () => ({
  agentHarnessServiceClient: { control: mocks.stop },
}));
vi.mock('@service-agent-harness/direct-messages', () => ({
  retryAgentDm: mocks.retry,
  startFreshAgentDm: mocks.fresh,
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { failure: mocks.failure },
}));
vi.mock('@ui', () => ({
  Button: (props: JSX.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props} />
  ),
}));

import { AgentDmControls } from './agent-dm-controls';

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function turn(sessionId: string, state: AgentDmTurn['state']): AgentDmTurn {
  return {
    sessionId,
    state,
    sourceMessageId: `${sessionId}-message`,
    actionId: `${sessionId}-attempt`,
    replyMessageId: null,
    createdAt: '2026-10-01T00:00:00Z',
  };
}

function conversation(
  turns: AgentDmTurn[],
  available = true
): AgentDmConversationResponse {
  return {
    channelId: 'dm',
    botId: 'bot',
    name: 'Researcher',
    avatarUrl: null,
    available,
    settingsChanged: false,
    segments: [
      {
        sessionId: 'current',
        isCurrent: true,
        createdAt: '2026-10-01T00:00:00Z',
      },
    ],
    turns,
  };
}

function mount(data: AgentDmConversationResponse) {
  const changed = vi.fn();
  const view = render(() => (
    <QueryClientProvider client={new QueryClient()}>
      <AgentDmControls conversation={data} onChanged={changed} />
    </QueryClientProvider>
  ));
  return { ...view, changed };
}

describe('agent DM controls', () => {
  it('keeps Stop available after access is revoked and scopes it to the active segment', async () => {
    mocks.stop.mockResolvedValue(ok({}));
    const view = mount(
      conversation([turn('old', 'running'), turn('current', 'running')], false)
    );
    await fireEvent.click(view.getByRole('button', { name: 'Stop' }));
    await waitFor(() =>
      expect(mocks.stop).toHaveBeenCalledWith('current', { type: 'stop' })
    );
    await waitFor(() => expect(view.changed).toHaveBeenCalledOnce());
  });

  it('retries only the chosen current-segment attempt with its original identity', async () => {
    mocks.retry.mockResolvedValue(ok(undefined));
    const current = turn('current', 'interrupted');
    const view = mount(conversation([turn('old', 'failed'), current]));
    expect(view.getByText(/Review any completed actions/)).toBeTruthy();
    expect(mocks.retry).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole('button', { name: 'Retry message' }));
    await waitFor(() =>
      expect(mocks.retry).toHaveBeenCalledWith('dm', current)
    );
  });

  it('keeps unavailable failed conversations readable without offering a new run', () => {
    const view = mount(conversation([turn('current', 'failed')], false));
    expect(view.getByText(/could not finish/)).toBeTruthy();
    expect(view.queryByRole('button', { name: 'Retry message' })).toBeNull();
  });
});

it('confirms starting fresh against the current segment and refreshes on success', async () => {
  mocks.fresh.mockResolvedValue(ok(undefined));
  const view = mount(conversation([turn('current', 'succeeded')]));
  fireEvent.click(view.getByRole('button', { name: 'Start fresh' }));
  expect(view.getByText(/Your history stays here/)).toBeTruthy();
  expect(mocks.fresh).not.toHaveBeenCalled();
  fireEvent.click(view.getByRole('button', { name: 'Start fresh' }));
  await waitFor(() =>
    expect(mocks.fresh).toHaveBeenCalledWith('dm', 'current')
  );
  await waitFor(() => expect(view.changed).toHaveBeenCalledOnce());
});

it('requires a running turn to be stopped before context reset', () => {
  const view = mount(conversation([turn('current', 'running')]));
  expect(
    view.getByRole('button', { name: 'Start fresh' }).hasAttribute('disabled')
  ).toBe(true);
  expect(view.getByRole('button', { name: 'Stop' })).toBeTruthy();
});
