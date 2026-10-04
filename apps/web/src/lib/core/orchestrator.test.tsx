import { render } from '@solidjs/testing-library';
import { createRoot, type ParentProps } from 'solid-js';
import { beforeEach, expect, it, vi } from 'vitest';
import { createBlockInstance, createBlockOrchestrator } from './orchestrator';

const flags = vi.hoisted(() => ({ videoEnabled: true }));
vi.mock('./constant/featureFlags', () => ({
  get ENABLE_VIDEO_BLOCK() {
    return flags.videoEnabled;
  },
}));
beforeEach(() => {
  flags.videoEnabled = true;
});

vi.mock('./block', () => ({
  Block: (props: ParentProps) => props.children,
  ValidNestingCombinations: {},
}));
vi.mock('./constant/allBlocks', () => ({
  resolveBlockAlias: (type: string) => type,
  blocks: {
    channel: { component: () => <div>Channel content</div> },
    video: { component: () => <div>Video content</div> },
    unknown: { component: () => <div>Download file</div> },
  },
}));
vi.mock('./internal/BlockLoader', () => ({ BlockLoader: () => null }));
vi.mock('./component/LoadingBlock', () => ({ LoadingBlock: () => null }));

it('keeps one live mount and preserves its handle when a duplicate unmounts', async () => {
  const orchestrator = createBlockOrchestrator();
  const first = orchestrator.createBlockInstance('channel', 'channel-1');
  const firstView = render(first.element);
  const navigate = vi.fn();
  first.handle.registerMethod('goToLocationFromParams', navigate);

  const duplicate = orchestrator.createBlockInstance('channel', 'channel-1');
  const duplicateView = render(duplicate.element);
  expect(duplicateView.container.textContent).toBe('Content already open.');
  expect(firstView.container.textContent).toBe('Channel content');
  expect(orchestrator.isBlockMounted('channel', 'channel-1')).toBe(true);

  duplicateView.unmount();
  const handle = await orchestrator.getBlockHandle('channel-1', 'channel');
  await handle?.goToLocationFromParams({ message: 'message-1' });
  expect(navigate).toHaveBeenCalledOnce();

  firstView.unmount();
  expect(orchestrator.isBlockMounted('channel', 'channel-1')).toBe(false);
  const reopened = orchestrator.createBlockInstance('channel', 'channel-1');
  const reopenedView = render(reopened.element);
  expect(reopenedView.container.textContent).toBe('Channel content');
  reopenedView.unmount();
});

it('exposes a handle for content mounted outside a block container until its owner disposes', async () => {
  const orchestrator = createBlockOrchestrator();
  const navigate = vi.fn();
  const dispose = createRoot((dispose) => {
    const handle = orchestrator.registerBlockHandle('channel', 'channel-1');
    handle?.registerMethod('goToLocationFromParams', navigate);
    expect(
      orchestrator.registerBlockHandle('channel', 'channel-1')
    ).toBeUndefined();
    return dispose;
  });

  const handle = await orchestrator.getBlockHandle('channel-1', 'channel');
  await handle?.goToLocationFromParams({ message: 'message-1' });
  expect(navigate).toHaveBeenCalledWith({ message: 'message-1' });

  dispose();
  const mounted = orchestrator.createBlockInstance('channel', 'channel-1');
  const view = render(mounted.element);
  expect(view.container.textContent).toBe('Channel content');
  expect(
    orchestrator.registerBlockHandle('channel', 'channel-1')
  ).toBeUndefined();
  view.unmount();
});

it.each([true, false])(
  'gates managed video rendering with playback enabled %s',
  (enabled) => {
    flags.videoEnabled = enabled;
    const orchestrator = createBlockOrchestrator();
    const video = orchestrator.createBlockInstance('video', 'video-1');
    const view = render(video.element);
    expect(view.container.textContent).toBe(
      enabled ? 'Video content' : 'Download file'
    );
    view.unmount();
  }
);

it.each([true, false])(
  'gates embedded video rendering with playback enabled %s',
  (enabled) => {
    flags.videoEnabled = enabled;
    const video = createBlockInstance('video', 'video-1');
    const view = render(video.element);
    expect(view.container.textContent).toBe(
      enabled ? 'Video content' : 'Download file'
    );
    view.unmount();
  }
);
