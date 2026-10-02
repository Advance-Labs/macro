import { createEffect, createResource, createSignal, onCleanup, Show } from 'solid-js';
import type { EmailDraftStorage } from '../context/compose-capabilities';

/** Durable recovery state remains visible after remounting a failed draft. */
export function DraftSyncStatus(props: {
  drafts: EmailDraftStorage;
  draftId?: string;
  retry(): Promise<unknown>;
  discard(): Promise<unknown>;
}) {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [state, { refetch }] = createResource(() => props.drafts.readDraft && props.draftId, async (id) => await props.drafts.readDraft!(id));
  createEffect(() => {
    const unsubscribe = props.drafts.watchDrafts?.(() => { void refetch(); });
    onCleanup(() => unsubscribe?.());
  });
  const local = () => state.latest?.local;
  const failed = () => ['failed', 'unconfirmed', 'delete-failed'].includes(local()?.status ?? '');
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true); setError(undefined);
    try { await action(); await refetch(); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  return <Show when={local() && local()?.status !== 'synced'}>
    <div class="flex flex-wrap items-center gap-2 px-3 py-2 text-xs text-ink-muted" role={failed() ? 'alert' : 'status'}>
      <span>Saved on this device · {local()?.status === 'queued' ? 'Syncing' : 'Not synced'}</span>
      <Show when={failed()}>
        <span>{local()?.status === 'delete-failed' ? 'Discard failed.' : local()?.status === 'unconfirmed' ? 'The last save could not be confirmed.' : 'The server could not save this draft.'}</span>
        <button class="underline" disabled={busy()} onClick={() => void run(local()?.status === 'delete-failed' ? props.discard : props.retry)}>{local()?.status === 'delete-failed' ? 'Retry discard' : 'Retry'}</button>
        <Show when={local()?.status === 'delete-failed'}><button class="underline" disabled={busy()} onClick={() => void run(props.retry)}>Keep editing</button></Show>
        <Show when={local()?.status !== 'delete-failed'}><button class="underline" disabled={busy()} onClick={() => void run(props.discard)}>Discard</button></Show>
      </Show>
      <Show when={error()}><span role="alert">{error()}</span></Show>
    </div>
  </Show>;
}
