import type { SaveEmailDraft } from '@app/features/email-compose/context/compose-capabilities';
import type { DraftAttempt, LocalDraft, LocalDraftAttachment } from '@app/features/email-compose/core/local-draft';
import type { DraftFormAttachment } from '@app/features/email-compose/primitives/email-form-state';
import type { EmailMessage } from '@app/features/email-message/core/email-message';
import { getNativeStagedUpload } from '@core/mobile/nativeStagedUpload';
import type { CacheHost } from '@graphql-cache/host/types';
import type { ClaimedMutation } from '@graphql-cache/protocol';
import type { NormalizedCacheExchangeOptions } from '@graphql-cache/exchange/normalized-cache-exchange';
import type { OperationResult } from '@urql/core';
import type { UserInfoData } from '../auth/user-info';
import { authKeys } from '../auth/keys';
import { queryClient } from '../client';
import { createLocalDraftStore } from './local-draft-store';

export const localDraftStore = createLocalDraftStore();
const fileIds = new WeakMap<File, string>();
const fileCopies = new WeakMap<File, Promise<Blob>>();
const pendingWrites = new Set<Promise<LocalDraft>>();
const EPOCH_KEY = 'email-working-copies:epoch';

function storageEpoch() {
  let epoch = localStorage.getItem(EPOCH_KEY);
  if (!epoch) { epoch = crypto.randomUUID(); localStorage.setItem(EPOCH_KEY, epoch); }
  return epoch;
}

export async function flushLocalDrafts(): Promise<void> {
  while (pendingWrites.size) await Promise.all([...pendingWrites]);
}

export async function clearLocalDrafts(): Promise<void> {
  // Quarantine before clearing so a failed disk wipe cannot expose old drafts.
  localStorage.setItem(EPOCH_KEY, crypto.randomUUID());
  try { await localDraftStore.clear(); } catch { /* A new epoch cannot read the old session. */ }
}

async function session() {
  const user = queryClient.getQueryData<UserInfoData>(authKeys.userInfo.queryKey);
  if (!user?.authenticated || !user.id) throw new Error('Sign in to save drafts on this device');
  return await localDraftStore.activate(user.id, storageEpoch());
}

export async function listLocalDrafts(): Promise<LocalDraft[]> {
  const owner = await session();
  return await localDraftStore.list(owner);
}
export async function readLocalDraft(id: string): Promise<LocalDraft | undefined> {
  return await localDraftStore.read(await session(), id);
}

/** Local body HTML uses the same base64 representation as the editor's save input. */
export function localDraftMessage(local: LocalDraft): EmailMessage {
  const content = local.content;
  const time = new Date(local.updatedAt).toISOString();
  return {
    ...content, db_id: local.serverDraftId ?? local.draftId,
    thread_db_id: local.serverThreadId ?? local.threadId ?? local.draftId,
    link_id: local.inboxId ?? '', is_draft: true,
    created_at: time, updated_at: time, body_html_sanitized: content.body_html,
    to: content.to ?? [], cc: content.cc ?? [], bcc: content.bcc ?? [],
    from: { email: local.senderEmail ?? '' }, labels: [], attachments: [],
    attachments_draft: local.attachments.flatMap((attachment) => attachment.type === 'remote' ? [{
      id: attachment.attachmentId, file_name: attachment.fileName, content_type: attachment.contentType, size: attachment.fileSize, s3_key: '',
    }] : []),
    attachments_forwarded: local.attachments.flatMap((attachment) => attachment.type === 'forwarded' ? [{ attachment_id: attachment.attachmentId, filename: attachment.fileName, mime_type: attachment.mimeType, size_bytes: attachment.fileSize }] : []),
  };
}

async function copyFile(file: File): Promise<Blob> {
  let pending = fileCopies.get(file);
  if (!pending) {
    pending = (async () => {
      const staged = getNativeStagedUpload(file);
      if (!staged) return file;
      if (!staged.previewSrc) throw new Error('Attachment bytes are unavailable on this device');
      const response = await fetch(staged.previewSrc);
      if (!response.ok) throw new Error('Unable to preserve attachment on this device');
      const blob = await response.blob();
      if (blob.size !== staged.size) throw new Error('Attachment size mismatch');
      return blob;
    })();
    fileCopies.set(file, pending);
  }
  try { return await pending; } catch (error) { fileCopies.delete(file); throw error; }
}

