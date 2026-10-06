import PencilSimpleIcon from '@phosphor/pencil-simple.svg';
import PhoneIcon from '@phosphor/phone.svg';
import PlusIcon from '@phosphor/plus.svg';
import { ActionDialogShell, Badge, badgeTriggerClasses, Dialog } from '@ui';
import { createSignal, For, Show } from 'solid-js';
import { ContactPhoneNumbersForm } from '../components/contact-phone-numbers-form';
import type { CrmContact } from '../core/contact';
import {
  useContactPhoneNumbersQuery,
  useCrmPhoneCalling,
  useSetContactPhoneNumbersMutation,
} from './use-crm';

/**
 * A contact's phone numbers as header pills. With calling available, a pill
 * calls the number; anyone who can see the contact can edit them.
 */
export function ContactPhoneNumbers(props: { contact: CrmContact }) {
  const phone = useCrmPhoneCalling();
  const numbers = useContactPhoneNumbersQuery(
    () => props.contact.id,
    phone.enabled
  );
  const [editing, setEditing] = createSignal(false);
  const list = () => numbers.data ?? [];

  return (
    <Show when={phone.enabled()}>
      <For each={list()}>
        {(number) => (
          <Show
            when={phone.canCall()}
            fallback={
              <Badge variant="outline" size="sm" class="tabular-nums">
                <PhoneIcon class="size-3" />
                {phone.format(number)}
              </Badge>
            }
          >
            <button
              type="button"
              aria-label={`Call ${phone.format(number)}`}
              title={`Call ${phone.format(number)}`}
              onClick={() => phone.call(number)}
              class={badgeTriggerClasses({ variant: 'outline', size: 'sm' })}
            >
              <PhoneIcon class="size-3" />
              <span class="tabular-nums">{phone.format(number)}</span>
            </button>
          </Show>
        )}
      </For>
      <Show when={numbers.isSuccess}>
        <button
          type="button"
          onClick={() => setEditing(true)}
          class={badgeTriggerClasses({ variant: 'outline', size: 'sm' })}
        >
          <Show
            when={list().length > 0}
            fallback={
              <>
                <PlusIcon class="size-3" />
                Add phone
              </>
            }
          >
            <PencilSimpleIcon class="size-3" />
            <span class="sr-only">Edit phone numbers</span>
          </Show>
        </button>
      </Show>
      <Show when={editing()}>
        <EditContactPhoneNumbers
          contactId={props.contact.id}
          numbers={list()}
          onClose={() => setEditing(false)}
        />
      </Show>
    </Show>
  );
}

function EditContactPhoneNumbers(props: {
  contactId: string;
  numbers: string[];
  onClose: () => void;
}) {
  const phone = useCrmPhoneCalling();
  const save = useSetContactPhoneNumbersMutation();
  const [value, setValue] = createSignal(
    props.numbers.map((number) => phone.format(number)).join('\n')
  );
  const [error, setError] = createSignal<string>();

  async function submit() {
    setError(undefined);
    const phoneNumbers = value()
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
    try {
      await save.mutateAsync({ contactId: props.contactId, phoneNumbers });
      props.onClose();
    } catch {
      setError(
        "Some numbers couldn't be read. Include the country code for numbers outside North America."
      );
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !save.isPending) props.onClose();
      }}
      position="center"
      class="w-110"
      visibleScrim
    >
      <ActionDialogShell>
        <ContactPhoneNumbersForm
          value={value()}
          onChange={(next) => {
            setValue(next);
            setError(undefined);
          }}
          onCancel={props.onClose}
          onSave={() => void submit()}
          saving={save.isPending}
          error={error()}
        />
      </ActionDialogShell>
    </Dialog>
  );
}
