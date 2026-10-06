import { remotePartyLabel } from '@app/features/phone/core/phone-call';

export function formatCallDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

/** The phone party of a call record, as the phone feature names them. */
export function callRecordPhoneParty(record: {
  phone?: {
    remoteNumber: string;
    contact?: { contactId: string; name?: string | null } | null;
  } | null;
}): string | undefined {
  const phone = record.phone;
  if (!phone) return undefined;
  return remotePartyLabel({
    contact: phone.contact
      ? { contactId: phone.contact.contactId, name: phone.contact.name ?? null }
      : null,
    remoteNumber: phone.remoteNumber,
  });
}

/**
 * A call record's name: its own, its channel's, or for a phone call the
 * party on the other end.
 */
export function callRecordName(
  record: Parameters<typeof callRecordPhoneParty>[0] & {
    customName?: string | null;
    channelName?: string | null;
  }
): string | undefined {
  return (
    record.customName ??
    record.channelName ??
    callRecordPhoneParty(record) ??
    undefined
  );
}
