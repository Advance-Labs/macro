import ArrowSquareOutIcon from '@phosphor/arrow-square-out.svg';
import PhoneIcon from '@phosphor/phone.svg';
import { buttonClasses } from '@ui';
import { Show } from 'solid-js';
import { RecordSection } from '../components/record-section';
import { useCrmContext } from '../context/crm-context';
import type { CrmRecordScope } from '../core/record';
import { RecordItemList } from './record-item-list';

/**
 * Documents associated with the record, plus attachments of emails
 * exchanged with its people.
 */
export function RecordFilesSection(props: { scope?: CrmRecordScope }) {
  const files = useCrmContext().createRecordFiles(() => props.scope);
  return (
    <RecordSection title="Files">
      <RecordItemList
        source={files}
        pending={!props.scope}
        empty={`No files with this ${props.scope?.type ?? 'record'} yet.`}
      />
    </RecordSection>
  );
}

/**
 * Outbound dialer this deployment calls from (`VITE_DIALER_URL`, no trailing
 * slash). Set, the Calls tab links to it; unset, nothing changes.
 */
const DIALER_URL = import.meta.env.VITE_DIALER_URL?.replace(/\/+$/, '');

/** Calls associated with the record, including calls with its people. */
export function RecordCallsSection(props: { scope?: CrmRecordScope }) {
  const calls = useCrmContext().createRecordCalls(() => props.scope);
  const companyId = () =>
    props.scope?.type === 'company' ? props.scope.id : props.scope?.companyId;
  return (
    <RecordSection
      title="Calls"
      actions={
        DIALER_URL && (
          <Show when={companyId()}>
            {(id) => (
              <a
                href={`${DIALER_URL}/macro/company/${encodeURIComponent(id())}`}
                target="_blank"
                rel="noopener noreferrer"
                class={buttonClasses({ variant: 'outline', size: 'sm' })}
                title="Opens this company in the dialer, or your queue if it has no lead there"
              >
                <PhoneIcon class="size-3.5" />
                Call in Dialer
                <ArrowSquareOutIcon class="size-3" />
              </a>
            )}
          </Show>
        )
      }
    >
      <RecordItemList
        source={calls}
        pending={!props.scope}
        empty={`No calls with this ${props.scope?.type ?? 'record'} yet.`}
      />
    </RecordSection>
  );
}