export type LocalDraftInput = SaveEmailDraft & { attachments: readonly DraftFormAttachment[]; senderEmail?: string };

/** Called on every edit, including edits made while server autosave is paused. */
export function saveLocalDraft(input: LocalDraftInput): Promise<LocalDraft> {
  const write = persistLocalDraft(input);
  pendingWrites.add(write);
  void write.then(() => pendingWrites.delete(write), () => pendingWrites.delete(write));
  return write;
}

async function persistLocalDraft(input: LocalDraftInput): Promise<LocalDraft> {
  const owner = await session();
  const id = input.clientHandles?.draftId ?? input.draft.db_id;
  if (!id) throw new Error('Draft identity must be minted before local persistence');
  const previous = await localDraftStore.read(owner, id);
  const key = previous?.key ?? id;
  const files = new Map<string, Blob>();
  const attachments: LocalDraftAttachment[] = [];
  for (const attachment of input.attachments) {
    if (attachment.type !== 'local') { attachments.push({ ...attachment }); continue; }
    let fileId = fileIds.get(attachment.file);
    if (!fileId) { fileId = crypto.randomUUID(); fileIds.set(attachment.file, fileId); }
    const blob = await copyFile(attachment.file);
    files.set(fileId, blob);
    const old = previous?.attachments.find((entry) => entry.type === 'local' && entry.id === fileId);
    attachments.push({ type: 'local', id: fileId, name: attachment.file.name, mimeType: blob.type || attachment.file.type, size: blob.size, lastModified: attachment.file.lastModified, attachmentId: attachment.attachmentId, uploaded: old?.type === 'local' && old.attachmentId === attachment.attachmentId ? old.uploaded : false });
  }
  return await localDraftStore.save(owner, {
    key, accountId: owner.accountId, generation: previous?.generation ?? id,
    draftId: previous?.draftId ?? id,
    threadId: previous?.threadId ?? input.clientHandles?.threadId ?? input.draft.thread_db_id ?? undefined,
    serverDraftId: previous?.serverDraftId ?? (input.clientHandles ? undefined : input.draft.db_id ?? undefined),
    serverThreadId: previous?.serverThreadId,
    inboxId: input.inboxId, senderEmail: input.senderEmail,
    content: structuredClone(input.draft), attachments, status: 'dirty',
  }, files);
}

export async function restoreLocalAttachments(local: LocalDraft): Promise<DraftFormAttachment[]> {
  const owner = await session();
  const attachments: DraftFormAttachment[] = [];
  for (const attachment of local.attachments) {
    if (attachment.type !== 'local') { attachments.push(attachment); continue; }
    const blob = await localDraftStore.file(owner, local.key, attachment.id);
    if (!blob) throw new Error(`The locally saved attachment ${attachment.name} is unavailable`);
    const file = new File([blob], attachment.name, { type: attachment.mimeType, lastModified: attachment.lastModified });
    fileIds.set(file, attachment.id);
    attachments.push({ type: 'local', file, attachmentId: attachment.attachmentId, uploadPending: !attachment.uploaded && !!attachment.attachmentId });
  }
  return attachments;
}

export function draftSyncPaused(local: LocalDraft): boolean {
  return local.status === 'failed' || local.status === 'unconfirmed' || local.status === 'delete-failed';
}

export async function resumeLocalDraft(id: string): Promise<void> {
  const owner = await session();
  const draft = await localDraftStore.read(owner, id);
  if (!draft) return;
  await localDraftStore.update(owner, draft.key, (current) => ({ ...current, status: 'dirty', errorCode: undefined }));
}

export async function forgetLocalDraft(id: string): Promise<void> {
  const owner = await session();
  const draft = await localDraftStore.read(owner, id);
  if (draft) await localDraftStore.update(owner, draft.key, () => undefined);
}

