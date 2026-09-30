import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DatabaseViewColumn } from '../core/database-view';
import type { DatabaseRow } from '../core/table';
import { DatabaseBoard } from './database-board';

const columns: DatabaseViewColumn[] = [
  {
    id: 'title',
    name: 'Name',
    dataType: 'STRING',
    isMultiSelect: false,
    options: [],
    writable: true,
  },
  {
    id: 'status',
    name: 'Status',
    dataType: 'SELECT_STRING',
    isMultiSelect: false,
    options: ['To do', 'Done'],
    writable: true,
  },
];
const initialRows: DatabaseRow[] = [
  { rowId: 'launch', cells: { title: 'Launch project', status: 'To do' } },
  { rowId: 'unassigned', cells: { title: 'Unassigned record', status: null } },
];

beforeEach(() => {
  vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('database board', () => {
  it('orders visible metadata without changing the record title or showing hidden fields', () => {
    const details = [
      { ...columns[0], id: 'owner', name: 'Owner' },
      { ...columns[0], id: 'team', name: 'Team' },
      { ...columns[0], id: 'notes', name: 'Notes' },
    ];
    render(() => (
      <DatabaseBoard
        rows={[
          {
            ...initialRows[0],
            cells: {
              ...initialRows[0].cells,
              owner: 'Ada',
              team: 'Design',
              notes: 'Bring draft',
            },
          },
        ]}
        columns={[...columns, ...details]}
        visibleColumnIds={['notes', 'title', 'owner']}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
      />
    ));
    const card = screen.getByRole('button', { name: 'Open Launch project' });
    expect(within(card).getByText('Launch project')).toBeTruthy();
    expect(within(card).queryByText('Design')).toBeNull();
    expect(
      Array.from(card.querySelectorAll('[title]'), (element) =>
        element.getAttribute('title')
      )
    ).toEqual(['Notes: Bring draft', 'Owner: Ada']);
  });

  it('keeps a failed new-group draft and lets the user retry it', async () => {
    const onAddGroup = vi.fn(async (_label: string) => {});
    onAddGroup.mockRejectedValueOnce(new Error('Connection unavailable'));
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
        onAddGroup={onAddGroup}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'New group' }));
    const input = screen.getByRole('textbox', {
      name: 'New group name',
    }) as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'In review' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add group' }));
    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toBe(
        'Connection unavailable'
      )
    );
    expect(input.value).toBe('In review');
    fireEvent.click(screen.getByRole('button', { name: 'Add group' }));
    await waitFor(() => expect(onAddGroup).toHaveBeenCalledTimes(2));
    expect(onAddGroup).toHaveBeenLastCalledWith('In review');
  });

  it('validates numeric groups and sends their canonical SQL labels', async () => {
    const onAddGroup = vi.fn(async (_label: string) => {});
    const groupColumn = {
      ...columns[1],
      dataType: 'SELECT_NUMBER',
      options: ['2'],
    };
    render(() => (
      <DatabaseBoard
        rows={[]}
        columns={[columns[0], groupColumn]}
        groupColumn={groupColumn}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
        onAddGroup={onAddGroup}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'New group' }));
    fireEvent.input(screen.getByRole('textbox', { name: 'New group name' }), {
      target: { value: 'not a number' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add group' }));
    expect(screen.getByRole('alert').textContent).toContain('valid number');
    expect(onAddGroup).not.toHaveBeenCalled();
    fireEvent.input(screen.getByRole('textbox', { name: 'New group name' }), {
      target: { value: '1.0' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add group' }));
    await waitFor(() => expect(onAddGroup).toHaveBeenCalledWith('1'));
  });

  it('drags the entire card at its original size without opening the record on drop', async () => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: HTMLElement) {
        if (this.classList.contains('overflow-auto'))
          return new DOMRect(0, 0, 900, 500);
        const label = this.getAttribute('aria-label');
        const x =
          label === 'Done lane' ? 300 : label?.startsWith('No ') ? 600 : 0;
        const isCard = this.hasAttribute('data-row-id');
        return new DOMRect(
          x,
          isCard ? 60 : 0,
          isCard ? 250 : 280,
          isCard ? 100 : 500
        );
      }
    );
    const onMove = vi.fn(async () => true);
    const onOpen = vi.fn();
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={onOpen}
        onMove={onMove}
        onCreate={vi.fn(async () => true)}
      />
    ));
    fireEvent.mouseDown(
      screen.getByRole('button', { name: 'Open Launch project' }),
      { button: 0, clientX: 200, clientY: 80 }
    );
    fireEvent.mouseMove(document, { clientX: 510, clientY: 100 });
    const preview = document.querySelector<HTMLElement>(
      '[data-kanban-preview]'
    )!;
    expect(preview.style.width).toBe('250px');
    expect(preview.style.height).toBe('100px');
    expect(preview.style.transform).toBe('none');
    expect(
      screen
        .getByRole('region', { name: 'Done lane' })
        .querySelector('[data-kanban-insertion="card"]')
    ).toBeTruthy();
    fireEvent.mouseUp(document, { button: 0, clientX: 510, clientY: 100 });
    fireEvent.click(
      screen.getByRole('button', { name: 'Open Launch project' })
    );
    expect(onOpen).not.toHaveBeenCalled();
    await waitFor(() => expect(onMove).toHaveBeenCalledWith('launch', 'Done'));
  });

  it('renders empty configured groups and a group for unassigned records', () => {
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
      />
    ));
    expect(screen.getByRole('region', { name: 'Done lane' })).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Open Unassigned record' })
    ).toBeTruthy();
    expect(screen.getAllByRole('button', { name: 'New record' })).toHaveLength(
      3
    );
  });

  it('offers an accessible move menu that writes the target value', async () => {
    const onMove = vi.fn(async () => true);
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={onMove}
        onCreate={vi.fn(async () => true)}
      />
    ));
    const trigger = screen.getByRole('button', { name: 'Move Launch project' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'Enter' });
    const done = await screen.findByRole('menuitem', { name: 'Done' });
    fireEvent.keyDown(done, { key: 'Enter' });
    await waitFor(() => expect(onMove).toHaveBeenCalledWith('launch', 'Done'));
  });

  it('keeps a new-card title after a failed write and unrelated row updates', async () => {
    const [rows, setRows] = createSignal(initialRows);
    const onCreate = vi.fn(async () => false);
    render(() => (
      <DatabaseBoard
        rows={rows()}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={onCreate}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Add record to Done' }));
    const input = screen.getByRole('textbox', {
      name: 'New record title',
    }) as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: 'Remember this draft' } });
    setRows([
      ...initialRows,
      { rowId: 'other', cells: { title: 'Other record', status: 'Done' } },
    ]);
    expect(
      (
        screen.getByRole('textbox', {
          name: 'New record title',
        }) as HTMLTextAreaElement
      ).value
    ).toBe('Remember this draft');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() =>
      expect(onCreate).toHaveBeenCalledWith(
        'Done',
        'Remember this draft',
        expect.any(String)
      )
    );
    await waitFor(() =>
      expect(
        screen
          .getAllByRole('textbox', { name: 'New record title' })
          .map((input) => (input as HTMLTextAreaElement).value)
      ).toEqual(['Remember this draft', ''])
    );
  });

  it('Enter saves a card into its lane and opens an empty card below it', async () => {
    const onCreate = vi.fn(async () => true);
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={onCreate}
      />
    ));
    const lane = screen.getByRole('region', { name: 'Done lane' });
    fireEvent.click(within(lane).getByRole('button', { name: 'New record' }));
    const first = within(lane).getByRole('textbox', {
      name: 'New record title',
    }) as HTMLTextAreaElement;
    expect(document.activeElement).toBe(first);
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.input(first, { target: { value: 'First idea' } });
    fireEvent.keyDown(first, { key: 'Enter' });
    expect(onCreate).toHaveBeenCalledWith(
      'Done',
      'First idea',
      expect.any(String)
    );
    const next = within(lane).getByRole('textbox', {
      name: 'New record title',
    }) as HTMLTextAreaElement;
    expect(next).not.toBe(first);
    expect(next.value).toBe('');
    expect(document.activeElement).toBe(next);
    fireEvent.input(next, { target: { value: 'Second idea' } });
    fireEvent.keyDown(next, { key: 'Enter' });
    expect(onCreate).toHaveBeenLastCalledWith(
      'Done',
      'Second idea',
      expect.any(String)
    );
    expect(onCreate).toHaveBeenCalledTimes(2);
    expect(
      within(screen.getByRole('region', { name: 'To do lane' })).queryByRole(
        'textbox'
      )
    ).toBeNull();
  });

  it('Escape cancels a new card and returns focus to its lane', () => {
    const onCreate = vi.fn(async () => true);
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={onCreate}
      />
    ));
    const lane = screen.getByRole('region', { name: 'Done lane' });
    fireEvent.click(within(lane).getByRole('button', { name: 'New record' }));
    const input = within(lane).getByRole('textbox', {
      name: 'New record title',
    });
    fireEvent.input(input, { target: { value: 'Not this one' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(within(lane).queryByRole('textbox')).toBeNull();
    expect(onCreate).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(
      within(lane).getByRole('button', { name: 'New record' })
    );
  });

  it('blurring a typed card saves it and blurring an empty card cancels it', () => {
    const onCreate = vi.fn(async () => true);
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={onCreate}
      />
    ));
    const lane = screen.getByRole('region', { name: 'Done lane' });
    fireEvent.click(within(lane).getByRole('button', { name: 'New record' }));
    fireEvent.blur(within(lane).getByRole('textbox'));
    expect(within(lane).queryByRole('textbox')).toBeNull();
    expect(onCreate).not.toHaveBeenCalled();
    fireEvent.click(within(lane).getByRole('button', { name: 'New record' }));
    const input = within(lane).getByRole('textbox');
    fireEvent.input(input, { target: { value: 'Typed then left' } });
    fireEvent.blur(input);
    expect(onCreate).toHaveBeenCalledWith(
      'Done',
      'Typed then left',
      expect.any(String)
    );
    expect(within(lane).queryByRole('textbox')).toBeNull();
  });

  it('n on a focused card or Enter on a lane header adds a card to that lane', () => {
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
      />
    ));
    const todo = screen.getByRole('region', { name: 'To do lane' });
    const card = within(todo).getByRole('button', {
      name: 'Open Launch project',
    });
    card.focus();
    fireEvent.keyDown(card, { key: 'n' });
    expect(document.activeElement).toBe(
      within(todo).getByRole('textbox', { name: 'New record title' })
    );
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    const done = screen.getByRole('region', { name: 'Done lane' });
    const header = within(done).getByRole('button', { name: 'Done lane' });
    header.focus();
    fireEvent.keyDown(header, { key: 'Enter' });
    expect(document.activeElement).toBe(
      within(done).getByRole('textbox', { name: 'New record title' })
    );
  });

  it('Shift+Enter saves a card and asks to open its record', () => {
    const onCreate = vi.fn(async () => true);
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={onCreate}
      />
    ));
    const lane = screen.getByRole('region', { name: 'Done lane' });
    fireEvent.click(within(lane).getByRole('button', { name: 'New record' }));
    const input = within(lane).getByRole('textbox');
    fireEvent.input(input, { target: { value: 'Needs detail' } });
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
    expect(onCreate).toHaveBeenCalledWith(
      'Done',
      'Needs detail',
      expect.any(String),
      { open: true }
    );
    expect(within(lane).queryByRole('textbox')).toBeNull();
  });

  it('opens records for viewers without exposing move or create actions', () => {
    const onOpen = vi.fn();
    render(() => (
      <DatabaseBoard
        rows={initialRows}
        columns={columns}
        groupColumn={columns[1]}
        canEdit={false}
        rowPending={() => false}
        onOpen={onOpen}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
      />
    ));
    fireEvent.click(
      screen.getByRole('button', { name: 'Open Launch project' })
    );
    expect(onOpen).toHaveBeenCalledWith('launch');
    expect(
      screen.queryByRole('button', { name: 'Move Launch project' })
    ).toBeNull();
    expect(screen.queryByRole('button', { name: 'New record' })).toBeNull();
    expect(
      within(screen.getByRole('region', { name: 'Done lane' })).getByText(
        'No records'
      )
    ).toBeTruthy();
  });
});

