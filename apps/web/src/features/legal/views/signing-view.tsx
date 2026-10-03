import CheckCircle from '@phosphor/check-circle.svg';
import Pen from '@phosphor/pen-nib.svg';
import ShieldCheck from '@phosphor/shield-check.svg';
import { Button } from '@ui';
import { createSignal, For, Match, Show, Switch } from 'solid-js';
import { PdfDocument } from '../components/pdf-document';
import type { SigningSource } from '../context/legal-source';
import { fieldLabels } from '../core/models';
import { createSigning } from '../primitives/signing';
export function SigningView(props: { source: SigningSource }) {
  const s = createSigning(props.source);
  const [page, setPage] = createSignal(1);
  const [declining, setDeclining] = createSignal(false);
  const [reason, setReason] = createSignal('');
  return (
    <div class="h-full min-h-screen flex flex-col bg-surface text-ink">
      <header class="px-6 py-4 border-b border-edge-muted flex items-center justify-between gap-4">
        <div class="flex items-center gap-2">
          <Pen class="size-6 text-accent" />
          <span class="font-semibold text-lg">
            Macro <span class="font-normal text-ink-muted">Legal</span>
          </span>
        </div>
        <div class="flex gap-2 text-xs text-ink-muted items-center">
          <ShieldCheck class="size-4" /> Private signing session
        </div>
      </header>
      <Show when={s.error()}>
        <div
          role="alert"
          class="m-4 p-3 border border-failure rounded-lg text-failure text-sm"
        >
          {s.error()}
          <Button size="sm" onClick={s.load} disabled={s.busy()}>
            Refresh request
          </Button>
        </div>
      </Show>
      <Switch>
        <Match when={s.declined()}>
          <div class="max-w-lg mx-auto py-24 px-6 text-center">
            <h1 class="text-2xl font-semibold">You declined this request</h1>
            <p class="text-ink-muted mt-3 text-sm">
              The sender can see your reason in the envelope’s activity. This
              request is now closed.
            </p>
          </div>
        </Match>
        <Match when={!s.session()}>
          <div class="p-16 text-center text-ink-muted">
            {s.busy()
              ? 'Opening your agreement…'
              : 'This signing link could not be opened.'}
          </div>
        </Match>
        <Match when={s.session()?.recipient.signedAt}>
          <div class="max-w-xl mx-auto py-20 px-6 text-center">
            <CheckCircle class="size-16 text-success mx-auto mb-6" />
            <h1 class="text-3xl font-semibold">You’re all signed.</h1>
            <p class="text-ink-muted mt-3">
              Your signature for{' '}
              <span class="text-ink font-medium">{s.session()?.title}</span> has
              been recorded.
            </p>
            <Show
              when={s.session()?.status === 'completed'}
              fallback={
                <p class="mt-5 text-sm text-ink-muted">
                  We’re waiting for the remaining recipients. Return to this
                  link to download the completed agreement when everyone has
                  signed.
                </p>
              }
            >
              <div class="mt-8">
                <Button
                  variant="cta"
                  size="lg"
                  onClick={s.download}
                  disabled={s.busy()}
                >
                  Download signed agreement
                </Button>
                <p class="text-xs text-ink-muted mt-4">
                  Includes the certificate of completion.
                </p>
              </div>
            </Show>
            <div class="border border-edge-muted rounded-xl p-5 mt-10 text-left">
              <div class="text-xs text-ink-muted">Signed as</div>
              <div class="font-medium mt-1">{s.session()?.recipient.name}</div>
              <div class="text-sm text-ink-muted">
                {s.session()?.recipient.email}
              </div>
              <div class="text-xs text-ink-muted mt-3">
                {new Date(
                  s.session()?.recipient.signedAt || ''
                ).toLocaleString()}
              </div>
            </div>
          </div>
        </Match>
        <Match when={s.session() && !s.session()?.canSign}>
          <div class="max-w-xl mx-auto p-12 text-center">
            <h1 class="text-2xl font-semibold">
              This request is not ready to sign
            </h1>
            <p class="text-ink-muted mt-3">
              An earlier recipient may still need to sign. Refresh this page to
              check the latest status.
            </p>
            <Button onClick={s.load} disabled={s.busy()} class="mt-6">
              Refresh request
            </Button>
          </div>
        </Match>
        <Match when={!s.reviewing()}>
          <div class="max-w-2xl mx-auto px-6 py-12">
            <div class="text-xs text-ink-muted mb-3">
              YOUR SIGNATURE IS REQUESTED
            </div>
            <h1 class="text-3xl font-semibold">{s.session()?.title}</h1>
            <p class="text-sm text-ink-muted mt-3">
              Hello {s.session()?.recipient.name}, please review the document
              and complete your assigned fields.
            </p>
            <Show when={s.session()?.message}>
              <div class="mt-7 border-l-2 border-accent pl-4 text-sm whitespace-pre-wrap">
                {s.session()?.message}
              </div>
            </Show>
            <div class="border border-edge-muted p-5 rounded-xl mt-8">
              <div class="font-medium text-sm">{s.session()?.filename}</div>
              <div class="text-xs text-ink-muted mt-2">
                {s.session()?.pageCount} pages ·{' '}
                {s.session()?.fields.filter((f) => f.required).length} required
                fields
              </div>
            </div>
            <section class="mt-8">
              <h2 class="text-sm font-semibold mb-3">
                Electronic records and signature disclosure
              </h2>
              <p class="text-sm text-ink-muted leading-relaxed">
                You will review and sign this agreement electronically. By
                checking the box below, you consent to receive electronic
                records and to use an electronic signature for this agreement.
                Typing your name in a signature field and selecting “Finish
                signing” expresses your intent to sign. You can decline before
                finishing. You can save a copy of the completed PDF from this
                link.
              </p>
              <label class="flex gap-3 items-start mt-5 p-4 rounded-lg border border-edge-muted text-sm">
                <input
                  aria-label="I agree to electronic records and signatures"
                  type="checkbox"
                  checked={s.consent()}
                  onChange={(e) => s.setConsent(e.currentTarget.checked)}
                  class="mt-0.5 size-4"
                />
                <span>
                  I agree to use electronic records and signatures for this
                  agreement.
                </span>
              </label>
            </section>
            <div class="mt-7 flex justify-between items-center">
              <Button onClick={() => setDeclining(!declining())}>
                Decline to sign
              </Button>
              <Button
                variant="cta"
                size="lg"
                disabled={!s.consent() || s.busy()}
                onClick={() => s.setReviewing(true)}
              >
                Review & sign
              </Button>
            </div>
          </div>
        </Match>
        <Match when={s.reviewing()}>
          <div class="flex-1 min-h-0 flex flex-col lg:flex-row">
            <main class="flex-1 min-w-0 bg-ink/4 p-6 overflow-auto">
              <div class="flex items-center justify-center gap-3 text-sm mb-5">
                <Button
                  disabled={page() <= 1}
                  onClick={() => setPage((p) => p - 1)}
                >
                  Previous
                </Button>
                <span>
                  Page {page()} of {s.session()?.pageCount}
                </span>
                <Button
                  disabled={page() >= (s.session()?.pageCount || 1)}
                  onClick={() => setPage((p) => p + 1)}
                >
                  Next page
                </Button>
              </div>
              <Show when={s.bytes()}>
                {(bytes) => (
                  <PdfDocument
                    bytes={bytes()}
                    page={page()}
                    fields={s.session()?.fields || []}
                    mode="sign"
                    values={s.values()}
                    onSelect={(field) => {
                      setPage(field.page);
                      document
                        .getElementById(`sign-field-${field.id}`)
                        ?.focus();
                    }}
                  />
                )}
              </Show>
            </main>
            <aside class="w-full lg:w-80 border-l border-edge-muted p-6 overflow-auto">
              <h2 class="font-semibold text-lg">Your fields</h2>
              <p class="text-xs text-ink-muted mt-2 mb-5">
                Enter your signature and complete the required fields. Your
                signature will appear on the document.
              </p>
              <For each={s.session()?.fields}>
                {(field) => (
                  <label class="block text-xs text-ink-muted mb-5">
                    {fieldLabels[field.kind]}
                    {field.required ? ' *' : ''} · page {field.page}
                    <input
                      id={`sign-field-${field.id}`}
                      aria-label={`${fieldLabels[field.kind]} field`}
                      class="block mt-2 w-full p-2.5 rounded-lg border border-edge-muted bg-input text-ink text-sm"
                      classList={{
                        'font-serif italic text-xl': field.kind === 'signature',
                      }}
                      value={
                        field.kind === 'date'
                          ? new Date().toISOString().slice(0, 10)
                          : s.values()[field.id] || ''
                      }
                      readOnly={field.kind === 'date'}
                      placeholder={
                        field.kind === 'signature'
                          ? 'Type your full name'
                          : field.kind === 'initials'
                            ? 'Your initials'
                            : 'Enter text'
                      }
                      onFocus={() => setPage(field.page)}
                      onInput={(event) =>
                        s.fill(field.id, event.currentTarget.value)
                      }
                      maxLength={500}
                    />
                  </label>
                )}
              </For>
              <div class="text-xs text-ink-muted border-t border-edge-muted pt-5 leading-relaxed">
                By finishing, you adopt the entered signature and intend to sign
                this document. The audit trail records your consent and the
                original document’s fingerprint.
              </div>
              <Button class="mt-5" onClick={() => setDeclining(!declining())}>
                Decline to sign
              </Button>
            </aside>
          </div>
          <footer class="p-4 border-t border-edge-muted flex items-center justify-between">
            <span class="text-sm text-ink-muted">
              {s.completedFields()} of {s.session()?.fields.length} fields
              completed
            </span>
            <Button
              variant="cta"
              size="lg"
              disabled={s.busy()}
              onClick={s.finish}
            >
              {s.busy() ? 'Finishing…' : 'Finish signing'}
            </Button>
          </footer>
        </Match>
      </Switch>
      <Show when={declining() && !s.declined()}>
        <section class="max-w-2xl mx-auto w-full px-6 pb-6">
          <label class="block text-sm font-medium">
            Reason for declining
            <textarea
              aria-label="Decline reason"
              rows={3}
              class="w-full mt-2 p-3 rounded-lg border border-edge-muted bg-input"
              value={reason()}
              onInput={(e) => setReason(e.currentTarget.value)}
              maxLength={1000}
            />
          </label>
          <p class="text-xs text-ink-muted mt-2">
            Declining closes the request for every recipient. Your reason is
            shared with the sender.
          </p>
          <div class="flex gap-3 mt-3">
            <Button
              variant="danger"
              disabled={!reason().trim() || s.busy()}
              onClick={() => s.decline(reason())}
            >
              Decline request
            </Button>
            <Button onClick={() => setDeclining(false)}>Cancel</Button>
          </div>
        </section>
      </Show>
    </div>
  );
}
