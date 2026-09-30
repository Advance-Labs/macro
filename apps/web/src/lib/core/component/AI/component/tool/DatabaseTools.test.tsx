import { databaseViewKeys } from '@app/features/block-database/queries/keys';
import type {
  NamedTool,
  ToolName,
} from '@service-cognition/generated/tools/tool';
import { cleanup, render, screen } from '@solidjs/testing-library';
import type { Component, ParentProps } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  changeColumnTypeHandler,
  deleteColumnHandler,
  deleteTableHandler,
  renameColumnHandler,
  renameDatabaseHandler,
  reorderColumnsHandler,
  reorderTablesHandler,
  saveDatabaseQueryHandler,
  saveDatabaseViewHandler,
} from './DatabaseTools';

vi.mock('@app/features/database-query/components/tool-query-results', () => ({
  ToolQueryResults: () => null,
}));
vi.mock('@app/signal/splitLayout', () => ({ globalSplitManager: () => null }));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => ({}),
}));
const invalidateQueries = vi.hoisted(() => vi.fn());
vi.mock('@queries/client', () => ({
  queryClient: { invalidateQueries },
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
const otherColumnId = '01992d2f-8444-7000-8000-000000000005';

const database: NamedTool<'DescribeDatabase', 'response'>['data'] = {
  id: databaseId,
  name: 'Launch',
  grant: 'edit',
  sqlGuide: '',
  tables: [
    {
      id: tableId,
      name: 'Tickets',
      sqlName: 'Tickets',
      version: 3,
      writable: true,
      columns: [
        {
          id: columnId,
          name: 'Status',
          sqlName: 'Status',
          dataType: 'select',
          isMultiSelect: false,
          writable: true,
        },
      ],
    },
  ],
};

function renderTool<Name extends ToolName>(
  handler: { render: Component<never> },
  name: Name,
  call: NamedTool<Name, 'call'>['data'],
  response?: NamedTool<Name, 'response'>['data']
) {
  return render(() => (
    <Dynamic
      component={handler.render as Component<Record<string, unknown>>}
      tool={{ id: 'tool-1', name, data: call }}
      response={
        response === undefined
          ? undefined
          : { id: 'tool-1', name, data: response }
      }
      chat_id="chat-1"
      message_id="message-1"
      part_index={0}
      isComplete={response !== undefined}
      renderContext={{ isStreaming: false, grouped: false }}
    />
  ));
}

function line(rendered: ReturnType<typeof render>) {
  return rendered.container.textContent?.replace(/\s+/g, ' ').trim();
}

describe('database schema tool activity', () => {
  it('renders RenameDatabase with the name the server kept', () => {
    const rendered = renderTool(
      renameDatabaseHandler,
      'RenameDatabase',
      { databaseId, name: 'launch ' },
      { databaseId, name: 'Launch', database }
    );
    expect(line(rendered)).toBe('Rename database to Launch');
  });

  it('renders DeleteTable with the database it left', () => {
    const rendered = renderTool(
      deleteTableHandler,
      'DeleteTable',
      { databaseId, tableId },
      { databaseId, tableId, database }
    );
    expect(line(rendered)).toBe('Delete table from Launch');
  });

  it('renders RenameColumn with its table', () => {
    const rendered = renderTool(
      renameColumnHandler,
      'RenameColumn',
      { databaseId, tableId, columnId, name: 'Status' },
      { databaseId, tableId, columnId, name: 'Status', database }
    );
    expect(line(rendered)).toBe('Rename column to Status in Tickets');
  });

  it('renders ChangeColumnType with the column name from the schema', () => {
    const rendered = renderTool(
      changeColumnTypeHandler,
      'ChangeColumnType',
      {
        databaseId,
        tableId,
        columnId,
        dataType: 'select',
        options: ['Open', 'Done'],
      },
      { databaseId, tableId, columnId, database }
    );
    expect(line(rendered)).toBe('Change Status to select · Open, Done');
  });

  it('renders ChangeColumnType before the schema arrives', () => {
    const rendered = renderTool(changeColumnTypeHandler, 'ChangeColumnType', {
      databaseId,
      tableId,
      columnId,
      dataType: 'number',
    });
    expect(line(rendered)).toBe('Change column type to number');
  });

  it('renders DeleteColumn with the table it left', () => {
    const rendered = renderTool(
      deleteColumnHandler,
      'DeleteColumn',
      { databaseId, tableId, columnId: otherColumnId },
      { databaseId, tableId, columnId: otherColumnId, database }
    );
    expect(line(rendered)).toBe('Delete column from Tickets');
  });

  it('renders ReorderColumns with its table', () => {
    const rendered = renderTool(
      reorderColumnsHandler,
      'ReorderColumns',
      { databaseId, tableId, columnIds: [columnId, otherColumnId] },
      { databaseId, tableId, database }
    );
    expect(line(rendered)).toBe('Reorder 2 columns in Tickets');
  });

  it('renders ReorderTables with its database', () => {
    const rendered = renderTool(
      reorderTablesHandler,
      'ReorderTables',
      { databaseId, tableIds: [tableId, otherColumnId] },
      { databaseId, tableIds: [tableId, otherColumnId], database }
    );
    expect(line(rendered)).toBe('Reorder 2 tables in Launch');
  });

  it('renders a delete without a refreshed schema', () => {
    const rendered = renderTool(
      deleteColumnHandler,
      'DeleteColumn',
      { databaseId, tableId, columnId },
      {
        databaseId,
        tableId,
        columnId,
        database: null,
        warning: 'Call DescribeDatabase before continuing.',
      }
    );
    expect(line(rendered)).toBe('Delete column');
  });
});

describe('SaveDatabaseQuery', () => {
  it('renders the saved block from the tool output', () => {
    const markdown =
      '<m-db-query>{"queryId":"01992d2f-8444-7000-8000-000000000004","title":"Open tickets","prompt":"How many open tickets?","displayMode":"scalar"}</m-db-query>';
    renderTool(
      saveDatabaseQueryHandler,
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
    renderTool(saveDatabaseQueryHandler, 'SaveDatabaseQuery', {
      sql: 'SELECT 1',
      title: 'One',
      displayMode: 'table',
    });
    expect(screen.getByText(/Save question/)).toBeTruthy();
    expect(screen.queryByLabelText('Saved answer')).toBeNull();
  });
});

describe('SaveDatabaseView', () => {
  it('refreshes saved views when the save responds, not on each render', async () => {
    renderTool(saveDatabaseViewHandler, 'SaveDatabaseView', {
      databaseId,
      tableId,
      name: 'Open',
      view: { layout: 'table' },
    });
    expect(invalidateQueries).not.toHaveBeenCalled();
    await saveDatabaseViewHandler.handleResponse?.({} as never);
    expect(invalidateQueries).toHaveBeenCalledExactlyOnceWith({
      queryKey: databaseViewKeys.saved.queryKey,
    });
  });
});