it('shows a submitted card in place while it saves and keeps its title when the save fails', async () => {
  const [pending, setPending] = createSignal(new Set<string>());
  let complete: (saved: boolean) => void = () => {};
  const onCreate = vi.fn((_value: unknown, _title: string, intent: string) => {
    setPending((ids) => new Set(ids).add(intent));
    return new Promise<boolean>((resolve) => {
      complete = (saved) => {
        setPending((ids) => new Set([...ids].filter((id) => id !== intent)));
        resolve(saved);
      };
    });
  });
  render(() => (
    <DatabaseBoard
      rows={[]}
      columns={columns}
      groupColumn={columns[1]}
      canEdit
      rowPending={() => false}
      createPending={(id) => pending().has(id)}
      onOpen={vi.fn()}
      onMove={vi.fn(async () => true)}
      onCreate={onCreate}
    />
  ));
  const lane = screen.getByRole('region', { name: 'Done lane' });
  fireEvent.click(within(lane).getByRole('button', { name: 'New record' }));
  const input = screen.getByRole('textbox', { name: 'New record title' });
  fireEvent.input(input, { target: { value: 'First idea' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  const saving = within(lane).getByRole('status', {
    name: 'Saving new record',
  });
  expect(saving.textContent).toContain('First idea');
  const next = within(lane).getByRole('textbox', { name: 'New record title' });
  expect(
    saving.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING
  ).toBeTruthy();
  expect(within(lane).getByText('1')).toBeTruthy();
  complete(false);
  await waitFor(() =>
    expect(
      screen
        .getAllByRole('textbox', { name: 'New record title' })
        .map((input) => (input as HTMLTextAreaElement).value)
    ).toEqual(['First idea', ''])
  );
});

it('reorders lanes with the keyboard while leaving every card value unchanged', () => {
  const [order, setOrder] = createSignal<string[]>();
  const move = vi.fn(async () => true);
  render(() => (
    <DatabaseBoard
      rows={initialRows}
      columns={columns}
      groupColumn={columns[1]}
      groupOrder={order()}
      onGroupOrderChange={setOrder}
      canEdit
      rowPending={() => false}
      onOpen={vi.fn()}
      onMove={move}
      onCreate={vi.fn(async () => true)}
    />
  ));
  fireEvent.keyDown(screen.getByRole('button', { name: 'Reorder Done lane' }), {
    key: 'ArrowRight',
    altKey: true,
  });
  expect(
    [...document.querySelectorAll('[data-kanban-lane]')].map((lane) =>
      lane.getAttribute('aria-label')
    )
  ).toEqual(['To do lane', 'Done lane', 'No status lane']);
  expect(move).not.toHaveBeenCalled();
});

describe('multi-select board', () => {
  it('shows a record in the lane of each of its values and an untagged record in the empty lane', () => {
    const tags: DatabaseViewColumn = {
      id: 'tags',
      name: 'Tags',
      dataType: 'SELECT_STRING',
      isMultiSelect: true,
      options: ['Bug', 'Feature', 'Docs'],
      writable: true,
    };
    render(() => (
      <DatabaseBoard
        rows={[
          {
            rowId: 'login',
            cells: { title: 'Fix login', tags: '["Bug","Feature"]' },
          },
          { rowId: 'idea', cells: { title: 'Loose idea', tags: '[]' } },
        ]}
        columns={[columns[0], tags]}
        groupColumn={tags}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
      />
    ));
    const cardsIn = (lane: string) =>
      [
        ...screen
          .getByRole('region', { name: `${lane} lane` })
          .querySelectorAll('[data-row-id]'),
      ].map((card) => card.getAttribute('data-row-id'));
    expect(cardsIn('Bug')).toEqual(['login']);
    expect(cardsIn('Feature')).toEqual(['login']);
    expect(cardsIn('Docs')).toEqual([]);
    expect(cardsIn('No tags')).toEqual(['idea']);
  });

  it("draws each of a card's multi-select values as a coloured pill", () => {
    const tags: DatabaseViewColumn = {
      id: 'tags',
      name: 'Tags',
      dataType: 'SELECT_STRING',
      isMultiSelect: true,
      options: ['Bug', 'Feature'],
      optionColors: { Bug: '#E5484D', Feature: '#46A758' },
      writable: true,
    };
    render(() => (
      <DatabaseBoard
        rows={[
          {
            rowId: 'login',
            cells: {
              title: 'Fix login',
              status: 'To do',
              tags: '["Bug","Feature"]',
            },
          },
        ]}
        columns={[...columns, tags]}
        groupColumn={columns[1]}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={vi.fn(async () => true)}
        onCreate={vi.fn(async () => true)}
      />
    ));
    const card = screen.getByRole('button', { name: 'Open Fix login' });
    const dot = (label: string) =>
      within(card)
        .getByTitle(label)
        .querySelector<HTMLElement>('[data-slot="tag-dot"]')?.style
        .backgroundColor;
    expect(dot('Bug')).toBe('rgb(229, 72, 77)');
    expect(dot('Feature')).toBe('rgb(70, 167, 88)');
  });

  it('moving a card out of one value lane replaces only that value', async () => {
    const tags: DatabaseViewColumn = {
      id: 'tags',
      name: 'Tags',
      dataType: 'SELECT_STRING',
      isMultiSelect: true,
      options: ['Bug', 'Feature', 'Docs'],
      writable: true,
    };
    const onMove = vi.fn(async () => true);
    render(() => (
      <DatabaseBoard
        rows={[
          {
            rowId: 'login',
            cells: { title: 'Fix login', tags: '["Bug","Feature"]' },
          },
        ]}
        columns={[columns[0], tags]}
        groupColumn={tags}
        canEdit
        rowPending={() => false}
        onOpen={vi.fn()}
        onMove={onMove}
        onCreate={vi.fn(async () => true)}
      />
    ));
    const trigger = within(
      screen.getByRole('region', { name: 'Feature lane' })
    ).getByRole('button', { name: 'Move Fix login' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'Enter' });
    fireEvent.keyDown(await screen.findByRole('menuitem', { name: 'Docs' }), {
      key: 'Enter',
    });
    await waitFor(() =>
      expect(onMove).toHaveBeenCalledWith('login', '["Bug","Docs"]')
    );
  });
});
