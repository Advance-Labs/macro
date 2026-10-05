import type { PreviewItem } from '@queries/preview/types';
import type { CalendarMentionEvent } from '@service-storage/generated/schemas/calendarMentionEvent';
import { fireEvent, render, screen } from '@solidjs/testing-library';
import type { JSX } from 'solid-js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  item: undefined as unknown,
  openDocument: vi.fn(),
  openExternalUrl: vi.fn(),
  manager: {
    getSplitByContent: vi.fn(),
    createNewSplit: vi.fn(),
    getBlockHandle: vi.fn(),
    replaceAllSplits: vi.fn(),
  },
}));
vi.mock('@app/signal/splitLayout', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  globalSplitManager: () => mocks.manager,
}));
vi.mock('@ui', async (importOriginal) => {
  const pass = (props: { children?: JSX.Element }) => props.children;
  return {
    ...(await importOriginal<typeof import('@ui')>()),
    Dropdown: Object.assign(pass, {
      Trigger: pass,
      Content: pass,
      Group: pass,
      Item: (props: { children?: JSX.Element; onSelect?: () => void }) => (
        <button onClick={() => props.onSelect?.()}>{props.children}</button>
      ),
    }),
  };
});
vi.mock(
  '@core/component/LexicalMarkdown/component/core/BlockLink',
  async (importOriginal) => ({
    ...(await importOriginal<object>()),
    openDocument: mocks.openDocument,
  })
);
vi.mock('@core/util/url', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  openExternalUrl: mocks.openExternalUrl,
}));
vi.mock('./ItemPreview', () => ({
  useItemPreviewData: () => ({
    item: () => mocks.item,
    ItemEntityIcon: () => null,
    documentProperties: () => undefined,
  }),
}));
vi.mock('@queries/storage/binary-document', () => ({
  useBinaryDocumentQuery: () => ({ isLoading: false, data: undefined }),
}));
// The preview's import graph opens the storage and connection-gateway sockets.
vi.mock('@service-storage/websocket', () => ({
  storageWS: { reconnectIfDisconnected: vi.fn() },
  createWebSocketJob: vi.fn(),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { addEventListener: vi.fn(), send: vi.fn() },
  state: () => 'closed',
  createConnectionWebsocketEffect: vi.fn(),
}));
vi.mock('@solidjs/router', () => ({ useNavigate: () => vi.fn() }));
vi.mock('./AccessErrorViews/Unauthorized', () => ({
  default: () => <p>You don't have access to this file</p>,
}));
vi.mock('./AccessErrorViews/NotFound', () => ({
  default: () => <p>Not found</p>,
}));

import { DocumentPreviewContent } from './DocumentPreview';

function calendarItem(event: Partial<CalendarMentionEvent>): PreviewItem {
  return {
    id: 'mentioned-event',
    type: 'calendar_event',
    access: 'access',
    loading: false,
    rawName: 'Pilates',
    name: 'Pilates',
    updatedAt: '2026-09-23T17:04:00Z',
    event: {
      title: 'Pilates',
      description: 'Bring a <b>mat</b> <script>alert(1)</script>',
      time: {
        kind: 'timed',
        startsAt: '2026-09-23T23:00:00Z',
        endsAt: '2026-09-23T23:30:00Z',
      },
      occurrenceKey: '2026-09-23T23:00:00+00:00',
      isRecurring: false,
      attendeeCount: 1,
      organizerEmail: 'gab@macro.com',
      updatedAt: '2026-09-23T17:04:00Z',
      viewerEventId: 'viewer-copy',
      ...event,
    },
  };
}

function renderCard() {
  return render(() => (
    <DocumentPreviewContent
      documentInfo={{
        id: 'mentioned-event',
        type: 'calendar',
        params: {},
        isOpenable: true,
      }}
    />
  ));
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.manager.getSplitByContent.mockReset();
  mocks.item = calendarItem({});
});

