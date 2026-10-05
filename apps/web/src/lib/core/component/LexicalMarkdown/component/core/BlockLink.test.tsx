import { openEntityInSplit } from '@app/features/activity/open-entity-in-split';
import { createCanvasRouteTarget } from '@app/features/block-canvas/primitives/create-canvas-route-target';
import { createChatRouteNavigation } from '@app/features/block-chat/primitives/create-chat-route-navigation';
import { createPdfRouteTarget } from '@app/features/block-pdf/primitives/create-pdf-route-target';
import {
  createMemorySplitRouterLocation,
  createRoutesManifest,
  createSplitRouter,
  SplitRouter,
} from '@app/lib/split-router';
import {
  SplitRouterContext,
  type SplitRouterContextValue,
} from '@app/lib/split-router/solid';
import {
  createSplitLayout,
  type SplitHandle,
  type SplitManager,
} from '@components/app/split-layout/layoutManager';
import { createAppSplitRouterMiddleware } from '@components/app/split-layout/split-router/app-middleware';
import { appSplitRoutes } from '@components/app/split-layout/split-router/app-routes';
import { createContentNavigator } from '@components/app/split-layout/split-router/content-navigation';
import { createAppSplitRouterLayout } from '@components/app/split-layout/splitRouterLayout';
import { toast } from '@core/component/Toast/Toast';
import type { BlockOrchestrator } from '@core/orchestrator';
import { render } from '@solidjs/testing-library';
import {
  createEffect,
  createRoot,
  createSignal,
  type JSX,
  onCleanup,
} from 'solid-js';
import { beforeEach, expect, it, onTestFinished, vi } from 'vitest';
import { openDocument } from './BlockLink';

const app = vi.hoisted(() => ({
  manager: undefined as SplitManager | undefined,
  source: undefined as SplitHandle | undefined,
  orchestrator: undefined as BlockOrchestrator | undefined,
}));
vi.mock('@app/signal/splitLayout', () => ({
  globalSplitManager: () => app.manager,
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalBlockOrchestrator: () => app.orchestrator,
}));
vi.mock('@components/app/split-layout/layout', () => ({
  useSplitLayout: () => ({
    openWithSplit: (...args: Parameters<SplitManager['openWithSplit']>) =>
      app.manager?.openWithSplit(args[0], { ...args[1], handle: app.source }),
  }),
}));
vi.mock('@components/app/split-layout/componentRegistry', () => ({
  resolveComponent: () => ({ element: undefined }),
}));
vi.mock('@core/constant/SettingsState', () => ({
  useSettingsState: vi.fn(),
}));
// Use the production routes without opening network connections in jsdom.
vi.mock('@service-storage/websocket', () => ({
  storageWS: { reconnectIfDisconnected: vi.fn() },
  createWebSocketJob: vi.fn(),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { addEventListener: vi.fn(), send: vi.fn() },
  state: () => 'closed',
  createConnectionWebsocketEffect: vi.fn(),
}));
vi.mock('@core/component/Toast/Toast', () => ({ toast: { alert: vi.fn() } }));
vi.mock('@core/util/useSplitNavigationHandler', () => ({
  useSplitNavigationHandler: vi.fn(),
}));

beforeEach(() => vi.clearAllMocks());

async function setup(path = '/search', sourceIndex = 0) {
  const getBlockHandle = vi.fn();
  const createBlockInstance = vi.fn(() => ({
    node: undefined,
    dispose: vi.fn(),
    detach: vi.fn(),
  }));
  const orchestrator = {
    createBlockInstance,
    getBlockHandle,
  } as unknown as BlockOrchestrator;
  app.orchestrator = orchestrator;
  const { manager, router, location } = createRoot((dispose) => {
    const manager = createSplitLayout(orchestrator, [
      { type: 'component', id: 'search' },
    ]);
    const routes = createRoutesManifest(appSplitRoutes);
    const location = createMemorySplitRouterLocation(path);
    const router = createSplitRouter({
      routes,
      layout: createAppSplitRouterLayout(manager, routes),
      location,
      middleware: createAppSplitRouterMiddleware({
        isTouchDevice: () => false,
      }),
    });
    manager.setContentNavigator(
      createContentNavigator(manager, router, routes)
    );
    onTestFinished(() => {
      router.dispose();
      dispose();
    });
    return { manager, router, location };
  });
  await router.settled();
  const owner = manager.splits()[0];
  const source = manager.splits()[sourceIndex];
  app.manager = manager;
  app.source = manager.getSplit(source.id);
  manager.activateSplit(source.id);
  createBlockInstance.mockClear();
  return {
    manager,
    router,
    location,
    owner,
    source,
    getBlockHandle,
    createBlockInstance,
  };
}

