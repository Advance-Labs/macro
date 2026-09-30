import { render } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import { AnswerDisplayProvider } from '../context/answer-display';
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

  it('draws each column kind as the database grid does', () => {
    const answer: QueryAnswer = {
      ...scalar,
      read_database_ids: ['party-planner'],
      results: [
        {
          columns: [
            { name: 'Name', entity_type: null, origin: ['Parties', 'Name'] },
            { name: 'Date', entity_type: null, origin: ['Parties', 'Date'] },
            { name: 'Guests', entity_type: null, origin: null },
            {
              name: 'Plus One',
              entity_type: null,
              origin: ['Parties', 'Plus One'],
            },
            { name: 'RSVP', entity_type: null, origin: ['Parties', 'RSVP'] },
            { name: 'Host', entity_type: 'user', origin: ['Parties', 'Host'] },
            {
              name: 'Plan',
              entity_type: 'document',
              origin: ['Parties', 'Plan'],
            },
            {
              name: 'Task',
              entity_type: 'document',
              origin: ['Parties', 'Task'],
            },
          ],
          rows: [
            [
              'Gala',
              '2025-12-31T00:00:00+00:00',
              1200,
              1,
              'Maybe',
              'macro|ada@macro.com',
              'doc-1',
              'task-1',
            ],
          ],
        },
      ],
    };
    const base = { isMultiSelect: false, options: [], writable: true };
    const result = render(() => (
      <AnswerDisplayProvider
        value={{
          mention: (id, entityType) => (
            <span
              data-testid={`${entityType}-mention`}
            >{`${entityType} ${id}`}</span>
          ),
          text: (markdown) => <span data-testid="markdown">{markdown}</span>,
          columns: (current) => () => (column) => {
            expect(current()?.read_database_ids).toEqual(['party-planner']);
            const name = column.origin?.[1];
            if (name === 'Name')
              return { ...base, id: 'n', name, dataType: 'STRING' };
            if (name === 'Date')
              return { ...base, id: 'd', name, dataType: 'DATE' };
            if (name === 'Plus One')
              return { ...base, id: 'p', name, dataType: 'BOOLEAN' };
            if (name === 'RSVP')
              return {
                ...base,
                id: 'r',
                name,
                dataType: 'SELECT_STRING',
                optionColors: { Maybe: 'amber' },
              };
            if (name === 'Host')
              return {
                ...base,
                id: 'h',
                name,
                dataType: 'ENTITY',
                specificEntityType: 'USER',
              };
            if (name === 'Plan')
              return {
                ...base,
                id: 'l',
                name,
                dataType: 'ENTITY',
                specificEntityType: 'DOCUMENT',
              };
            if (name === 'Task')
              return {
                ...base,
                id: 't',
                name,
                dataType: 'ENTITY',
                specificEntityType: 'TASK',
              };
          },
        }}
      >
        <QueryResults answer={answer} displayMode="table" />
      </AnswerDisplayProvider>
    ));
    const cells = Array.from(result.getByRole('table').querySelectorAll('td'));
    expect(cells.map((cell) => cell.textContent)).toEqual([
      'Gala',
      'Dec 31, 2025',
      '1,200',
      '',
      'Maybe',
      'USER macro|ada@macro.com',
      'DOCUMENT doc-1',
      'TASK task-1',
    ]);
    expect(cells[0].querySelector('[data-testid="markdown"]')).toBeTruthy();
    expect(
      cells[3].querySelector<HTMLInputElement>('input[type="checkbox"]')
        ?.checked
    ).toBe(true);
    expect(cells[4].querySelector('[title="Maybe"]')).toBeTruthy();
    expect(cells[5].querySelector('[data-testid="USER-mention"]')).toBeTruthy();
    expect(
      cells[6].querySelector('[data-testid="DOCUMENT-mention"]')
    ).toBeTruthy();
    expect(cells[7].querySelector('[data-testid="TASK-mention"]')).toBeTruthy();
    result.unmount();
  });

  it('draws entities and dates without a known column from the engine’s types', () => {
    const answer: QueryAnswer = {
      ...scalar,
      results: [
        {
          columns: [
            { name: 'Owner', entity_type: 'user' },
            { name: 'Due', entity_type: null },
          ],
          rows: [['macro|ada@macro.com', '2025-07-19T00:00:00+00:00']],
        },
      ],
    };
    const result = render(() => (
      <AnswerDisplayProvider
        value={{
          mention: (id, entityType) => (
            <span
              data-testid={`${entityType}-mention`}
            >{`${entityType} ${id}`}</span>
          ),
          text: (markdown) => markdown,
          columns: () => () => () => undefined,
        }}
      >
        <QueryResults answer={answer} displayMode="table" />
      </AnswerDisplayProvider>
    ));
    const cells = Array.from(result.getByRole('table').querySelectorAll('td'));
    expect(cells.map((cell) => cell.textContent)).toEqual([
      'USER macro|ada@macro.com',
      'Jul 19, 2025',
    ]);
    expect(cells[0].querySelector('[data-testid="USER-mention"]')).toBeTruthy();
    result.unmount();
  });

  it('shows a single date answer as a date', () => {
    const answer: QueryAnswer = {
      ...scalar,
      results: [
        {
          columns: [{ name: 'Next party', entity_type: null }],
          rows: [['2025-12-31T00:00:00+00:00']],
        },
      ],
    };
    const result = render(() => (
      <QueryResults answer={answer} displayMode="scalar" />
    ));
    expect(result.getByText('Dec 31, 2025')).toBeTruthy();
    expect(result.queryByText(/2025-12-31/)).toBeNull();
    result.unmount();
  });
});
