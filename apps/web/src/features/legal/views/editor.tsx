import ArrowLeft from '@phosphor/arrow-left.svg';
import ArrowRight from '@phosphor/arrow-right.svg';
import Check from '@phosphor/check.svg';
import FileText from '@phosphor/file-text.svg';
import PaperPlane from '@phosphor/paper-plane-tilt.svg';
import Plus from '@phosphor/plus.svg';
import Trash from '@phosphor/trash.svg';
import Upload from '@phosphor/upload-simple.svg';
import { Button } from '@ui';
import { createSignal, For, Index, Match, Show, Switch } from 'solid-js';
import { PdfDocument } from '../components/pdf-document';
import { type FieldKind, fieldLabels, sendIssue } from '../core/models';
import type { Workspace } from '../primitives/workspace';

export function Editor(props: { workspace: Workspace }) {
  const w = props.workspace;
  const [page, setPage] = createSignal(1);
  const [recipientId, setRecipientId] = createSignal('');
  const [kind, setKind] = createSignal<FieldKind>('signature');
  const [selected, setSelected] = createSignal('');
  const [dragOver, setDragOver] = createSignal(false);
  const [localError, setLocalError] = createSignal('');
  const currentRecipient = () =>
    recipientId() || w.draft()?.recipients[0]?.id || '';
  const selectedField = () =>
    w.draft()?.fields.find((f) => f.id === selected());
  const recipientIssue = () => {
    const d = w.draft();
    if (!d?.recipients.length) return 'Add at least one recipient.';
    if (
      d.recipients.some(
        (r) => !r.name.trim() || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(r.email)
      )
    )
      return 'Add a name and valid email for every recipient.';
    if (
      new Set(d.recipients.map((r) => r.email.toLowerCase())).size !==
      d.recipients.length
    )
      return 'Each signer needs a unique email.';
    return '';
  };
  function next() {
    setLocalError('');
    if (w.step() === 1 && recipientIssue()) {
      setLocalError(recipientIssue());
      return;
    }
    if (w.step() === 2) {
      const d = w.draft();
      const issue = d ? sendIssue(d) : 'Upload a PDF first.';
      if (issue) {
        setLocalError(issue);
        return;
      }
    }
    w.setStep(w.step() + 1);
  }
  async function saveClose() {
    if (!w.active()) {
      w.close();
      return;
    }
    const saved = await w.save();
    if (saved) w.close();
  }
  function accept(file?: File) {
    if (file) {
      setLocalError('');
      void w.upload(file);
    }
  }
  return (
    <div class="flex flex-col h-full bg-surface text-ink">
      <header class="flex items-center justify-between border-b border-edge-muted px-6 py-4 shrink-0 gap-4">
        <div class="flex items-center gap-3">
          <Button
            size="icon-md"
            aria-label="Save and return to Legal"
            onClick={saveClose}
            disabled={w.busy()}
          >
            <ArrowLeft class="size-4" />
          </Button>
          <div>
            <div class="text-xs text-ink-muted">Legal / New envelope</div>
            <h1 class="text-lg font-semibold">Prepare for signature</h1>
          </div>
        </div>
        <Button variant="strong" onClick={saveClose} disabled={w.busy()}>
          {w.busy() ? 'Saving…' : w.active() ? 'Save & close' : 'Cancel'}
        </Button>
      </header>
      <nav
        class="border-b border-edge-muted flex justify-center gap-3 sm:gap-8 py-4 shrink-0"
        aria-label="Envelope preparation steps"
      >
        <For each={['Document', 'Recipients', 'Fields', 'Review']}>
          {(label, index) => (
            <div
              class="flex items-center gap-2 text-sm"
              classList={{
                'text-accent font-medium': w.step() === index(),
                'text-ink-muted': w.step() !== index(),
              }}
            >
              <span
                class="size-6 rounded-full border flex items-center justify-center text-xs"
                classList={{
                  'bg-accent text-accent-contrast border-accent':
                    w.step() >= index(),
                  'border-edge-muted': w.step() < index(),
                }}
              >
                {w.step() > index() ? <Check class="size-3" /> : index() + 1}
              </span>
              <span class="hidden sm:inline">{label}</span>
            </div>
          )}
        </For>
      </nav>
      <Show when={localError()}>
        <div
          role="alert"
          class="mx-6 mt-3 p-3 rounded-lg bg-failure/10 text-failure text-sm"
        >
          {localError()}
        </div>
      </Show>
      <div class="flex-1 min-h-0 overflow-auto">
        <Switch>
          <Match when={w.step() === 0}>
            <div class="max-w-2xl mx-auto p-8 sm:p-12">
              <h2 class="text-2xl font-semibold mb-2">
                Start with your document
              </h2>
              <p class="text-ink-muted text-sm mb-8">
                Upload the agreement you want signed. We’ll keep a secure
                snapshot of the exact document.
              </p>
              <label
                class="flex flex-col items-center justify-center border-2 border-dashed border-edge-muted rounded-2xl p-12 text-center bg-ink/2 hover:bg-ink/4 transition-colors"
                classList={{ 'border-accent bg-accent-bg': dragOver() }}
                onDragOver={(event) => {
                  event.preventDefault();
                  setDragOver(true);
                }}
                onDragLeave={() => setDragOver(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setDragOver(false);
                  accept(event.dataTransfer?.files[0]);
                }}
              >
                <div class="rounded-full bg-accent-bg text-accent p-4 mb-5">
                  <Upload class="size-8" />
                </div>
                <span class="font-semibold">Drop your PDF here</span>
                <span class="text-sm text-ink-muted mt-2">
                  or choose a file from your computer
                </span>
                <span class="mt-6 px-5 py-2 rounded-full border border-edge-button text-sm font-medium">
                  Choose PDF
                </span>
                <input
                  type="file"
                  accept="application/pdf,.pdf"
                  aria-label="Upload agreement PDF"
                  class="sr-only"
                  disabled={w.busy()}
                  onChange={(event) => accept(event.currentTarget.files?.[0])}
                />
                <span class="text-xs text-ink-muted mt-5">
                  PDF · Up to 10 MB · Up to 100 pages
                </span>
              </label>
              <p class="text-xs text-ink-muted mt-6 leading-relaxed">
                Use a static PDF without passwords or existing digital
                signatures. Your document is frozen when you send the envelope.
              </p>
            </div>
          </Match>
          <Match when={w.step() === 1}>
            <div class="max-w-3xl mx-auto p-8">
              <h2 class="text-2xl font-semibold">Who needs to sign?</h2>
              <p class="text-sm text-ink-muted mt-2 mb-6">
                Each recipient gets a private invitation and only fills their
                assigned fields.
              </p>
              <Index each={w.draft()?.recipients}>
                {(recipient, index) => (
                  <div class="p-5 rounded-xl border border-edge-muted mb-4">
                    <div class="flex items-center justify-between mb-4">
                      <div class="font-medium text-sm">
                        Recipient {index + 1}
                      </div>
                      <Button
                        size="icon-sm"
                        aria-label={`Remove recipient ${index + 1}`}
                        onClick={() => w.removeRecipient(recipient().id)}
                      >
                        <Trash class="size-4" />
                      </Button>
                    </div>
                    <div class="grid sm:grid-cols-2 gap-4">
                      <label class="text-xs text-ink-muted">
                        Full name
                        <input
                          aria-label={`Recipient ${index + 1} name`}
                          class="block mt-1.5 w-full rounded-lg p-2.5 border border-edge-muted bg-input text-ink text-sm"
                          placeholder="Alex Morgan"
                          value={recipient().name}
                          onInput={(e) =>
                            w.editRecipient(recipient().id, {
                              name: e.currentTarget.value,
                            })
                          }
                        />
                      </label>
                      <label class="text-xs text-ink-muted">
                        Email address
                        <input
                          type="email"
                          aria-label={`Recipient ${index + 1} email`}
                          class="block mt-1.5 w-full rounded-lg p-2.5 border border-edge-muted bg-input text-ink text-sm"
                          placeholder="alex@company.com"
                          value={recipient().email}
                          onInput={(e) =>
                            w.editRecipient(recipient().id, {
                              email: e.currentTarget.value,
                            })
                          }
                        />
                      </label>
                    </div>
                    <div class="flex gap-3 items-center mt-4 text-xs text-ink-muted">
                      <label>
                        Signing order{' '}
                        <select
                          aria-label={`Recipient ${index + 1} signing order`}
                          class="ml-2 rounded border border-edge-muted bg-input text-ink p-1.5"
                          value={recipient().order}
                          onChange={(e) =>
                            w.editRecipient(recipient().id, {
                              order: Number(e.currentTarget.value),
                            })
                          }
                        >
                          <For
                            each={Array.from({ length: 20 }, (_, n) => n + 1)}
                          >
                            {(order) => <option value={order}>{order}</option>}
                          </For>
                        </select>
                      </label>
                      <span>Same order = sign together</span>
                    </div>
                  </div>
                )}
              </Index>
              <Button
                variant="strong"
                onClick={w.addRecipient}
                disabled={(w.draft()?.recipients.length || 0) >= 20}
              >
                <Plus class="size-4" /> Add recipient
              </Button>
              <div class="mt-8 p-4 bg-ink/3 rounded-lg text-xs text-ink-muted leading-relaxed">
                Recipients sign in order, starting with the lowest number. We
                invite the next group when earlier recipients finish.
              </div>
            </div>
          </Match>
          <Match when={w.step() === 2}>
            <div class="flex flex-col lg:flex-row min-h-full">
              <aside class="w-full lg:w-72 shrink-0 border-b lg:border-r border-edge-muted p-6 space-y-6">
                <div>
                  <h2 class="font-semibold text-lg">Add signing fields</h2>
                  <p class="text-xs text-ink-muted mt-2">
                    Choose a signer and field, then click the document to place
                    it. Drag a field to reposition it.
                  </p>
                </div>
                <label class="block text-xs font-medium text-ink-muted">
                  Fields assigned to
                  <select
                    aria-label="Assign fields to recipient"
                    class="block w-full mt-2 bg-input border border-edge-muted text-ink rounded-lg p-2 text-sm"
                    value={currentRecipient()}
                    onChange={(e) => {
                      setRecipientId(e.currentTarget.value);
                      setSelected('');
                    }}
                  >
                    <For each={w.draft()?.recipients}>
                      {(r) => <option value={r.id}>{r.name}</option>}
                    </For>
                  </select>
                </label>
                <div class="grid grid-cols-2 gap-2">
                  <For
                    each={['signature', 'initials', 'date', 'text'] as const}
                  >
                    {(fieldKind) => (
                      <button
                        type="button"
                        aria-pressed={kind() === fieldKind}
                        class="border rounded-lg py-3 px-2 text-sm text-left"
                        classList={{
                          'border-accent bg-accent-bg text-accent':
                            kind() === fieldKind,
                          'border-edge-muted': kind() !== fieldKind,
                        }}
                        onClick={() => setKind(fieldKind)}
                      >
                        {fieldLabels[fieldKind]}
                      </button>
                    )}
                  </For>
                </div>
                <div class="border-t border-edge-muted pt-5">
                  <div class="text-xs text-ink-muted mb-2">
                    {w.draft()?.fields.length || 0} fields placed
                  </div>
                  <For each={w.draft()?.fields}>
                    {(field) => (
                      <button
                        type="button"
                        class="flex justify-between text-xs w-full text-left py-2"
                        onClick={() => {
                          setSelected(field.id);
                          setPage(field.page);
                        }}
                      >
                        <span>
                          {fieldLabels[field.kind]} ·{' '}
                          {
                            w
                              .draft()
                              ?.recipients.find(
                                (r) => r.id === field.recipientId
                              )?.name
                          }
                        </span>
                        <span class="text-ink-muted">p. {field.page}</span>
                      </button>
                    )}
                  </For>
                </div>
                <Show when={selectedField()}>
                  {(field) => (
                    <div class="rounded-lg border border-edge-muted p-3 space-y-3">
                      <div class="text-xs font-medium">
                        Selected: {fieldLabels[field().kind]}
                      </div>
                      <label class="flex gap-2 text-xs">
                        <input
                          type="checkbox"
                          checked={field().required}
                          disabled={field().kind === 'signature'}
                          onChange={(e) =>
                            w.editField(field().id, {
                              required: e.currentTarget.checked,
                            })
                          }
                        />
                        Required field
                      </label>
                      <label class="block text-xs">
                        Width
                        <input
                          aria-label="Field width"
                          type="range"
                          min="0.05"
                          max={1 - field().x}
                          step="0.01"
                          value={field().width}
                          onInput={(e) =>
                            w.editField(field().id, {
                              width: Number(e.currentTarget.value),
                            })
                          }
                          class="w-full"
                        />
                      </label>
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => {
                          w.removeField(field().id);
                          setSelected('');
                        }}
                      >
                        <Trash class="size-3" /> Remove field
                      </Button>
                    </div>
                  )}
                </Show>
              </aside>
              <div class="flex-1 bg-ink/4 p-6 min-w-0">
                <div class="flex justify-center items-center gap-4 mb-5 text-sm">
                  <Button
                    size="sm"
                    disabled={page() <= 1}
                    onClick={() => setPage((p) => p - 1)}
                  >
                    Previous
                  </Button>
                  <span>
                    Page {page()} of {w.active()?.pageCount}
                  </span>
                  <Button
                    size="sm"
                    disabled={page() >= (w.active()?.pageCount || 1)}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    Next page
                  </Button>
                </div>
                <Show when={w.bytes()}>
                  {(bytes) => (
                    <PdfDocument
                      bytes={bytes()}
                      page={page()}
                      fields={w.draft()?.fields || []}
                      mode="place"
                      selectedId={selected()}
                      onPlace={(x, y) =>
                        w.place(kind(), currentRecipient(), page(), x, y)
                      }
                      onSelect={(field) => setSelected(field.id)}
                      onMove={(id, x, y) => w.editField(id, { x, y })}
                    />
                  )}
                </Show>
              </div>
            </div>
          </Match>
          <Match when={w.step() === 3}>
            <div class="max-w-3xl mx-auto p-8">
              <h2 class="text-2xl font-semibold">Ready to send?</h2>
              <p class="text-sm text-ink-muted mt-2 mb-8">
                Review the details and add a personal note.
              </p>
              <div class="p-4 flex gap-3 items-center border border-edge-muted rounded-xl mb-6">
                <FileText class="size-7 text-ink-muted" />
                <div>
                  <div class="text-sm font-medium">{w.active()?.filename}</div>
                  <div class="text-xs text-ink-muted mt-1">
                    {w.active()?.pageCount} pages · {w.draft()?.fields.length}{' '}
                    signing fields
                  </div>
                </div>
              </div>
              <label class="text-sm font-medium block mb-5">
                Email subject
                <input
                  aria-label="Envelope subject"
                  class="block mt-2 w-full rounded-lg p-3 border border-edge-muted bg-input text-ink font-normal"
                  value={w.draft()?.title || ''}
                  maxLength={200}
                  onInput={(e) => w.patch({ title: e.currentTarget.value })}
                />
              </label>
              <label class="text-sm font-medium block mb-6">
                Message to recipients{' '}
                <span class="text-ink-muted font-normal">(optional)</span>
                <textarea
                  aria-label="Message to recipients"
                  class="block mt-2 w-full rounded-lg p-3 border border-edge-muted bg-input text-ink font-normal text-sm"
                  rows={4}
                  placeholder="Hi, please review and sign the attached agreement. Thank you!"
                  maxLength={5000}
                  value={w.draft()?.message || ''}
                  onInput={(e) => w.patch({ message: e.currentTarget.value })}
                />
              </label>
              <div class="border-t border-edge-muted pt-5">
                <h3 class="text-sm font-medium mb-3">Recipients</h3>
                <For each={w.draft()?.recipients}>
                  {(r) => (
                    <div class="flex items-center justify-between py-3 text-sm">
                      <div>
                        <span class="font-medium">{r.name}</span>
                        <div class="text-ink-muted text-xs mt-1">{r.email}</div>
                      </div>
                      <span class="text-xs text-ink-muted">
                        Order {r.order} ·{' '}
                        {
                          w
                            .draft()
                            ?.fields.filter((f) => f.recipientId === r.id)
                            .length
                        }{' '}
                        fields
                      </span>
                    </div>
                  )}
                </For>
              </div>
              <p class="text-xs text-ink-muted border-t border-edge-muted pt-5 mt-3 leading-relaxed">
                Sending freezes this PDF, its recipients, and field positions.
                Private signing links expire in 30 days. Every signature records
                electronic consent and adds to the audit trail.
              </p>
            </div>
          </Match>
        </Switch>
      </div>
      <footer class="border-t border-edge-muted px-6 py-4 flex items-center justify-between shrink-0">
        <Button
          onClick={() => {
            setLocalError('');
            w.setStep(w.step() - 1);
          }}
          disabled={w.busy() || w.step() <= 1}
        >
          <ArrowLeft class="size-4" /> Back
        </Button>
        <div class="flex gap-3">
          <Show when={w.active()}>
            <Button variant="strong" onClick={w.save} disabled={w.busy()}>
              Save draft
            </Button>
          </Show>
          <Show when={w.step() > 0 && w.step() < 3}>
            <Button variant="cta" onClick={next} disabled={w.busy()}>
              Next <ArrowRight class="size-4" />
            </Button>
          </Show>
          <Show when={w.step() === 3}>
            <Button variant="cta" onClick={w.send} disabled={w.busy()}>
              <PaperPlane class="size-4" />
              {w.busy() ? 'Sending…' : 'Send for signature'}
            </Button>
          </Show>
        </div>
      </footer>
    </div>
  );
}
