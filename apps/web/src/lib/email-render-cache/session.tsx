import { prepareEmailThreads } from '@app/features/email-thread/preparation-adapter';
import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { clearLocalAuthSession } from '@core/auth/logout';
import { enableEmailRenderCache } from '@core/constant/featureFlags';
import { useUserContext } from '@core/context/user';
import { createTabLeaderSignal } from '@core/cross-tab/tab-leader';
import { isMobile } from '@core/mobile/isMobile';
import { isTauri } from '@core/util/platform';
import { registerCacheResetListener } from '@graphql-cache/lifecycle';
import { getOrCreateCacheScope } from '@graphql-cache/scope';
import {
  type Accessor,
  createContext,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  type ParentProps,
  untrack,
  useContext,
} from 'solid-js';
import { registerEmailPreparationHints } from './hints';
import {
  invalidateEmailRenders,
  registerEmailRenderInvalidation,
} from './lifecycle';
import type { EmailRenderCache } from './service';
import { createEmailRenderSession } from './session-runtime';

const SessionContext = createContext<Accessor<EmailRenderCache | undefined>>(
  () => undefined
);
export const useEmailRenderCache = () => useContext(SessionContext);

/** One service per verified viewer session, shared by every thread/split. */
export function EmailRenderCacheProvider(props: ParentProps) {
  const user = useUserContext();
  const flag = useFeatureFlag(enableEmailRenderCache);
  const [endedViewer, setEndedViewer] = createSignal<string>();
  createEffect(() => {
    if (user.isAuthenticated() === false) setEndedViewer(undefined);
  });
  const viewer = createMemo(() =>
    user.isAuthenticated() === true && user.userId() !== endedViewer()
      ? user.userId()
      : undefined
  );
  const [epoch, setEpoch] = createSignal(0);
  let barrier: Promise<unknown> = Promise.resolve();
  let resetCurrent: ((sessionEnded: boolean) => Promise<void>) | undefined;
  const unregister = registerEmailRenderInvalidation(async (reason) => {
    const sessionEnded = reason === 'session-ended';
    const clearing = resetCurrent?.(sessionEnded);
    if (clearing) barrier = Promise.allSettled([barrier, clearing]);
    if (sessionEnded) setEndedViewer(user.userId());
    else setEpoch((value) => value + 1);
    await barrier;
  });
  onCleanup(unregister);
  onCleanup(registerCacheResetListener(() => invalidateEmailRenders()));

  const enabled = createMemo(() => flag().enabled);
  const native = isTauri();
  // Speculative hydration runs in one tab, and only while enabled.
  const isLeader = createMemo(() =>
    enabled() && !native && navigator.locks
      ? createTabLeaderSignal('email-render-cache:preparation')
      : () => false
  );
  let releaseHydration = () => {};
  function stopHydration() {
    releaseHydration();
    releaseHydration = () => {};
  }
  createEffect(() => {
    if (!enabled()) stopHydration();
  });

  // The session follows the viewer and invalidations, never the flag: a flag
  // that resolves after mount must not clear persisted artifacts or other tabs.
  const session = createMemo(() => {
    epoch();
    const identity = viewer();
    if (!identity || !globalThis.crypto?.subtle) {
      resetCurrent = undefined;
      return;
    }
    // Keep namespace/logout ownership even when the feature is disabled: cold
    // artifacts and another tab's enabled cache still belong to this viewer.
    const sessionEnded = () =>
      user.isAuthenticated() !== true || user.userId() !== identity;
    const session = createEmailRenderSession({
      origin: location.origin,
      environment: import.meta.env.MODE,
      profileScope: getOrCreateCacheScope(),
      viewerId: identity,
      enabled: untrack(enabled),
      native,
      mobile: untrack(isMobile),
      waitForInvalidation: () => barrier,
      onRemoteInvalidation(ended, clearing) {
        barrier = Promise.allSettled([barrier, clearing]);
        // End auth before cached source can be reused under a cleared
        // generation. A disabled tab holds no prepared bodies to protect.
        if (ended && enabled()) {
          setEndedViewer(identity);
          void clearLocalAuthSession();
        } else setEpoch((value) => value + 1);
      },
    });
    onCleanup(
      registerEmailPreparationHints((ids) => {
        stopHydration();
        if (!enabled() || !isLeader()()) return;
        releaseHydration = prepareEmailThreads(session.cache, ids, 4, true);
      })
    );

    resetCurrent = (ended) => session.invalidate(ended || sessionEnded());
    onCleanup(() => {
      stopHydration();
      // Account switches and owner disposal clear cold artifacts too. Browser
      // reloads do not run Solid cleanup and therefore retain persistence.
      barrier = Promise.allSettled([barrier, session.dispose(sessionEnded())]);
    });
    return session;
  });
  // A flag that turns on after mount opens storage for the current session.
  createEffect(() => {
    if (enabled()) session()?.cache.initializeStorage();
  });
  const cache = () => (enabled() ? session()?.cache : undefined);
  return (
    <SessionContext.Provider value={cache}>
      {props.children}
    </SessionContext.Provider>
  );
}
