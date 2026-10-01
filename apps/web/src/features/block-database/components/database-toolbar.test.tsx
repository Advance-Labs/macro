import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { errAsync, okAsync } from 'neverthrow';
import { createSignal } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ViewChange } from '../queries/views';
import { DatabaseToolbar } from './database-toolbar';

vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => false }));

let menuStyles: HTMLStyleElement;
beforeEach(() => {
  vi.stubGlobal('scrollTo', vi.fn());
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
  // JSDOM reports an empty animation name; presence expects CSS's default none.
  menuStyles = document.createElement('style');
  menuStyles.textContent =
    '[role=menu], [role=dialog] { animation-name: none; }';
  document.head.append(menuStyles);
});
afterEach(() => {
  cleanup();
  menuStyles.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('database toolbar views', () => {
  it('selects a stored view, and All records', () => {
    const select = vi.fn();
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
          {
            id: 'next',
            databaseId: 'database',
            tableId: 'table',
            name: 'Next week',
            position: 'a1',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={select}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    expect(
      screen
        .getByRole('button', { name: 'My work' })
        .getAttribute('aria-pressed')
    ).toBe('true');
    expect(
      screen
        .getByRole('button', { name: 'All records' })
        .getAttribute('aria-pressed')
    ).toBe('false');
    fireEvent.click(screen.getByRole('button', { name: 'Next week' }));
    expect(select).toHaveBeenLastCalledWith('next');
    fireEvent.click(screen.getByRole('button', { name: 'All records' }));
    expect(select).toHaveBeenLastCalledWith();
  });

  it('renames a view inline from a double click, saving on Enter', async () => {
    const rename = vi.fn(() => okAsync(undefined));
    const select = vi.fn();
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
          {
            id: 'next',
            databaseId: 'database',
            tableId: 'table',
            name: 'Next week',
            position: 'a1',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={select}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={rename}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    const tab = screen.getByRole('button', { name: 'Next week' });
    fireEvent.dblClick(tab);
    const name = (await screen.findByRole('textbox', {
      name: 'View name',
    })) as HTMLInputElement;
    await waitFor(() => expect(document.activeElement).toBe(name));
    expect(name.value).toBe('Next week');
    expect(name.selectionStart).toBe(0);
    expect(name.selectionEnd).toBe('Next week'.length);
    fireEvent.input(name, { target: { value: '  Soon  ' } });
    // Enter in the input submits its form; JSDOM does not do that itself.
    fireEvent.submit(name.closest('form')!);
    await waitFor(() =>
      expect(screen.queryByRole('textbox', { name: 'View name' })).toBeNull()
    );
    expect(rename).toHaveBeenCalledExactlyOnceWith(
      {
        id: 'next',
        databaseId: 'database',
        tableId: 'table',
        name: 'Next week',
        position: 'a1',
        query: { filter: null, sort: [] },
        layout: { kind: 'table', columns: [] },
        createdAt: '2026-09-01T00:00:00Z',
        updatedAt: '2026-09-01T00:00:00Z',
      },
      'Soon'
    );
    expect(select).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole('button', { name: 'Next week' })
      )
    );
  });

  it('cancels an inline rename started with F2 on Escape', async () => {
    const rename = vi.fn(() => okAsync(undefined));
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={rename}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    fireEvent.keyDown(screen.getByRole('button', { name: 'My work' }), {
      key: 'F2',
    });
    const name = await screen.findByRole('textbox', { name: 'View name' });
    fireEvent.input(name, { target: { value: 'Abandoned' } });
    fireEvent.keyDown(name, { key: 'Escape' });
    await waitFor(() =>
      expect(screen.queryByRole('textbox', { name: 'View name' })).toBeNull()
    );
    expect(rename).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'My work' })).toBeTruthy();
  });

  it('keeps the draft and says why when a rename is refused', async () => {
    const rename = vi.fn(() =>
      errAsync({
        kind: 'ops' as const,
        error: {
          code: 'INVALID_OP' as const,
          message: 'a view named `My work` already exists on this table',
          refusal: {
            op: 0,
            row: null,
            column: null,
            message: 'a view named `My work` already exists on this table',
          },
        },
      })
    );
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
          {
            id: 'next',
            databaseId: 'database',
            tableId: 'table',
            name: 'Next week',
            position: 'a1',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={rename}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    fireEvent.dblClick(screen.getByRole('button', { name: 'Next week' }));
    const name = (await screen.findByRole('textbox', {
      name: 'View name',
    })) as HTMLInputElement;
    fireEvent.input(name, { target: { value: 'My work' } });
    fireEvent.submit(name.closest('form')!);
    expect((await screen.findByRole('alert')).textContent).toBe(
      'a view named `My work` already exists on this table'
    );
    expect(rename).toHaveBeenCalledOnce();
    expect(
      (screen.getByRole('textbox', { name: 'View name' }) as HTMLInputElement)
        .value
    ).toBe('My work');
  });

  it('deletes a view after confirming in the dialog', async () => {
    const remove = vi.fn(() => okAsync(undefined));
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
          {
            id: 'next',
            databaseId: 'database',
            tableId: 'table',
            name: 'Next week',
            position: 'a1',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={remove}
        onReorderViews={vi.fn()}
      />
    ));
    fireEvent.contextMenu(screen.getByRole('button', { name: 'Next week' }), {
      clientX: 100,
      clientY: 40,
    });
    fireEvent(
      await screen.findByRole('menuitem', { name: 'Delete view' }),
      new MouseEvent('pointerup', { button: 0, bubbles: true })
    );
    await screen.findByRole('dialog', { name: 'Delete view?' });
    await waitFor(() =>
      expect(screen.queryByRole('menu', { hidden: true })).toBeNull()
    );
    expect(
      screen.getByText('“Next week” will be deleted for everyone.', {
        exact: false,
      })
    ).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Delete view' }));
    await waitFor(() =>
      expect(remove).toHaveBeenCalledExactlyOnceWith({
        id: 'next',
        databaseId: 'database',
        tableId: 'table',
        name: 'Next week',
        position: 'a1',
        query: { filter: null, sort: [] },
        layout: { kind: 'table', columns: [] },
        createdAt: '2026-09-01T00:00:00Z',
        updatedAt: '2026-09-01T00:00:00Z',
      })
    );
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Delete view?' })).toBeNull()
    );
  });

  it('creates a view from the New view dialog', async () => {
    const create = vi.fn(() => okAsync(undefined));
    render(() => (
      <DatabaseToolbar
        columns={[
          {
            id: 'name',
            name: 'Name',
            dataType: 'STRING',
            isMultiSelect: false,
            writable: true,
            options: [],
          },
        ]}
        views={[]}
        view={{
          id: 'table',
          databaseId: 'database',
          tableId: 'table',
          name: 'All records',
          position: '',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '1970-01-01T00:00:00.000Z',
          updatedAt: '1970-01-01T00:00:00.000Z',
        }}
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={create}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'New view' }));
    await screen.findByRole('dialog', { name: 'New view' });
    const name = screen.getByRole('textbox', {
      name: 'View name',
    }) as HTMLInputElement;
    expect(name.value).toBe('Table view');
    fireEvent.input(name, { target: { value: 'Planning' } });
    fireEvent.submit(name.closest('form')!);
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'New view' })).toBeNull()
    );
    expect(create).toHaveBeenCalledExactlyOnceWith({
      name: 'Planning',
      layout: 'table',
    });
  });
});

