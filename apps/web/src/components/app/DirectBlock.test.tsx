import type { Entry, PaneId } from '@app/lib/split-router/routes/types';
import { PaneContext } from '@app/lib/split-router/solid/context';
import { cleanup, render, screen, waitFor } from '@solidjs/testing-library';
import { createSignal, onCleanup } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DirectBlock } from './DirectBlock';

const mocks = vi.hoisted(() => ({
  register: vi.fn(),
  disposed: vi.fn(),
  routed: false,
  claim: 'block:canvas:canvas-1',
}));

vi.mock('@app/lib/split-router', () => ({
  useOptionalSplitRouter: () => (mocks.routed ? { routes: [] } : undefined),
  claimOf: () => mocks.claim,
}));

vi.mock('./GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => ({ registerBlockHandle: mocks.register }),
}));

vi.mock('@block-canvas/CanvasBlock', () => ({
  CanvasBlock: (props: {
    documentId: string;
    params: object;
    navigationRequest: string;
  }) => {
    const id = props.documentId;
    onCleanup(() => mocks.disposed(id));
    return (
      <div
        data-testid="canvas"
        data-id={props.documentId}
        data-params={JSON.stringify(props.params)}
        data-request={props.navigationRequest}
      />
    );
  },
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.routed = false;
  mocks.claim = 'block:canvas:canvas-1';
  mocks.register.mockImplementation((type: string, id: string) => ({
    type,
    id,
  }));
});

describe('direct Canvas app wiring', () => {
  const routeEntry = (): Entry => ({
    id: 'entry-1',
    props: { x: '10' },
    location: {
      route: {
        matches: [{ id: 'canvas-detail', params: { documentId: 'canvas-1' } }],
      },
      search: { canvas: { x: ['10', '20'], y: ['30'] } },
    },
  });

  it('reads the owning route and repeats navigation without remounting', async () => {
    mocks.routed = true;
    const [entry, setEntry] = createSignal(routeEntry());
    render(() => (
      <PaneContext.Provider
        value={{ pane: () => 'test-pane' as PaneId, entry, depth: () => 0 }}
      >
        <DirectBlock type="canvas" id="canvas-1" params={{ x: 'local' }} />
      </PaneContext.Provider>
    ));

    const canvas = await screen.findByTestId('canvas');
    expect(canvas.dataset.params).toBe('{"x":"20","y":"30"}');
    const request = canvas.dataset.request;
    setEntry({ ...routeEntry(), id: 'entry-2' });
    expect(canvas.dataset.request).not.toBe(request);
    expect(screen.getByTestId('canvas')).toBe(canvas);
    expect(mocks.register).toHaveBeenCalledExactlyOnceWith(
      'canvas',
      'canvas-1'
    );
  });

  it('keeps a nested Canvas independent from the global handle registry', async () => {
    const view = { x: 1, y: 2, scale: 100 };
    render(() => (
      <DirectBlock
        type="canvas"
        id="nested"
        params={{ view }}
        nested={{ parentContext: {} }}
      />
    ));

    expect((await screen.findByTestId('canvas')).dataset.params).toBe(
      JSON.stringify({ view })
    );
    expect(mocks.register).not.toHaveBeenCalled();
  });

  it('replaces Canvas identity and releases the previous host', async () => {
    const [id, setId] = createSignal('first');
    render(() => <DirectBlock type="canvas" id={id()} />);
    await screen.findByTestId('canvas');
    setId('second');
    await waitFor(() =>
      expect(screen.getByTestId('canvas').dataset.id).toBe('second')
    );
    expect(mocks.disposed).toHaveBeenCalledWith('first');
  });

  it('does not render a duplicate managed host', async () => {
    mocks.register.mockReturnValue(undefined);
    render(() => <DirectBlock type="canvas" id="duplicate" />);
    expect(await screen.findByText('Content already open.')).toBeTruthy();
    expect(screen.queryByTestId('canvas')).toBeNull();
  });
});
