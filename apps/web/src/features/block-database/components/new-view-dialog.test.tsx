import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { errAsync, okAsync } from 'neverthrow';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DatabaseViewColumn } from '../core/database-view';
import { NewViewDialog } from './new-view-dialog';

vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => false }));

afterEach(cleanup);

const priority: DatabaseViewColumn = {
  id: 'priority',
  name: 'Priority',
  dataType: 'SELECT_STRING',
  isMultiSelect: false,
  writable: true,
  options: [
    { id: 'high', label: 'High', color: null },
    { id: 'low', label: 'Low', color: null },
  ],
};

describe('new database view', () => {
  it('focuses and selects the name, then tabs to Cancel', async () => {
    render(() => (
      <NewViewDialog
        initialName="Planning"
        columns={[]}
        onSubmit={vi.fn(() => okAsync(undefined))}
        onClose={vi.fn()}
      />
    ));
    const name = screen.getByRole('textbox', {
      name: 'View name',
    }) as HTMLInputElement;
    await waitFor(() => expect(document.activeElement).toBe(name));
    expect(name.selectionStart).toBe(0);
    expect(name.selectionEnd).toBe(name.value.length);
    await userEvent.tab();
    expect(document.activeElement).toBe(
      screen.getByRole('button', { name: 'Cancel' })
    );
  });

  it('creates a board grouped by the first single select, keeping a custom name', async () => {
    const submit = vi.fn(() => okAsync(undefined));
    const close = vi.fn();
    render(() => (
      <NewViewDialog
        initialName="Table view"
        initialLayout="table"
        columns={[priority]}
        onSubmit={submit}
        onClose={close}
      />
    ));
    const input = screen.getByRole('textbox', { name: 'View name' });
    fireEvent.input(input, { target: { value: 'Delivery board' } });
    fireEvent.click(screen.getByRole('button', { name: /Board Cards/ }));
    fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(submit).toHaveBeenCalledExactlyOnceWith(
      'Delivery board',
      'board',
      'priority'
    );
  });

  it('keeps the layout and draft, and says why, when creating fails', async () => {
    const submit = vi
      .fn()
      .mockReturnValueOnce(
        errAsync({
          kind: 'ops',
          error: {
            code: 'INVALID_OP',
            message: 'a view named `Board view` already exists on this table',
            refusal: {
              op: 0,
              row: null,
              column: null,
              message: 'a view named `Board view` already exists on this table',
            },
          },
        })
      )
      .mockReturnValueOnce(okAsync(undefined));
    const close = vi.fn();
    render(() => (
      <NewViewDialog
        initialName="Table view"
        initialLayout="table"
        columns={[priority]}
        onSubmit={submit}
        onClose={close}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: /Board Cards/ }));
    const input = screen.getByRole('textbox', { name: 'View name' });
    fireEvent.submit(input.closest('form')!);
    expect((await screen.findByRole('alert')).textContent).toBe(
      'a view named `Board view` already exists on this table'
    );
    expect(close).not.toHaveBeenCalled();
    expect((input as HTMLInputElement).value).toBe('Board view');
    expect(
      screen
        .getByRole('button', { name: /Board Cards/ })
        .getAttribute('aria-pressed')
    ).toBe('true');
    fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(submit).toHaveBeenLastCalledWith('Board view', 'board', 'priority');
  });
});
