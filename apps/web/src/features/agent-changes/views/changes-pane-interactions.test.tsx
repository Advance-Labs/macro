import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  type Mock,
  vi,
} from 'vitest';
import { AgentChangesControllerProvider } from '../context/agent-changes-controller';
import { createLocalPaneViewState } from '../pane-view-state';
import { createAgentChanges } from '../primitives/create-agent-changes';
import { createMemoryStorage } from '../tests/memory-storage';
import {
  createMockAgentChangesContext,
  MOCK_PATCH,
  mockChangeset,
} from '../tests/mock-context';
import { ChangesPane } from './ChangesPane';

type Motion = {
  target: HTMLElement;
  frames: Keyframe[];
  options: KeyframeAnimationOptions;
  cancel: Mock<() => void>;
  finish: Mock<() => void>;
  onfinish: (() => void) | null;
};
const originalAnimate = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  'animate'
);
const originalGetAnimations = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  'getAnimations'
);
const originalScrollIntoView = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  'scrollIntoView'
);
beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
    configurable: true,
    value: vi.fn(),
  });
});

function mockMotion(reduced = false) {
  const motions: Motion[] = [];
  const presentation = new Map<HTMLElement, { width: string; left?: string }>();
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({ matches: reduced }))
  );
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: HTMLElement) {
      const width =
        Number.parseFloat(presentation.get(this)?.width ?? this.style.width) ||
        0;
      return {
        width,
        height: 800,
        top: 0,
        left: 0,
        bottom: 800,
        right: width,
        x: 0,
        y: 0,
        toJSON() {},
      };
    }
  );
  const computedStyle = window.getComputedStyle.bind(window);
  vi.spyOn(window, 'getComputedStyle').mockImplementation((element) => {
    const style = computedStyle(element);
    const values = presentation.get(element as HTMLElement);
    if (!values) return style;
    return new Proxy(style, {
      get(target, property) {
        if (property === 'width') return values.width;
        if (property === 'left' && values.left) return values.left;
        const value = Reflect.get(target, property, target);
        return typeof value === 'function' ? value.bind(target) : value;
      },
    });
  });
  Object.defineProperty(HTMLElement.prototype, 'animate', {
    configurable: true,
    value: function (
      this: HTMLElement,
      frames: Keyframe[],
      options: KeyframeAnimationOptions
    ) {
      const motion: Motion = {
        target: this,
        frames,
        options,
        cancel: vi.fn(),
        finish: vi.fn(() => motion.onfinish?.()),
        onfinish: null,
      };
      motions.push(motion);
      return motion;
    },
  });
  Object.defineProperty(HTMLElement.prototype, 'getAnimations', {
    configurable: true,
    value: function (this: HTMLElement) {
      return motions.filter(
        (motion) =>
          motion.target === this && motion.cancel.mock.calls.length === 0
      );
    },
  });
  const widthMotions = (target: HTMLElement) =>
    motions.filter(
      (motion) => motion.target === target && 'width' in motion.frames[0]
    );
  return { motions, presentation, widthMotions };
}
const device = vi.hoisted(() => ({ touch: false }));
const dimensions = vi.hoisted(() => ({ width: (): number => 900 }));
vi.mock('@core/mobile/isTouchDevice', () => ({
  isTouchDevice: () => device.touch,
}));
vi.mock('@app/components/diff-view/pierre/PierreFileDiff', () => ({
  PierreFileDiff: (props: { path: string }) => (
    <div data-testid="diff" data-path={props.path} />
  ),
}));
vi.mock('@solid-primitives/resize-observer', () => ({
  createResizeObserver: () => {},
  createElementSize: (element: () => HTMLElement | undefined) => ({
    get width() {
      return element() ? dimensions.width() : null;
    },
    height: 800,
  }),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { send() {}, addEventListener() {}, removeEventListener() {} },
  state: () => 'closed',
  createConnectionBlockWebsocketEffect() {},
  createConnectionWebsocketEffect() {},
  parseWebsocketPayload: () => undefined,
}));
vi.mock('@core/constant/allBlocks', () => ({
  blocks: {},
  blockAcceptedMimetypeToFileExtension: {},
  blockAcceptedFileExtensionToMimeType: {},
}));
vi.mock('@service-storage/websocket', () => ({
  storageWS: { send() {}, addEventListener() {}, removeEventListener() {} },
  createWebSocketJob: () => Promise.reject(new Error('no websocket in tests')),
}));

