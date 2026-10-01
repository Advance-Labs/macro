import { describe, expect, it } from 'vitest';
import { parseQueryProposal, type QueryAnswer } from './query';
import {
  isChartMode,
  parseQueryChart,
  prepareQueryChart,
  queryChartSpec,
} from './query-chart';

const answer: QueryAnswer = {
  results: [
    {
      columns: [
        { name: 'Month', entity_type: null },
        { name: 'Revenue', entity_type: null },
        { name: 'Cost', entity_type: null },
      ],
      rows: [
        ['Jan', 20, 5],
        ['Feb', null, 10],
        ['Mar', -4, 2],
      ],
    },
  ],
  read_tables: [],
  read_versions: {},
  truncated_tables: [],
};

describe('saved chart settings', () => {
  it('accepts AI chart settings and rejects incomplete or executable specifications', () => {
    const chart = { x: 'Month', y: ['Revenue', 'Cost'], title: 'Cash flow' };
    expect(
      parseQueryProposal({
        sql: 'SELECT month AS Month, revenue AS Revenue, cost AS Cost FROM finances',
        explanation: 'Monthly totals.',
        displayMode: 'line',
        chart,
      })
    ).toMatchObject({ displayMode: 'line', chart });
    expect(() =>
      parseQueryProposal({
        sql: 'SELECT 1',
        explanation: 'One.',
        displayMode: 'pie',
      })
    ).toThrow('chart settings');
    expect(
      parseQueryChart({ x: 'Month', y: ['Revenue', 'Revenue'] })
    ).toBeUndefined();
    expect(parseQueryChart({ x: 'Month', y: ['Month'] })).toBeUndefined();
    expect(
      parseQueryChart({ x: 'Month', y: ['Revenue'], script: 'alert(1)' })
    ).toEqual({ x: 'Month', y: ['Revenue'] });
  });

  it('knows every chart display mode, including area and scatter', () => {
    expect(
      ['bar', 'line', 'area', 'scatter', 'pie', 'table', 'scalar', 'chart'].map(
        isChartMode
      )
    ).toEqual([true, true, true, true, true, false, false, false]);
    expect(
      parseQueryProposal({
        sql: 'SELECT month AS Month, revenue AS Revenue FROM finances',
        explanation: 'Revenue.',
        displayMode: 'area',
        chart: { x: 'Month', y: ['Revenue'] },
      })
    ).toMatchObject({ displayMode: 'area' });
  });

  it('keeps the optional color split and stacking, and drops renderer options', () => {
    expect(
      parseQueryChart({
        x: 'Month',
        y: ['Tickets'],
        color: 'Team',
        stack: true,
        marks: [{ type: 'barY' }],
        marginLeft: 80,
      })
    ).toEqual({ x: 'Month', y: ['Tickets'], color: 'Team', stack: true });
    // The AI's strict schema always sends both fields; absence is the stored form.
    expect(
      parseQueryChart({ x: 'Month', y: ['Tickets'], color: null, stack: false })
    ).toEqual({ x: 'Month', y: ['Tickets'] });
  });

  it('rejects a color split that is not one extra column over one series', () => {
    expect(
      parseQueryChart({ x: 'Month', y: ['Tickets'], color: 'Month' })
    ).toBeUndefined();
    expect(
      parseQueryChart({ x: 'Month', y: ['Tickets'], color: 'Tickets' })
    ).toBeUndefined();
    expect(
      parseQueryChart({ x: 'Month', y: ['Tickets', 'Hours'], color: 'Team' })
    ).toBeUndefined();
    expect(
      parseQueryChart({ x: 'Month', y: ['Tickets'], color: 7 })
    ).toBeUndefined();
    expect(
      parseQueryChart({ x: 'Month', y: ['Tickets'], stack: 'yes' })
    ).toBeUndefined();
  });
});

describe('chart spec from saved settings', () => {
  it('reads an old bar, line or pie answer as the same chart', () => {
    expect(
      queryChartSpec('bar', { x: 'Status', y: ['Count'], title: 'Tasks' })
    ).toEqual({
      mark: 'bar',
      x: 'Status',
      y: ['Count'],
      title: 'Tasks',
      stack: false,
    });
    expect(
      queryChartSpec('line', { x: 'Month', y: ['Revenue', 'Cost'] })
    ).toEqual({
      mark: 'line',
      x: 'Month',
      y: ['Revenue', 'Cost'],
      stack: false,
    });
    expect(queryChartSpec('pie', { x: 'Status', y: ['Count'] })).toEqual({
      mark: 'pie',
      x: 'Status',
      y: ['Count'],
      stack: false,
    });
  });

  it('carries the new optional fields onto the new marks', () => {
    expect(
      queryChartSpec('area', {
        x: 'Month',
        y: ['Tickets'],
        color: 'Team',
        stack: true,
      })
    ).toEqual({
      mark: 'area',
      x: 'Month',
      y: ['Tickets'],
      color: 'Team',
      stack: true,
    });
    expect(queryChartSpec('scatter', { x: 'Hours', y: ['Cost'] })).toEqual({
      mark: 'scatter',
      x: 'Hours',
      y: ['Cost'],
      stack: false,
    });
  });

  it('only stacks the marks that can stack', () => {
    expect(
      queryChartSpec('line', { x: 'Month', y: ['A', 'B'], stack: true }).stack
    ).toBe(false);
    expect(
      queryChartSpec('scatter', { x: 'Month', y: ['A', 'B'], stack: true })
        .stack
    ).toBe(false);
    expect(
      queryChartSpec('pie', { x: 'Month', y: ['A'], stack: true }).stack
    ).toBe(false);
    expect(
      queryChartSpec('bar', { x: 'Month', y: ['A', 'B'], stack: true }).stack
    ).toBe(true);
  });
});

