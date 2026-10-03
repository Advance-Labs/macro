import { itemToBlockName } from '@app/lib/constants/file-metadata';
import type { SplitContent } from '@components/app/split-layout/layoutManager';
import type { ReminderEntity } from '@entity';

/** Source content for ordinary opening; explicit editing retains reminder identity. */
export function reminderSourceContent(
  reminder: Pick<ReminderEntity, 'referencedEntity'>
): Exclude<SplitContent, { type: 'component' }> | undefined {
  const reference = reminder.referencedEntity;
  if (!reference) return;
  const type =
    reference.type === 'crm_company'
      ? 'company'
      : reference.type === 'crm_contact'
        ? 'contact'
        : itemToBlockName({ type: 'reminder', referencedEntity: reference });
  if (type === 'unknown') return;
  return { type, id: reference.id };
}
