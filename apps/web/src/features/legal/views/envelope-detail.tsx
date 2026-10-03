import { SplitPanel } from '@components/app/split-panel';
import ArrowLeft from '@phosphor/arrow-left.svg';
import CheckCircle from '@phosphor/check-circle.svg';
import Clock from '@phosphor/clock.svg';
import Download from '@phosphor/download-simple.svg';
import { Button } from '@ui';
import { createSignal, For, Show } from 'solid-js';
import { PdfDocument } from '../components/pdf-document';
import { StatusBadge } from '../components/status-badge';
import { isExpired } from '../core/models';
import type { Workspace } from '../primitives/workspace';

export function EnvelopeDetail(props: { workspace: Workspace }) {
  const w = props.workspace;
  const [page, setPage] = createSignal(1);
  const [voiding, setVoiding] = createSignal(false);
  const [reason, setReason] = createSignal('');
  return (
    <Show when={w.active()}>
      {(envelope) => (
        <div class="flex min-h-0 flex-1 flex-col bg-panel text-ink">
          <SplitPanel.Toolbar class="flex-wrap justify-between gap-3 px-4">
            <div class="flex gap-3 items-center">
              <Button
                size="icon-md"
                aria-label="Back to Legal"
                onClick={w.close}
              >
                <ArrowLeft class="size-4" />
              </Button>
              <div>
                <h1 class="text-sm font-medium">{envelope().title}</h1>
              </div>
            </div>
            <div class="flex items-center gap-3">
              <StatusBadge envelope={envelope()} />
              <Show when={envelope().status === 'completed'}>
                <Button
                  variant="cta"
                  disabled={w.busy()}
                  onClick={() => w.download(true)}
                >
                  <Download class="size-4" /> Download signed PDF
                </Button>
              </Show>
            </div>
          </SplitPanel.Toolbar>
          <div class="flex-1 min-h-0 overflow-auto flex flex-col @min-[1180px]/view-shell:flex-row">
            <div class="flex-1 min-w-0 bg-ink/4 p-6">
              <div class="flex justify-center items-center gap-4 mb-5 text-sm">
                <Button
                  disabled={page() <= 1}
                  onClick={() => setPage((p) => p - 1)}
                >
                  Previous
                </Button>
                <span>
                  Page {page()} of {envelope().pageCount}
                </span>
                <Button
                  disabled={page() >= envelope().pageCount}
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
                    fields={envelope().fields}
                  />
                )}
              </Show>
            </div>
            <aside class="w-full @min-[1180px]/view-shell:w-96 border-l border-edge-muted p-6 space-y-7">
              <div>
                <h2 class="text-lg font-semibold">Recipients</h2>
                <For each={envelope().recipients}>
                  {(recipient) => (
                    <div class="flex gap-3 py-4 border-b border-edge-muted">
                      <div class="size-8 rounded-full bg-ink/5 text-ink-muted grid place-items-center">
                        <Show
                          when={recipient.signedAt}
                          fallback={<Clock class="size-4" />}
                        >
                          <CheckCircle class="size-5 text-success" />
                        </Show>
                      </div>
                      <div>
                        <div class="text-sm font-medium">{recipient.name}</div>
                        <div class="text-xs text-ink-muted mt-1">
                          {recipient.email}
                        </div>
                        <div class="text-xs text-ink-muted mt-2">
                          {recipient.signedAt
                            ? `Signed ${new Date(recipient.signedAt).toLocaleString()}`
                            : recipient.deliveredAt
                              ? 'Invitation delivered · awaiting signature'
                              : recipient.order >
                                  Math.min(
                                    ...envelope()
                                      .recipients.filter((r) => !r.signedAt)
                                      .map((r) => r.order)
                                  )
                                ? 'Waiting for earlier recipients'
                                : 'Invitation not delivered · use Resend'}
                        </div>
                      </div>
                    </div>
                  )}
                </For>
              </div>
              <Show
                when={envelope().status === 'sent' && !isExpired(envelope())}
              >
                <div class="flex gap-2">
                  <Button
                    variant="strong"
                    disabled={w.busy()}
                    onClick={w.resend}
                  >
                    Resend invitations
                  </Button>
                  <Button
                    variant="danger"
                    disabled={w.busy()}
                    onClick={() => setVoiding(!voiding())}
                  >
                    Void
                  </Button>
                </div>
                <Show when={voiding()}>
                  <div class="border border-edge-muted rounded-lg p-3 space-y-3">
                    <label class="text-xs font-medium">
                      Reason for voiding
                      <textarea
                        aria-label="Reason for voiding"
                        rows={3}
                        value={reason()}
                        onInput={(e) => setReason(e.currentTarget.value)}
                        class="w-full mt-2 p-2 bg-input border border-edge-muted rounded text-sm"
                      />
                    </label>
                    <p class="text-xs text-ink-muted">
                      Voiding disables all signing links and preserves the audit
                      trail.
                    </p>
                    <Button
                      variant="danger"
                      disabled={w.busy() || !reason().trim()}
                      onClick={async () => {
                        await w.voidEnvelope(reason());
                        setVoiding(false);
                      }}
                    >
                      Void envelope
                    </Button>
                  </div>
                </Show>
              </Show>
              <Button
                variant="strong"
                disabled={w.busy()}
                onClick={() => w.download(false)}
              >
                <Download class="size-4" /> Download original
              </Button>
              <div>
                <h2 class="text-sm font-semibold mb-4">Activity</h2>
                <For each={[...envelope().audit].reverse()}>
                  {(event) => (
                    <div class="border-l-2 border-edge-muted pl-4 pb-5">
                      <div class="text-sm font-medium capitalize">
                        {event.action}
                      </div>
                      <div class="text-xs text-ink-muted mt-1 break-words">
                        {event.actor}
                      </div>
                      <div class="text-xs text-ink-muted mt-1">
                        {new Date(event.at).toLocaleString()}
                      </div>
                      <Show
                        when={['voided', 'declined'].includes(event.action)}
                      >
                        <p class="text-xs mt-2">{event.detail}</p>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
              <div class="border-t border-edge-muted pt-4">
                <h3 class="text-xs font-medium mb-2">
                  Document fingerprint · SHA-256
                </h3>
                <p class="text-xs text-ink-muted font-mono break-all">
                  {envelope().sourceSha256}
                </p>
                <Show when={envelope().completedSha256}>
                  <h3 class="text-xs font-medium mt-4 mb-2">
                    Completed PDF · SHA-256
                  </h3>
                  <p class="text-xs text-ink-muted font-mono break-all">
                    {envelope().completedSha256}
                  </p>
                </Show>
                <div class="text-xs text-ink-muted mt-4">
                  Envelope ID: {envelope().id}
                </div>
              </div>
            </aside>
          </div>
        </div>
      )}
    </Show>
  );
}
