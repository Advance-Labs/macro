import PlusIcon from '@phosphor/plus.svg';
import { createDatabaseColumn } from '@queries/storage/databases';
import type { DatabaseColumnDetail } from '@service-storage/databases';
import { Button } from '@ui/components/Button';
import { createSignal, Show } from 'solid-js';
import type { PropertyCreatorVariant } from '../components/property-creator';
import {
  type DatabasePropertyType,
  defaultDatabaseColumnName,
} from '../core/property-creation';

/**
 * Add a Text column with the next free default name at the table's end; its
 * type is inferred from what is typed. Returns the new column's id.
 */
export async function createDefaultColumn(args: {
  databaseId: string;
  tableId: string;
  columns: DatabaseColumnDetail[];
}): Promise<string> {
  const name = defaultDatabaseColumnName(
    args.columns.map(
      (entry) =>
        entry.column.display_name ?? entry.definition.definition.display_name
    )
  );
  const id = await createDatabaseColumn({
    databaseId: args.databaseId,
    tableId: args.tableId,
    request: {
      infer_type: true,
      binding: {
        kind: 'new',
        name,
        data_type: 'STRING',
        is_multi_select: false,
      },
    },
  });
  if (!id) throw new Error('Could not add this column.');
  return id;
}

/** A new column starts as Text; name and type are edited in its header. */
export function AddColumnMenu(props: {
  databaseId: string;
  tableId: string;
  columns: DatabaseColumnDetail[];
  label?: string;
  variant?: PropertyCreatorVariant;
  initialType?: DatabasePropertyType;
  onCreated?: (columnId: string) => boolean;
}) {
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal('');
  async function add() {
    if (pending()) return;
    setPending(true);
    setError('');
    try {
      const id = await createDefaultColumn(props);
      props.onCreated?.(id);
    } catch (error) {
      setError(
        error instanceof Error ? error.message : 'Could not add this column.'
      );
    } finally {
      setPending(false);
    }
  }
  return (
    <div class="relative">
      <Button
        variant="ghost"
        size="sm"
        disabled={pending()}
        aria-label={props.label ?? 'Add column'}
        onClick={() => void add()}
      >
        <PlusIcon class="size-3.5" />
        {props.label ?? 'Add column'}
      </Button>
      <Show when={error()}>
        <span
          role="alert"
          class="absolute top-full right-0 z-2 w-52 rounded-md border border-edge-muted bg-panel p-2 text-xs text-failure-ink shadow-md"
        >
          {error()}
        </span>
      </Show>
    </div>
  );
}
