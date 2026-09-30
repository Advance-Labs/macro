import { cleanup, render, screen } from '@solidjs/testing-library';
import type { ParentProps } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { pendingDatabaseTool } from './DatabaseTools';

vi.mock('@app/features/database-query/components/tool-query-results', () => ({
  ToolQueryResults: () => null,
}));
vi.mock('@app/signal/splitLayout', () => ({ globalSplitManager: () => null }));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => ({}),
}));
vi.mock('@queries/client', () => ({
  queryClient: { invalidateQueries: vi.fn() },
}));
vi.mock(
  '@core/component/LexicalMarkdown/component/core/StaticMarkdown',
  () => ({
    StaticMarkdownContext: (props: ParentProps) => props.children,
    StaticMarkdown: (props: { markdown: string }) => (
      <output aria-label="Saved answer">{props.markdown}</output>
    ),
  })
);
afterEach(cleanup);

const databaseId = '01992d2f-8444-7000-8000-000000000001';
const tableId = '01992d2f-8444-7000-8000-000000000002';
const columnId = '01992d2f-8444-7000-8000-000000000003';

function tool(name: string, json: unknown, response?: unknown) {
  const Tool = pendingDatabaseTool(name);
  if (!Tool) throw new Error(`No renderer for ${name}`);
  return render(() => (
    <Tool
      json={json}
      response={response === undefined ? undefined : { json: response }}
      isComplete={response !== undefined}
      renderContext={{ isStreaming: false, grouped: false }}
    />
  ));
}

describe('database schema tool activity', () => {
  it.each([
    [
      'RenameColumn',
      { databaseId, tableId, columnId, name: 'Owner' },
      'Rename column to Owner',
    ],
    [
      'ChangeColumnType',
      {
        databaseId,
        tableId,
        columnId,
        dataType: 'SELECT_STRING',
        options: ['Open', 'Done'],
      },
      'Change column type to SELECT_STRING · Open, Done',
    ],
    ['DeleteColumn', { databaseId, tableId, columnId }, 'Delete column'],
    [
      'ReorderColumns',
      { databaseId, tableId, columnIds: [columnId, tableId] },
      'Reorder 2 columns',
    ],
    ['DeleteTable', { databaseId, tableId }, 'Delete table'],
    [
      'RenameDatabase',
      { databaseId, name: 'Launch' },
      'Rename database to Launch',
    ],
  ])('renders %s as one line', (name, json, text) => {
    const rendered = tool(name, json, {});
    expect(rendered.container.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      text
    );
    expect(screen.queryByText('Failed')).toBeNull();
  });

  it('marks a completed tool without a readable response as failed', () => {
    tool('DeleteTable', { databaseId, tableId }, 'not an object');
    expect(screen.getByText('Failed')).toBeTruthy();
  });

  it('renders nothing for a call that does not match the tool contract', () => {
    const rendered = tool('RenameColumn', { databaseId, name: 'Owner' });
    expect(rendered.container.textContent).toBe('');
  });

  it('knows only the pending database tools', () => {
    expect(pendingDatabaseTool('QueryDatabase')).toBeUndefined();
    expect(pendingDatabaseTool('toString')).toBeUndefined();
  });
});

describe('SaveDatabaseQuery', () => {
  it('renders the saved block from the tool output', () => {
    const markdown =
      '<m-db-query>{"queryId":"01992d2f-8444-7000-8000-000000000004","title":"Open tickets","prompt":"How many open tickets?","displayMode":"scalar"}</m-db-query>';
    tool(
      'SaveDatabaseQuery',
      {
        databaseId,
        sql: 'SELECT COUNT(*) FROM "Tickets"',
        title: 'Open tickets',
        displayMode: 'scalar',
      },
      { queryId: '01992d2f-8444-7000-8000-000000000004', markdown }
    );
    expect(screen.getByText('Saved question')).toBeTruthy();
    expect(screen.getByLabelText('Saved answer').textContent).toBe(markdown);
  });

  it('shows the pending save before the output arrives', () => {
    tool('SaveDatabaseQuery', {
      sql: 'SELECT 1',
      title: 'One',
      displayMode: 'table',
    });
    expect(screen.getByText(/Save question/)).toBeTruthy();
    expect(screen.queryByLabelText('Saved answer')).toBeNull();
  });
});