afterEach(() => {
  if (originalScrollIntoView)
    Object.defineProperty(
      HTMLElement.prototype,
      'scrollIntoView',
      originalScrollIntoView
    );
  else Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView');
  device.touch = false;
  dimensions.width = () => 900;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (originalAnimate)
    Object.defineProperty(HTMLElement.prototype, 'animate', originalAnimate);
  else Reflect.deleteProperty(HTMLElement.prototype, 'animate');
  if (originalGetAnimations)
    Object.defineProperty(
      HTMLElement.prototype,
      'getAnimations',
      originalGetAnimations
    );
  else Reflect.deleteProperty(HTMLElement.prototype, 'getAnimations');
});

function mount(storage = createMemoryStorage(), fullWidth = false) {
  const context = createMockAgentChangesContext({
    summary: { capturing: false, changeset: mockChangeset() },
    patch: MOCK_PATCH,
  });
  const [dismissed, setDismissed] = createSignal<string>();
  let controller!: ReturnType<typeof createAgentChanges>;
  const view = render(() => {
    controller = createAgentChanges({
      context,
      view: createLocalPaneViewState(),
      storage,
      dismissed: [dismissed, setDismissed],
    });
    return (
      <AgentChangesControllerProvider value={controller}>
        <ChangesPane fullWidth={fullWidth} />
      </AgentChangesControllerProvider>
    );
  });
  controller.layout.open();
  return { ...view, controller: () => controller, storage, context };
}

describe('Changes pane resize interactions', () => {
  it('resizes and persists the tree with the keyboard without remounting diffs', async () => {
    const [width, setWidth] = createSignal(900);
    dimensions.width = width;
    const { controller, storage } = mount();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const diff = screen.getAllByTestId('diff')[0];
    const divider = await screen.findByRole('separator', {
      name: 'resize at 0',
    });
    expect(controller().layout.treeWidth()).toBe(256);
    fireEvent.keyDown(divider, { key: 'ArrowRight' });
    await waitFor(() => expect(controller().layout.treeWidth()).toBe(276));
    expect(
      JSON.parse(storage.getItem('agent-changes:layout:session-1') ?? '{}')
        .treeWidth
    ).toBe(276);
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    expect(screen.queryByRole('separator')).toBeNull();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    expect(screen.getByRole('separator', { name: 'resize at 0' })).toBeTruthy();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    expect(controller().layout.treeWidth()).toBe(276);
    setWidth(1200);
    const tree = screen.getByRole('group', { name: 'Changed files' });
    expect(
      tree.closest('[data-resize-panel]')?.getAttribute('style')
    ).toContain('width: 276px');
  });

  it('starts with a closed drawer and no divider on touch devices', async () => {
    device.touch = true;
    mount();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    expect(screen.queryByRole('separator')).toBeNull();
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Show file tree' })).toBeTruthy();
  });
});