/** A record ID is not an upload receipt; persist both stages independently. */
export async function recordLocalAttachment(draftId: string, file: File, attachmentId: string | undefined, uploaded: boolean): Promise<void> {
  const owner = await session();
  const draft = await localDraftStore.read(owner, draftId);
  const id = fileIds.get(file);
  if (!draft || !id) return;
  await localDraftStore.update(owner, draft.key, (current) => {
    if (current.generation !== draft.generation) return current;
    const attachments = current.attachments.map((attachment) => attachment.type === 'local' && attachment.id === id ? { ...attachment, attachmentId, uploaded } : attachment);
    return { ...current, attachments, status: !draftSyncPaused(current) && current.status !== 'deleting' && current.acknowledgedRevision === current.revision && !attachments.some((attachment) => attachment.type === 'local' && !attachment.uploaded) ? 'synced' : current.status };
  });
}

export async function beginDraftAttempt(local: LocalDraft, operation: 'save' | 'delete'): Promise<DraftAttempt> {
  const owner = await session();
  const attempt: DraftAttempt = { kind: 'email-draft', id: crypto.randomUUID(), draftKey: local.key, accountId: owner.accountId, generation: local.generation, revision: local.revision, operation };
  await localDraftStore.recordAttempt(owner, attempt);
  await localDraftStore.update(owner, local.key, (current) => current.generation !== local.generation || (operation === 'save' && draftSyncPaused(current)) ? current : { ...current, status: operation === 'delete' ? 'deleting' : 'queued' });
  return attempt;
}

function attemptOf(mutation: Pick<ClaimedMutation, 'clientMetadata'>): DraftAttempt | undefined {
  const value = mutation.clientMetadata;
  if (!value || value.kind !== 'email-draft' || typeof value.id !== 'string' || typeof value.draftKey !== 'string' || typeof value.accountId !== 'string' || typeof value.generation !== 'string' || typeof value.revision !== 'number' || (value.operation !== 'save' && value.operation !== 'delete')) return;
  return value as DraftAttempt;
}

