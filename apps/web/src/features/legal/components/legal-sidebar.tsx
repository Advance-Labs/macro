import { ViewSidebar } from '@app/components/view-shell';
import { SidebarCreateHeader } from '@app/components/view-shell/SidebarCreateButton';
import CheckCircle from '@phosphor/check-circle.svg';
import Clock from '@phosphor/clock.svg';
import FileText from '@phosphor/file-text.svg';
import Pen from '@phosphor/pen-nib.svg';
import XCircle from '@phosphor/x-circle.svg';
import { For } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { Status } from '../core/models';
import type { Workspace } from '../primitives/workspace';

const sections = [
  { id: 'all', label: 'All agreements', icon: FileText },
  { id: 'sent', label: 'In progress', icon: Clock },
  { id: 'completed', label: 'Completed', icon: CheckCircle },
  { id: 'draft', label: 'Drafts', icon: Pen },
  { id: 'declined', label: 'Declined', icon: XCircle },
  { id: 'voided', label: 'Voided', icon: XCircle },
] satisfies { id: Status | 'all'; label: string; icon: typeof FileText }[];

export function LegalSidebar(props: { workspace: Workspace }) {
  const w = props.workspace;
  return (
    <ViewSidebar.Root aria-label="Legal navigation">
      <SidebarCreateHeader
        title="Legal"
        label="New envelope"
        onCreate={w.start}
      />
      <ViewSidebar.Content>
        <ViewSidebar.Nav aria-label="Agreement views">
          <For each={sections}>
            {(section) => (
              <ViewSidebar.Item
                active={w.filter() === section.id}
                onClick={() => {
                  w.setFilter(section.id);
                  w.close();
                }}
              >
                <ViewSidebar.Icon>
                  <Dynamic component={section.icon} class="size-4" />
                </ViewSidebar.Icon>
                <span class="truncate">{section.label}</span>
                <span class="ml-auto text-xs text-ink-extra-muted">
                  {
                    w
                      .envelopes()
                      .filter(
                        (e) => section.id === 'all' || e.status === section.id
                      ).length
                  }
                </span>
              </ViewSidebar.Item>
            )}
          </For>
        </ViewSidebar.Nav>
        <p class="px-(--sidebar-item-inset) text-xs leading-relaxed text-ink-extra-muted">
          eSignature
          <br />
          Prepare, send, and track agreements.
        </p>
      </ViewSidebar.Content>
    </ViewSidebar.Root>
  );
}
