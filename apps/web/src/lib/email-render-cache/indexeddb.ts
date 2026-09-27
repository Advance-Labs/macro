import type { Artifact, ArtifactStore, Association } from './store';

interface NamespaceState {
  generation: number;
  bytes: number;
}

function request<T>(operation: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    operation.onsuccess = () => resolve(operation.result);
    operation.onerror = () => reject(operation.error);
  });
}

function complete(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = transaction.onerror = () =>
      reject(transaction.error ?? new Error('Artifact transaction aborted'));
  });
}

function recordBytes(record: Association): number {
  return 2 * JSON.stringify(record).length;
}

/** Separate disposable database; namespace is a digest, never an email address. */
export class IndexedDbArtifacts implements ArtifactStore {
  private database?: Promise<IDBDatabase>;
  private closed = false;
  private touched = new Set<string>();
  private touchTimer?: ReturnType<typeof setTimeout>;

  constructor(
    private namespace: string,
    private budget: number
  ) {}

  private open(): Promise<IDBDatabase> {
    if (this.closed) return Promise.reject(new Error('Artifact store closed'));
    this.database ??= new Promise((resolve, reject) => {
      const opening = indexedDB.open(
        `macro-email-renders-${this.namespace}`,
        1
      );
      let failed = false;
      const timer = setTimeout(() => {
        failed = true;
        reject(new Error('Artifact database open timed out'));
      }, 2000);
      opening.onupgradeneeded = () => {
        const db = opening.result;
        db.createObjectStore('artifacts', { keyPath: 'key' }).createIndex(
          'lastUsed',
          'lastUsed'
        );
        const associations = db.createObjectStore('messageAssociations', {
          keyPath: 'id',
        });
        associations.createIndex('thread', 'threadId');
        associations.createIndex('mailbox', 'mailboxId');
        associations.createIndex('artifacts', 'keys', { multiEntry: true });
        db.createObjectStore('namespaceState').put(
          { generation: 0, bytes: 0 },
          'state'
        );
      };
      opening.onsuccess = () => {
        clearTimeout(timer);
        if (this.closed || failed) {
          opening.result.close();
          reject(new Error('Artifact store closed'));
          return;
        }
        opening.result.onversionchange = () => opening.result.close();
        resolve(opening.result);
      };
      opening.onerror = () => {
        clearTimeout(timer);
        reject(opening.error);
      };
      opening.onblocked = () => {
        failed = true;
        clearTimeout(timer);
        reject(new Error('Artifact database blocked'));
      };
    });
    return this.database;
  }

  async generation(): Promise<number> {
    const db = await this.open();
    const state: NamespaceState = await request(
      db
        .transaction('namespaceState')
        .objectStore('namespaceState')
        .get('state')
    );
    return state.generation;
  }

  async read(key: string): Promise<unknown> {
    const db = await this.open();
    const value: unknown = await request(
      db.transaction('artifacts').objectStore('artifacts').get(key)
    );
    if (value) {
      this.touched.add(key);
      this.touchTimer ??= setTimeout(() => void this.touch(), 5000);
    }
    return value;
  }

  /** Runs `body` synchronously inside one read-write transaction. */
  private async transact(
    stores: string | string[],
    body: (transaction: IDBTransaction) => void
  ): Promise<void> {
    const db = await this.open();
    const transaction = db.transaction(stores, 'readwrite');
    const done = complete(transaction);
    body(transaction);
    await done;
  }

  private async touch(): Promise<void> {
    this.touchTimer = undefined;
    const keys = [...this.touched];
    this.touched.clear();
    try {
      await this.transact('artifacts', (transaction) => {
        const store = transaction.objectStore('artifacts');
        for (const key of keys) {
          const read = store.get(key);
          read.onsuccess = () => {
            const value = read.result as Artifact | undefined;
            if (value) store.put({ ...value, lastUsed: Date.now() });
          };
        }
      });
    } catch {
      /* Disposable metadata. */
    }
  }

