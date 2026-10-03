import { SearchBar, ViewShell } from '@app/components/view-shell';
import ArrowRight from '@phosphor/arrow-right.svg';
import FileText from '@phosphor/file-text.svg';
import Plus from '@phosphor/plus.svg';
import { Button } from '@ui';
import { For, onMount, Show } from 'solid-js';
import { StatusBadge } from '../components/status-badge';
import type { Workspace } from '../primitives/workspace';

export function Dashboard(props: { workspace: Workspace }) {
  const w = props.workspace;
  onMount(w.reset);
  const rows = () =>
    w
      .envelopes()
      .filter(
        (e) =>
          (w.filter() === 'all' || e.status === w.filter()) &&
          `${e.title} ${e.recipients.map((r) => `${r.name} ${r.email}`).join(' ')}`
            .toLowerCase()
            .includes(w.search().toLowerCase())
      );
  return (
    <>
      <ViewShell.Header>
        <div class="flex min-w-0 items-center gap-3">
          <SearchBar
            label="Search envelopes"
            placeholder="Search agreements"
            value={w.search()}
            onValueChange={w.setSearch}
            class="max-w-md flex-1"
          />
          <Button
            variant="cta"
            class="ml-auto"
            onClick={w.start}
            disabled={w.busy()}
          >
            <Plus class="size-4" />
            New envelope
          </Button>
        </div>
      </ViewShell.Header>
      <ViewShell.Content class="overflow-auto">
        <Show
          when={!w.loading()}
          fallback={
            <div class="p-12 text-center text-sm text-ink-muted">
              Loading your agreements…
            </div>
          }
        >
          <Show
            when={rows().length}
            fallback={
              <div class="flex h-full min-h-64 flex-col items-center justify-center gap-3 p-8 text-center">
                <FileText class="size-8 text-ink-extra-muted" />
                <h2 class="text-base font-semibold">
                  {w.envelopes().length
                    ? 'No matching agreements'
                    : 'No agreements yet'}
                </h2>
                <p class="max-w-sm text-sm text-ink-muted">
                  {w.envelopes().length
                    ? 'Try another search or status filter.'
                    : 'Upload a PDF, add recipients, and collect signatures.'}
                </p>
                <Show when={!w.envelopes().length}>
                  <Button variant="strong" onClick={w.start}>
                    Create your first envelope
                  </Button>
                </Show>
              </div>
            }
          >
            <table class="w-full text-sm">
              <thead class="sticky top-0 z-10 border-y border-edge-muted bg-panel text-xs text-ink-muted">
                <tr>
                  <th class="px-4 py-3 text-left font-normal">Agreement</th>
                  <th class="px-4 py-3 text-left font-normal">Status</th>
                  <th class="px-4 py-3 text-left font-normal">Recipients</th>
                  <th class="px-4 py-3 text-left font-normal">Updated</th>
                  <th class="w-10">
                    <span class="sr-only">Open</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                <For each={rows()}>
                  {(envelope) => (
                    <tr class="border-b border-edge-muted hover:bg-hover">
                      <td class="px-4 py-4">
                        <button
                          type="button"
                          class="flex min-w-0 items-center gap-3 text-left"
                          onClick={() => w.open(envelope)}
                          disabled={w.busy()}
                        >
                          <FileText class="size-5 shrink-0 text-ink-muted" />
                          <div>
                            <div class="font-medium">{envelope.title}</div>
                            <div class="mt-1 text-xs text-ink-extra-muted">
                              {envelope.filename} · {envelope.pageCount}{' '}
                              {envelope.pageCount === 1 ? 'page' : 'pages'}
                            </div>
                          </div>
                        </button>
                      </td>
                      <td class="px-4 py-4">
                        <StatusBadge envelope={envelope} />
                      </td>
                      <td class="px-4 py-4">
                        <div>
                          {envelope.recipients[0]?.name || 'No recipients yet'}
                          {envelope.recipients.length > 1
                            ? ` +${envelope.recipients.length - 1}`
                            : ''}
                        </div>
                        <div class="mt-1 text-xs text-ink-extra-muted">
                          {envelope.recipients.filter((r) => r.signedAt).length}{' '}
                          of {envelope.recipients.length} signed
                        </div>
                      </td>
                      <td class="px-4 py-4 text-ink-muted">
                        {new Date(envelope.updatedAt).toLocaleDateString(
                          undefined,
                          { month: 'short', day: 'numeric' }
                        )}
                      </td>
                      <td class="pr-4">
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
          </Show>
        </Show>
      </ViewShell.Content>
    </>
  );
}
