import { createRoot } from 'solid-js';
import { describe, expect, it, vi } from 'vitest';
import type { LegalSource } from '../context/legal-source';
import type { Envelope } from '../core/models';
import { createWorkspace } from './workspace';

function envelope(id: string): Envelope {
  return {
    id,
    title: id,
    message: '',
    filename: 'agreement.pdf',
    pageCount: 1,
    sourceSha256: 'source',
    completedSha256: null,
    status: 'sent',
    revision: 3,
    createdAt: '',
    updatedAt: '',
    expiresAt: null,
    recipients: [],
    fields: [],
    audit: [],
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
function source(get: LegalSource['get']): LegalSource {
  return {
    list: async () => [],
    get,
    document: async () => new Uint8Array([1]),
    create: vi.fn(),
    update: vi.fn(),
    send: vi.fn(),
    resend: vi.fn(),
    void: vi.fn(),
  };
}

describe('Legal route selection', () => {
  it('does not let a slow earlier agreement replace a later route selection', async () => {
    const old = deferred<Envelope>();
    const w = createRoot(() =>
      createWorkspace(
        source(async (id) => (id === 'old' ? old.promise : envelope(id)))
      )
    );
    const first = w.load('old');
    await w.load('new');
    old.resolve(envelope('old'));
    await first;
    expect(w.active()?.id).toBe('new');
  });
  it('keeps the collection closed when a pending agreement finishes loading after back navigation', async () => {
    const pending = deferred<Envelope>();
    const close = vi.fn();
    const w = createRoot(() =>
      createWorkspace(
        source(() => pending.promise),
        { start: vi.fn(), open: vi.fn(), close }
      )
    );
    const load = w.load('old');
    w.close();
    pending.resolve(envelope('old'));
    await load;
    expect(w.active()).toBeUndefined();
    expect(w.bytes()).toBeUndefined();
    expect(close).toHaveBeenCalledOnce();
  });
});