it.each([false, true])(
  'follows a channel mention into its routed detail (new split intent: %s)',
  async (newSplit) => {
    const {
      manager,
      router,
      owner,
      source,
      getBlockHandle,
      createBlockInstance,
    } = await setup(
      '/channels/channel/~/home?s0.channels.tab=threads&s0.home.tab=noise',
      1
    );
    const ownerRoute = router.route(owner.id);
    const sourceRoute = router.route(source.id);
    const mount = owner.mount;
    let previousSeek: string[] | undefined;
    for (let request = 0; request < 2; request++) {
      openDocument(
        'channel',
        'channel',
        {
          channel_message_id: 'message',
          channel_thread_id: 'thread',
        },
        newSplit
      );
      await router.settled();
      const target = router.search(owner.id, 'channels');
      expect(target).toMatchObject({
        tab: ['threads'],
        messageId: ['message'],
        threadId: ['thread'],
        seek: [expect.any(String)],
      });
      expect(target?.seek).not.toEqual(previousSeek);
      previousSeek = target?.seek;
      expect(router.search(owner.id, 'home')).toEqual({ tab: ['noise'] });
      expect(router.route(owner.id)).toEqual(ownerRoute);
      expect(router.route(source.id)).toEqual(sourceRoute);
      expect(manager.splits()[0].mount).toBe(mount);
      expect(manager.activeSplitId()).toBe(owner.id);
      expect(manager.splits()).toHaveLength(2);
    }
    expect(getBlockHandle).not.toHaveBeenCalled();
    expect(createBlockInstance).not.toHaveBeenCalled();
    expect(toast.alert).not.toHaveBeenCalled();
  }
);

it('keeps explicit latest-message navigation when reusing an open channel', async () => {
  const { router, owner, getBlockHandle } = await setup(
    '/channels/channel?s0.channels.messageId=old&s0.channels.threadId=root'
  );
  openDocument('channel', 'channel');
  await router.settled();
  expect(router.search(owner.id, 'channels')).toMatchObject({
    latest: ['true'],
    seek: [expect.any(String)],
  });
  expect(router.search(owner.id, 'channels')?.messageId).toBeUndefined();
  expect(router.search(owner.id, 'channels')?.threadId).toBeUndefined();
  expect(getBlockHandle).not.toHaveBeenCalled();
  expect(toast.alert).not.toHaveBeenCalled();
});

