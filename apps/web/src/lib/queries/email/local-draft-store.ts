import type { DraftAttempt, LocalDraft } from '@app/features/email-compose/core/local-draft';

type AttemptRecord = DraftAttempt & { transactionId?: string; storageGeneration?: string; settled?: boolean };
type Session = { accountId: string; epoch: string };
type StoredDraft = LocalDraft & { epoch: string };
const STORES = ['drafts', 'files', 'attempts', 'meta'] as const;

/** Transactional working-copy storage. Unlike a query cache, it never evicts edits. */
export function createLocalDraftStore(dbName = 'macro-email-working-copies-v1') {
  let connection: Promise<IDBDatabase> | undefined;
  const listeners = new Set<() => void>();
  let channel: BroadcastChannel | undefined;
  const notify = () => { for (const listener of listeners) listener(); };
  const changed = () => { notify(); channel?.postMessage(null); };
  const open = () => connection ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(dbName, 1);
    request.onupgradeneeded = () => {
      for (const name of STORES) request.result.createObjectStore(name);
    };
    request.onerror = () => { connection = undefined; reject(request.error); };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => { db.close(); connection = undefined; };
      if (typeof BroadcastChannel !== 'undefined' && !channel) {
        channel = new BroadcastChannel(dbName);
        channel.onmessage = notify;
      }
      resolve(db);
    };
  });
  async function transaction<T>(mode: IDBTransactionMode, run: (tx: IDBTransaction, finish: (value: T) => void) => void, publish = true): Promise<T> {
    const db = await open();
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction([...STORES], mode);
      let value: T;
      tx.oncomplete = () => { if (mode === 'readwrite' && publish) changed(); resolve(value); };
      tx.onabort = () => reject(tx.error ?? new Error('Draft storage transaction aborted'));
      tx.onerror = () => reject(tx.error ?? new Error('Unable to save draft on this device'));
      try { run(tx, (next) => { value = next; }); } catch (error) { tx.abort(); reject(error); }
    });
  }
  function withSession<T>(tx: IDBTransaction, session: Session, run: () => void, finish: (value: T) => void, stale: T) {
    const request = tx.objectStore('meta').get('session');
    request.onsuccess = () => {
      const current = request.result as Session | undefined;
      if (current?.accountId !== session.accountId || current.epoch !== session.epoch) { finish(stale); return; }
      run();
    };
  }
  const read = async (session: Session, id: string) => await transaction<LocalDraft | undefined>('readonly', (tx, finish) => {
    withSession(tx, session, () => {
      const request = tx.objectStore('drafts').getAll();
      request.onsuccess = () => finish((request.result as StoredDraft[]).find((draft) => draft.epoch === session.epoch && draft.accountId === session.accountId && (draft.key === id || draft.draftId === id || draft.serverDraftId === id)));
    }, finish, undefined);
  });
  return {
    /** Account transitions clear old content and fence outstanding transactions. */
    async activate(accountId: string, epoch?: string): Promise<Session> {
      return await transaction('readwrite', (tx, finish) => {
        const meta = tx.objectStore('meta');
        const request = meta.get('session');
        request.onsuccess = () => {
          const previous = request.result as Session | undefined;
          if (previous?.accountId === accountId && (!epoch || previous.epoch === epoch)) { finish(previous); return; }
          for (const name of STORES) tx.objectStore(name).clear();
          const session = { accountId, epoch: epoch ?? crypto.randomUUID() };
          meta.put(session, 'session');
          finish(session);
        };
      }, false);
    },
    read,
    async list(session: Session): Promise<LocalDraft[]> {
      return await transaction('readonly', (tx, finish) => withSession(tx, session, () => {
        const request = tx.objectStore('drafts').getAll();
        request.onsuccess = () => finish((request.result as StoredDraft[]).filter((draft) => draft.accountId === session.accountId && draft.epoch === session.epoch));
      }, finish, []));
    },
    /** Revision allocation and file references commit together; the last transaction wins. */
    async save(session: Session, input: Omit<LocalDraft, 'revision' | 'acknowledgedRevision' | 'updatedAt'>, files: ReadonlyMap<string, Blob> = new Map()): Promise<LocalDraft> {
      const result = await transaction<LocalDraft | undefined>('readwrite', (tx, finish) => withSession(tx, session, () => {
        const drafts = tx.objectStore('drafts');
        const retired = tx.objectStore('meta').get(['retired', input.key, input.generation]);
        retired.onsuccess = () => {
        if (retired.result) { finish(undefined); return; }
        const request = drafts.get(input.key);
        request.onsuccess = () => {
          const previous = request.result as StoredDraft | undefined;
          if (previous && previous.generation !== input.generation) { finish(undefined); return; }
          const paused = previous?.status === 'failed' || previous?.status === 'unconfirmed' || previous?.status === 'delete-failed';
          if (previous?.status === 'deleting') { finish(undefined); return; }
          const draft: StoredDraft = {
            ...input, epoch: session.epoch,
            serverDraftId: previous?.serverDraftId ?? input.serverDraftId,
            serverThreadId: previous?.serverThreadId ?? input.serverThreadId,
            revision: (previous?.revision ?? 0) + 1,
            acknowledgedRevision: previous?.acknowledgedRevision ?? 0,
            status: paused ? previous.status : 'dirty',
            errorCode: paused ? previous.errorCode : undefined,
            updatedAt: Date.now(),
          };
          for (const [id, blob] of files) tx.objectStore('files').put(blob, [input.key, id]);
          const retained = new Set(input.attachments.flatMap((a) => a.type === 'local' ? [a.id] : []));
          for (const attachment of previous?.attachments ?? []) if (attachment.type === 'local' && !retained.has(attachment.id)) tx.objectStore('files').delete([input.key, attachment.id]);
          drafts.put(draft, input.key);
          finish(draft);
        };
        };
      }, finish, undefined));
      if (!result) throw new Error('This draft session is no longer active');
      return result;
    },
    async file(session: Session, key: string, id: string): Promise<Blob | undefined> {
      return await transaction('readonly', (tx, finish) => withSession(tx, session, () => {
        const request = tx.objectStore('files').get([key, id]);
        request.onsuccess = () => finish(request.result);
      }, finish, undefined));
    },
    async update(session: Session, key: string, change: (draft: LocalDraft) => LocalDraft | undefined): Promise<LocalDraft | undefined> {
      return await transaction('readwrite', (tx, finish) => withSession(tx, session, () => {
        const drafts = tx.objectStore('drafts');
        const request = drafts.get(key);
        request.onsuccess = () => {
          const previous = request.result as StoredDraft | undefined;
          if (!previous) { finish(undefined); return; }
          const next = change(previous);
          if (next) drafts.put({ ...next, epoch: session.epoch }, key);
          else {
            tx.objectStore('meta').put(true, ['retired', key, previous.generation]);
            drafts.delete(key);
            for (const attachment of previous.attachments) if (attachment.type === 'local') tx.objectStore('files').delete([key, attachment.id]);
          }
          finish(next);
        };
      }, finish, undefined));
    },
    async recordAttempt(session: Session, attempt: AttemptRecord): Promise<void> {
      await transaction<void>('readwrite', (tx, finish) => withSession(tx, session, () => {
        tx.objectStore('attempts').put(attempt, attempt.id); finish();
      }, finish, undefined));
    },
    async attempts(session: Session): Promise<AttemptRecord[]> {
      return await transaction('readonly', (tx, finish) => withSession(tx, session, () => {
        const request = tx.objectStore('attempts').getAll();
        request.onsuccess = () => finish((request.result as AttemptRecord[]).filter((attempt) => attempt.accountId === session.accountId));
      }, finish, []));
    },
    async clear(): Promise<void> {
      await transaction<void>('readwrite', (tx, finish) => { for (const name of STORES) tx.objectStore(name).clear(); finish(); });
    },
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    async close() { if (connection) (await connection).close(); connection = undefined; channel?.close(); channel = undefined; },
  };
}
