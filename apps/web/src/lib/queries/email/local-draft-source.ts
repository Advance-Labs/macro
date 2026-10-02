import type { LocalDraft } from '@app/features/email-compose/core/local-draft';
import type { EmailEntity } from '@entity';
import { createSignal, onCleanup } from 'solid-js';
import { authKeys } from '../auth/keys';
import { queryClient } from '../client';
import { listLocalDrafts, localDraftStore } from './local-drafts';

/** Account-scoped local data source; notifications are invalidations, never the truth. */
export function createLocalDraftSource() {
  const [drafts, setDrafts] = createSignal<LocalDraft[]>([]);
  const [ready, setReady] = createSignal(false);
  let generation = 0;
  let disposed = false;
  const refresh = async () => {
    const request = ++generation;
    try {
      const entries = await listLocalDrafts();
      if (!disposed && request === generation) setDrafts(entries.sort((a, b) => b.updatedAt - a.updatedAt));
    } catch {
      if (!disposed && request === generation) setDrafts([]);
    } finally { if (!disposed && request === generation) setReady(true); }
  };
  const unsubscribe = localDraftStore.subscribe(() => { void refresh(); });
  const auth = queryClient.getQueryCache().subscribe((event) => {
    if (event.query.queryHash === JSON.stringify(authKeys.userInfo.queryKey)) void refresh();
  });
  onCleanup(() => { disposed = true; unsubscribe(); auth(); });
  void refresh();
  return { drafts, ready };
}

/** One conversation row, even when several local reply drafts share it. */
export function localDraftEntities(drafts: readonly LocalDraft[]): EmailEntity[] {
  const rows = new Map<string, EmailEntity>();
  for (const draft of drafts) {
    if (draft.status === 'synced' || draft.status === 'deleting') continue;
    const id = draft.serverThreadId ?? draft.threadId ?? draft.key;
    if (rows.has(id)) continue;
    rows.set(id, {
      type: 'email', id, name: draft.content.subject || 'Draft email', ownerId: draft.accountId,
      isRead: true, isDraft: true, isImportant: false, isSignal: true, done: false,
      linkId: draft.inboxId, senderEmail: draft.senderEmail,
      participants: (draft.content.to ?? []).map((contact) => ({ email: contact.email, name: contact.name ?? undefined })),
      createdAt: new Date(draft.updatedAt), updatedAt: new Date(draft.updatedAt),
      snippet: draft.status === 'failed' || draft.status === 'unconfirmed' || draft.status === 'delete-failed' ? 'Saved on this device · Not synced' : 'Saved on this device',
    });
  }
  return [...rows.values()];
}