const targets = [
  {
    type: 'md',
    path: '/drive/md/entity',
    params: { node_id: 'node', comment_id: 'comment' },
    namespace: 'markdown-detail',
    fields: {
      documentId: ['entity'],
      nodeId: ['node'],
      commentId: ['comment'],
    },
  },
  {
    type: 'task',
    path: '/tasks/entity',
    params: { node_id: 'node' },
    namespace: 'markdown-detail',
    fields: { documentId: ['entity'], nodeId: ['node'] },
  },
  {
    type: 'pdf',
    path: '/drive/pdf/entity',
    params: { pdf_ann_id: 'annotation' },
    namespace: 'pdf-detail',
    fields: { documentId: ['entity'], annotationId: ['annotation'] },
  },
  {
    type: 'pdf',
    path: '/drive/pdf/entity',
    params: {
      pdf_page_number: '2',
      pdf_page_y: '0.25',
      pdf_page_x: '0.1',
      pdf_width: '0.2',
      pdf_height: '0.3',
    },
    namespace: 'pdf-detail',
    fields: { documentId: ['entity'], pageNumber: ['2'] },
  },
  {
    type: 'pdf',
    path: '/drive/pdf/entity',
    params: {
      pdf_search_page: '0',
      pdf_search_snippet: 'first page',
      pdf_search_highlight_terms: '["term"]',
      pdf_search_raw_query: 'term',
    },
    namespace: 'pdf-detail',
    fields: {
      documentId: ['entity'],
      snippet: ['first page'],
      query: ['term'],
    },
  },
  {
    type: 'canvas',
    path: '/drive/canvas/entity',
    params: { x: '10', y: '20', s: '150' },
    namespace: 'canvas-detail',
    fields: { documentId: ['entity'], x: ['10'], y: ['20'], scale: ['150'] },
  },
  {
    type: 'canvas',
    path: '/drive/canvas/entity',
    params: { canvas_x: '10', canvas_y: '20', canvas_scale: '150' },
    namespace: 'canvas-detail',
    fields: { documentId: ['entity'], x: ['10'], y: ['20'], scale: ['150'] },
  },
  {
    type: 'chat',
    path: '/chat/entity',
    params: { message_id: 'message' },
    namespace: 'chat-detail',
    fields: { chatId: ['entity'], messageId: ['message'] },
  },
  {
    type: 'email',
    path: '/mail/entity',
    params: { email_message_id: 'message' },
    namespace: 'email-detail',
    fields: { messageId: ['message'] },
  },
] as const;

it.each(targets)(
  'replays same-owner $type locations without remounting',
  async ({ type, path, params, namespace, fields }) => {
    const { manager, router, owner, getBlockHandle, createBlockInstance } =
      await setup(`${path}?s0.drive.filter=keep`);
    const ownerRoute = router.route(owner.id);
    const mount = owner.mount;
    let previousSeek: string[] | undefined;
    for (let request = 0; request < 2; request++) {
      openDocument(type, 'entity', params);
      await router.settled();
      const target = router.search(owner.id, namespace);
      expect(target).toMatchObject({ ...fields, seek: [expect.any(String)] });
      expect(target?.seek).not.toEqual(previousSeek);
      previousSeek = target?.seek;
      expect(router.route(owner.id)).toEqual(ownerRoute);
      expect(manager.splits()[0].mount).toBe(mount);
      expect(router.search(owner.id, 'drive')).toEqual({ filter: ['keep'] });
      expect(manager.splits()).toHaveLength(1);
    }
    expect(getBlockHandle).not.toHaveBeenCalled();
    expect(createBlockInstance).not.toHaveBeenCalled();
  }
);

it.each(targets)(
  'delivers a cold $type link through its destination route',
  async ({ type, params, namespace, fields }) => {
    const { manager, router, source, getBlockHandle } = await setup();
    openDocument(type, 'entity', params, true);
    await router.settled();
    const owner = manager.splits().find((split) => split.id !== source.id)!;
    expect(router.search(owner.id, namespace)).toMatchObject({
      ...fields,
      seek: [expect.any(String)],
    });
    expect(router.route(source.id)!.matches[0].id).toBe('view-search');
    expect(router.search(source.id, namespace)).toBeUndefined();
    expect(manager.splits()).toHaveLength(2);
    expect(getBlockHandle).not.toHaveBeenCalled();
  }
);

it('replaces PDF annotation and precise fields before applying a search link', async () => {
  const { router, owner, getBlockHandle } = await setup(
    '/drive/pdf/entity?s0.pdf-detail.annotationId=old&s0.pdf-detail.pageNumber=3&s0.pdf-detail.yPos=0.5&s0.pdf-detail.x=0.1&s0.pdf-detail.width=0.2&s0.pdf-detail.height=0.3'
  );
  openDocument('pdf', 'entity', {
    pdf_search_page: '0',
    pdf_search_snippet: 'first page',
  });
  await router.settled();
  const target = router.search(owner.id, 'pdf-detail');
  expect(target).toMatchObject({
    documentId: ['entity'],
    snippet: ['first page'],
    seek: [expect.any(String)],
  });
  for (const field of [
    'annotationId',
    'pageNumber',
    'yPos',
    'x',
    'width',
    'height',
  ])
    expect(target?.[field]).toBeUndefined();
  expect(getBlockHandle).not.toHaveBeenCalled();
});

