import { EntityActivitySectionConditional } from '@app/features/activity/views/entity-activity-section';
import {
  DateValueDisplay,
  OwnerValue,
  SidePanel,
} from '@components/app/side-panel';
import type { DatabaseSummary } from '@service-storage/databases';
import { Show } from 'solid-js';

export function DatabaseSidePanelSections(props: {
  databaseId: string;
  database: DatabaseSummary | undefined;
}) {
  return (
    <>
      <SidePanel.Section id="details" title="Details" defaultOpen order={10}>
        <Show when={props.database} fallback={<SidePanel.Loading />}>
          {(database) => (
            <SidePanel.Grid>
              <SidePanel.Row label="Owner">
                <OwnerValue ownerId={database().owner_id} />
              </SidePanel.Row>
              <SidePanel.Row label="Created">
                <DateValueDisplay value={database().created_at} />
              </SidePanel.Row>
            </SidePanel.Grid>
          )}
        </Show>
      </SidePanel.Section>
      <EntityActivitySectionConditional
        entityId={props.databaseId}
        entityType="DATABASE"
        order={40}
      />
    </>
  );
}
