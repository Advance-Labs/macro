import { Button } from '@ui/components/Button';
import { Dialog } from '@ui/components/Dialog';
import { Panel } from '@ui/components/Panel';
import { TextField } from '@ui/components/TextField';
import { createSignal } from 'solid-js';
import { isDatabaseNameTaken } from '../core/property-creation';

export function RenameTableDialog(props: {
  table: { id: string; name: string };
  otherNames: string[];
  onRename: (
    tableId: string,
    name: string,
    previousName: string
  ) => Promise<void>;
  onClose: () => void;
  returnFocus?: HTMLElement;
}) {
  const originalName = props.table.name;
  const [name, setName] = createSignal(originalName);
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal('');
  const duplicate = () => isDatabaseNameTaken(name(), props.otherNames);
  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (pending() || !name().trim() || duplicate()) return;
    setPending(true);
    setError('');
    try {
      if (name().trim() !== originalName)
        await props.onRename(props.table.id, name().trim(), originalName);
      props.onClose();
    } catch (error) {
      setError(
        error instanceof Error
          ? error.message
          : 'Could not rename this table. Try again.'
      );
    } finally {
      setPending(false);
    }
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !pending() && props.onClose()}
      onCloseAutoFocus={(event) => {
        if (props.returnFocus?.isConnected) {
          event.preventDefault();
          props.returnFocus.focus();
        }
      }}
      class="w-108 max-w-[calc(100vw-2rem)]"
    >
      <Panel>
        <Panel.Body>
          <form
            class="flex flex-col gap-5 p-6 [&_button:focus-visible]:ring-2 [&_button:focus-visible]:ring-ink/50"
            onSubmit={submit}
            aria-busy={pending()}
          >
            <div>
              <Dialog.Title class="text-lg font-semibold tracking-tight text-ink">
                Rename table
              </Dialog.Title>
              <Dialog.Description class="mt-1.5 text-sm text-ink-muted">
                Give this table a name that describes its records.
              </Dialog.Description>
            </div>
            <TextField
              value={name()}
              onChange={(value) => {
                setName(value);
                setError('');
              }}
              readOnly={pending()}
              required
              validationState={duplicate() || error() ? 'invalid' : 'valid'}
            >
              <TextField.Label>Table name</TextField.Label>
              <TextField.Input
                maxlength={200}
                onFocus={(event) => event.currentTarget.select()}
              />
              <TextField.ErrorMessage role="alert" class="text-sm">
                {duplicate()
                  ? 'A table with this name already exists. Try another name.'
                  : error()}
              </TextField.ErrorMessage>
            </TextField>
            <div class="flex justify-end gap-2">
              <Button
                type="button"
                variant="ghost"
                disabled={pending()}
                onClick={props.onClose}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                variant="strong"
                disabled={pending() || !name().trim() || duplicate()}
              >
                {pending() ? 'Saving…' : 'Save name'}
              </Button>
            </div>
          </form>
        </Panel.Body>
      </Panel>
    </Dialog>
  );
}