it.each([false, true])(
  'routes a cold document target with new-split intent %s',
  async (newSplit) => {
    const { manager, router, source, getBlockHandle } = await setup();
    openDocument('md', 'entity', { node_id: 'node' }, newSplit);
    await router.settled();
    expect(manager.splits()).toHaveLength(newSplit ? 2 : 1);
    const owner = manager
      .splits()
      .find(
        (split) =>
          router.search(split.id, 'markdown-detail')?.nodeId?.[0] === 'node'
      )!;
    expect(router.search(owner.id, 'markdown-detail')).toMatchObject({
      documentId: ['entity'],
      nodeId: ['node'],
      seek: [expect.any(String)],
    });
    if (newSplit)
      expect(router.route(source.id)!.matches[0].id).toBe('view-search');
    expect(getBlockHandle).not.toHaveBeenCalled();
  }
);

it('routes only the latest location when requests race before settlement', async () => {
  const { router, owner, getBlockHandle } = await setup('/drive/md/entity');
  openDocument('md', 'entity', { node_id: 'first' });
  openDocument('md', 'entity', { node_id: 'second' });
  await router.settled();
  expect(router.search(owner.id, 'markdown-detail')?.nodeId).toEqual([
    'second',
  ]);
  expect(getBlockHandle).not.toHaveBeenCalled();
});

it('orients users when an activity selection reuses a registry-only non-location preview', () => {
  const manager = createRoot((dispose) => {
    onTestFinished(dispose);
    return createSplitLayout(
      { createBlockInstance: vi.fn() } as unknown as BlockOrchestrator,
      [
        { type: 'component', id: 'channels' },
        { type: 'component', id: 'home' },
      ]
    );
  });
  const [owner, source] = manager.splits();
  manager.activateSplit(source.id);
  const activate = vi.fn(() => manager.activateSplit(owner.id));
  manager.registerOpenViews(() => [
    { owner: 'preview', content: { type: 'code', id: 'document' }, activate },
  ]);
  app.manager = manager;
  app.source = manager.getSplit(source.id);
  openEntityInSplit({ block: 'code', id: 'document', newSplit: true });
  expect(activate).toHaveBeenCalledOnce();
  expect(manager.activeSplitId()).toBe(owner.id);
  expect(toast.alert).toHaveBeenCalledWith('Content already open');
});

it('orients users only after a routed activity selection reuses its owner', async () => {
  const { manager, router, owner, source, getBlockHandle } = await setup(
    '/channels/channel/~/home',
    1
  );
  manager.registerOpenViews(() => [
    {
      owner: Symbol('route-backed-preview'),
      content: { type: 'channel', id: 'channel' },
      activate: () => manager.activateSplit(owner.id),
    },
  ]);
  openEntityInSplit({ block: 'channel', id: 'channel', newSplit: true });
  await router.settled();
  expect(toast.alert).toHaveBeenCalledOnce();
  expect(manager.activeSplitId()).toBe(owner.id);
  expect(router.route(source.id)!.matches[0].id).toBe('view-home');
  expect(getBlockHandle).not.toHaveBeenCalled();
});

it('does not report same-owner activity target navigation as a reused foreign pane', async () => {
  const { router } = await setup('/channels/channel');
  openEntityInSplit({
    block: 'channel',
    id: 'channel',
    params: { channel_message_id: 'message' },
    newSplit: false,
  });
  await router.settled();
  expect(toast.alert).not.toHaveBeenCalled();
});

