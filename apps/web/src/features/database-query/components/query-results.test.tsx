import { render } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import type { QueryAnswer } from '../core/query';
import { QueryResults } from './query-results';

vi.mock('@solid-primitives/resize-observer', () => ({
  createElementSize: () => ({ width: 346, height: 300 }),
}));

const scalar: QueryAnswer = {
  results: [
    { columns: [{ name: 'Approved', entity_type: null }], rows: [[5]] },
  ],
  read_tables: [],
  read_versions: {},
  truncated_tables: [],
};
describe('question result display', () => {
  it.each(['bar', 'line', 'pie'] as const)(
    'renders a real %s chart and retains its underlying data',
    (displayMode) => {
      const answer = {
        ...scalar,
        results: [
          {
            columns: [
              { name: 'Status', entity_type: null },
              { name: 'Count', entity_type: null },
            ],
            rows: [
              ['Todo', 3],
              ['Done', 2],
            ],
          },
        ],
      };
      const result = render(() => (
        <QueryResults
          answer={answer}
          displayMode={displayMode}
          chart={{ x: 'Status', y: ['Count'], title: 'Tasks by status' }}
        />
      ));
      expect(
        result.getByRole('img', {
          name: new RegExp(`Tasks by status. ${displayMode}`, 'i'),
        })
      ).toBeTruthy();
      expect(result.getByText('View data')).toBeTruthy();
      expect(result.getByRole('table', { hidden: true }).textContent).toContain(
        'Todo'
      );
      result.unmount();
    }
  );
  it('explains invalid chart data and falls back to the actual table', () => {
    const result = render(() => (
      <QueryResults
        answer={scalar}
        displayMode="bar"
        chart={{ x: 'Status', y: ['Count'] }}
      />
    ));
    expect(result.queryByRole('img')).toBeNull();
    expect(result.getByRole('status').textContent).toContain('label column');
    expect(result.getByRole('cell').textContent).toBe('5');
    result.unmount();
  });
  it('honors an explicit table choice for a one-cell answer', () => {
    const result = render(() => (
      <QueryResults answer={scalar} displayMode="table" />
    ));
    expect(result.getByRole('table')).toBeTruthy();
    expect(result.getByRole('columnheader').textContent).toBe('Approved');
    expect(result.getByRole('cell').textContent).toBe('5');
    result.unmount();
  });
  it('renders string scalar answers without calling them numbers', () => {
    const answer = {
      ...scalar,
      results: [{ ...scalar.results[0], rows: [['Ready']] }],
    };
    const result = render(() => (
      <QueryResults answer={answer} displayMode="scalar" />
    ));
    expect(result.queryByRole('table')).toBeNull();
    expect(result.getByText('Ready')).toBeTruthy();
    result.unmount();
  });

  it('hides row ids from result tables', () => {
    const answer: QueryAnswer = {
      ...scalar,
      results: [
        {
          columns: [
            { name: 'row_id', entity_type: null },
            { name: 'Title', entity_type: null },
          ],
          rows: [
            ['0190a3c4-row-1', 'Launch plan'],
            ['0190a3c4-row-2', 'Hiring'],
          ],
        },
      ],
    };
    const result = render(() => (
      <QueryResults answer={answer} displayMode="table" />
    ));
    const table = result.getByRole('table');
    expect(
      Array.from(table.querySelectorAll('th')).map((cell) => cell.textContent)
    ).toEqual(['Title']);
    expect(
      Array.from(table.querySelectorAll('td')).map((cell) => cell.textContent)
    ).toEqual(['Launch plan', 'Hiring']);
    result.unmount();
  });

  it('shows row ids when they are the only column', () => {
    const answer: QueryAnswer = {
      ...scalar,
      results: [
        {
          columns: [{ name: 'row_id', entity_type: null }],
          rows: [['0190a3c4-row-1'], ['0190a3c4-row-2']],
        },
      ],
    };
    const result = render(() => (
      <QueryResults answer={answer} displayMode="table" />
    ));
    const table = result.getByRole('table');
    expect(
      Array.from(table.querySelectorAll('th')).map((cell) => cell.textContent)
    ).toEqual(['row id']);
    expect(
      Array.from(table.querySelectorAll('td')).map((cell) => cell.textContent)
    ).toEqual(['0190a3c4-row-1', '0190a3c4-row-2']);
    result.unmount();
  });
});