describe('Changes pane tree motion', () => {
  it('uses the sidebar width transition while retaining directory state and diff owners', async () => {
    const { motions, widthMotions } = mockMotion();
    mount();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const tree = screen.getByRole('group', { name: 'Changed files' });
    const treeBody = screen.getByRole('group', { name: 'File tree controls' })
      .parentElement!;
    const treePanel = treeBody.closest<HTMLElement>('[data-resize-panel]')!;
    const diffPanel = screen
      .getByRole('group', { name: 'Diff controls' })
      .closest<HTMLElement>('[data-resize-panel]')!;
    const gutter = screen.getByRole('separator');
    const diff = screen.getAllByTestId('diff')[0];
    fireEvent.click(
      within(tree).getByRole('button', { name: /^Collapse apps/ })
    );
    motions.find((motion) => 'height' in motion.frames[0])?.finish();
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    const closing = widthMotions(treePanel)[0];
    expect(closing.frames.map((frame) => frame.width)).toEqual([
      '256px',
      '0px',
    ]);
    expect(closing.options).toMatchObject({
      duration: 140,
      easing: 'ease-out',
    });
    const diffClosing = widthMotions(diffPanel)[0];
    expect(diffClosing.frames).toEqual([
      { left: '257px', width: '643px' },
      { left: '0px', width: '900px' },
    ]);
    expect(tree.isConnected).toBe(true);
    expect(treeBody.inert).toBe(true);
    expect(treeBody.getAttribute('aria-hidden')).toBe('true');
    expect(screen.queryByRole('separator')).toBeNull();
    expect(gutter.isConnected).toBe(true);
    expect(gutter.inert).toBe(true);
    const gutterClosing = motions.find((motion) => motion.target === gutter)!;
    expect(gutterClosing.frames.at(-1)).toEqual({ left: '-3.5px' });
    closing.finish();
    expect(tree.isConnected).toBe(false);
    expect(diffClosing.cancel).toHaveBeenCalledOnce();
    expect(gutter.isConnected).toBe(false);
    expect(gutterClosing.cancel).toHaveBeenCalledOnce();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    await waitFor(() => expect(widthMotions(treePanel)).toHaveLength(2));
    const opening = widthMotions(treePanel)[1];
    expect(opening.frames.map((frame) => frame.width)).toEqual([
      '0px',
      '256px',
    ]);
    expect(widthMotions(diffPanel)[1].frames).toEqual([
      { left: '0px', width: '900px' },
      { left: '257px', width: '643px' },
    ]);
    opening.finish();
    expect(screen.getByRole('group', { name: 'Changed files' })).toBe(tree);
    expect(
      within(tree).getByRole('button', { name: /^Expand apps/ })
    ).toBeTruthy();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
  });

  it('reverses from the current tree and diff geometry and cleans up companions', async () => {
    const { motions, presentation, widthMotions } = mockMotion();
    const { unmount } = mount();
    const treeBody = screen.getByRole('group', { name: 'File tree controls' })
      .parentElement!;
    const treePanel = treeBody.closest<HTMLElement>('[data-resize-panel]')!;
    const diffPanel = screen
      .getByRole('group', { name: 'Diff controls' })
      .closest<HTMLElement>('[data-resize-panel]')!;
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    const closing = widthMotions(treePanel)[0];
    presentation.set(treePanel, { width: '128px' });
    presentation.set(diffPanel, { width: '771px', left: '129px' });
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    await waitFor(() => expect(widthMotions(treePanel)).toHaveLength(2));
    const opening = widthMotions(treePanel)[1];
    expect(opening.frames[0].width).toBe('128px');
    expect(widthMotions(diffPanel)[1].frames[0]).toEqual({
      left: '129px',
      width: '771px',
    });
    expect(closing.cancel).toHaveBeenCalledOnce();
    closing.onfinish?.();
    expect(treeBody.isConnected).toBe(true);
    unmount();
    expect(
      motions.every((motion) => motion.cancel.mock.calls.length === 1)
    ).toBe(true);
  });

  it.each(['keyboard', 'pointer'] as const)(
    'settles opening before a %s resize and persists the dragged width',
    async (interaction) => {
      const { widthMotions } = mockMotion();
      const { controller } = mount();
      const treePanel = screen
        .getByRole('group', { name: 'File tree controls' })
        .closest<HTMLElement>('[data-resize-panel]')!;
      fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
      widthMotions(treePanel)[0].finish();
      fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
      await waitFor(() => expect(widthMotions(treePanel)).toHaveLength(2));
      const opening = widthMotions(treePanel)[1];
      const divider = screen.getByRole('separator');
      if (interaction === 'keyboard') {
        fireEvent.keyDown(divider, { key: 'ArrowRight' });
      } else {
        fireEvent(
          divider,
          new MouseEvent('pointerdown', {
            bubbles: true,
            button: 0,
            clientX: 0,
          })
        );
        fireEvent(window, new MouseEvent('pointermove', { clientX: 20 }));
        fireEvent(window, new MouseEvent('pointerup', { clientX: 20 }));
      }
      await waitFor(() => expect(controller().layout.treeWidth()).toBe(276));
      // Settle synchronously instead of waiting for a native WAAPI finish event.
      expect(opening.finish).not.toHaveBeenCalled();
      expect(opening.cancel).toHaveBeenCalledOnce();
      expect(widthMotions(treePanel)).toHaveLength(2);
    }
  );

  it.each([
    ['opening', 1200],
    ['closing', 1200],
  ] as const)(
    'settles %s when the outer split width becomes %s and releases all geometry overrides',
    async (phase, nextWidth) => {
      const { motions, widthMotions } = mockMotion();
      const [width, setWidth] = createSignal(900);
      dimensions.width = width;
      mount();
      const treePanel = screen
        .getByRole('group', { name: 'File tree controls' })
        .closest<HTMLElement>('[data-resize-panel]')!;
      const diffPanel = screen
        .getByRole('group', { name: 'Diff controls' })
        .closest<HTMLElement>('[data-resize-panel]')!;
      fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
      if (phase === 'opening') {
        widthMotions(treePanel)[0].finish();
        fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
        await waitFor(() => expect(widthMotions(treePanel)).toHaveLength(2));
      }
      const active = widthMotions(treePanel).at(-1)!;
      const count = motions.length;
      setWidth(nextWidth);
      expect(active.finish).not.toHaveBeenCalled();
      expect(active.cancel).toHaveBeenCalledOnce();
      expect(
        motions.every((motion) => motion.cancel.mock.calls.length === 1)
      ).toBe(true);
      expect(motions).toHaveLength(count);
      const treeWidth = Number.parseFloat(treePanel.style.width);
      const offset = phase === 'opening' ? treeWidth + 1 : 0;
      expect(diffPanel.style.width).toBe(`${nextWidth - offset}px`);
      expect(diffPanel.style.left).toBe(`${offset}px`);
      expect(treeWidth).toBe(
        phase === 'opening' ? Math.min(256, nextWidth - 241) : 0
      );
    }
  );
  it.each(['opening', 'closing'] as const)(
    'drops docked %s motion immediately when entering drawer mode',
    async (phase) => {
      const { motions, widthMotions } = mockMotion();
      const [width, setWidth] = createSignal(900);
      dimensions.width = width;
      mount();
      await waitFor(() =>
        expect(screen.getAllByTestId('diff')).toHaveLength(2)
      );
      const tree = screen.getByRole('group', { name: 'Changed files' });
      const treePanel = tree.closest<HTMLElement>('[data-resize-panel]')!;
      const diffPanel = screen
        .getByRole('group', { name: 'Diff controls' })
        .closest<HTMLElement>('[data-resize-panel]')!;
      fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
      if (phase === 'opening') {
        widthMotions(treePanel)[0].finish();
        fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
        await waitFor(() => expect(widthMotions(treePanel)).toHaveLength(2));
      }
      setWidth(400);
      expect(treePanel.style.width).toBe('0px');
      expect(diffPanel.style.width).toBe('400px');
      expect(screen.queryByRole('separator')).toBeNull();
      const count = motions.length;
      await Promise.resolve();
      await Promise.resolve();
      expect(motions).toHaveLength(count);
      expect(
        motions.every((motion) => motion.cancel.mock.calls.length === 1)
      ).toBe(true);
      fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
      expect(screen.getByRole('group', { name: 'Changed files' })).toBe(tree);
      expect(diffPanel.style.width).toBe('400px');
    }
  );

  it('ignores an opening callback after another close wins before the next microtask', async () => {
    const { widthMotions } = mockMotion();
    mount();
    const treeBody = screen.getByRole('group', { name: 'File tree controls' })
      .parentElement!;
    const treePanel = treeBody.closest<HTMLElement>('[data-resize-panel]')!;
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    widthMotions(treePanel)[0].finish();
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    await Promise.resolve();
    expect(
      widthMotions(treePanel).every(
        (motion) => motion.frames[1].width === '0px'
      )
    ).toBe(true);
    widthMotions(treePanel).at(-1)!.finish();
    expect(treeBody.isConnected).toBe(false);
  });
  it('skips animation for reduced motion and keeps touch drawers closed initially', async () => {
    const { motions } = mockMotion(true);
    const { unmount } = mount();
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    await waitFor(() =>
      expect(screen.getByRole('group', { name: 'Changed files' })).toBeTruthy()
    );
    expect(motions).toHaveLength(0);
    unmount();
    device.touch = true;
    mount();
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    expect(motions).toHaveLength(0);
  });
});
describe('Changes pane narrow drawer', () => {
  it('keeps the same tree and diffs through width changes without rewriting wide preferences', async () => {
    const [width, setWidth] = createSignal(900);
    dimensions.width = width;
    const { controller, storage } = mount();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const tree = screen.getByRole('group', { name: 'Changed files' });
    const diff = screen.getAllByTestId('diff')[0];
    fireEvent.click(
      within(tree).getByRole('button', { name: /^Collapse apps/ })
    );
    const preferences = storage.getItem('agent-changes:layout:session-1');
    setWidth(400);
    expect(screen.queryByRole('separator')).toBeNull();
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    expect(screen.getByRole('group', { name: 'Changed files' })).toBe(tree);
    expect(
      within(tree).getByRole('button', { name: /^Expand apps/ })
    ).toBeTruthy();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    expect(storage.getItem('agent-changes:layout:session-1')).toBe(preferences);
    expect(controller().layout.treeOpen()).toBe(true);
    setWidth(900);
    expect(screen.getByRole('group', { name: 'Changed files' })).toBe(tree);
    expect(
      screen.getAllByRole('group', { name: 'Changed files' })
    ).toHaveLength(1);
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
  });

  it('retains a hidden wide tree and its width after opening the narrow drawer', async () => {
    const [width, setWidth] = createSignal(900);
    dimensions.width = width;
    const { controller, storage } = mount();
    fireEvent.click(screen.getByRole('button', { name: 'Hide file tree' }));
    expect(controller().layout.treeOpen()).toBe(false);
    const preferences = storage.getItem('agent-changes:layout:session-1');
    setWidth(400);
    fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
    await waitFor(() =>
      expect(screen.getByRole('group', { name: 'Changed files' })).toBeTruthy()
    );
    setWidth(900);
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    expect(controller().layout.treeOpen()).toBe(false);
    expect(controller().layout.treeWidth()).toBe(256);
    expect(storage.getItem('agent-changes:layout:session-1')).toBe(preferences);
  });

  it('offers the drawer toggle on touch devices', () => {
    device.touch = true;
    const { controller } = mount();
    const trigger = screen.getByRole('button', { name: 'Show file tree' });
    fireEvent.click(trigger);
    expect(screen.getByRole('group', { name: 'Changed files' })).toBeTruthy();
    expect(screen.queryByRole('separator')).toBeNull();
    expect(controller().layout.treeOpen()).toBe(true);
  });

  it('closes on selection, backdrop, Escape, and the close toggle, restoring trigger focus', async () => {
    dimensions.width = () => 400;
    mount();
    const trigger = screen.getByRole('button', { name: 'Show file tree' });
    trigger.focus();
    fireEvent.click(trigger);
    const tree = screen.getByRole('group', { name: 'Changed files' });
    fireEvent.click(within(tree).getByTitle('apps/web/src/a.ts'));
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    fireEvent.click(trigger);
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(screen.getByRole('dialog')).getByRole('button', {
          name: 'Hide file tree',
        })
      )
    );
    const backdrop = screen.getByTestId('changes-tree-backdrop');
    fireEvent.pointerDown(backdrop, { button: 0 });
    fireEvent.click(backdrop);
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
    fireEvent.click(trigger);
    fireEvent.click(
      within(tree.parentElement!.parentElement!).getByRole('button', {
        name: 'Hide file tree',
      })
    );
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(screen.queryByRole('group', { name: 'Changed files' })).toBeNull();
  });

  it('cleans up drawer animations on reversal, width change, and unmount', async () => {
    const { motions, widthMotions } = mockMotion();
    const [width, setWidth] = createSignal(400);
    dimensions.width = width;
    const { unmount } = mount();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const trigger = screen.getByRole('button', { name: 'Show file tree' });
    const diff = screen.getAllByTestId('diff')[0];
    fireEvent.click(trigger);
    const drawer = screen
      .getByRole('dialog', { name: 'Changed files' })
      .closest<HTMLElement>('[data-changes-tree-drawer-frame]')!;
    await waitFor(() => expect(widthMotions(drawer)).toHaveLength(1));
    const opening = widthMotions(drawer)[0];
    expect(opening.options.duration).toBe(140);
    fireEvent.click(screen.getByTestId('changes-tree-backdrop'));
    expect(widthMotions(drawer)).toHaveLength(2);
    fireEvent.click(trigger);
    expect(opening.cancel).toHaveBeenCalledOnce();
    expect(screen.getAllByTestId('diff')[0]).toBe(diff);
    setWidth(900);
    unmount();
    expect(
      motions.every((motion) => motion.cancel.mock.calls.length === 1)
    ).toBe(true);
  });

  it('retains only an inert visual frame during exit and releases Escape immediately', async () => {
    const { widthMotions } = mockMotion();
    dimensions.width = () => 400;
    mount();
    const trigger = screen.getByRole('button', { name: 'Show file tree' });
    trigger.focus();
    fireEvent.click(trigger);
    const tree = screen.getByRole('group', { name: 'Changed files' });
    const frame = screen
      .getByRole('dialog', { name: 'Changed files' })
      .closest<HTMLElement>('[data-changes-tree-drawer-frame]')!;
    await waitFor(() => expect(widthMotions(frame)).toHaveLength(1));
    widthMotions(frame)[0].finish();
    fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', {
        name: 'Hide file tree',
      })
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(frame.isConnected).toBe(true);
    expect(frame.inert).toBe(true);
    expect(tree.isConnected).toBe(true);
    const escapeEvent = new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    });
    fireEvent(document, escapeEvent);
    expect(escapeEvent.defaultPrevented).toBe(false);
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    widthMotions(frame).at(-1)!.finish();
    expect(frame.isConnected).toBe(false);
  });

  it('does not let queued close autofocus steal focus from an immediately reopened drawer', async () => {
    const { widthMotions } = mockMotion();
    dimensions.width = () => 400;
    mount();
    const trigger = screen.getByRole('button', { name: 'Show file tree' });
    trigger.focus();
    fireEvent.click(trigger);
    const frame = screen
      .getByRole('dialog', { name: 'Changed files' })
      .closest<HTMLElement>('[data-changes-tree-drawer-frame]')!;
    await waitFor(() => expect(widthMotions(frame)).toHaveLength(1));
    fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', {
        name: 'Hide file tree',
      })
    );
    fireEvent.click(trigger);
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(screen.getByRole('dialog')).getByRole('button', {
          name: 'Hide file tree',
        })
      )
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(document.activeElement).toBe(
      within(screen.getByRole('dialog')).getByRole('button', {
        name: 'Hide file tree',
      })
    );
    expect(screen.getAllByRole('dialog')).toHaveLength(1);
  });

  it.each([32, 120, 144, 160, 176, 200, 256, 400, 720])(
    'keeps drawer motion inside a %spx pane without resizing the diffs',
    async (width) => {
      const { widthMotions } = mockMotion();
      dimensions.width = () => width;
      mount();
      const diffPanel = screen
        .getByRole('group', { name: 'Diff controls' })
        .closest<HTMLElement>('[data-resize-panel]')!;
      const restingWidth = diffPanel.style.width;
      fireEvent.click(screen.getByRole('button', { name: 'Show file tree' }));
      const frame = screen
        .getByRole('dialog', { name: 'Changed files' })
        .closest<HTMLElement>('[data-changes-tree-drawer-frame]')!;
      await waitFor(() => expect(widthMotions(frame)).toHaveLength(1));
      const expected = `${Math.min(256, Math.max(Math.min(144, width), width - 32))}px`;
      expect(frame.style.width).toBe(expected);
      expect(widthMotions(frame)[0].frames.at(-1)?.width).toBe(expected);
      expect(diffPanel.style.width).toBe(restingWidth);
      expect(restingWidth).toBe(`${width}px`);
    }
  );

  it('accepts fullWidth without altering the pane layout', () => {
    const { controller } = mount(createMemoryStorage(), true);
    expect(controller().layout.layout()).toBe('split');
    expect(
      screen.queryByRole('button', { name: 'Back to the split' })
    ).toBeNull();
  });
});
describe('Changes pane copy path', () => {
  it('reports success after notifying without collapsing the diff', async () => {
    const { controller, context } = mount();
    await waitFor(() => expect(screen.getAllByTestId('diff')).toHaveLength(2));
    const collapsed = controller().review.isCollapsed('a.ts');
    fireEvent.click(screen.getAllByRole('button', { name: 'Copy path' })[0]);
    await waitFor(() => expect(screen.getByText('Path copied')).toBeTruthy());
    expect(context.notified).toContainEqual({
      message: 'Path copied',
      tone: 'success',
    });
    expect(controller().review.isCollapsed('a.ts')).toBe(collapsed);
    expect(screen.getAllByRole('button', { name: 'Copy path' })).toHaveLength(
      2
    );
  });

  it('returns failure after the existing notification when clipboard access fails', async () => {
    const { controller, context } = mount();
    context.host.copyText = async () => false;
    expect(await controller().copyPath('a.ts')).toBe(false);
    expect(context.notified).toContainEqual({
      message: 'The path could not be copied',
      tone: 'failure',
    });
    context.host.copyText = async () => {
      throw new Error('clipboard denied');
    };
    expect(await controller().copyPath('a.ts')).toBe(false);
  });
});
