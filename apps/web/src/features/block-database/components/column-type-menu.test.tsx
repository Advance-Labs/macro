import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type {
  DatabaseColumnCasts,
  DatabaseColumnTypeChange,
} from '../core/column-schema';
import type { DatabaseViewColumn } from '../core/database-view';
import { DatabaseColumnHeader } from './database-column-header';

vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => false }));

const column: DatabaseViewColumn = {
  id: 'price',
  name: 'Price',
  dataType: 'STRING',
  isMultiSelect: false,
  options: [],
  writable: true,
};

const casts: DatabaseColumnCasts = {
  status: 'ready',
  casts: [
    {
      target: { dataType: 'STRING', isMultiSelect: false, relation: false },
      cast: { verdict: 'safe' },
    },
    {
      target: { dataType: 'NUMBER', isMultiSelect: false, relation: false },
      cast: {
        verdict: 'checked',
        failures: 3,
        summary: "3 values aren't numbers",
        examples: ['TBD', 'n/a', '12.5.0'],
      },
    },
    {
      target: {
        dataType: 'SELECT_STRING',
        isMultiSelect: false,
        relation: false,
      },
      cast: {
        verdict: 'checked',
        failures: 0,
        summary: undefined,
        examples: [],
      },
    },
    {
      target: {
        dataType: 'ENTITY',
        isMultiSelect: false,
        specificEntityType: 'USER',
        relation: false,
      },
      cast: {
        verdict: 'never',
        reason: 'Only an empty column can become a reference column.',
      },
    },
  ],
};

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
  menuStyles = document.createElement('style');
  menuStyles.textContent = '[role=menu] { animation-name: none; }';
  document.head.append(menuStyles);
});
afterEach(() => {
  cleanup();
  menuStyles.remove();
  vi.unstubAllGlobals();
});

function renderHeader() {
  const changeType = vi.fn<
    (columnId: string, change: DatabaseColumnTypeChange) => Promise<void>
  >(async () => {});
  const opened: string[] = [];
  render(() => (
    <DatabaseColumnHeader
      column={column}
      canRename
      onRename={vi.fn()}
      onSort={vi.fn()}
      onChangeType={changeType}
      columnCasts={(columnId, open) => () => {
        if (open()) opened.push(columnId);
        return casts;
      }}
    />
  ));
  return { changeType, opened };
}

async function openTypeMenu() {
  fireEvent.keyDown(screen.getByRole('columnheader', { name: 'Price' }), {
    key: 'Enter',
  });
  const submenu = await screen.findByRole('menuitem', { name: 'Change type' });
  submenu.focus();
  fireEvent.keyDown(submenu, { key: 'ArrowRight' });
}

function choose(item: HTMLElement) {
  fireEvent(item, new MouseEvent('pointerup', { button: 0, bubbles: true }));
}

it('lists only the types the column can become', async () => {
  const { opened } = renderHeader();
  await openTypeMenu();

  expect(await screen.findByRole('menuitem', { name: /^Number/ })).toBeTruthy();
  expect(screen.queryByRole('menuitem', { name: /^People/ })).toBeNull();
  expect(opened).toContain('price');
});

it('confirms a checked type with failures, then converts with clearing', async () => {
  const { changeType } = renderHeader();
  await openTypeMenu();

  const number = await screen.findByRole('menuitem', { name: 'Number' });
  expect(number.textContent).toContain("3 values aren't numbers");
  choose(number);

  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('TBD');
  expect(dialog.textContent).toContain('n/a');
  expect(dialog.textContent).toContain('12.5.0');
  expect(changeType).not.toHaveBeenCalled();
  fireEvent.click(
    screen.getByRole('button', { name: 'Convert anyway, clearing 3 values' })
  );
  await waitFor(() =>
    expect(changeType).toHaveBeenCalledExactlyOnceWith('price', {
      dataType: 'NUMBER',
      clearInvalid: true,
    })
  );
});

it('applies a type every value fits directly', async () => {
  const { changeType } = renderHeader();
  await openTypeMenu();

  choose(await screen.findByRole('menuitem', { name: 'Select' }));

  await waitFor(() =>
    expect(changeType).toHaveBeenCalledExactlyOnceWith('price', {
      dataType: 'SELECT_STRING',
    })
  );
  expect(screen.queryByRole('dialog')).toBeNull();
});
