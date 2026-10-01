import { render, waitFor } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import type { QueryAnswer } from '../core/query';
import {
  prepareQueryChart,
  type QueryChartConfig,
  type QueryChartMode,
} from '../core/query-chart';
import { QueryChart } from './query-chart';

vi.mock('@solid-primitives/resize-observer', () => ({
  createElementSize: () => ({ width: 346, height: 300 }),
}));

const answer: QueryAnswer = {
  results: [
    {
      columns: [
        { name: 'Day', entity_type: null },
        { name: 'Signups', entity_type: null },
        { name: 'Churn', entity_type: null },
      ],
      rows: [
        ['2026-01-01', 2, 1],
        ['2026-01-04', 20, 3],
        ['2026-02-10', 12, 2],
      ],
    },
  ],
  read_tables: [],
  read_versions: {},
  truncated_tables: [],
};

function chartData(mode: QueryChartMode, config: QueryChartConfig) {
  const prepared = prepareQueryChart(answer, mode, config);
  if (!prepared.data) throw new Error(prepared.error);
  return prepared.data;
}

describe('database chart', () => {
  it.each([
    ['bar', 'bar'],
    ['line', 'line'],
    ['area', 'area'],
    ['scatter', 'dot'],
  ] as const)(
    'draws a %s chart as Plot %s marks in an SVG',
    async (mode, mark) => {
      const rendered = render(() => (
        <QueryChart data={chartData(mode, { x: 'Day', y: ['Signups'] })} />
      ));
      const chart = rendered.getByRole('img', {
        name: new RegExp(`^Signups by Day\\. ${mode} chart\\.$`, 'i'),
      });
      await waitFor(() =>
        expect(chart.querySelector(`svg g[aria-label="${mark}"]`)).toBeTruthy()
      );
      expect(chart.querySelectorAll('svg').length).toBe(1);
      rendered.unmount();
    }
  );

  it('holds the chart’s height while Plot loads', () => {
    const rendered = render(() => (
      <QueryChart data={chartData('line', { x: 'Day', y: ['Signups'] })} />
    ));
    const chart = rendered.getByRole('img');
    expect(chart.getAttribute('aria-busy')).toBe('true');
    expect(chart.style.height).toBe('240px');
    rendered.unmount();
  });

  it('adds a color legend when there are several series', async () => {
    const rendered = render(() => (
      <QueryChart
        data={chartData('line', { x: 'Day', y: ['Signups', 'Churn'] })}
      />
    ));
    await waitFor(() =>
      expect(
        Array.from(
          rendered.container.querySelectorAll('.macro-chart-swatch'),
          (swatch) => swatch.textContent
        )
      ).toEqual(['Signups', 'Churn'])
    );
    rendered.unmount();
  });

  it('still draws a pie, labelling the slices that have room', () => {
    const shares: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Status', entity_type: null },
            { name: 'Count', entity_type: null },
          ],
          rows: [
            ['Todo', 6],
            ['Done', 3],
            ['Blocked', 0.1],
          ],
        },
      ],
    };
    const prepared = prepareQueryChart(shares, 'pie');
    if (!prepared.data) throw new Error(prepared.error);
    const data = prepared.data;
    const rendered = render(() => <QueryChart data={data} />);
    const chart = rendered.getByRole('img', {
      name: 'Count by Status. Pie chart.',
    });
    const slices = chart.querySelectorAll('svg path[data-slice]');
    expect(slices.length).toBe(3);
    expect(
      Array.from(slices).map((slice) => slice.getAttribute('fill'))
    ).toEqual([
      'var(--color-blue)',
      'var(--color-orange)',
      'var(--color-teal)',
    ]);
    expect(
      Array.from(chart.querySelectorAll('svg text')).map(
        (label) => label.textContent
      )
    ).toEqual(['Todo6', 'Done3']);
    expect(rendered.getByText('Blocked')).toBeTruthy();
    rendered.unmount();
  });
});