describe('chart data', () => {
  it('lays out one point per row and series, keeping missing values as gaps', () => {
    expect(prepareQueryChart(answer, 'line')).toEqual({
      data: {
        config: { x: 'Month', y: ['Revenue', 'Cost'] },
        spec: {
          mark: 'line',
          x: 'Month',
          y: ['Revenue', 'Cost'],
          stack: false,
        },
        title: 'Revenue, Cost by Month',
        scale: 'category',
        categories: ['Jan', 'Feb', 'Mar'],
        series: ['Revenue', 'Cost'],
        omitted: 0,
        points: [
          {
            x: 'Jan',
            label: 'Jan',
            series: 'Revenue',
            value: 20,
            tip: 'Jan\nRevenue: 20',
          },
          {
            x: 'Jan',
            label: 'Jan',
            series: 'Cost',
            value: 5,
            tip: 'Jan\nCost: 5',
          },
          {
            x: 'Feb',
            label: 'Feb',
            series: 'Revenue',
            value: null,
            tip: 'Feb\nRevenue: —',
          },
          {
            x: 'Feb',
            label: 'Feb',
            series: 'Cost',
            value: 10,
            tip: 'Feb\nCost: 10',
          },
          {
            x: 'Mar',
            label: 'Mar',
            series: 'Revenue',
            value: -4,
            tip: 'Mar\nRevenue: -4',
          },
          {
            x: 'Mar',
            label: 'Mar',
            series: 'Cost',
            value: 2,
            tip: 'Mar\nCost: 2',
          },
        ],
      },
    });
  });

  it('puts numbers on a numeric axis and calendar days on a time axis', () => {
    const numeric: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Hours', entity_type: null },
            { name: 'Cost', entity_type: null },
          ],
          rows: [
            [1, 1200],
            [10, 3],
          ],
        },
      ],
    };
    const numericChart = prepareQueryChart(numeric, 'scatter').data;
    expect(numericChart?.scale).toBe('number');
    expect(numericChart?.points).toEqual([
      { x: 1, label: '1', series: 'Cost', value: 1200, tip: '1\nCost: 1,200' },
      { x: 10, label: '10', series: 'Cost', value: 3, tip: '10\nCost: 3' },
    ]);
    const dates: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Day', entity_type: null },
            { name: 'Signups', entity_type: null },
          ],
          rows: [
            ['2026-01-01T00:00:00+00:00', 2],
            ['2026-01-04', 20],
          ],
        },
      ],
    };
    const dateChart = prepareQueryChart(dates, 'line').data;
    expect(dateChart?.scale).toBe('date');
    expect(dateChart?.points).toEqual([
      {
        x: new Date(2026, 0, 1),
        label: 'Jan 1, 2026',
        series: 'Signups',
        value: 2,
        tip: 'Jan 1, 2026\nSignups: 2',
      },
      {
        x: new Date(2026, 0, 4),
        label: 'Jan 4, 2026',
        series: 'Signups',
        value: 20,
        tip: 'Jan 4, 2026\nSignups: 20',
      },
    ]);
    expect(prepareQueryChart(answer, 'line').data?.scale).toBe('category');
  });

  it('leaves rows without a date off a time axis and counts them', () => {
    const dates: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Date', entity_type: null },
            { name: 'COUNT(*)', entity_type: null },
          ],
          rows: [
            ['2025-07-19T00:00:00+00:00', 1],
            ['2025-10-20T00:00:00+00:00', 1],
            [null, 1],
          ],
        },
      ],
    };
    const chart = prepareQueryChart(dates, 'line').data;
    expect(chart?.scale).toBe('date');
    expect(chart?.omitted).toBe(1);
    expect(chart?.points.map((point) => point.label)).toEqual([
      'Jul 19, 2025',
      'Oct 20, 2025',
    ]);
  });

  it('splits one series by a color column', () => {
    const tickets: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Month', entity_type: null },
            { name: 'Team', entity_type: null },
            { name: 'Tickets', entity_type: null },
          ],
          rows: [
            ['Jan', 'Support', 12],
            ['Jan', 'Sales', 4],
            ['Feb', 'Support', 9],
          ],
        },
      ],
    };
    expect(
      prepareQueryChart(tickets, 'bar', {
        x: 'Month',
        y: ['Tickets'],
        color: 'Team',
        stack: true,
      })
    ).toEqual({
      data: {
        config: { x: 'Month', y: ['Tickets'], color: 'Team', stack: true },
        spec: {
          mark: 'bar',
          x: 'Month',
          y: ['Tickets'],
          color: 'Team',
          stack: true,
        },
        title: 'Tickets by Month and Team',
        scale: 'category',
        categories: ['Jan', 'Feb'],
        series: ['Support', 'Sales'],
        omitted: 0,
        points: [
          {
            x: 'Jan',
            label: 'Jan',
            series: 'Support',
            value: 12,
            tip: 'Jan\nSupport: 12',
          },
          {
            x: 'Jan',
            label: 'Jan',
            series: 'Sales',
            value: 4,
            tip: 'Jan\nSales: 4',
          },
          {
            x: 'Feb',
            label: 'Feb',
            series: 'Support',
            value: 9,
            tip: 'Feb\nSupport: 9',
          },
        ],
      },
    });
    expect(
      prepareQueryChart(tickets, 'bar', {
        x: 'Month',
        y: ['Tickets'],
        color: 'Owner',
      }).error
    ).toContain('unavailable');
  });

  it('refuses more series than the palette has colors', () => {
    const many: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Month', entity_type: null },
            { name: 'Team', entity_type: null },
            { name: 'Tickets', entity_type: null },
          ],
          rows: Array.from({ length: 10 }, (_, index) => [
            'Jan',
            `Team ${index}`,
            index,
          ]),
        },
      ],
    };
    expect(
      prepareQueryChart(many, 'bar', {
        x: 'Month',
        y: ['Tickets'],
        color: 'Team',
      }).error
    ).toContain('more than 9 groups');
  });

  it('refuses missing or ambiguous aliases and text values instead of drawing a misleading chart', () => {
    expect(
      prepareQueryChart(answer, 'bar', { x: 'Month', y: ['Profit'] }).error
    ).toContain('unavailable');
    const ambiguous: QueryAnswer = {
      ...answer,
      results: [
        {
          ...answer.results[0],
          columns: [
            { name: 'Month', entity_type: null },
            { name: 'Revenue', entity_type: null },
            { name: 'Revenue', entity_type: null },
          ],
        },
      ],
    };
    expect(
      prepareQueryChart(ambiguous, 'bar', { x: 'Month', y: ['Revenue'] }).error
    ).toContain('unavailable');
    const text: QueryAnswer = {
      ...answer,
      results: [{ ...answer.results[0], rows: [['Jan', 'high', 1]] }],
    };
    expect(
      prepareQueryChart(text, 'bar', { x: 'Month', y: ['Revenue'] }).error
    ).toContain('numeric');
  });

  it('explains empty results instead of drawing an empty frame', () => {
    const empty: QueryAnswer = {
      ...answer,
      results: [{ ...answer.results[0], rows: [] }],
    };
    expect(prepareQueryChart(empty, 'bar').error).toContain(
      'no matching records'
    );
    const blank: QueryAnswer = {
      ...answer,
      results: [{ ...answer.results[0], rows: [['Jan', null, null]] }],
    };
    expect(
      prepareQueryChart(blank, 'line', { x: 'Month', y: ['Revenue', 'Cost'] })
        .error
    ).toContain('no numeric values');
  });

  it('rejects invalid pie distributions and never silently truncates chart data', () => {
    expect(prepareQueryChart(answer, 'pie').error).toContain('nonnegative');
    const zeroes: QueryAnswer = {
      ...answer,
      results: [{ ...answer.results[0], rows: [['Jan', 0, 0]] }],
    };
    expect(prepareQueryChart(zeroes, 'pie').error).toContain(
      'greater than zero'
    );
    const many: QueryAnswer = {
      ...answer,
      results: [
        {
          ...answer.results[0],
          rows: Array.from({ length: 21 }, (_, index) => [String(index), 1, 2]),
        },
      ],
    };
    expect(prepareQueryChart(many, 'pie').error).toContain('20 categories');
    expect(prepareQueryChart(many, 'bar').data?.categories).toHaveLength(21);
    expect(
      prepareQueryChart(answer, 'pie', {
        x: 'Month',
        y: ['Cost'],
        color: 'Revenue',
      }).error
    ).toContain('pie');
  });

  it('folds the smallest pie slices into Other past the palette', () => {
    const shares: QueryAnswer = {
      ...answer,
      results: [
        {
          columns: [
            { name: 'Party', entity_type: null },
            { name: 'Guests', entity_type: null },
          ],
          rows: [
            ['A', 10],
            ['B', 1],
            ['C', 9],
            ['D', 8],
            ['E', 7],
            ['F', 6],
            ['G', 5],
            ['H', 4],
            ['I', 3],
            ['J', 2],
          ],
        },
      ],
    };
    const chart = prepareQueryChart(shares, 'pie').data;
    expect(chart?.categories).toEqual([
      'A',
      'C',
      'D',
      'E',
      'F',
      'G',
      'H',
      'I',
      'Other',
    ]);
    expect(chart?.points.at(-1)).toEqual({
      x: 'Other',
      label: 'Other',
      series: 'Guests',
      value: 3,
      tip: 'Other (2 categories)\nGuests: 3',
    });
  });
});
