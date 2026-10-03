import { createSignal, onMount } from 'solid-js';
import type { LegalSource } from '../context/legal-source';
import {
  type Draft,
  type Envelope,
  type Field,
  type FieldKind,
  type Recipient,
  sendIssue,
} from '../core/models';

export function createWorkspace(source: LegalSource) {
  const [envelopes, setEnvelopes] = createSignal<Envelope[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal('');
  const [active, setActive] = createSignal<Envelope>();
  const [draft, setDraft] = createSignal<Draft>();
  const [bytes, setBytes] = createSignal<Uint8Array>();
  const [creating, setCreating] = createSignal(false);
  const [step, setStep] = createSignal(0);
  const remember = (envelope: Envelope) => {
    setActive(envelope);
    setEnvelopes((items) => [
      envelope,
      ...items.filter((e) => e.id !== envelope.id),
    ]);
  };
  async function run<T>(task: () => Promise<T>) {
    if (busy()) return;
    setBusy(true);
    setError('');
    try {
      return await task();
    } catch (e) {
      setError(
        e instanceof Error ? e.message : 'Something went wrong. Try again.'
      );
    } finally {
      setBusy(false);
    }
  }
  async function refresh() {
    setLoading(true);
    await run(async () => setEnvelopes(await source.list()));
    setLoading(false);
  }
  onMount(refresh);
  function close() {
    setActive(undefined);
    setDraft(undefined);
    setBytes(undefined);
    setCreating(false);
    setError('');
    setStep(0);
  }
  function start() {
    close();
    setCreating(true);
  }
  async function open(envelope: Envelope) {
    await run(async () => {
      const file = await source.document(envelope.id);
      remember(envelope);
      setBytes(file);
      setCreating(envelope.status === 'draft');
      setDraft(
        envelope.status === 'draft'
          ? {
              title: envelope.title,
              message: envelope.message,
              revision: envelope.revision,
              recipients: envelope.recipients,
              fields: envelope.fields,
            }
          : undefined
      );
      setStep(1);
    });
  }
  async function upload(file: File) {
    await run(async () => {
      const envelope = await source.create(
        file,
        file.name.replace(/\.pdf$/i, '')
      );
      remember(envelope);
      setBytes(new Uint8Array(await file.arrayBuffer()));
      setDraft({
        title: envelope.title,
        message: '',
        revision: envelope.revision,
        recipients: [],
        fields: [],
      });
      setStep(1);
    });
  }
  function patch(patch: Partial<Draft>) {
    setDraft((d) => (d ? { ...d, ...patch } : d));
  }
  function addRecipient() {
    const d = draft();
    if (!d || d.recipients.length >= 20) return;
    patch({
      recipients: [
        ...d.recipients,
        {
          id: crypto.randomUUID(),
          name: '',
          email: '',
          order: d.recipients.length + 1,
          signedAt: null,
          deliveredAt: null,
        },
      ],
    });
  }
  function editRecipient(id: string, update: Partial<Recipient>) {
    const d = draft();
    if (d)
      patch({
        recipients: d.recipients.map((r) =>
          r.id === id ? { ...r, ...update } : r
        ),
      });
  }
  function removeRecipient(id: string) {
    const d = draft();
    if (d)
      patch({
        recipients: d.recipients.filter((r) => r.id !== id),
        fields: d.fields.filter((f) => f.recipientId !== id),
      });
  }
  function place(
    kind: FieldKind,
    recipientId: string,
    page: number,
    x: number,
    y: number
  ) {
    const d = draft();
    if (!d || !recipientId || d.fields.length >= 200) return;
    const width = kind === 'signature' ? 0.27 : 0.2;
    const height = 0.045;
    patch({
      fields: [
        ...d.fields,
        {
          id: crypto.randomUUID(),
          kind,
          recipientId,
          page,
          x: Math.min(Math.max(0, x), 1 - width),
          y: Math.min(Math.max(0, y), 1 - height),
          width,
          height,
          required: true,
          value: null,
        },
      ],
    });
  }
  function editField(id: string, update: Partial<Field>) {
    const d = draft();
    if (d)
      patch({
        fields: d.fields.map((f) => (f.id === id ? { ...f, ...update } : f)),
      });
  }
  function removeField(id: string) {
    const d = draft();
    if (d) patch({ fields: d.fields.filter((f) => f.id !== id) });
  }
  async function save() {
    const d = draft();
    const a = active();
    if (!d || !a) return;
    return run(async () => {
      const envelope = await source.update(a.id, d);
      remember(envelope);
      patch({ revision: envelope.revision });
      return envelope;
    });
  }
  async function send() {
    const d = draft();
    const a = active();
    if (!d || !a) return;
    const issue = sendIssue(d);
    if (issue) {
      setError(issue);
      return;
    }
    await run(async () => {
      const saved = await source.update(a.id, d);
      remember(saved);
      patch({ revision: saved.revision });
      const sent = await source.send(a.id, saved.revision);
      remember(sent);
      setDraft(undefined);
      setCreating(false);
    });
  }
  async function resend() {
    const a = active();
    if (a) await run(async () => remember(await source.resend(a.id)));
  }
  async function voidEnvelope(reason: string) {
    const a = active();
    if (a)
      await run(async () =>
        remember(await source.void(a.id, a.revision, reason))
      );
  }
  async function download(completed = true) {
    const a = active();
    if (!a) return;
    await run(async () => {
      const data = await source.document(a.id, completed);
      const url = URL.createObjectURL(
        new Blob([data as Uint8Array<ArrayBuffer>], { type: 'application/pdf' })
      );
      const link = document.createElement('a');
      link.href = url;
      link.download = `${a.title}${completed ? ' - signed' : ''}.pdf`;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }
  return {
    envelopes,
    loading,
    busy,
    error,
    active,
    draft,
    bytes,
    creating,
    step,
    setStep,
    refresh,
    close,
    start,
    open,
    upload,
    patch,
    addRecipient,
    editRecipient,
    removeRecipient,
    place,
    editField,
    removeField,
    save,
    send,
    resend,
    voidEnvelope,
    download,
  };
}
export type Workspace = ReturnType<typeof createWorkspace>;
