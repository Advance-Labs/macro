import { StaticMarkdownContext } from '@core/component/LexicalMarkdown/component/core/StaticMarkdown';
import { refreshInBackground } from '@queries/database-sql/create-database-sql-query';
import {
  addDatabaseColumnOptions,
  applyDatabaseOps,
  applyDatabaseTableVersions,
  useDatabaseDetailQuery,
} from '@queries/storage/databases';
import {
  useDatabaseAwareness,
  useDatabaseTableChanges,
} from '@queries/storage/databases-sync';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import type { TableDetail } from '@service-storage/generated/schemas/tableDetail';
import {
  createMemo,
  createSignal,
  For,
  type JSX,
  Show,
  Suspense,
} from 'solid-js';
import { DatabaseRelationCell } from '../components/database-relation-cell';
import type { DatabaseCellFocus } from '../components/database-table';
import {
  type OptionEditing,
  OptionEditingContext,
} from '../context/option-editing';
import { mergeDatabaseColumnOrder } from '../core/column-order';
import type { DatabaseRelatedDestination } from '../core/database-relations';
import {
  DatabaseMentionPicker,
  DatabaseMentionValue,
  DatabaseTextEditor,
  DatabaseTextValue,
} from '../database-mentions';
import { createColumnCasts } from '../queries/column-casts';
import { updateDatabaseColumns } from '../queries/column-schema';
import { createDatabaseRelations } from '../queries/database-relations';
import { useRelatedDatabaseSync } from '../queries/database-relations-sync';
import { deleteDatabaseOption, updateDatabaseOption } from '../queries/options';
import { renameDatabaseColumn } from '../queries/rename-column';
import { createDatabaseRowsSource } from '../queries/table-rows';
import {
  moveDatabaseCard,
  refreshCardPositions,
  setCardPositions,
  useCardPositions,
  type ViewChange,
} from '../queries/views';
import {
  type DatabaseTableActions,
  DatabaseTableView,
} from '../views/database-table-view';
import { AddColumnMenu, createDefaultColumn } from './AddColumnMenu';

export type DatabaseGridProps = {
  databaseId: string;
  table: TableDetail;
  canEdit: boolean;
  /** The view on screen: a stored one, or the table's own All records. */
  view: DatabaseView;
  stored: boolean;
  search: string;
  onViewChange?: (change: ViewChange) => void;
  onClearConstraints?: () => void;
  renderToolbar?: (actions: DatabaseTableActions) => JSX.Element;
  onOpenRelated?: (destination: DatabaseRelatedDestination) => void;
};

/** Production composition. A table switch owns a new query/controller lifetime. */
export function DatabaseGrid(props: DatabaseGridProps) {
  return (
    <Suspense
      fallback={<div class="p-6 text-xs text-ink-muted">Loading records…</div>}
    >
      <Show when={props.table.table.id} keyed>
        {(tableId) => <TableAdapter {...props} tableId={tableId} />}
      </Show>
    </Suspense>
  );
}

