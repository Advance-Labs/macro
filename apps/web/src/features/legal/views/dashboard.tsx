import ArrowRight from '@phosphor/arrow-right.svg';
import CheckCircle from '@phosphor/check-circle.svg';
import Clock from '@phosphor/clock.svg';
import FileText from '@phosphor/file-text.svg';
import Search from '@phosphor/magnifying-glass.svg';
import Pen from '@phosphor/pen-nib.svg';
import Plus from '@phosphor/plus.svg';
import Scales from '@phosphor/scales.svg';
import { Button } from '@ui';
import { createSignal, For, Show } from 'solid-js';
import { StatusBadge } from '../components/status-badge';
import { isExpired, type Status } from '../core/models';
import type { Workspace } from '../primitives/workspace';

export function Dashboard(props: { workspace: Workspace }) {
  const w = props.workspace;
  const [filter, setFilter] = createSignal<Status | 'all'>('all');
  const [search, setSearch] = createSignal('');
  const rows = () =>
    w
      .envelopes()
      .filter(
        (e) =>
          (filter() === 'all' || e.status === filter()) &&
          `${e.title} ${e.recipients.map((r) => `${r.name} ${r.email}`).join(' ')}`
            .toLowerCase()
            .includes(search().toLowerCase())
      );
  const stats = () => [
    {
      label: 'In progress',
      status: 'sent' as const,
      count: w.envelopes().filter((e) => e.status === 'sent' && !isExpired(e))
        .length,
      icon: Clock,
    },
    {
      label: 'Completed',
      status: 'completed' as const,
      count: w.envelopes().filter((e) => e.status === 'completed').length,
      icon: CheckCircle,
    },
    {
      label: 'Drafts',
      status: 'draft' as const,
      count: w.envelopes().filter((e) => e.status === 'draft').length,
      icon: Pen,
    },
  ];
  return (
    <div class="flex h-full min-h-0 bg-surface text-ink">
      <aside class="hidden lg:flex w-52 shrink-0 flex-col border-r border-edge-muted p-5 gap-5">
        <div class="flex items-center gap-2 text-lg font-semibold">
          <Scales class="size-5" /> Legal
        </div>
        <div class="text-xs uppercase tracking-widest text-ink-muted mt-4">
          Workspace
        </div>
        <button
          type="button"
          class="flex items-center gap-2.5 text-sm rounded-lg px-3 py-2.5 bg-accent-bg text-accent font-medium"
          onClick={() => setFilter('all')}
        >
          <FileText class="size-4" /> eSignature
        </button>
        <div class="mt-auto border-t border-edge-muted pt-4 text-xs text-ink-muted leading-relaxed">
          Your agreements, from first draft to final signature.
        </div>
      </aside>
      <main class="flex-1 min-w-0 overflow-auto p-6 lg:p-10">
        <div class="max-w-6xl mx-auto space-y-8">
          <header class="flex items-start justify-between gap-4">
            <div>
              <div class="text-xs text-ink-muted mb-2">LEGAL / ESIGNATURE</div>
              <h1 class="text-3xl font-semibold tracking-tight">
                Agreements, made simple.
              </h1>
              <p class="text-sm text-ink-muted mt-2">
                Prepare, send, and track every signature in one place.
              </p>
            </div>
            <Button
              variant="cta"
              size="lg"
              onClick={w.start}
              disabled={w.busy()}
            >
              <Plus class="size-4" /> New envelope
            </Button>
          </header>
          <section
            class="grid grid-cols-1 sm:grid-cols-3 gap-4"
            aria-label="Envelope overview"
          >
            <For each={stats()}>
              {(stat) => (
                <button
                  type="button"
                  class="rounded-xl border border-edge-muted p-5 text-left hover:bg-ink/3 transition-colors"
                  onClick={() => setFilter(stat.status)}
                >
                  <div class="flex justify-between items-center text-ink-muted text-sm">
                    {stat.label}
                    <stat.icon class="size-5" />
                  </div>
                  <div class="text-3xl font-semibold mt-3">{stat.count}</div>
                  <div class="text-xs text-ink-muted mt-2 flex items-center gap-1">
                    View envelopes <ArrowRight class="size-3" />
                  </div>
                </button>
              )}
            </For>
          </section>
          <section class="border border-edge-muted rounded-xl overflow-hidden">
            <div class="p-5 border-b border-edge-muted flex flex-wrap items-center justify-between gap-4">
              <h2 class="font-semibold text-lg">Your envelopes</h2>
              <div class="relative">
                <Search class="size-4 absolute left-3 top-3 text-ink-muted" />
                <input
                  aria-label="Search envelopes"
                  placeholder="Search envelopes…"
                  value={search()}
                  onInput={(e) => setSearch(e.currentTarget.value)}
                  class="rounded-lg bg-input border border-edge-muted text-ink placeholder:text-ink-placeholder pl-9 pr-3 py-2 text-sm w-60"
                />
              </div>
            </div>
            <div class="flex gap-5 px-5 border-b border-edge-muted overflow-auto">
              <For
                each={
                  [
                    'all',
                    'sent',
                    'completed',
                    'draft',
                    'declined',
                    'voided',
                  ] as const
                }
              >
                {(status) => (
                  <button
                    type="button"
                    aria-pressed={filter() === status}
                    onClick={() => setFilter(status)}
                    class="py-3 text-sm border-b-2 whitespace-nowrap"
                    classList={{
                      'border-accent text-accent font-medium':
                        filter() === status,
                      'border-transparent text-ink-muted': filter() !== status,
                    }}
                  >
                    {status === 'all'
                      ? 'All envelopes'
                      : status === 'sent'
                        ? 'In progress'
                        : status[0].toUpperCase() + status.slice(1)}
                  </button>
                )}
              </For>
            </div>
            <Show
              when={!w.loading()}
              fallback={
                <div class="p-12 text-center text-ink-muted">
                  Loading your envelopes…
                </div>
              }
            >
              <Show
                when={rows().length}
                fallback={
                  <div class="flex flex-col items-center p-14 text-center">
                    <div class="rounded-2xl bg-accent-bg text-accent p-5 mb-5">
                      <FileText class="size-9" />
                    </div>
                    <h3 class="text-lg font-semibold">
                      {w.envelopes().length
                        ? 'No matching envelopes'
                        : 'Your next agreement starts here'}
                    </h3>
                    <p class="text-sm text-ink-muted max-w-sm mt-2 mb-6">
                      {w.envelopes().length
                        ? 'Try another search or status filter.'
                        : 'Upload a PDF, add your recipients, and collect signatures without leaving Macro.'}
                    </p>
                    <Show when={!w.envelopes().length}>
                      <Button variant="cta" onClick={w.start}>
                        <Plus class="size-4" /> Create your first envelope
                      </Button>
                    </Show>
                  </div>
                }
              >
                <div class="overflow-x-auto">
                  <table class="w-full text-sm">
                    <thead class="text-ink-muted text-xs border-b border-edge-muted bg-ink/2">
                      <tr>
                        <th class="px-5 py-3 text-left font-medium">
                          Envelope
                        </th>
                        <th class="px-5 py-3 text-left font-medium">Status</th>
                        <th class="px-5 py-3 text-left font-medium">
                          Recipients
                        </th>
                        <th class="px-5 py-3 text-left font-medium">
                          Last updated
                        </th>
                        <th class="w-12">
                          <span class="sr-only">Open</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={rows()}>
                        {(envelope) => (
                          <tr class="border-b last:border-0 border-edge-muted hover:bg-ink/3">
                            <td class="px-5 py-5">
                              <button
                                type="button"
                                class="flex gap-3 items-center text-left"
                                onClick={() => w.open(envelope)}
                                disabled={w.busy()}
                              >
                                <div class="p-2.5 rounded-lg bg-ink/5 text-ink-muted">
                                  <FileText class="size-5" />
                                </div>
                                <div>
                                  <div class="font-medium">
                                    {envelope.title}
                                  </div>
                                  <div class="text-xs text-ink-muted mt-1">
                                    {envelope.filename} · {envelope.pageCount}{' '}
                                    {envelope.pageCount === 1
                                      ? 'page'
                                      : 'pages'}
                                  </div>
                                </div>
                              </button>
                            </td>
                            <td class="px-5 py-5">
                              <StatusBadge envelope={envelope} />
                            </td>
                            <td class="px-5 py-5">
                              <div>
                                {envelope.recipients[0]?.name ||
                                  'No recipients yet'}
                                {envelope.recipients.length > 1
                                  ? ` +${envelope.recipients.length - 1}`
                                  : ''}
                              </div>
                              <div class="text-xs text-ink-muted mt-1">
                                {
                                  envelope.recipients.filter((r) => r.signedAt)
                                    .length
                                }{' '}
                                of {envelope.recipients.length} signed
                              </div>
                            </td>
                            <td class="px-5 py-5 text-ink-muted">
                              {new Date(envelope.updatedAt).toLocaleDateString(
                                undefined,
                                { month: 'short', day: 'numeric' }
                              )}
                            </td>
                            <td class="pr-5">
                              <Button
                                size="icon-sm"
                                aria-label={`Open ${envelope.title}`}
                                onClick={() => w.open(envelope)}
                                disabled={w.busy()}
                              >
                                <ArrowRight class="size-4" />
                              </Button>
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>
            </Show>
          </section>
          <div class="flex gap-3 items-center text-xs text-ink-muted">
            <CheckCircle class="size-4" /> Every completed envelope includes an
            audit trail and a certificate of completion.
          </div>
        </div>
      </main>
    </div>
  );
}
