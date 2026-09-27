import { createPreparationExecutor } from './executor';
import { IndexedDbArtifacts } from './indexeddb';
import { digest } from './keys';
import { EmailRenderCache } from './service';
import { storageDeadline } from './store';

export interface EmailRenderSessionOptions {
  origin: string;
  environment: string;
  profileScope: string;
  viewerId: string;
  enabled: boolean;
  native: boolean;
  mobile: boolean;
  waitForInvalidation(): Promise<unknown>;
  onRemoteInvalidation(sessionEnded: boolean, clearing: Promise<void>): void;
}

/** Browser cache lifetime with explicit inputs; no reactive owner or app hooks. */
export function createEmailRenderSession(options: EmailRenderSessionOptions) {
  const namespace = digest(
    JSON.stringify([
      options.origin,
      options.environment,
      options.profileScope,
      options.viewerId,
    ])
  );
  let store: IndexedDbArtifacts | undefined;
  let invalidated = false;
  let sentSessionEnd = false;
  let channel: BroadcastChannel | undefined;
  let disposed = false;
  let clearing: Promise<void> | undefined;
  const cache = new EmailRenderCache({
    memoryBytes: (options.mobile ? 8 : 16) * 1024 * 1024,
    // Native stays memory-only until its worker and storage origins are verified.
    executor: createPreparationExecutor(!options.native),
    store: options.native
      ? undefined
      : async () => {
          await options.waitForInvalidation();
          const name = await namespace;
          if (
            disposed ||
            invalidated ||
            localStorage.getItem(`email-render-quarantine:${name}`)
          )
            return;
          let budget = (options.mobile ? 32 : 128) * 1024 * 1024;
          const estimate = await storageDeadline(
            navigator.storage?.estimate?.() ?? Promise.resolve(undefined),
            50
          );
          if (estimate?.quota)
            budget = Math.min(
              budget,
              Math.max(0, (estimate.quota - (estimate.usage ?? 0)) / 4)
            );
          if (disposed || invalidated || budget < 1024 * 1024) return;
          store ??= new IndexedDbArtifacts(name, budget);
          return store;
        },
  });
  // Initialize storage ahead of body requests, without fetching or preparing.
  if (options.enabled) cache.initializeStorage();

  async function broadcastInvalidation(sessionEnded: boolean) {
    if (sessionEnded && sentSessionEnd) return;
    if (sessionEnded) sentSessionEnd = true;
    const name = await namespace;
    try {
      const sender = new BroadcastChannel(`email-render:${name}`);
      sender.postMessage({ kind: 'invalidate', sessionEnded });
      sender.close();
    } catch {
      /* Storage quarantine still protects subsequent sessions. */
    }
  }

  async function clearStorage(broadcast: boolean, sessionEnded: boolean) {
    if (!broadcast) {
      store?.close();
      return;
    }
    const name = await namespace;
    await broadcastInvalidation(sessionEnded);
    if (options.native) return;
    // Failed clears make this namespace ineligible for subsequent sessions.
    try {
      localStorage.setItem(`email-render-quarantine:${name}`, '1');
    } catch {
      // Still clear cold artifacts when the quarantine store is unavailable.
    }
    const target = store ?? new IndexedDbArtifacts(name, 0);
    try {
      const result = await storageDeadline(
        (async () => {
          await target.invalidate();
          return true;
        })(),
        2000
      );
      if (result) localStorage.removeItem(`email-render-quarantine:${name}`);
    } catch {
      /* Quarantine remains when storage is inaccessible. */
    } finally {
      target.close();
    }
  }

  async function clear(
    broadcast: boolean,
    sessionEnded = false
  ): Promise<void> {
    if (invalidated) {
      // A pending source reset must not swallow a subsequent logout.
      if (broadcast && sessionEnded) await broadcastInvalidation(true);
      await clearing;
      return;
    }
    invalidated = true;
    cache.dispose();
    clearing = clearStorage(broadcast, sessionEnded);
    await clearing;
  }

  async function connect(): Promise<void> {
    const name = await namespace;
    if (disposed || invalidated || typeof BroadcastChannel === 'undefined')
      return;
    try {
      channel = new BroadcastChannel(`email-render:${name}`);
    } catch {
      return;
    }
    channel.onmessage = (event: MessageEvent<unknown>) => {
      if (disposed) return;
      const message = event.data as {
        kind?: string;
        sessionEnded?: boolean;
      } | null;
      if (message?.kind !== 'invalidate') return;
      options.onRemoteInvalidation(message.sessionEnded === true, clear(false));
    };
  }
  void connect();

  return {
    cache,
    invalidate: (sessionEnded = false) => clear(true, sessionEnded),
    dispose(sessionEnded = false) {
      disposed = true;
      cache.dispose();
      channel?.close();
      return clear(true, sessionEnded);
    },
  };
}
