import { createSignal, onMount } from 'solid-js';
import type { SigningSource } from '../context/legal-source';
import { type SigningSession, signingIssue } from '../core/models';
export function createSigning(source: SigningSource) {
  const [session, setSession] = createSignal<SigningSession>();
  const [bytes, setBytes] = createSignal<Uint8Array>();
  const [values, setValues] = createSignal<Record<string, string>>({});
  const [consent, setConsent] = createSignal(false);
  const [reviewing, setReviewing] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal('');
  const [declined, setDeclined] = createSignal(false);
  async function run(task: () => Promise<unknown>) {
    if (busy()) return;
    setBusy(true);
    setError('');
    try {
      await task();
    } catch (e) {
      setError(
        e instanceof Error ? e.message : 'Something went wrong. Try again.'
      );
    } finally {
      setBusy(false);
    }
  }
  async function load() {
    await run(async () => {
      setSession(await source.session());
      setBytes(await source.document());
    });
  }
  onMount(load);
  const fill = (id: string, value: string) =>
    setValues((previous) => ({ ...previous, [id]: value }));
  async function finish() {
    const current = session();
    if (!current) return;
    const issue = signingIssue(current, values(), consent());
    if (issue) {
      setError(issue);
      return;
    }
    await run(async () =>
      setSession(
        await source.sign({
          revision: current.revision,
          consent: consent(),
          values: current.fields
            .filter((f) => f.kind !== 'date')
            .map((f) => ({ fieldId: f.id, value: values()[f.id] || '' })),
        })
      )
    );
  }
  async function decline(reason: string) {
    const current = session();
    if (current)
      await run(async () => {
        await source.decline(current.revision, reason);
        setDeclined(true);
      });
  }
  async function download() {
    await run(async () => {
      const data = await source.document(true);
      const url = URL.createObjectURL(
        new Blob([data as Uint8Array<ArrayBuffer>], { type: 'application/pdf' })
      );
      const link = document.createElement('a');
      link.href = url;
      link.download = `${session()?.title || 'Agreement'} - signed.pdf`;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }
  const completedFields = () =>
    session()?.fields.filter((f) => f.kind === 'date' || values()[f.id]?.trim())
      .length || 0;
  return {
    session,
    bytes,
    values,
    consent,
    setConsent,
    reviewing,
    setReviewing,
    busy,
    error,
    declined,
    fill,
    finish,
    decline,
    download,
    load,
    completedFields,
  };
}
