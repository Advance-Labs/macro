import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { complete } = vi.hoisted(() => ({ complete: vi.fn() }));
vi.mock('./client', () => ({
  cognitionApiServiceClient: { structuredCompletion: complete },
}));
vi.mock('@core/component/AI/constant', () => ({
  DEFAULT_MODEL: 'anthropic/claude-sonnet-5',
}));

import { generateDatabaseQuery } from './database-query';

const input = {
  prompt: 'Show tickets by status',
  sql: '',
  schema: {
    databaseId: 'support',
    name: 'Support',
    focusTableId: 'tickets',
    tables: [],
  },
};
const proposal = {
  answerable: true,
  sql: 'SELECT status, COUNT(*) AS total FROM tickets GROUP BY status',
  explanation: 'Tickets by status.',
  displayMode: 'bar',
  chart: { x: 'status', y: ['total'], title: 'Tickets' },
};
beforeEach(() => complete.mockReset());

describe('database AI transport boundaries', () => {
  it('answers from the supplied schema without tools and preserves chart configuration', async () => {
    complete.mockResolvedValue(ok({ result: proposal }));
    const result = await generateDatabaseQuery(input);
    expect(complete.mock.calls[0][0].toolset).toEqual({ type: 'none' });
    expect(JSON.parse(complete.mock.calls[0][0].prompt).schema).toEqual(
      input.schema
    );
    expect(result.chart).toEqual(proposal.chart);
    expect(result.displayMode).toBe('bar');
  });

  it('keeps server errors visible without fabricating a proposal', async () => {
    complete.mockResolvedValue(
      err([{ message: 'Access denied', code: 'FORBIDDEN' }])
    );
    await expect(generateDatabaseQuery(input)).rejects.toThrow('Access denied');
  });
});
