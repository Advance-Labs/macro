import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  DatabaseFilter,
  DatabaseFilterConjunction,
  DatabaseViewColumn,
} from '../core/database-view';
import { FilterPanel } from './database-view-filters';

afterEach(cleanup);

const columns: DatabaseViewColumn[] = [
  {
    id: 'name',
    name: 'Name',
    dataType: 'STRING',
    options: [],
    isMultiSelect: false,
    writable: true,
  },
  {
    id: 'status',
    name: 'Status',
    dataType: 'SELECT_STRING',
    options: ['To do', 'Done'],
    optionColors: { Done: '#16a34a' },
    isMultiSelect: false,
    writable: true,
  },
  {
    id: 'amount',
    name: 'Amount',
    dataType: 'NUMBER',
    options: [],
    isMultiSelect: false,
    writable: true,
  },
];

describe('database filter controls', () => {
  it('keeps text input mounted and focused through successive filter edits', () => {
    const [filters, setFilters] = createSignal<DatabaseFilter[]>([]);
    render(() => (
      <FilterPanel
        columns={columns}
        filters={filters()}
        conjunction="and"
        onChange={setFilters}
        onConjunctionChange={() => {}}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Add condition' }));
    const input = screen.getByRole('textbox', { name: 'Filter value' });
    input.focus();
    fireEvent.input(input, { target: { value: 'P' } });
    fireEvent.input(input, { target: { value: 'Priya' } });
    expect(screen.getByRole('textbox', { name: 'Filter value' })).toBe(input);
    expect(document.activeElement).toBe(input);
    expect(filters()[0].value).toBe('Priya');
  });

  it('switches to named choices for select properties and resets an incompatible value', async () => {
    const [filters, setFilters] = createSignal<DatabaseFilter[]>([
      { id: '1', columnId: 'name', operator: 'contains', value: 'draft' },
    ]);
    render(() => (
      <FilterPanel
        columns={columns}
        filters={filters()}
        conjunction="and"
        onChange={setFilters}
        onConjunctionChange={() => {}}
      />
    ));
    fireEvent.keyDown(
      screen.getByRole('button', { name: /^Filter property/ }),
      { key: 'Enter' }
    );
    fireEvent.keyDown(await screen.findByRole('option', { name: 'Status' }), {
      key: 'Enter',
    });
    expect(filters()[0]).toMatchObject({
      columnId: 'status',
      operator: 'equals',
      value: '',
    });
    fireEvent.keyDown(screen.getByRole('button', { name: /^Filter value/ }), {
      key: 'Enter',
    });
    fireEvent.keyDown(await screen.findByRole('option', { name: 'Done' }), {
      key: 'Enter',
    });
    expect(filters()[0].value).toBe('Done');
    fireEvent.keyDown(
      screen.getByRole('button', { name: /^Filter condition/ }),
      { key: 'Enter' }
    );
    fireEvent.keyDown(await screen.findByRole('option', { name: 'is empty' }), {
      key: 'Enter',
    });
    expect(screen.queryByRole('button', { name: /^Filter value/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Remove filter' }));
    expect(filters()).toEqual([]);
  });

  it('reads Where on the first condition and shares one And/Or between the rest', async () => {
    const [conjunction, setConjunction] =
      createSignal<DatabaseFilterConjunction>('and');
    const changeConjunction = vi.fn(setConjunction);
    render(() => (
      <FilterPanel
        columns={columns}
        filters={[
          { id: '1', columnId: 'name', operator: 'contains', value: 'plan' },
          { id: '2', columnId: 'status', operator: 'equals', value: 'Done' },
          { id: '3', columnId: 'amount', operator: 'gt', value: '5' },
        ]}
        conjunction={conjunction()}
        onChange={() => {}}
        onConjunctionChange={changeConjunction}
      />
    ));
    expect(screen.queryByText('Match all conditions')).toBeNull();
    expect(screen.getByText('Where')).toBeTruthy();
    const controls = screen.getAllByRole('button', {
      name: /^Match conditions with/,
    });
    expect(controls.map((control) => control.textContent)).toEqual([
      'And',
      'And',
    ]);
    fireEvent.keyDown(controls[1], { key: 'Enter' });
    fireEvent.keyDown(await screen.findByRole('option', { name: 'Or' }), {
      key: 'Enter',
    });
    expect(changeConjunction).toHaveBeenCalledExactlyOnceWith('or');
    expect(
      screen
        .getAllByRole('button', { name: /^Match conditions with/ })
        .map((control) => control.textContent)
    ).toEqual(['Or', 'Or']);
  });

  it('offers select values as the pills cells show, behind a short placeholder', async () => {
    render(() => (
      <FilterPanel
        columns={columns}
        filters={[
          { id: '1', columnId: 'status', operator: 'equals', value: '' },
        ]}
        conjunction="and"
        onChange={() => {}}
        onConjunctionChange={() => {}}
      />
    ));
    const value = screen.getByRole('button', { name: /^Filter value/ });
    expect(value.textContent).toBe('Choose');
    fireEvent.keyDown(value, { key: 'Enter' });
    const done = await screen.findByRole('option', { name: 'Done' });
    expect(done.querySelector('[title="Done"]')).toBeTruthy();
    expect(
      done.querySelector<HTMLElement>('span[aria-hidden="true"]')?.style
        .backgroundColor
    ).toBe('rgb(22, 163, 74)');
    const toDo = screen.getByRole('option', { name: 'To do' });
    expect(toDo.querySelector('[title="To do"]')).toBeTruthy();
    expect(toDo.querySelector('span[aria-hidden="true"]')).toBeNull();
  });
});