function mountProbe(
  router: Awaited<ReturnType<typeof setup>>['router'],
  splitId: SplitHandle['id'],
  Probe: () => JSX.Element
) {
  const view = render(() => {
    const [revision, setRevision] = createSignal(0);
    const unsubscribe = router.subscribe(() =>
      setRevision((value) => value + 1)
    );
    onCleanup(unsubscribe);
    const context: SplitRouterContextValue = {
      router: router as unknown as SplitRouterContextValue['router'],
      globalRevision: revision,
      track: () => {
        revision();
      },
    };
    return (
      <SplitRouterContext.Provider value={context}>
        <SplitRouter.Scope splitId={splitId}>
          <Probe />
        </SplitRouter.Scope>
      </SplitRouterContext.Provider>
    );
  });
  onTestFinished(view.unmount);
}

it.each(['pdf', 'canvas'] as const)(
  'delivers repeated %s route targets to the feature reader and rejects foreign identity',
  async (type) => {
    const { router, owner } = await setup(`/drive/${type}/entity`);
    const seen: Array<Record<string, string> | undefined> = [];
    mountProbe(router, owner.id, () => {
      const target =
        type === 'pdf'
          ? createPdfRouteTarget(() => 'entity')
          : createCanvasRouteTarget(() => 'entity');
      createEffect(() => seen.push(target()));
      return null;
    });
    const params: Record<string, string> =
      type === 'pdf' ? { pdf_ann_id: 'annotation' } : { x: '10' };
    openDocument(type, 'entity', params);
    await router.settled();
    openDocument(type, 'entity', params);
    await router.settled();
    expect(seen.filter(Boolean)).toHaveLength(2);
    const namespace = `${type}-detail`;
    router.updateSearch(owner.id, namespace, (current) => ({
      ...current,
      unrelated: ['changed'],
    }));
    await router.settled();
    expect(seen.filter(Boolean)).toHaveLength(2);
    router.updateSearch(owner.id, namespace, (current) => ({
      ...current,
      documentId: ['other'],
    }));
    await router.settled();
    expect(seen.at(-1)).toBeUndefined();
  }
);

it('delivers newest Chat route requests without replaying unrelated search and clears foreign targets', async () => {
  const { router, owner } = await setup('/chat/entity');
  const seen: Array<Record<string, string> | undefined> = [];
  mountProbe(router, owner.id, () => {
    createChatRouteNavigation(
      () => 'entity',
      (params) => seen.push(params)
    );
    return null;
  });
  openDocument('chat', 'entity', { message_id: 'first' });
  openDocument('chat', 'entity', { message_id: 'second' });
  await router.settled();
  expect(seen.at(-1)).toEqual({ message_id: 'second' });
  const count = seen.length;
  router.updateSearch(owner.id, 'drive', { filter: ['other'] });
  await router.settled();
  expect(seen).toHaveLength(count);
  openDocument('chat', 'entity', { message_id: 'second' });
  await router.settled();
  expect(seen).toHaveLength(count + 1);
  router.updateSearch(owner.id, 'chat-detail', {
    chatId: ['foreign'],
    messageId: ['first'],
    seek: ['foreign'],
  });
  await router.settled();
  expect(seen.at(-1)).toBeUndefined();
});

it('does not let disabled local Canvas and Chat readers borrow an enclosing route target', async () => {
  const { router, owner } = await setup('/drive/canvas/entity');
  const seen = vi.fn();
  mountProbe(router, owner.id, () => {
    const target = createCanvasRouteTarget(() => 'entity', false);
    createChatRouteNavigation(() => 'entity', seen, false);
    createEffect(() => seen(target()));
    return null;
  });
  openDocument('canvas', 'entity', { x: '10' });
  await router.settled();
  expect(seen).toHaveBeenCalledOnce();
  expect(seen).toHaveBeenCalledWith(undefined);
});

it('keeps ordinary image preview reuse outside the location fallback', async () => {
  const { manager, router, getBlockHandle } = await setup('/home');
  const activate = vi.fn();
  manager.registerOpenViews(() => [
    {
      owner: Symbol('image-preview'),
      content: { type: 'image', id: 'image' },
      activate,
    },
  ]);
  const result = openDocument('image', 'image', undefined, true);
  await router.settled();
  expect(result?.status).toBe('reused');
  expect(activate).toHaveBeenCalledOnce();
  expect(getBlockHandle).not.toHaveBeenCalled();
});

