import { StaticMarkdownContext } from '@core/component/LexicalMarkdown/component/core/StaticMarkdown';
import { refreshInBackground } from '@queries/database-sql/create-database-sql-query';
import {
  addDatabaseColumnOptions,
  applyDatabaseTableVersions,
  execSql,
  useDatabaseDetailQuery,
} from '@queries/storage/databases';
import {
  useDatabaseAwareness,
  useDatabaseTableChanges,
} from '@queries/storage/databases-sync';
import type {
  DatabaseTableDetail,
  ExecRequest,
} from '@service-storage/databases';
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
import { mergeDatabaseColumnOrder } from '../core/column-order';
import type { DatabaseRelatedDestination } from '../core/database-relations';
import {
  type DatabaseViewConfig,
  defaultDatabaseView,
} from '../core/database-view';
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
import { renameDatabaseColumn } from '../queries/rename-column';
import { createDatabaseRowsSource } from '../queries/table-rows';
import {
  type DatabaseTableActions,
  DatabaseTableView,
} from '../views/database-table-view';
import { AddColumnMenu, createDefaultColumn } from './AddColumnMenu';

export type DatabaseGridProps = {
  databaseId: string;
  table: DatabaseTableDetail;
  canEdit: boolean;
  view?: DatabaseViewConfig;
  onViewChange?: (view: DatabaseViewConfig) => void;
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
  const exec = (request: ExecRequest) =>
    execSql({ ...request, scope: databaseId });
  const relations = createDatabaseRelations({
    columns: () => table().columns,
    onTableChanged: (listener) =>
      useDatabaseTableChanges((change) => listener(change.tableId)),
  });
  const source = createDatabaseRowsSource({
    databaseId,
    table,
    view: () => props.view ?? defaultDatabaseView(),
    exec,
    onTableChanged: (listener) =>
      useDatabaseTableChanges((change) => {
        if (change.tableId === props.tableId) listener(change.version);
      }),
    applyVersions: (versions) =>
      applyDatabaseTableVersions(databaseId, versions),
    addOption: async (columnId, label) => {
      const updated = await addDatabaseColumnOptions({
        databaseId,
        tableId: props.tableId,
        columnId,
        labels: [label],
      });
      if (!updated) throw new Error('That option could not be added.');
    },
  });
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
        <DatabaseTableView
          name={table().table.name}
          source={source}
          canEdit={props.canEdit}
          view={props.view ?? defaultDatabaseView()}
          onViewChange={props.onViewChange}
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
          addColumn={(label, initialType, variant, onCreated) => (
            <AddColumnMenu
              databaseId={databaseId}
              tableId={props.tableId}
              columns={table().columns}
              label={label}
              variant={variant}
              initialType={initialType}
              onCreated={onCreated}
            />
          )}
        />
      </StaticMarkdownContext>
    </>
  );
}
