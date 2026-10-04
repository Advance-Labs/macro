import { fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal, type JSX } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DiffView } from './DiffView';
import { SAMPLE_FILES, SAMPLE_PATCH } from './debug/fixtures';
import type { DiffFile } from './model/diff-file';
import type { DiffEntry } from './model/patch';

const mocks = vi.hoisted(() => ({ parse: vi.fn(), mount: vi.fn() }));
vi.mock('./model/patch', async (importOriginal) => {
  const original = await importOriginal<typeof import('./model/patch')>();
  return {
    ...original,
    parsePatch: (patch: string) => {
      mocks.parse(patch);
      return original.parsePatch(patch);
    },
  };
});
vi.mock('./pierre/PierreFileDiff', () => ({
  PierreFileDiff: (props: { path: string; diffStyle: string }) => {
    mocks.mount(props.path);
    return (
      <div
        data-testid="diff"
        data-path={props.path}
        data-style={props.diffStyle}
      />
    );
  },
}));
vi.mock('@ui', () => ({
  cn: (...classes: (string | undefined | false)[]) =>
    classes.filter(Boolean).join(' '),
  Card: (props: JSX.HTMLAttributes<HTMLDivElement>) => <div {...props} />,
  Button: (props: JSX.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props} />
  ),
  SegmentedControl: () => null,
}));

let intersect: IntersectionObserverCallback;
const observe = vi.fn();
const unobserve = vi.fn();
const disconnect = vi.fn();
const scrollIntoView = vi.fn();
let resize: ResizeObserverCallback | undefined;
const resizeObserve = vi.fn();
const resizeUnobserve = vi.fn();
const resizeDisconnect = vi.fn();

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  resize = undefined;
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: ResizeObserverCallback) {
        resize = callback;
      }
      observe = resizeObserve;
      unobserve = resizeUnobserve;
      disconnect = resizeDisconnect;
    }
  );
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      constructor(callback: IntersectionObserverCallback) {
        intersect = callback;
      }
      observe = observe;
      unobserve = unobserve;
      disconnect = disconnect;
    }
  );
  HTMLElement.prototype.scrollIntoView = scrollIntoView;
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function enter(element: Element) {
  intersect(
    [{ target: element, isIntersecting: true } as IntersectionObserverEntry],
    {} as IntersectionObserver
  );
}

function grow(element: Element, height: number) {
  resize?.(
    [
      {
        target: element,
        contentRect: { width: 800, height },
      } as ResizeObserverEntry,
    ],
    {} as ResizeObserver
  );
}

function mount(
  header?: (entry: DiffEntry) => JSX.Element,
  initialActive?: string
) {
  const [patch, setPatch] = createSignal(SAMPLE_PATCH);
  const [active, setActive] = createSignal(initialActive);
  const [files, setFiles] = createSignal<readonly DiffFile[]>(SAMPLE_FILES);
  const [revision, setRevision] = createSignal(0);
  const readPatch = () => {
    revision();
    return patch();
  };
  const result = render(() => (
    <DiffView.Root
      files={files()}
      patch={readPatch()}
      diffStyle="unified"
      active={active()}
    >
      <h1>Pane shell</h1>
      <DiffView.Stack header={header} />
    </DiffView.Root>
  ));
  return {
    ...result,
    setPatch,
    setActive,
    setFiles,
    invalidatePatch: () => setRevision((value) => value + 1),
  };
}

function expectHoverableHeader(header: HTMLElement) {
  expect(header.classList.contains('sticky')).toBe(true);
  expect(header.classList.contains('bg-surface')).toBe(true);
  expect(header.classList.contains('hover:overlay-hover')).toBe(true);
  expect(header.classList.contains('hover:bg-hover')).toBe(false);
  expect(header.classList.contains('transition-colors')).toBe(true);
  expect(header.classList.contains('duration-150')).toBe(true);
  expect(header.classList.contains('motion-reduce:transition-none')).toBe(true);
}

