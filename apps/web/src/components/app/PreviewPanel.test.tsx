import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { createSignal, type JSX, onCleanup, onMount } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  PreviewPanel,
  type PreviewPanelProps,
  useMaybePreviewPanel,
} from './PreviewPanel';
import type {
  PreviewBlockTarget,
  PreviewPanelSelection,
} from './previewTarget';

const mocks = vi.hoisted(() => ({
  goToLocationFromParams: vi.fn(),
  goToLatest: vi.fn(),
  mounts: vi.fn(),
  unmounts: vi.fn(),
  imageMounts: vi.fn(),
  imageUnmounts: vi.fn(),
}));

vi.mock('@core/hotkey/hotkeys', () => ({
  useHotkeyDOMScope: () => [() => {}, {}],
}));
vi.mock('./split-layout/components/PriorityCollapseOverflowSensor', () => ({
  createPriorityCollapseController: () => ({
    setRow: () => {},
    collapser: {},
  }),
  PriorityCollapseOverflowSensor: () => null,
}));
vi.mock('@app/features/block-image/ImageBlock', () => ({
  ImageBlock: (props: {
    documentId: string;
    children?: (context: unknown, content: JSX.Element) => JSX.Element;
  }) => {
    onMount(mocks.imageMounts);
    onCleanup(mocks.imageUnmounts);
    const preview = useMaybePreviewPanel();
    const content = (
      <div>
        <span data-testid="image">{props.documentId}</span>
        <span data-testid="selection">{preview?.previewEntity()?.id}</span>
        <input aria-label="Image preview focus" />
      </div>
    );
    return (
      props.children?.(
        {
          documentId: props.documentId,
          data: {},
          documentMetadata: { documentId: props.documentId },
          userAccessLevel: 'view',
          download: () => {},
        },
        content
      ) ?? content
    );
  },
}));
vi.mock('@app/components/entity-detail/FileEntityDetail', () => ({
  FileEntityDetail: (props: {
    content: JSX.Element;
    children?: (context: unknown) => JSX.Element;
  }) => (
    <>
      {props.children?.({})}
      {props.content}
    </>
  ),
}));
vi.mock('@app/features/block-image/ImageBlockTopBar', () => ({
  ImageBlockTopBar: () => null,
}));
vi.mock('./side-panel', () => ({
  SidePanel: { Root: (props: { children: unknown }) => props.children },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup(initial: PreviewBlockTarget, entity?: PreviewPanelSelection) {
  const [target, setTarget] = createSignal(initial);
  const [selectedEntity, setSelectedEntity] = createSignal(entity);
  const [navigationRequest, setNavigationRequest] = createSignal(0);
  let preview: ReturnType<typeof useMaybePreviewPanel>;
  const createBlockInstance = vi.fn((type: string, id: string) => ({
    type,
    id,
    element: () => {
      onMount(mocks.mounts);
      onCleanup(mocks.unmounts);
      preview = useMaybePreviewPanel();
      return (
        <div>
          <span data-testid="block">{id}</span>
          <span data-testid="selection">{preview?.previewEntity()?.id}</span>
          <input aria-label="Message draft" />
        </div>
      );
    },
  }));
  const getBlockHandle = vi.fn(async () => ({
    goToLocationFromParams: mocks.goToLocationFromParams,
    goToLatest: mocks.goToLatest,
  }));
  const orchestrator = {
    isBlockMounted: () => false,
    createBlockInstance,
    getBlockHandle,
  } as unknown as PreviewPanelProps['orchestrator'];
  const onFocusOut = vi.fn();
  const view = render(() => (
    <PreviewPanel
      target={target()}
      selectedEntity={selectedEntity()}
      navigationRequest={navigationRequest()}
      orchestrator={orchestrator}
      splitPanelContext={{} as PreviewPanelProps['splitPanelContext']}
      onFocusOut={onFocusOut}
    />
  ));
  return {
    ...view,
    setTarget,
    setSelectedEntity,
    requestNavigation: () => setNavigationRequest((count) => count + 1),
    previewEntity: () => preview?.previewEntity(),
    createBlockInstance,
    getBlockHandle,
    onFocusOut,
  };
}

const channel = (
  id: string,
  params?: Record<string, string>
): PreviewBlockTarget => ({
  blockType: 'channel',
  blockId: id,
  aliasContext: undefined,
  params,
});

describe('preview block navigation', () => {
  it('does not relocate or remount when the same target arrives as a fresh object', async () => {
    const view = setup(channel('channel-1', { channel_message_id: 'm-1' }));
    await flush();
    const draft = view.getByLabelText('Message draft');
    fireEvent.input(draft, { target: { value: 'Unsent draft' } });
    expect(view.getBlockHandle).toHaveBeenCalledTimes(1);
    expect(mocks.goToLocationFromParams).toHaveBeenCalledTimes(1);

    // Hosts recompute targets from cache revisions unrelated to this block.
    view.setTarget(channel('channel-1', { channel_message_id: 'm-1' }));
    view.setTarget(channel('channel-1', { channel_message_id: 'm-1' }));
    await flush();

    expect(view.getBlockHandle).toHaveBeenCalledTimes(1);
    expect(view.createBlockInstance).toHaveBeenCalledTimes(1);
    expect(mocks.mounts).toHaveBeenCalledTimes(1);
    expect(mocks.unmounts).not.toHaveBeenCalled();
    expect(view.getByLabelText('Message draft')).toBe(draft);
    expect((draft as HTMLInputElement).value).toBe('Unsent draft');
  });

  it('preserves focus ownership after interacting with a refreshed preview', () => {
    const view = setup(channel('channel-1'));
    const draft = view.getByLabelText('Message draft');
    fireEvent.pointerDown(draft);
    view.setTarget(channel('channel-1'));
    fireEvent.focusIn(draft);
    expect(view.onFocusOut).not.toHaveBeenCalled();
  });

  it('lands untargeted channels on their latest message', async () => {
    setup(channel('channel-1'));
    await flush();
    expect(mocks.goToLatest).toHaveBeenCalledTimes(1);
    expect(mocks.goToLocationFromParams).not.toHaveBeenCalled();
  });

  it('creates a new block when selecting another channel', async () => {
    const view = setup(channel('channel-1'));
    view.setTarget(channel('channel-2'));
    await flush();
    expect(view.createBlockInstance).toHaveBeenCalledTimes(2);
    expect(mocks.goToLatest).toHaveBeenCalledTimes(2);
    expect(view.getByTestId('block').textContent).toBe('channel-2');
  });

  it('relocates within the same block without remounting it', async () => {
    const view = setup(
      channel('channel-1', {
        channel_message_id: 't-1',
        channel_thread_id: 't-1',
      })
    );
    view.setTarget(
      channel('channel-1', {
        channel_message_id: 't-2',
        channel_thread_id: 't-2',
      })
    );
    await flush();
    expect(mocks.goToLocationFromParams).toHaveBeenCalledTimes(2);
    expect(mocks.goToLocationFromParams).toHaveBeenLastCalledWith({
      channel_message_id: 't-2',
      channel_thread_id: 't-2',
    });
    expect(view.createBlockInstance).toHaveBeenCalledTimes(1);
    expect(mocks.mounts).toHaveBeenCalledTimes(1);
  });

  it('re-aims the same block on an explicit request without remounting', async () => {
    const view = setup(channel('channel-1', { channel_message_id: 'm-1' }));
    await flush();
    expect(mocks.goToLocationFromParams).toHaveBeenCalledTimes(1);

    view.requestNavigation();
    await flush();
    expect(mocks.goToLocationFromParams).toHaveBeenCalledTimes(2);
    expect(view.createBlockInstance).toHaveBeenCalledTimes(1);
    expect(mocks.mounts).toHaveBeenCalledTimes(1);
  });

  it('keeps live selection metadata in its preview context', () => {
    const view = setup(channel('channel-1'), {
      type: 'channel',
      id: 'channel-1',
    });
    const refreshed = { type: 'channel' as const, id: 'channel-1' };
    view.setSelectedEntity(refreshed);
    expect(view.previewEntity()).toBe(refreshed);
    expect(view.getByTestId('selection').textContent).toBe('channel-1');
  });
});

const image = (id: string): PreviewBlockTarget => ({
  blockType: 'image',
  blockId: id,
  aliasContext: undefined,
});

describe('direct image previews', () => {
  it('does not require an orchestrator for an image preview', async () => {
    const view = render(() => (
      <PreviewPanel
        target={image('image-1')}
        splitPanelContext={{} as PreviewPanelProps['splitPanelContext']}
      />
    ));
    await flush();
    expect(view.getByTestId('image').textContent).toBe('image-1');
  });
  it('renders without creating a legacy instance or requesting a handle', async () => {
    const view = setup(image('image-1'));
    await flush();

    expect(view.getByTestId('image').textContent).toBe('image-1');
    expect(view.createBlockInstance).not.toHaveBeenCalled();
    expect(view.getBlockHandle).not.toHaveBeenCalled();
  });

  it('keeps the image mounted for repeated selections and navigation requests', async () => {
    const view = setup(image('image-1'));
    await flush();
    const focus = view.getByLabelText('Image preview focus');
    fireEvent.pointerDown(focus);
    view.setTarget(image('image-1'));
    view.requestNavigation();
    await flush();

    expect(view.getByLabelText('Image preview focus')).toBe(focus);
    expect(mocks.imageMounts).toHaveBeenCalledTimes(1);
    expect(mocks.imageUnmounts).not.toHaveBeenCalled();
    expect(view.createBlockInstance).not.toHaveBeenCalled();
    expect(view.getBlockHandle).not.toHaveBeenCalled();
  });

  it('passes live selection metadata to the direct image host', async () => {
    const selection: PreviewPanelSelection = {
      type: 'document',
      id: 'image-1',
      fileType: 'png',
    };
    const view = setup(image('image-1'), selection);
    await flush();

    expect(view.getByTestId('selection').textContent).toBe('image-1');
    view.setSelectedEntity({ ...selection, id: 'selected-image' });
    expect(view.getByTestId('selection').textContent).toBe('selected-image');
  });

  it('disposes the image host when selecting a remaining legacy feature', async () => {
    const view = setup(image('image-1'));
    await flush();
    view.setTarget(channel('channel-1'));
    await flush();

    expect(mocks.imageUnmounts).toHaveBeenCalledTimes(1);
    expect(view.queryByTestId('image')).toBeNull();
    expect(view.createBlockInstance).toHaveBeenCalledTimes(1);
    expect(view.createBlockInstance).toHaveBeenCalledWith(
      'channel',
      'channel-1',
      expect.any(Object)
    );
  });
});
