import Copy from '@phosphor/copy.svg';
import DotsThree from '@phosphor/list.svg';
import TextT from '@phosphor/text-t.svg';
import TrashSimple from '@phosphor/trash-simple.svg';
import { Dropdown } from '@ui';
import { createSignal, useContext } from 'solid-js';
import { LexicalWrapperContext } from '../../../context/LexicalWrapperContext';

/** Copy / Convert to text / Delete, behind a `⋯` trigger. */
export function PasteActionsMenu(props: {
  onCopy: () => void;
  onConvertToText: () => void;
  onDelete: () => void;
  class?: string;
}) {
  const [open, setOpen] = createSignal(false);
  const wrapper = useContext(LexicalWrapperContext);
  const portalMount = () => wrapper?.portalMount?.();

  return (
    <div class={props.class} on:click={(e) => e.stopPropagation()}>
      <Dropdown open={open()} onOpenChange={setOpen}>
        <Dropdown.Trigger size="icon-sm" variant="ghost">
          <DotsThree />
        </Dropdown.Trigger>
        <Dropdown.Content mount={portalMount()}>
          <Dropdown.Group>
            <Dropdown.Item onSelect={props.onCopy}>
              <Copy class="size-4 shrink-0" />
              <span class="flex-1 truncate">Copy</span>
            </Dropdown.Item>
            <Dropdown.Item onSelect={props.onConvertToText}>
              <TextT class="size-4 shrink-0" />
              <span class="flex-1 truncate">Convert to text</span>
            </Dropdown.Item>
          </Dropdown.Group>
          <Dropdown.Group>
            <Dropdown.Item onSelect={props.onDelete}>
              <TrashSimple class="size-4 shrink-0" />
              <span class="flex-1 truncate">Delete</span>
            </Dropdown.Item>
          </Dropdown.Group>
        </Dropdown.Content>
      </Dropdown>
    </div>
  );
}