describe('database toolbar view controls', () => {
  it('adds a sort and changes its direction', async () => {
    const [view, setView] = createSignal<DatabaseView>({
      id: 'work',
      databaseId: 'database',
      tableId: 'table',
      name: 'My work',
      position: 'a0',
      query: {
        filter: {
          conjunction: 'and',
          conditions: [
            {
              kind: 'condition',
              column: 'name',
              test: { kind: 'text', operator: 'contains', value: 'Plan' },
            },
          ],
        },
        sort: [],
      },
      layout: { kind: 'table', columns: [] },
      createdAt: '2026-09-01T00:00:00Z',
      updatedAt: '2026-09-01T00:00:00Z',
    });
    const change = vi.fn((next: ViewChange) => setView({ ...view(), ...next }));
    render(() => (
      <DatabaseToolbar
        columns={[
          {
            id: 'name',
            name: 'Name',
            dataType: 'STRING',
            isMultiSelect: false,
            writable: true,
            options: [],
          },
          {
            id: 'due',
            name: 'Due',
            dataType: 'DATE',
            isMultiSelect: false,
            writable: true,
            options: [],
          },
        ]}
        views={[view()]}
        view={view()}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={change}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    expect(screen.getByRole('button', { name: 'Filter 1' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Sort' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Add sort' }));
    expect(change).toHaveBeenLastCalledWith({
      query: {
        filter: {
          conjunction: 'and',
          conditions: [
            {
              kind: 'condition',
              column: 'name',
              test: { kind: 'text', operator: 'contains', value: 'Plan' },
            },
          ],
        },
        sort: [{ column: 'name', direction: 'ascending' }],
      },
    });
    fireEvent.keyDown(
      await screen.findByRole('button', { name: /^Sort direction/ }),
      { key: 'Enter' }
    );
    fireEvent.keyDown(
      await screen.findByRole('option', { name: 'Descending' }),
      { key: 'Enter' }
    );
    expect(change).toHaveBeenLastCalledWith({
      query: {
        filter: {
          conjunction: 'and',
          conditions: [
            {
              kind: 'condition',
              column: 'name',
              test: { kind: 'text', operator: 'contains', value: 'Plan' },
            },
          ],
        },
        sort: [{ column: 'name', direction: 'descending' }],
      },
    });
    expect(screen.getByRole('button', { name: 'Sort 1' })).toBeTruthy();
  });

  it('hides a table column and switches a stored view to a board', async () => {
    const change = vi.fn();
    render(() => (
      <DatabaseToolbar
        columns={[
          {
            id: 'name',
            name: 'Name',
            dataType: 'STRING',
            isMultiSelect: false,
            writable: true,
            options: [],
          },
          {
            id: 'status',
            name: 'Status',
            dataType: 'SELECT_STRING',
            isMultiSelect: false,
            writable: true,
            options: [
              { id: 'option-to-do', label: 'To do', color: null },
              { id: 'option-done', label: 'Done', color: null },
            ],
          },
          {
            id: 'due',
            name: 'Due',
            dataType: 'DATE',
            isMultiSelect: false,
            writable: true,
            options: [],
          },
        ]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: {
              kind: 'table',
              columns: [
                { column: 'due', width: 180, hidden: true },
                { column: 'name', width: 240, hidden: false },
              ],
            },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: {
            kind: 'table',
            columns: [
              { column: 'due', width: 180, hidden: true },
              { column: 'name', width: 240, hidden: false },
            ],
          },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="work"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={change}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
        addColumn={<button type="button">Add column</button>}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'View settings' }));
    expect(
      await screen.findByRole('button', { name: 'Add column' })
    ).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Table' }).getAttribute('aria-pressed')
    ).toBe('true');
    expect(
      (screen.getByRole('switch', { name: 'Due' }) as HTMLInputElement).checked
    ).toBe(false);
    fireEvent.click(screen.getByRole('switch', { name: 'Status' }));
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'table',
        columns: [
          { column: 'due', width: 180, hidden: true },
          { column: 'name', width: 240, hidden: false },
          { column: 'status', width: null, hidden: true },
        ],
      },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Show all columns' }));
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'table',
        columns: [
          { column: 'due', width: 180, hidden: false },
          { column: 'name', width: 240, hidden: false },
        ],
      },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Board' }));
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'board',
        groupBy: 'status',
        lanes: [],
        cardFields: ['due'],
        hideEmptyLanes: false,
      },
    });
  });

  it('changes a board view’s lanes and card fields', async () => {
    const change = vi.fn();
    render(() => (
      <DatabaseToolbar
        columns={[
          {
            id: 'name',
            name: 'Name',
            dataType: 'STRING',
            isMultiSelect: false,
            writable: true,
            options: [],
          },
          {
            id: 'status',
            name: 'Status',
            dataType: 'SELECT_STRING',
            isMultiSelect: false,
            writable: true,
            options: [
              { id: 'option-to-do', label: 'To do', color: null },
              { id: 'option-done', label: 'Done', color: null },
            ],
          },
          {
            id: 'priority',
            name: 'Priority',
            dataType: 'SELECT_STRING',
            isMultiSelect: false,
            writable: true,
            options: [{ id: 'option-high', label: 'High', color: null }],
          },
        ]}
        views={[
          {
            id: 'board',
            databaseId: 'database',
            tableId: 'table',
            name: 'Board',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: {
              kind: 'board',
              groupBy: 'status',
              lanes: [
                { option: 'option-done', hidden: true },
                { option: null, hidden: true },
              ],
              cardFields: ['priority'],
              hideEmptyLanes: false,
            },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'board',
          databaseId: 'database',
          tableId: 'table',
          name: 'Board',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: {
            kind: 'board',
            groupBy: 'status',
            lanes: [
              { option: 'option-done', hidden: true },
              { option: null, hidden: true },
            ],
            cardFields: ['priority'],
            hideEmptyLanes: false,
          },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="board"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={change}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'View settings' }));
    fireEvent.click(
      await screen.findByRole('switch', { name: 'Hide empty lanes' })
    );
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'board',
        groupBy: 'status',
        lanes: [
          { option: 'option-done', hidden: true },
          { option: null, hidden: true },
        ],
        cardFields: ['priority'],
        hideEmptyLanes: true,
      },
    });
    expect(screen.getByText('Hidden lanes')).toBeTruthy();
    expect(screen.getByText('No status')).toBeTruthy();
    const [showDone] = screen.getAllByRole('button', { name: 'Show' });
    fireEvent.click(showDone);
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'board',
        groupBy: 'status',
        lanes: [
          { option: 'option-done', hidden: false },
          { option: null, hidden: true },
        ],
        cardFields: ['priority'],
        hideEmptyLanes: false,
      },
    });
    expect(screen.getByText('Card fields')).toBeTruthy();
    expect(screen.queryByRole('switch', { name: 'Status' })).toBeNull();
    fireEvent.click(screen.getByRole('switch', { name: 'Name' }));
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'board',
        groupBy: 'status',
        lanes: [
          { option: 'option-done', hidden: true },
          { option: null, hidden: true },
        ],
        cardFields: ['priority', 'name'],
        hideEmptyLanes: false,
      },
    });
    fireEvent.click(screen.getByRole('switch', { name: 'Priority' }));
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'board',
        groupBy: 'status',
        lanes: [
          { option: 'option-done', hidden: true },
          { option: null, hidden: true },
        ],
        cardFields: [],
        hideEmptyLanes: false,
      },
    });
    fireEvent.keyDown(screen.getByRole('button', { name: /^Group board by/ }), {
      key: 'Enter',
    });
    fireEvent.keyDown(await screen.findByRole('option', { name: 'Priority' }), {
      key: 'Enter',
    });
    expect(change).toHaveBeenLastCalledWith({
      layout: {
        kind: 'board',
        groupBy: 'priority',
        lanes: [],
        cardFields: ['priority'],
        hideEmptyLanes: false,
      },
    });
  });

  it('opens search inline, clears it in place, and closes it on Escape', async () => {
    const [search, setSearch] = createSignal('');
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[]}
        view={{
          id: 'table',
          databaseId: 'database',
          tableId: 'table',
          name: 'All records',
          position: '',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '1970-01-01T00:00:00.000Z',
          updatedAt: '1970-01-01T00:00:00.000Z',
        }}
        canEdit
        search={search()}
        onSearchChange={setSearch}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));
    const input = await screen.findByRole('searchbox', {
      name: 'Search records',
    });
    await waitFor(() => expect(document.activeElement).toBe(input));
    fireEvent.input(input, { target: { value: 'launch' } });
    expect(search()).toBe('launch');
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(search()).toBe('');
    expect(document.activeElement).toBe(input);
    fireEvent.input(input, { target: { value: 'second search' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole('button', { name: 'Search' })
      )
    );
    expect(screen.queryByRole('searchbox')).toBeNull();
    expect(search()).toBe('');
  });

  it('shows viewers no view controls on a stored view, but keeps them on All records', () => {
    const [selected, setSelected] = createSignal<string | undefined>('work');
    render(() => (
      <DatabaseToolbar
        columns={[
          {
            id: 'name',
            name: 'Name',
            dataType: 'STRING',
            isMultiSelect: false,
            writable: false,
            options: [],
          },
        ]}
        views={[
          {
            id: 'work',
            databaseId: 'database',
            tableId: 'table',
            name: 'My work',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: { kind: 'table', columns: [] },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'work',
          databaseId: 'database',
          tableId: 'table',
          name: 'My work',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId={selected()}
        canEdit={false}
        search=""
        onSearchChange={vi.fn()}
        onSelectView={setSelected}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
      />
    ));
    expect(screen.queryByRole('button', { name: 'Filter' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Sort' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'View settings' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'New view' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Search' })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole('button', { name: 'My work' }), {
      key: 'F2',
    });
    expect(screen.queryByRole('textbox', { name: 'View name' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'All records' }));
    expect(screen.getByRole('button', { name: 'Filter' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Sort' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'View settings' })).toBeTruthy();
  });

  it('creates a record from a board’s New button', () => {
    const createRecord = vi.fn();
    const [creating, setCreating] = createSignal(false);
    render(() => (
      <DatabaseToolbar
        columns={[
          {
            id: 'status',
            name: 'Status',
            dataType: 'SELECT_STRING',
            isMultiSelect: false,
            writable: true,
            options: [{ id: 'option-done', label: 'Done', color: null }],
          },
        ]}
        views={[
          {
            id: 'board',
            databaseId: 'database',
            tableId: 'table',
            name: 'Board',
            position: 'a0',
            query: { filter: null, sort: [] },
            layout: {
              kind: 'board',
              groupBy: 'status',
              lanes: [],
              cardFields: [],
              hideEmptyLanes: false,
            },
            createdAt: '2026-09-01T00:00:00Z',
            updatedAt: '2026-09-01T00:00:00Z',
          },
        ]}
        view={{
          id: 'board',
          databaseId: 'database',
          tableId: 'table',
          name: 'Board',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: {
            kind: 'board',
            groupBy: 'status',
            lanes: [],
            cardFields: [],
            hideEmptyLanes: false,
          },
          createdAt: '2026-09-01T00:00:00Z',
          updatedAt: '2026-09-01T00:00:00Z',
        }}
        selectedViewId="board"
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
        onCreateRecord={createRecord}
        canCreateRecord
        creating={creating()}
      />
    ));
    const button = screen.getByRole('button', { name: 'New record' });
    expect(button.textContent).toBe('New');
    fireEvent.click(button);
    expect(createRecord).toHaveBeenCalledOnce();
    setCreating(true);
    const saving = screen.getByRole('button', {
      name: 'Saving record',
    }) as HTMLButtonElement;
    expect(saving.disabled).toBe(true);
    expect(saving.textContent).toBe('Saving…');
  });

  it('has no New button on a table view', () => {
    render(() => (
      <DatabaseToolbar
        columns={[]}
        views={[]}
        view={{
          id: 'table',
          databaseId: 'database',
          tableId: 'table',
          name: 'All records',
          position: '',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '1970-01-01T00:00:00.000Z',
          updatedAt: '1970-01-01T00:00:00.000Z',
        }}
        canEdit
        search=""
        onSearchChange={vi.fn()}
        onSelectView={vi.fn()}
        onChangeView={vi.fn()}
        onCreateView={vi.fn(() => okAsync(undefined))}
        onRenameView={vi.fn(() => okAsync(undefined))}
        onDeleteView={vi.fn(() => okAsync(undefined))}
        onReorderViews={vi.fn()}
        onCreateRecord={vi.fn()}
        canCreateRecord
      />
    ));
    expect(screen.queryByRole('button', { name: 'New record' })).toBeNull();
  });
});