describe('calendar mention preview card', () => {
  it("opens the viewer's own copy and shows its description", () => {
    const view = renderCard();

    expect(screen.getByRole('button', { name: 'Pilates' })).toBeTruthy();
    expect(screen.queryByText(/not on your calendar/)).toBeNull();
    expect(view.container.textContent).toContain('Bring a mat');
    expect(view.container.querySelector('b')?.textContent).toBe('mat');
    expect(view.container.querySelector('script')).toBeNull();
  });

  // The byline's last-updated time reads as the meeting time on a calendar
  // card, which already shows the event's own schedule.
  it('omits the last-updated byline', () => {
    const view = renderCard();

    expect(view.container.textContent).not.toContain('5:04');
    expect(view.container.textContent).not.toMatch(/Sep 23, 2026/);
  });

  it('is read-only for a channel-shared meeting', () => {
    mocks.item = calendarItem({ viewerEventId: null });
    const view = renderCard();

    expect(
      screen.getByText(/Shared with you · not on your calendar/)
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Pilates' })).toBeNull();
    expect(view.container.textContent).toContain('Pilates');
    expect(view.container.textContent).toContain('Bring a mat');
  });

  it('shows no event details without access', () => {
    mocks.item = {
      id: 'mentioned-event',
      type: 'calendar_event',
      access: 'no_access',
      loading: false,
    } satisfies PreviewItem;
    const view = renderCard();

    expect(screen.getByText("You don't have access to this file")).toBeTruthy();
    expect(view.container.textContent).not.toContain('Pilates');
  });

  it('opens description links like the event view does', () => {
    mocks.item = calendarItem({
      description:
        'See <a href="https://macro.com/app/md/doc-1?line=3">notes</a> and <a href="https://zoom.us/j/1">Zoom</a>',
    });
    renderCard();

    fireEvent.click(screen.getByText('notes'));
    expect(mocks.openDocument).toHaveBeenCalledWith(
      'md',
      'doc-1',
      { line: '3' },
      false
    );

    fireEvent.click(screen.getByText('Zoom'));
    expect(mocks.openExternalUrl).toHaveBeenCalledWith('https://zoom.us/j/1');
  });
});

it('routes the preview menu location through the shared split-opening action', () => {
  mocks.item = {
    id: 'document',
    type: 'document',
    access: 'access',
    loading: false,
    rawName: 'Notes',
    name: 'Notes',
    fileType: 'md',
  } satisfies PreviewItem;
  render(() => (
    <DocumentPreviewContent
      documentInfo={{
        id: 'document',
        type: 'md',
        params: { node_id: 'node' },
        isOpenable: true,
      }}
    />
  ));
  fireEvent.click(screen.getByRole('button', { name: 'Open in New Split' }));
  expect(mocks.openDocument).toHaveBeenCalledExactlyOnceWith(
    'md',
    'document',
    { node_id: 'node' },
    true
  );
  expect(mocks.manager.createNewSplit).not.toHaveBeenCalled();
  expect(mocks.manager.getBlockHandle).not.toHaveBeenCalled();
});

it.each([false, true])(
  'routes an already-open Markdown node from its preview title (Shift: %s)',
  (shiftKey) => {
    mocks.item = {
      id: 'document',
      type: 'document',
      access: 'access',
      loading: false,
      rawName: 'Notes',
      name: 'Notes',
      fileType: 'md',
    } satisfies PreviewItem;
    mocks.manager.getSplitByContent.mockReturnValue({ activate: vi.fn() });
    render(() => (
      <DocumentPreviewContent
        documentInfo={{
          id: 'document',
          type: 'md',
          params: { node_id: 'node' },
          isOpenable: true,
        }}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Notes' }), {
      shiftKey,
    });
    expect(mocks.openDocument).toHaveBeenCalledExactlyOnceWith(
      'md',
      'document',
      { node_id: 'node' },
      shiftKey
    );
    expect(mocks.manager.replaceAllSplits).not.toHaveBeenCalled();
    expect(mocks.manager.getBlockHandle).not.toHaveBeenCalled();
  }
);

it('preserves plain preview title navigation without a location target', () => {
  mocks.item = {
    id: 'image',
    type: 'document',
    access: 'access',
    loading: false,
    rawName: 'Image',
    name: 'Image',
    fileType: 'png',
  } satisfies PreviewItem;
  render(() => (
    <DocumentPreviewContent
      documentInfo={{
        id: 'image',
        type: 'image',
        params: {},
        isOpenable: true,
      }}
    />
  ));
  fireEvent.click(screen.getByRole('button', { name: 'Image' }));
  expect(mocks.manager.replaceAllSplits).toHaveBeenCalledExactlyOnceWith({
    type: 'image',
    id: 'image',
    params: {},
  });
  expect(mocks.openDocument).not.toHaveBeenCalled();
});