/** Replay bookkeeping runs even when no composer is mounted. */
export function localDraftQueueLifecycle(host: CacheHost): Pick<NormalizedCacheExchangeOptions, 'prepareMutationQueue' | 'beforeMutationAttempt' | 'onMutationAttemptResult'> {
  let initialized: Promise<void> | undefined;
  const legacy = new Map<string, DraftAttempt>();
  async function prepare() {
    const owner = await session();
    if (!host.inspectMutations) throw new Error('Draft recovery requires an updated cache runtime');
    const queue = await host.inspectMutations();
    const storageGeneration = await host.currentStorageGeneration();
    const imported = new Set<string>();
    const previousAttempts = await localDraftStore.attempts(owner);
    for (const mutation of queue) {
      if (attemptOf(mutation)) continue;
      if (mutation.operationName !== 'SaveEmailDraft' && mutation.operationName !== 'DeleteEmailDraft') continue;
      const input = mutation.variables.input as Record<string, unknown>;
      if (typeof input?.draftId !== 'string') continue;
      let draft = await localDraftStore.read(owner, input.draftId);
      if (mutation.operationName === 'SaveEmailDraft' && (!draft || imported.has(input.draftId))) {
        imported.add(input.draftId);
        const contact = (value: unknown) => Array.isArray(value) ? value.map((item: { email: string; name?: string; photoUrl?: string }) => ({ email: item.email, name: item.name, photo_url: item.photoUrl })) : [];
        draft = await localDraftStore.save(owner, {
          key: input.draftId, accountId: owner.accountId, generation: draft?.generation ?? crypto.randomUUID(), draftId: input.draftId,
          threadId: typeof input.threadDbId === 'string' ? input.threadDbId : undefined,
          inboxId: typeof input.linkId === 'string' ? input.linkId : undefined,
          content: { subject: String(input.subject ?? ''), body_html: input.bodyHtml as string | undefined, body_text: input.bodyText as string | undefined, body_macro: input.bodyMacro as string | undefined, replying_to_id: input.replyingToId as string | undefined, to: contact(input.to), cc: contact(input.cc), bcc: contact(input.bcc) },
          attachments: [], status: 'queued',
        });
      }
      if (!draft) continue;
      const attempt = previousAttempts.find((attempt) => attempt.transactionId === mutation.transactionId && attempt.storageGeneration === storageGeneration) ?? await beginDraftAttempt(draft, mutation.operationName === 'DeleteEmailDraft' ? 'delete' : 'save');
      legacy.set(mutation.transactionId, attempt);
      await localDraftStore.recordAttempt(owner, { ...attempt, transactionId: mutation.transactionId, storageGeneration });
    }
    const activeDraftKeys = new Set(queue.map((mutation) => (attemptOf(mutation) ?? legacy.get(mutation.transactionId))?.draftKey));
    for (const draft of await localDraftStore.list(owner)) {
      if (!activeDraftKeys.has(draft.key) && ['dirty', 'queued', 'deleting'].includes(draft.status)) await localDraftStore.update(owner, draft.key, (current) => ({ ...current, status: current.status === 'deleting' ? 'delete-failed' : 'unconfirmed' }));
    }
    const queuedAttempts = new Set(queue.map((mutation) => attemptOf(mutation)?.id ?? legacy.get(mutation.transactionId)?.id));
    for (const attempt of await localDraftStore.attempts(owner)) {
      if (attempt.settled || queuedAttempts.has(attempt.id)) continue;
      await localDraftStore.update(owner, attempt.draftKey, (draft) => draft.generation === attempt.generation && draft.revision === attempt.revision && draft.status === 'queued' ? { ...draft, status: 'unconfirmed' } : draft);
    }
  }
  return {
    async prepareMutationQueue() {
      try { await (initialized ??= prepare()); } catch (error) { initialized = undefined; throw error; }
    },
    async beforeMutationAttempt(mutation) {
      const attempt = attemptOf(mutation) ?? legacy.get(mutation.transactionId);
      if (!attempt) return true;
      const owner = await session();
      if (owner.accountId !== attempt.accountId) return false;
      const local = await localDraftStore.read(owner, attempt.draftKey);
      if (!local || local.generation !== attempt.generation) return false;
      await localDraftStore.recordAttempt(owner, { ...attempt, transactionId: mutation.transactionId, storageGeneration: await host.currentStorageGeneration() });
      return attempt.operation === 'delete' || (!draftSyncPaused(local) && local.status !== 'deleting');
    },
    async onMutationAttemptResult(mutation, result, retry) {
      const attempt = attemptOf(mutation) ?? legacy.get(mutation.transactionId);
      if (!attempt || retry) return;
      await settleDraftAttempt(attempt, result, mutation.superseded);
    },
  };
}

async function settleDraftAttempt(attempt: DraftAttempt, result: OperationResult, superseded: boolean) {
  const owner = await session();
  if (owner.accountId !== attempt.accountId) return;
  const code = result.error?.graphQLErrors[0]?.extensions.code;
  const failed = !!result.error || result.data == null;
  const payload = result.data?.saveEmailDraft;
  await localDraftStore.update(owner, attempt.draftKey, (draft) => {
    if (draft.generation !== attempt.generation) return draft;
    if (code === 'DRAFT_ALREADY_SENT' || (!failed && attempt.operation === 'delete')) return undefined;
    if (failed) return superseded ? draft : { ...draft, status: attempt.operation === 'delete' ? 'delete-failed' : 'failed', errorCode: typeof code === 'string' ? code : 'INTERNAL' };
    const acknowledgedRevision = Math.max(draft.acknowledgedRevision, attempt.revision);
    return { ...draft, acknowledgedRevision, serverDraftId: payload?.draftId ?? draft.serverDraftId, serverThreadId: payload?.thread?.id ?? draft.serverThreadId,
      status: draft.revision === attempt.revision ? (draft.attachments.some((a) => a.type === 'local' && !a.uploaded) ? 'dirty' : 'synced') : draft.status,
    };
  });
  await localDraftStore.recordAttempt(owner, { ...attempt, settled: true });
}