describe('DiffView rendering', () => {
  it('paints the shell before parsing and renders only intersecting diff bodies', () => {
    const { container } = mount();
    expect(screen.getByRole('heading', { name: 'Pane shell' })).toBeTruthy();
    expect(mocks.parse).not.toHaveBeenCalled();
    expect(mocks.mount).not.toHaveBeenCalled();
    vi.advanceTimersByTime(16);
    expect(mocks.parse).toHaveBeenCalledExactlyOnceWith(SAMPLE_PATCH);
    const cards = container.querySelectorAll('[data-path]');
    expect(cards).toHaveLength(SAMPLE_FILES.length);
    expect(observe).toHaveBeenCalledTimes(SAMPLE_FILES.length);
    expect(resizeObserve).toHaveBeenCalledTimes(SAMPLE_FILES.length);
    expect(mocks.mount).not.toHaveBeenCalled();
    enter(cards[0]);
    expect(mocks.mount).not.toHaveBeenCalled();
    vi.advanceTimersByTime(16);
    expect(mocks.mount).toHaveBeenCalledExactlyOnceWith(SAMPLE_FILES[0].path);
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
    expect(unobserve).toHaveBeenCalledWith(cards[0]);
  });

  it('keeps parsed entries and rendered bodies when the host invalidates unchanged patch text', () => {
    const { container, invalidatePatch } = mount();
    vi.advanceTimersByTime(16);
    const card = container.querySelector('[data-path]')!;
    enter(card);
    vi.advanceTimersByTime(16);
    const diff = screen.getByTestId('diff');
    invalidatePatch();
    vi.advanceTimersByTime(32);
    expect(mocks.parse).toHaveBeenCalledExactlyOnceWith(SAMPLE_PATCH);
    expect(mocks.mount).toHaveBeenCalledOnce();
    expect(container.querySelector('[data-path]')).toBe(card);
    expect(screen.getByTestId('diff')).toBe(diff);
  });

  it('coalesces patch updates and cancels parsing and rendering on unmount', () => {
    const first = mount();
    first.setPatch('');
    vi.advanceTimersByTime(16);
    expect(mocks.parse).toHaveBeenCalledExactlyOnceWith('');
    first.setPatch(SAMPLE_PATCH);
    first.unmount();
    vi.advanceTimersByTime(100);
    expect(mocks.parse).toHaveBeenCalledTimes(1);
    expect(disconnect).toHaveBeenCalledOnce();
    const second = mount();
    vi.advanceTimersByTime(16);
    enter(second.container.querySelector('[data-path]')!);
    second.unmount();
    vi.advanceTimersByTime(100);
    expect(mocks.mount).not.toHaveBeenCalled();
  });

  it('renders an off-screen active file and scrolls to it without intersection', () => {
    const { setActive, container } = mount();
    setActive(SAMPLE_FILES[1].path);
    vi.advanceTimersByTime(16);
    expect(scrollIntoView).toHaveBeenCalledWith({
      block: 'start',
      behavior: 'smooth',
    });
    vi.advanceTimersByTime(32);
    expect(mocks.mount).toHaveBeenCalledWith(SAMPLE_FILES[1].path);
    expect(
      container.querySelector(
        `[data-testid="diff"][data-path="${SAMPLE_FILES[1].path}"]`
      )
    ).toBeTruthy();
  });

  it('applies an initial active file once its queued card is ready', () => {
    const { container } = mount(undefined, SAMPLE_FILES[1].path);
    vi.advanceTimersByTime(16);
    expect(scrollIntoView).toHaveBeenCalledExactlyOnceWith({
      block: 'start',
      behavior: 'smooth',
    });
    vi.advanceTimersByTime(16);
    expect(
      container.querySelector(
        `[data-testid="diff"][data-path="${SAMPLE_FILES[1].path}"]`
      )
    ).toBeTruthy();
  });

  it.each(['files', 'patch'] as const)(
    'preserves manual scroll when %s refreshes without changing the active path',
    (refreshed) => {
      const { container, setActive, setFiles, setPatch } = mount();
      vi.advanceTimersByTime(16);
      setActive(SAMPLE_FILES[1].path);
      vi.advanceTimersByTime(120);
      const scroller = container.querySelector<HTMLElement>('[aria-busy]')!;
      fireEvent.wheel(scroller);
      scroller.scrollTop = 231;
      scrollIntoView.mockClear();
      if (refreshed === 'files') {
        setFiles(
          SAMPLE_FILES.map((file) => ({
            ...file,
            additions: file.additions + 1,
          }))
        );
      } else {
        setPatch(`${SAMPLE_PATCH}\n`);
      }
      vi.advanceTimersByTime(32);
      const card = container.querySelector<HTMLElement>(
        `[data-path="${SAMPLE_FILES[1].path}"]`
      )!;
      grow(card, 900);
      expect(scrollIntoView).not.toHaveBeenCalled();
      expect(scroller.scrollTop).toBe(231);
      // A deliberate re-selection still jumps to the same file.
      setActive(undefined);
      setActive(SAMPLE_FILES[1].path);
      expect(scrollIntoView).toHaveBeenCalledExactlyOnceWith({
        block: 'start',
        behavior: 'smooth',
      });
    }
  );

  it('does not leave the active-file flash stuck after a file refresh', () => {
    const { container, setActive, setFiles } = mount();
    vi.advanceTimersByTime(16);
    setActive(SAMPLE_FILES[1].path);
    vi.advanceTimersByTime(100);
    setFiles([...SAMPLE_FILES]);
    vi.advanceTimersByTime(800);
    const card = container.querySelector<HTMLElement>(
      `[data-path="${SAMPLE_FILES[1].path}"]`
    )!;
    expect(card.classList.contains('ring-selected')).toBe(false);
  });

  it('keeps a selected file in view when queued and asynchronous bodies above it grow', () => {
    const { setActive, container, unmount } = mount();
    vi.advanceTimersByTime(16);
    const cards = container.querySelectorAll<HTMLElement>('[data-path]');
    const target = cards[1];
    let top = 400;
    vi.spyOn(target, 'getBoundingClientRect').mockImplementation(
      () => ({ top }) as DOMRect
    );
    scrollIntoView.mockImplementation(function (this: HTMLElement) {
      if (this === target) top = 0;
    });
    mocks.mount.mockImplementation((path: string) => {
      if (path === SAMPLE_FILES[0].path) top += 400;
    });
    enter(cards[0]);
    setActive(SAMPLE_FILES[1].path);
    expect(top).toBe(0);
    vi.advanceTimersByTime(16);
    expect(top).toBe(400);
    grow(cards[0], 600);
    vi.advanceTimersByTime(120);
    expect(top).toBe(0);
    expect(scrollIntoView).toHaveBeenLastCalledWith({
      block: 'start',
      behavior: 'instant',
    });
    // Pierre's later highlighting or annotation render also changes height.
    top += 200;
    grow(cards[0], 800);
    expect(top).toBe(0);
    unmount();
    expect(resizeDisconnect).toHaveBeenCalledOnce();
  });

  it('waits for the initial smooth scroll to settle before correcting layout shifts', () => {
    const { setActive, container } = mount();
    vi.advanceTimersByTime(16);
    const card = container.querySelector<HTMLElement>('[data-path]')!;
    vi.spyOn(card, 'getBoundingClientRect').mockReturnValue({
      top: 400,
    } as DOMRect);
    setActive(SAMPLE_FILES[0].path);
    grow(card, 600);
    expect(scrollIntoView).toHaveBeenCalledExactlyOnceWith({
      block: 'start',
      behavior: 'smooth',
    });
    vi.advanceTimersByTime(100);
    const scroller = container.querySelector<HTMLElement>('[aria-busy]')!;
    fireEvent.scroll(scroller);
    vi.advanceTimersByTime(100);
    expect(scrollIntoView).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(20);
    expect(scrollIntoView).toHaveBeenCalledTimes(2);
    expect(scrollIntoView).toHaveBeenLastCalledWith({
      block: 'start',
      behavior: 'instant',
    });
  });

  it('respects scroll margin and leaves an already aligned header alone', () => {
    const { setActive, container } = mount();
    vi.advanceTimersByTime(16);
    const card = container.querySelector<HTMLElement>('[data-path]')!;
    card.style.scrollMarginTop = '12px';
    const bounds = vi
      .spyOn(card, 'getBoundingClientRect')
      .mockReturnValue({ top: 12 } as DOMRect);
    setActive(SAMPLE_FILES[0].path);
    expect(scrollIntoView).toHaveBeenCalledOnce();
    grow(card, 600);
    expect(scrollIntoView).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(120);
    bounds.mockReturnValue({ top: 112 } as DOMRect);
    grow(card, 800);
    expect(scrollIntoView).toHaveBeenCalledTimes(2);
    expect(scrollIntoView).toHaveBeenLastCalledWith({
      block: 'start',
      behavior: 'instant',
    });
  });

  it.each(['wheel', 'pointerDown', 'keyDown'] as const)(
    'releases the selected-file anchor on %s interaction',
    (interaction) => {
      const { setActive, container } = mount();
      vi.advanceTimersByTime(16);
      const card = container.querySelector<HTMLElement>(
        `[data-path="${SAMPLE_FILES[1].path}"]`
      )!;
      vi.spyOn(card, 'getBoundingClientRect').mockReturnValue({
        top: 400,
      } as DOMRect);
      setActive(SAMPLE_FILES[1].path);
      const calls = scrollIntoView.mock.calls.length;
      const scroller = container.querySelector<HTMLElement>('[aria-busy]')!;
      fireEvent[interaction](scroller);
      grow(card, 600);
      vi.advanceTimersByTime(200);
      expect(scrollIntoView).toHaveBeenCalledTimes(calls);
    }
  );

  it('styles the default sticky header without making the header a collapse control', () => {
    const { container } = mount();
    vi.advanceTimersByTime(16);
    const card = container.querySelector<HTMLElement>('[data-path]')!;
    const header = card.querySelector('header')!;
    expectHoverableHeader(header);
    fireEvent.click(header);
    expect(
      screen.getByRole('button', {
        name: `Hide ${SAMPLE_FILES[0].path.split('/').at(-1)}`,
      })
    ).toBeTruthy();
    fireEvent.click(header.querySelector('button')!);
    expect(
      screen.getByRole('button', {
        name: `Show ${SAMPLE_FILES[0].path.split('/').at(-1)}`,
      })
    ).toBeTruthy();
  });

  it('styles custom headers while leaving the copy action and diff rendering intact', () => {
    const copyPath = vi.fn();
    const { container } = mount((entry) => (
      <>
        <DiffView.CollapseButton />
        <DiffView.FilePath />
        <DiffView.FileCounts />
        <button type="button" onClick={() => copyPath(entry.file.path)}>
          Copy path
        </button>
      </>
    ));
    vi.advanceTimersByTime(16);
    const card = container.querySelector<HTMLElement>('[data-path]')!;
    const header = card.querySelector('header')!;
    expectHoverableHeader(header);
    enter(card);
    vi.advanceTimersByTime(16);
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
    fireEvent.click(header);
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
    expect(copyPath).not.toHaveBeenCalled();
    fireEvent.click(header.querySelector('button:last-child')!);
    expect(copyPath).toHaveBeenCalledExactlyOnceWith(SAMPLE_FILES[0].path);
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
    fireEvent.click(header.querySelector('button')!);
    expect(screen.queryByTestId('diff')).toBeNull();
  });

  it('keeps collapse behavior after a visible body loads', () => {
    const { container } = mount();
    vi.advanceTimersByTime(16);
    enter(container.querySelector('[data-path]')!);
    vi.advanceTimersByTime(16);
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
    fireEvent.click(
      screen.getByRole('button', {
        name: `Hide ${SAMPLE_FILES[0].path.split('/').at(-1)}`,
      })
    );
    expect(screen.queryByTestId('diff')).toBeNull();
    fireEvent.click(
      screen.getByRole('button', {
        name: `Show ${SAMPLE_FILES[0].path.split('/').at(-1)}`,
      })
    );
    expect(screen.getAllByTestId('diff')).toHaveLength(1);
  });
});
