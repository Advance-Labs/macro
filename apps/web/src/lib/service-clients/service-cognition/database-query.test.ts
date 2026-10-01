import { ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { complete } = vi.hoisted(() => ({ complete: vi.fn() }));
vi.mock('./client', () => ({
  cognitionApiServiceClient: { structuredCompletion: complete },
}));
vi.mock('@core/component/AI/constant', () => ({
  DATABASE_MODEL: 'google/gemini-3.8-flash',
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
  it('gives document questions only read-only discovery and preserves chart configuration', async () => {
    complete.mockResolvedValue(ok({ result: proposal, toolActivity: [] }));
    const result = await generateDatabaseQuery(input);
    expect(complete.mock.calls[0][0].model).toBe('google/gemini-3.8-flash');
    expect(complete.mock.calls[0][0].toolset).toEqual({
      type: 'databases_read_only',
    });
    expect(JSON.parse(complete.mock.calls[0][0].prompt).schema).toEqual(
      input.schema
    );
    expect(result._unsafeUnwrap()).toEqual(proposal);
  });

  it('asks for every chart mark and keeps the strict schema’s color and stack', async () => {
    complete.mockResolvedValue(
      ok({
        result: {
          ...proposal,
          sql: 'SELECT month, team, COUNT(*) AS total FROM tickets GROUP BY month, team',
          displayMode: 'area',
          chart: {
            x: 'month',
            y: ['total'],
            title: 'Tickets',
            color: 'team',
            stack: true,
          },
        },
        toolActivity: [],
      })
    );
    const result = await generateDatabaseQuery(input);
    const schema = complete.mock.calls[0][0].output_schema.schema;
    expect(schema.properties.displayMode.enum).toEqual([
      'scalar',
      'table',
      'bar',
      'line',
      'area',
      'scatter',
      'pie',
    ]);
    expect(schema.properties.chart.anyOf[1].required).toEqual([
      'x',
      'y',
      'title',
      'color',
      'stack',
    ]);
    expect(result._unsafeUnwrap()).toEqual({
      ...proposal,
      sql: 'SELECT month, team, COUNT(*) AS total FROM tickets GROUP BY month, team',
      displayMode: 'area',
      chart: {
        x: 'month',
        y: ['total'],
        title: 'Tickets',
        color: 'team',
        stack: true,
      },
    });
  });
});
