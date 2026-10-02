import { Blob as NodeBlob } from 'node:buffer';
import 'fake-indexeddb/auto';
import { afterEach, describe, expect, it } from 'vitest';
import type { LocalDraft } from '@app/features/email-compose/core/local-draft';
import { createLocalDraftStore } from './local-draft-store';

const stores: ReturnType<typeof createLocalDraftStore>[] = [];
const open = (name = crypto.randomUUID()) => {
  const store = createLocalDraftStore(name); stores.push(store); return store;
};
const snapshot = (subject = 'Keep this draft'): Omit<LocalDraft, 'revision' | 'acknowledgedRevision' | 'updatedAt'> => ({
  key: 'local', accountId: 'owner', generation: 'generation', draftId: 'local', threadId: 'thread', content: { subject }, attachments: [], status: 'dirty',
});
afterEach(async () => { for (const store of stores.splice(0)) await store.close(); });

describe('durable email working copies', () => {
  it('retains rejected content and further edits across reopening', async () => {
    const name = crypto.randomUUID();
    const first = open(name); const session = await first.activate('owner');
    await first.save(session, snapshot());
    await first.update(session, 'local', (draft) => ({ ...draft, status: 'failed', errorCode: 'INTERNAL' }));
    await first.save(session, snapshot('Edited after rejection'));
    await first.close();
    const second = open(name);
    expect(await second.read(await second.activate('owner'), 'local')).toMatchObject({ revision: 2, status: 'failed', content: { subject: 'Edited after rejection' } });
  });
  it('commits file bytes with their references and restores them after reopening', async () => {
    const name = crypto.randomUUID(); const first = open(name); const owner = await first.activate('owner');
    const draft = snapshot();
    draft.attachments = [{ type: 'local', id: 'file', name: 'note.txt', mimeType: 'text/plain', size: 5, lastModified: 0, uploaded: false }];
    await first.save(owner, draft, new Map([['file', new NodeBlob(['hello']) as unknown as Blob]]));
    await first.close();
    const second = open(name); const session = await second.activate('owner');
    expect(await (await second.file(session, 'local', 'file'))?.text()).toBe('hello');
    await second.save(session, snapshot());
    expect(await second.file(session, 'local', 'file')).toBeUndefined();
  });
  it('allocates revisions transactionally across tabs with last-write-wins', async () => {
    const name = crypto.randomUUID(); const first = open(name); const second = open(name);
    const owner = await first.activate('owner'); await second.activate('owner');
    await Promise.all([first.save(owner, snapshot('first')), second.save(owner, snapshot('second'))]);
    expect(await first.read(owner, 'local')).toMatchObject({ revision: 2, content: { subject: 'second' } });
  });
  it('fences callbacks after discard and account changes', async () => {
    const store = open(); const owner = await store.activate('owner');
    await store.save(owner, snapshot()); await store.update(owner, 'local', () => undefined);
    await expect(store.save(owner, snapshot('late edit'))).rejects.toThrow('no longer active');
    await store.activate('another-owner');
    await expect(store.save(owner, snapshot())).rejects.toThrow('no longer active');
    expect(await store.list(owner)).toEqual([]);
  });
  it('does not notify subscribers from reads or session activation', async () => {
    const store = open(); const owner = await store.activate('owner'); let notifications = 0;
    const stop = store.subscribe(() => notifications++);
    await store.activate('owner'); await store.list(owner); await store.read(owner, 'local');
    expect(notifications).toBe(0);
    await store.save(owner, snapshot()); expect(notifications).toBe(1); stop();
  });
});