function TableAdapter(props: DatabaseGridProps & { tableId: string }) {
  // Keep the final schema for this table available to already-queued writes
  // after its tab is closed; a new selected table must never redirect them.
  let ownedTable = props.table;
  const table = () => {
    if (props.table.table.id === props.tableId) ownedTable = props.table;
    return ownedTable;
  };
  const databaseId = props.databaseId;
  const detail = useDatabaseDetailQuery(() => databaseId);
  const relatedTargets = () =>
    table().columns.flatMap((column) =>
      column.column.config?.kind === 'link' &&
      column.column.config.database_id !== databaseId
        ? [column.column.config]
        : []
    );
  const relatedDatabases = () => [
    ...new Set(relatedTargets().map((target) => target.database_id)),
  ];
  const relations = createDatabaseRelations({
    targets: () =>
      table().columns.flatMap((column) =>
        column.column.config?.kind === 'link'
          ? [
              {
                databaseId: column.column.config.database_id,
                tableId: column.column.config.table_id,
              },
            ]
          : []
      ),
    onTableChanged: (listener) =>
      useDatabaseTableChanges((change) => listener(change.tableId)),
  });
  const source = createDatabaseRowsSource({
    databaseId,
    table,
    view: () => props.view,
    search: () => props.search,
    applyOps: (ops) => applyDatabaseOps(databaseId, ops),
    onTableChanged: (listener) =>
      useDatabaseTableChanges((change) => {
        if (change.tableId !== props.tableId) return;
        listener(change.version);
        if (props.stored && props.view.layout.kind === 'board')
          void refreshCardPositions(databaseId, props.view.id);
      }),
    applyVersions: (versions) =>
      applyDatabaseTableVersions(databaseId, versions),
    addOption: (columnId, label) =>
      addDatabaseColumnOptions({
        databaseId,
        tableId: props.tableId,
        columnId,
        labels: [label],
      }).map(() => undefined),
  });
  const boardViewId = () =>
    props.stored && props.view.layout.kind === 'board'
      ? props.view.id
      : undefined;
  const positions = useCardPositions(databaseId, boardViewId);
  const optionEditing: OptionEditing = {
    update: (columnId, optionId, change) =>
      updateDatabaseOption(
        { databaseId, tableId: props.tableId, columnId, optionId },
        change
      ),
    remove: (columnId, optionId) =>
      deleteDatabaseOption({
        databaseId,
        tableId: props.tableId,
        columnId,
        optionId,
      }),
  };
  const [focusedCell, setFocusedCell] = createSignal<DatabaseCellFocus>();
  const awareness = useDatabaseAwareness(
    () => databaseId,
    () => ({ tableId: props.tableId, ...focusedCell() })
  );
  const remoteUsers = () =>
    awareness.remote().filter((user) => user.tableId === props.tableId);
  const rawColumns = source.columns;
  source.columns = createMemo(() =>
    rawColumns().map((column) =>
      column.relation
        ? {
            ...column,
            relation: {
              ...column.relation,
              labels: Object.fromEntries(
                relations(column.relation.tableId)
                  .rows()
                  .map((row) => [row.id, row.name])
              ),
            },
          }
        : column
    )
  );
  return (
    <>
      <For each={relatedDatabases()}>
        {(id) => {
          useRelatedDatabaseSync(id, () => {
            for (const target of relatedTargets())
              if (target.database_id === id)
                refreshInBackground(relations(target.table_id));
          });
          return null;
        }}
      </For>
      <StaticMarkdownContext>
        <OptionEditingContext.Provider
          value={props.canEdit ? optionEditing : undefined}
        >
          <DatabaseTableView
            name={table().table.name}
            source={source}
            canEdit={props.canEdit}
            view={props.view}
            stored={props.stored}
            search={props.search}
            onViewChange={props.onViewChange}
            onClearConstraints={props.onClearConstraints}
            boardPositions={{
              positions: () =>
                positions.isSuccess ? positions.data : undefined,
              setPositions: (change) => {
                const viewId = boardViewId();
                if (viewId) setCardPositions(databaseId, viewId, change);
              },
              move: moveDatabaseCard,
            }}
            onCellFocus={setFocusedCell}
            remoteUsers={remoteUsers()}
            renderTextEditor={(editor) => <DatabaseTextEditor {...editor} />}
            renderTextValue={(value) => <DatabaseTextValue value={value} />}
            renderMentionPicker={(picker) => (
              <DatabaseMentionPicker {...picker} />
            )}
            renderMentionValue={(id, entityType) => (
              <DatabaseMentionValue id={id} entityType={entityType} />
            )}
            renderRelationCell={(cell) => (
              <DatabaseRelationCell
                {...cell}
                source={relations(cell.column.relation!.tableId)}
                onOpen={(rowId) =>
                  props.onOpenRelated?.({ ...cell.column.relation!, rowId })
                }
              />
            )}
            relationTables={
              detail.isSuccess
                ? detail.data.tables.map(({ table }) => ({
                    id: table.id,
                    name: table.name,
                  }))
                : []
            }
            columnCasts={createColumnCasts({
              databaseId,
              tableId: props.tableId,
              version: () => table().table.version,
            })}
            onChangeColumnType={(columnId, change) =>
              updateDatabaseColumns({
                databaseId,
                tableId: props.tableId,
                baseVersion: table().table.version,
                mutation: { kind: 'type', columnId, change },
              })
            }
            onDeleteColumn={(columnId) =>
              updateDatabaseColumns({
                databaseId,
                tableId: props.tableId,
                baseVersion: table().table.version,
                mutation: { kind: 'delete', columnId },
              })
            }
            onReorderColumns={(columnIds) => {
              const current = table();
              return updateDatabaseColumns({
                databaseId,
                tableId: props.tableId,
                baseVersion: current.table.version,
                mutation: {
                  kind: 'order',
                  columnIds: mergeDatabaseColumnOrder(
                    current.columns.map(({ column }) => column.id),
                    columnIds
                  ),
                },
              });
            }}
            onRenameColumn={(columnId, name, previousName) =>
              renameDatabaseColumn({
                databaseId,
                tableId: props.tableId,
                columnId,
                name,
                previousName,
              })
            }
            renderToolbar={props.renderToolbar}
            createColumn={() =>
              createDefaultColumn({
                databaseId,
                tableId: props.tableId,
                columns: table().columns,
              })
            }
            addColumn={(label, onCreated) => (
              <AddColumnMenu
                databaseId={databaseId}
                tableId={props.tableId}
                columns={table().columns}
                label={label}
                onCreated={onCreated}
              />
            )}
          />
        </OptionEditingContext.Provider>
      </StaticMarkdownContext>
    </>
  );
}
