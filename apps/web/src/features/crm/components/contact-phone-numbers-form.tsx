import { ActionDialogShell, Button, TextField } from '@ui';
import { Show } from 'solid-js';

/** Editing a contact's phone numbers: one number per line. */
export function ContactPhoneNumbersForm(props: {
  value: string;
  onChange: (value: string) => void;
  onCancel: () => void;
  onSave: () => void;
  saving: boolean;
  error?: string;
}) {
  return (
    <form
      class="flex min-h-0 flex-col"
      aria-busy={props.saving}
      onSubmit={(event) => {
        event.preventDefault();
        if (!props.saving) props.onSave();
      }}
    >
      <ActionDialogShell.Body>
        <ActionDialogShell.Header>
          <ActionDialogShell.Title>Phone numbers</ActionDialogShell.Title>
          <ActionDialogShell.Description>
            One number per line, with the country code. North American numbers
            can leave out +1.
          </ActionDialogShell.Description>
        </ActionDialogShell.Header>
        <TextField
          value={props.value}
          onChange={props.onChange}
          validationState={props.error ? 'invalid' : 'valid'}
        >
          <TextField.Label>Numbers</TextField.Label>
          <TextField.TextArea
            autoResize
            rows={3}
            placeholder={'(555) 234-5678\n+44 20 7946 0958'}
            readOnly={props.saving}
            class="tabular-nums"
          />
          <Show when={props.error}>
            <TextField.ErrorMessage>{props.error}</TextField.ErrorMessage>
          </Show>
        </TextField>
      </ActionDialogShell.Body>
      <ActionDialogShell.Footer>
        <Button
          type="button"
          variant="ghost"
          depth={2}
          disabled={props.saving}
          onClick={props.onCancel}
        >
          Cancel
        </Button>
        <Button
          type="submit"
          variant="strong"
          depth={2}
          disabled={props.saving}
        >
          {props.saving ? 'Saving…' : 'Save'}
        </Button>
      </ActionDialogShell.Footer>
    </form>
  );
}