it.each(targets)(
  'retargets a route-backed Home $type preview without legacy delivery',
  async ({ type, params, namespace, fields }) => {
    const {
      manager,
      router,
      owner,
      source,
      getBlockHandle,
      createBlockInstance,
    } = await setup(`/home/${type}/entity/~/search?s0.home.tab=noise`, 1);
    const previewOwner = Symbol('home-preview');
    manager.registerOpenViews(() => [
      {
        owner: previewOwner,
        content: { type, id: 'entity' },
        activate: () => manager.activateSplit(owner.id),
      },
    ]);
    const ownerRoute = router.route(owner.id);
    const sourceLocation = router.location(source.id);
    const sourceHistory = router.history(source.id);
    const mount = owner.mount;
    const applied = vi.fn();
    let previousSeek: string[] | undefined;
    for (let repeat = 0; repeat < 2; repeat++) {
      openDocument(type, 'entity', params, true, applied);
      await router.settled();
      const target = router.search(owner.id, namespace);
      expect(target).toMatchObject({ ...fields, seek: [expect.any(String)] });
      expect(target?.seek).not.toEqual(previousSeek);
      previousSeek = target?.seek;
      expect(router.route(owner.id)).toEqual(ownerRoute);
      expect(manager.splits()[0].mount).toBe(mount);
      expect(router.search(owner.id, 'home')).toEqual({ tab: ['noise'] });
      expect(router.location(source.id)).toEqual(sourceLocation);
      expect(router.history(source.id)).toEqual(sourceHistory);
      expect(manager.activeSplitId()).toBe(owner.id);
      expect(manager.splits()).toHaveLength(2);
    }
    expect(applied).toHaveBeenCalledTimes(2);
    expect(getBlockHandle).not.toHaveBeenCalled();
    expect(createBlockInstance).not.toHaveBeenCalled();
  }
);

it('keeps the newest PDF target available until a Home preview becomes ready', async () => {
  const { router, owner, getBlockHandle } = await setup(
    '/home/pdf/entity/~/search',
    1
  );
  const seen: Array<Record<string, string> | undefined> = [];
  let makeReady!: () => void;
  mountProbe(router, owner.id, () => {
    const target = createPdfRouteTarget(() => 'entity');
    const [ready, setReady] = createSignal(false);
    makeReady = () => setReady(true);
    createEffect(() => {
      if (ready()) seen.push(target());
    });
    return null;
  });
  openDocument('pdf', 'entity', { pdf_ann_id: 'old' });
  openDocument('pdf', 'entity', { pdf_ann_id: 'new' });
  await router.settled();
  expect(seen).toEqual([]);
  makeReady();
  expect(seen).toEqual([{ pdf_ann_id: 'new' }]);
  expect(getBlockHandle).not.toHaveBeenCalled();
});

it('reuses an Agents Chat owner and delivers repeated targets through its route', async () => {
  const { manager, router, owner, source, getBlockHandle } = await setup(
    '/agents/chat/entity/~/home',
    1
  );
  const seen: Array<Record<string, string> | undefined> = [];
  mountProbe(router, owner.id, () => {
    createChatRouteNavigation(
      () => 'entity',
      (params) => seen.push(params)
    );
    return null;
  });
  const ownerRoute = router.route(owner.id);
  const sourceLocation = router.location(source.id);
  for (let repeat = 0; repeat < 2; repeat++) {
    openDocument('chat', 'entity', { message_id: 'message' }, true);
    await router.settled();
    expect(seen.filter(Boolean)).toHaveLength(repeat + 1);
    expect(seen.at(-1)).toEqual({ message_id: 'message' });
    expect(router.route(owner.id)).toEqual(ownerRoute);
    expect(router.location(source.id)).toEqual(sourceLocation);
    expect(manager.activeSplitId()).toBe(owner.id);
    expect(manager.splits()).toHaveLength(2);
  }
  expect(getBlockHandle).not.toHaveBeenCalled();
});
