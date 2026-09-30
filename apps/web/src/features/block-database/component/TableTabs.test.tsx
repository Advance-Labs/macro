import { toast } from '@core/component/Toast/Toast';
import { fetchWithToken } from '@core/util/fetchWithToken';
import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import type { DatabaseDetail } from '@service-storage/databases';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { err, ok } from 'neverthrow';
import { createSignal, onCleanup } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TableTabs } from './TableTabs';

vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => false }));
vi.mock('@core/util/fetchWithToken', () => ({ fetchWithToken: vi.fn() }));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { failure: vi.fn(), success: vi.fn() },
}));
vi.mock('@service-storage/client', async () => {
  const { databasesClient } = await import('@service-storage/databases');
  return { storageServiceClient: { databases: databasesClient } };
});
vi.mock('@queries/client', async () => {
  const { QueryClient } = await import('@tanstack/solid-query');
  return {
    queryClient: new QueryClient({
      defaultOptions: { queries: { retry: false } },
    }),
  };
});

const fetch = vi.mocked(fetchWithToken);
const key = databasesKeys.detail('db').queryKey;
const detail: DatabaseDetail = {
  database: {
    id: 'db',
    name: 'Offsite',
    owner_id: 'owner',
    created_at: '',
    trashed_at: null,
  },
  grant: 'owner',
  tables: [
    {
      table: {
        id: 'guests',
        database_id: 'db',
        name: 'Guests',
        position: '000000000001',
        version: 3,
      },
      sql_name: '"Guests"',
      columns: [],
    },
    {
      table: {
        id: 'budget',
        database_id: 'db',
        name: 'Budget',
        position: '000000000002',
        version: 1,
      },
      sql_name: '"Budget"',
      columns: [],
    },
    {
      table: {
        id: 'venues',
        database_id: 'db',
        name: 'Venues',
        position: '000000000003',
        version: 0,
      },
      sql_name: '"Venues"',
      columns: [],
    },
  ],
};

function renderTabs() {
  render(() => {
    const [cached, setCached] = createSignal(
      queryClient.getQueryData<DatabaseDetail>(key)
    );
    onCleanup(
      queryClient
        .getQueryCache()
        .subscribe(() => setCached(queryClient.getQueryData(key)))
    );
    return (
      <TableTabs
        databaseId="db"
        tables={cached()?.tables ?? []}
        activeTableId="guests"
        canEdit
        onSelect={vi.fn()}
      />
    );
  });
}

function tabNames() {
  return screen.getAllByRole('tab').map((tab) => tab.textContent);
}

async function chooseMenuItem(tabName: string, item: string) {
  fireEvent.contextMenu(screen.getByRole('tab', { name: tabName }), {
    clientX: 100,
    clientY: 40,
  });
  fireEvent(
    await screen.findByRole('menuitem', { name: item }),
    new MouseEvent('pointerup', { button: 0, bubbles: true })
  );
}

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
  fetch.mockReset();
  vi.mocked(toast.failure).mockReset();
  queryClient.setQueryData(key, detail);
});
afterEach(() => {
  cleanup();
  menuStyles.remove();
  queryClient.clear();
  vi.unstubAllGlobals();
});

describe('reordering tabs', () => {
  it('moves a tab right from its menu, sends the full order, and keeps it', async () => {
    fetch.mockResolvedValue(
      ok([
        { ...detail.tables[1].table, position: '000000000001', version: 2 },
        { ...detail.tables[0].table, position: '000000000002', version: 4 },
        { ...detail.tables[2].table, position: '000000000003', version: 1 },
      ])
    );
    renderTabs();
    expect(tabNames()).toEqual(['Guests', 'Budget', 'Venues']);

    fireEvent.contextMenu(screen.getByRole('tab', { name: 'Guests' }), {
      clientX: 100,
      clientY: 40,
    });
    expect(
      (await screen.findByRole('menuitem', { name: 'Move left' })).getAttribute(
        'aria-disabled'
      )
    ).toBe('true');
    fireEvent(
      screen.getByRole('menuitem', { name: 'Move right' }),
      new MouseEvent('pointerup', { button: 0, bubbles: true })
    );

    await waitFor(() =>
      expect(tabNames()).toEqual(['Budget', 'Guests', 'Venues'])
    );
    await waitFor(() => expect(fetch).toHaveBeenCalledOnce());
    const [url, init] = fetch.mock.calls[0];
    expect(url).toMatch(/\/databases\/db\/tables\/order$/);
    expect(init?.method).toBe('PUT');
    expect(init?.body).toBe(
      JSON.stringify({ tableIds: ['budget', 'guests', 'venues'] })
    );
    await waitFor(() =>
      expect(
        queryClient
          .getQueryData<DatabaseDetail>(key)
          ?.tables.map((entry) => [entry.table.id, entry.table.version])
      ).toEqual([
        ['budget', 2],
        ['guests', 4],
        ['venues', 1],
      ])
    );
    expect(tabNames()).toEqual(['Budget', 'Guests', 'Venues']);
    expect(toast.failure).not.toHaveBeenCalled();
  });

  it('disables Move right on the last tab', async () => {
    renderTabs();
    fireEvent.contextMenu(screen.getByRole('tab', { name: 'Venues' }), {
      clientX: 100,
      clientY: 40,
    });
    expect(
      (
        await screen.findByRole('menuitem', { name: 'Move right' })
      ).getAttribute('aria-disabled')
    ).toBe('true');
    expect(
      screen
        .getByRole('menuitem', { name: 'Move left' })
        .getAttribute('aria-disabled')
    ).not.toBe('true');
  });

  it('puts the tabs back and says so when the server refuses the order', async () => {
    fetch.mockResolvedValue(
      err([
        {
          code: 'HTTP_ERROR',
          message: 'The table order must include every table',
        },
      ])
    );
    renderTabs();

    await chooseMenuItem('Venues', 'Move left');

    await waitFor(() => expect(toast.failure).toHaveBeenCalledOnce());
    expect(fetch.mock.calls[0][1]?.body).toBe(
      JSON.stringify({ tableIds: ['guests', 'venues', 'budget'] })
    );
    await waitFor(() =>
      expect(tabNames()).toEqual(['Guests', 'Budget', 'Venues'])
    );
    expect(queryClient.getQueryState(key)?.isInvalidated).toBe(true);
  });
});