  async write(
    generation: number,
    artifact: Artifact,
    association: Association
  ): Promise<void> {
    if (artifact.bytes > this.budget) return;
    // All checks and writes share a transaction with invalidation. No async
    // hashing or worker await is permitted inside this transaction.
    await this.transact(
      ['artifacts', 'messageAssociations', 'namespaceState'],
      (transaction) => {
        const states = transaction.objectStore('namespaceState');
        const artifacts = transaction.objectStore('artifacts');
        const associations = transaction.objectStore('messageAssociations');
        const readState = states.get('state');
        readState.onsuccess = () => {
          const state = readState.result as NamespaceState;
          if (state.generation !== generation) return;
          const readArtifact = artifacts.get(artifact.key);
          readArtifact.onsuccess = () => {
            const previous = readArtifact.result as Artifact | undefined;
            state.bytes += artifact.bytes - (previous?.bytes ?? 0);
            artifacts.put(artifact);
            const readAssociation = associations.get(association.id);
            readAssociation.onsuccess = () => {
              const old = readAssociation.result as Association | undefined;
              const keys =
                old?.sourceHash === association.sourceHash ? old.keys : [];
              // There are four quote/full variants per image policy. Cap policy
              // history too, so changing proxy settings cannot grow associations.
              const next = {
                ...association,
                keys: [...new Set([...keys, artifact.key])].slice(-8),
              };
              state.bytes += recordBytes(next) - (old ? recordBytes(old) : 0);
              associations.put(next);
              if (state.bytes <= this.budget) {
                states.put(state, 'state');
                return;
              }
              const cursor = artifacts.index('lastUsed').openCursor();
              cursor.onsuccess = () => {
                const row = cursor.result;
                if (!row || state.bytes <= this.budget) {
                  states.put(state, 'state');
                  return;
                }
                const value = row.value as Artifact;
                state.bytes -= value.bytes;
                row.delete();
                const references = associations
                  .index('artifacts')
                  .openCursor(value.key);
                references.onsuccess = () => {
                  const reference = references.result;
                  if (!reference) {
                    row.continue();
                    return;
                  }
                  const old = reference.value as Association;
                  const next = {
                    ...old,
                    keys: old.keys.filter((key) => key !== value.key),
                  };
                  state.bytes -= recordBytes(old);
                  if (next.keys.length) {
                    reference.update(next);
                    state.bytes += recordBytes(next);
                  } else reference.delete();
                  reference.continue();
                };
              };
            };
          };
        };
      }
    );
  }

  async remove(key: string): Promise<void> {
    await this.transact(['artifacts', 'namespaceState'], (transaction) => {
      const store = transaction.objectStore('artifacts');
      const states = transaction.objectStore('namespaceState');
      const read = store.get(key);
      read.onsuccess = () => {
        const value = read.result as Artifact | undefined;
        store.delete(key);
        const stateRead = states.get('state');
        stateRead.onsuccess = () => {
          const state = stateRead.result as NamespaceState;
          state.bytes = Math.max(
            0,
            state.bytes - (Number.isFinite(value?.bytes) ? value!.bytes : 0)
          );
          states.put(state, 'state');
        };
      };
    });
  }

  invalidate(): Promise<void> {
    return this.reset(1);
  }

  /** One bounded quota recovery: discard this derived tier, preserving generation. */
  evict(): Promise<void> {
    // Quota is origin-wide. Discard only our derived tier and retry once; source
    // databases and mutation queues are never opened by this adapter.
    return this.reset(0);
  }

  private async reset(generationStep: 0 | 1): Promise<void> {
    await this.transact(
      ['artifacts', 'messageAssociations', 'namespaceState'],
      (transaction) => {
        const states = transaction.objectStore('namespaceState');
        const read = states.get('state');
        read.onsuccess = () => {
          const state = read.result as NamespaceState;
          states.put(
            { generation: state.generation + generationStep, bytes: 0 },
            'state'
          );
          transaction.objectStore('artifacts').clear();
          transaction.objectStore('messageAssociations').clear();
        };
      }
    );
  }

  close(): void {
    this.closed = true;
    clearTimeout(this.touchTimer);
    this.touched.clear();
    void this.closeDatabase();
  }

  private async closeDatabase(): Promise<void> {
    try {
      (await this.database)?.close();
    } catch {
      /* Opening failed. */
    }
  }
}
