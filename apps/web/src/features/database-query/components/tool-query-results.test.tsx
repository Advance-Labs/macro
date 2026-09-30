import { showDatabaseSql } from '@core/constant/featureFlags';
import { fireEvent, render, screen } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { createSignal, type JSX } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { QueryAnswer } from '../core/query';
import { ToolQueryResults } from './tool-query-results';

vi.mock('@solid-primitives/resize-observer', () => ({
  createElementSize: () => ({ width: 346, height: 300 }),
}));
vi.mock('../answer-display', () => ({
  AppAnswerDisplay: (props: { children: JSX.Element }) => props.children,
}));

const answer: QueryAnswer = {
  results: [
    {
      columns: [
        { name: 'Team', entity_type: null },
        { name: 'Tickets', entity_type: null },
      ],
      rows: [
        ['Support', 12],
        ['Sales', 4],
      ],
    },
  ],
  read_tables: [],
  read_versions: {},
  truncated_tables: [],
};

afterEach(() => {
  showDatabaseSql.enabled = false;
});

describe('native chat database answers', () => {
  it('opens the requested chart and lets the reader override it', async () => {
    const rendered = render(() => (
      <ToolQueryResults
        answer={answer}
        sql="SELECT team, count FROM tickets"
        preferredDisplay="bar"
      />
    ));
    expect(rendered.getByRole('img', { name: /Tickets by Team/ })).toBeTruthy();
    await fireEvent.keyDown(
      rendered.getByRole('button', { name: /Display database results/ }),
      { key: 'ArrowDown' }
    );
    await userEvent.click(
      await screen.findByRole('option', { name: /^Table$/ })
    );
    expect(rendered.getByRole('table').textContent).toContain('Support12');
    rendered.unmount();
  });

  it('falls back to a table when the requested chart cannot represent the result', () => {
    const textOnly = {
      ...answer,
      results: [
        {
          columns: [{ name: 'Name', entity_type: null }],
          rows: [['Ada'], ['Grace']],
        },
      ],
    };
    const rendered = render(() => (
      <ToolQueryResults
        answer={textOnly}
        sql="SELECT name FROM customers"
        preferredDisplay="bar"
      />
    ));
    expect(rendered.getByRole('table').textContent).toContain('Ada');
    expect(rendered.queryByRole('img')).toBeNull();
    rendered.unmount();
  });

  it('switches real query data between table and compatible charts', async () => {
    showDatabaseSql.enabled = true;
    const rendered = render(() => (
      <ToolQueryResults
        answer={answer}
        sql="SELECT team, count(*) AS Tickets FROM tickets GROUP BY team"
      />
    ));
    expect(rendered.getByRole('table').textContent).toContain('Support12');
    await fireEvent.keyDown(
      rendered.getByRole('button', { name: /Display database results/ }),
      { key: 'ArrowDown' }
    );
    await userEvent.click(
      await screen.findByRole('option', { name: 'Bar chart' })
    );
    expect(rendered.getByRole('img', { name: /Tickets by Team/ })).toBeTruthy();
    expect(rendered.getByText('View data')).toBeTruthy();
    expect(rendered.getByText('View SQL')).toBeTruthy();
    rendered.unmount();
  });

  it('only offers a table for text-only results and recovers when streamed result shape changes', async () => {
    const [value, setValue] = createSignal(answer);
    const rendered = render(() => (
      <ToolQueryResults answer={value()} sql="SELECT name FROM customers" />
    ));
    await fireEvent.keyDown(
      rendered.getByRole('button', { name: /Display database results/ }),
      { key: 'ArrowDown' }
    );
    await userEvent.click(
      await screen.findByRole('option', { name: 'Pie chart' })
    );
    setValue({
      ...answer,
      results: [
        {
          columns: [{ name: 'Customer', entity_type: null }],
          rows: [['Acme'], ['Macro']],
        },
      ],
    });
    expect(rendered.queryByRole('img')).toBeNull();
    expect(rendered.getByRole('table').textContent).toContain('Acme');
    await fireEvent.keyDown(
      rendered.getByRole('button', { name: /Display database results/ }),
      { key: 'ArrowDown' }
    );
    expect(screen.queryByRole('option', { name: /chart/ })).toBeNull();
    rendered.unmount();
  });
});

describe('native chat database answers with SQL hidden', () => {
  it('shows the result without the statement', () => {
    const rendered = render(() => (
      <ToolQueryResults answer={answer} sql="SELECT team, count FROM tickets" />
    ));
    expect(rendered.getByRole('table').textContent).toContain('Support12');
    expect(rendered.queryByText('View SQL')).toBeNull();
    expect(rendered.container.textContent).not.toContain('SELECT');
    rendered.unmount();
  });
});
