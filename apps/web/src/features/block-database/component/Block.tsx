import { openChatWithInput } from '@app/features/chat/ChatWithAgentButton';
import { toQuerySchema } from '@app/features/database-query/queries/query-source';
import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { makePersistedState } from '@app/lib/persistence';
import { useGlobalBlockOrchestrator } from '@components/app/GlobalAppState';
import { SidePanel } from '@components/app/side-panel';
import { useSplitLayout } from '@components/app/split-layout/layout';
import {
  returnSplitToRecentListView,
  useCanAutofocusSplitContent,
  useSplitPanelOrThrow,
} from '@components/app/split-layout/layoutUtils';
import { useNavigatedFromJK } from '@components/app/useNavigatedFromJK';
import { useHasPaidAccess } from '@core/auth/license';
import { useBlockId } from '@core/block';
import { DATABASE_MODEL, modelsForPlan } from '@core/component/AI/constant';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import { toast } from '@core/component/Toast/Toast';
import { enableDatabases } from '@core/constant/featureFlags';
import { PaywallKey, usePaywallState } from '@core/constant/PaywallState';
import { useUserId } from '@core/context/user';
import { createMethodRegistration } from '@core/orchestrator';
import { blockHandleSignal } from '@core/signal/load';
import { createUserScopedStorage } from '@core/util/userScopedStorage';
import { useDatabaseDetailQuery } from '@queries/storage/databases';
import { useDatabaseTableChangedSync } from '@queries/storage/databases-sync';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import { getEntityGraphqlClient } from '@service-storage/graphql-soup';
import { Button } from '@ui';
import {
  type Component,
  createMemo,
  createSignal,
  ErrorBoundary,
  For,
  Show,
  untrack,
} from 'solid-js';
import { DatabaseToolbar } from '../components/database-toolbar';
import type { NewViewLayout } from '../components/new-view-dialog';
import { databaseChatContext } from '../core/chat-context';
import type { DatabaseRelatedDestination } from '../core/database-relations';
import {
  type DatabaseViewSelection,
  readViewSelection,
} from '../core/view-selection';
import { allRecordsView, boardLayout } from '../core/views';
import { databaseOpMessage } from '../core/write-failure';
import { toViewColumn } from '../queries/table-rows';
import { trashDatabase } from '../queries/trash-database';
import {
  createDatabaseView,
  deleteDatabaseView,
  reorderDatabaseViews,
  updateDatabaseView,
  type ViewChange,
} from '../queries/views';
import { AddColumnMenu } from './AddColumnMenu';
import { DatabaseGrid } from './DatabaseGrid';
import { DatabaseSidePanelSections } from './sidepanel/DatabaseSidePanelSections';
import { TopBar } from './TopBar';

const Block: Component = () => {
  const databaseId = useBlockId();
  const panel = useSplitPanelOrThrow();
  const { replaceOrInsertSplit } = useSplitLayout();
  const orchestrator = useGlobalBlockOrchestrator();
  let requestedRecord: DatabaseRelatedDestination | undefined;
  const canAutofocus = useCanAutofocusSplitContent();
  const { navigatedFromJK } = useNavigatedFromJK();
  const userId = untrack(useUserId());
  const storage = createUserScopedStorage(
    `database-view-selection:${databaseId}`
  );
  const [selection, setSelection] = makePersistedState(
    createSignal<DatabaseViewSelection>({ views: {} }),
    {
      storages: {
        restore: () => {
          if (!userId) return;
          return readViewSelection(storage.read(userId));
        },
        write: (value) => {
          if (userId) storage.write(userId, JSON.stringify(value));
        },
      },
    }
  );
  useDatabaseTableChangedSync(() => databaseId);
  const detailQuery = useDatabaseDetailQuery(() => databaseId);
  let gridEntry:
    | {
        tableId: string;
        focus: () => Promise<void>;
        openRecord: (rowId: string) => void;
      }
    | undefined;
  function openRequestedRecord() {
    const target = requestedRecord;
    if (!target || !gridEntry || gridEntry.tableId !== target.tableId) return;
    requestedRecord = undefined;
    gridEntry.openRecord(target.rowId);
  }
  async function openRelated(target: DatabaseRelatedDestination) {
    if (target.databaseId !== databaseId) {
      replaceOrInsertSplit({
        type: 'database',
        id: target.databaseId,
      });
      try {
        const handle = await orchestrator.getBlockHandle(
          target.databaseId,
          'database'
        );
        await handle?.goToLocationFromParams({
          tableId: target.tableId,
          rowId: target.rowId,
        });
      } catch {
        toast.failure('This related record could not be opened.');
      }
      return;
    }
    requestedRecord = target;
    setSelection((current) => ({ ...current, tableId: target.tableId }));
    queueMicrotask(openRequestedRecord);
  }
  createMethodRegistration(blockHandleSignal.get, {
    goToLocationFromParams: (params: Record<string, string>) => {
      if (params.tableId && params.viewId)
        setSelection((current) => ({
          ...current,
          tableId: params.tableId,
          views: { ...current.views, [params.tableId]: params.viewId },
        }));
      if (params.tableId && params.rowId)
        void openRelated({
          databaseId,
          tableId: params.tableId,
          rowId: params.rowId,
        });
    },
  });
  let requestedGridEntry = false;
  const enterFirstCell = () => {
    if (!gridEntry || gridEntry.tableId !== activeTableId()) {
      requestedGridEntry = true;
      return;
    }
    requestedGridEntry = false;
    void gridEntry.focus();
  };
  const [openingChat, setOpeningChat] = createSignal(false);
  const hasPaidAccess = useHasPaidAccess();
  const { showPaywall } = usePaywallState();
  async function openDatabaseChat() {
    const current = detail();
    if (!current || openingChat()) return;
    setOpeningChat(true);
    // A locked model gets the picker's treatment: the chat opens on the plan's
    // model and the paywall says why.
    const canUseDatabaseModel = modelsForPlan(hasPaidAccess()).includes(
      DATABASE_MODEL
    );
    try {
      await openChatWithInput(
        databaseChatContext(toQuerySchema(current, activeTableId())),
        canUseDatabaseModel ? { model: DATABASE_MODEL } : undefined
      );
      if (!canUseDatabaseModel) showPaywall(PaywallKey.O1_LIMIT);
    } finally {
      setOpeningChat(false);
    }
  }
  // Reading data only after status resolves keeps the database shell mounted.
  const detail = () => (!detailQuery.isPending ? detailQuery.data : undefined);
  const tables = () => detail()?.tables ?? [];
  const activeTable = createMemo(
    () =>
      tables().find((table) => table.table.id === selection().tableId) ??
      tables()[0]
  );
  const activeTableId = () => activeTable()?.table.id;
  const canEdit = () =>
    detail()?.grant === 'edit' || detail()?.grant === 'owner';
  const columns = () =>
    activeTable()
      ?.columns.filter((column) => column.column.config?.kind !== 'lookup')
      .map(toViewColumn) ?? [];
  const storedViews = () => activeTable()?.views ?? [];
  const selectedView = () => {
    const tableId = activeTableId();
    const id = tableId ? selection().views[tableId] : undefined;
    return storedViews().find((view) => view.id === id);
  };
  /** All records, as this viewer has filtered, sorted and laid it out, per table. */
  const [allRecords, setAllRecords] = createSignal<
    Record<string, DatabaseView>
  >({});
  const [searches, setSearches] = createSignal<Record<string, string>>({});
  const search = () => {
    const tableId = activeTableId();
    return tableId ? (searches()[tableId] ?? '') : '';
  };
  const view = (): DatabaseView | undefined => {
    const table = activeTable();
    if (!table) return undefined;
    return (
      selectedView() ??
      allRecords()[table.table.id] ??
      allRecordsView(table.table)
    );
  };
  function setSearch(value: string) {
    const tableId = activeTableId();
    if (tableId) setSearches((current) => ({ ...current, [tableId]: value }));
  }
  function selectView(id?: string) {
    const tableId = activeTableId();
    if (!tableId) return;
    setSelection((current) => {
      const views = { ...current.views };
      if (id) views[tableId] = id;
      else delete views[tableId];
      return { ...current, views };
    });
  }
  function changeView(change: ViewChange) {
    const current = view();
    if (!current) return;
    const stored = selectedView();
    if (stored) {
      void updateDatabaseView(stored, change).mapErr((failure) =>
        toast.failure(databaseOpMessage(failure, 'this view'))
      );
      return;
    }
    setAllRecords((views) => ({
      ...views,
      [current.tableId]: { ...current, ...change },
    }));
  }
  function createView(
    current: DatabaseView,
    name: string,
    layout: NewViewLayout,
    groupBy: string | undefined
  ) {
    return createDatabaseView(databaseId, current.tableId, {
      name,
      query: current.query,
      layout:
        layout === 'board' && groupBy
          ? boardLayout(groupBy, columns())
          : current.layout.kind === 'table'
            ? current.layout
            : { kind: 'table', columns: [] },
    }).map((created) => selectView(created.id));
  }

  return (
    <DocumentBlockContainer>
      <SidePanel.Layout defaultOpen={false}>
        <DatabaseSidePanelSections
          databaseId={databaseId}
          database={detail()?.database}
        />
        <TopBar
          detail={detail()}
          activeTable={activeTable()}
          autoFocusTitle={
            canAutofocus &&
            !navigatedFromJK() &&
            detail()?.database.name === 'Untitled database'
          }
          onTitleConfirm={enterFirstCell}
          onSelectTable={(tableId) =>
            setSelection((current) => ({ ...current, tableId }))
          }
          onDelete={() =>
            trashDatabase(getEntityGraphqlClient(), databaseId).map(() =>
              returnSplitToRecentListView(panel.handle)
            )
          }
          openingChat={openingChat()}
          onOpenChat={() => void openDatabaseChat()}
        />
        <div
          class="@container/database flex size-full min-h-0 min-w-0 flex-col overflow-hidden bg-canvas-base text-ink"
          style={{ '--database-title-column-width': '18rem' }}
        >
          <ErrorBoundary
            fallback={(error: unknown, reset) => (
              <div
                class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center"
                role="alert"
              >
                <p class="font-medium">This database could not be displayed</p>
                <p class="max-w-md text-sm text-ink-muted">
                  {error instanceof Error
                    ? error.message
                    : 'Please try opening it again.'}
                </p>
                <Button
                  variant="outline"
                  onClick={() => {
                    reset();
                    void detailQuery.refetch();
                  }}
                >
                  Try again
                </Button>
              </div>
            )}
          >
            <Show when={!detailQuery.isPending} fallback={<DatabaseSkeleton />}>
              <Show
                when={detail()}
                fallback={
                  <div
                    class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center"
                    role="alert"
                  >
                    <p class="font-medium">Could not open this database</p>
                    <p class="max-w-md text-sm text-ink-muted">
                      It may be unavailable, or you may no longer have access.
                    </p>
                    <Button
                      variant="outline"
                      onClick={() => void detailQuery.refetch()}
                    >
                      Try again
                    </Button>
                  </div>
                }
              >
                <Show
                  when={activeTable()}
                  fallback={
                    <div class="grid flex-1 place-items-center p-6 text-sm text-ink-muted">
                      Add a table to start organizing your data.
                    </div>
                  }
                >
                  <div class="flex min-h-0 min-w-0 flex-1 flex-col @min-[1000px]/database:flex-row">
                    <div class="flex min-h-0 min-w-0 flex-1 flex-col">
                      <Show
                        when={
                          activeTable() && view()
                            ? { table: activeTable()!, view: view()! }
                            : undefined
                        }
                      >
                        {(shown) => {
                          const table = () => shown().table;
                          return (
                            <DatabaseGrid
                              databaseId={databaseId}
                              table={table()}
                              canEdit={canEdit()}
                              view={shown().view}
                              stored={!!selectedView()}
                              search={search()}
                              onViewChange={changeView}
                              onClearConstraints={() => {
                                setSearch('');
                                changeView({
                                  query: {
                                    ...shown().view.query,
                                    filter: null,
                                  },
                                });
                              }}
                              onOpenRelated={openRelated}
                              renderToolbar={(actions) => {
                                gridEntry = {
                                  tableId: table().table.id,
                                  focus: actions.focusFirstCell,
                                  openRecord: actions.openRecord,
                                };
                                if (requestedRecord)
                                  queueMicrotask(openRequestedRecord);
                                if (requestedGridEntry)
                                  queueMicrotask(enterFirstCell);
                                return (
                                  <DatabaseToolbar
                                    columns={columns()}
                                    views={storedViews()}
                                    view={shown().view}
                                    selectedViewId={selectedView()?.id}
                                    canEdit={canEdit()}
                                    search={search()}
                                    onSearchChange={setSearch}
                                    onSelectView={selectView}
                                    onChangeView={changeView}
                                    onCreateView={(name, layout, groupBy) =>
                                      createView(
                                        shown().view,
                                        name,
                                        layout,
                                        groupBy
                                      )
                                    }
                                    onRenameView={(target, name) =>
                                      updateDatabaseView(target, { name }).map(
                                        () => undefined
                                      )
                                    }
                                    onDeleteView={(target) => {
                                      if (selectedView()?.id === target.id)
                                        selectView();
                                      return deleteDatabaseView(target);
                                    }}
                                    onReorderViews={(order) =>
                                      void reorderDatabaseViews(
                                        databaseId,
                                        table().table.id,
                                        order
                                      ).mapErr((failure) =>
                                        toast.failure(
                                          databaseOpMessage(
                                            failure,
                                            'these views'
                                          )
                                        )
                                      )
                                    }
                                    onCreateRecord={
                                      canEdit()
                                        ? () => void actions.createRecord()
                                        : undefined
                                    }
                                    canCreateRecord={columns().length > 0}
                                    creating={actions.pending()}
                                    addColumn={
                                      <Show when={canEdit()}>
                                        <AddColumnMenu
                                          databaseId={databaseId}
                                          tableId={table().table.id}
                                          columns={table().columns}
                                          label="Add column"
                                          onCreated={actions.focusColumn}
                                        />
                                      </Show>
                                    }
                                  />
                                );
                              }}
                            />
                          );
                        }}
                      </Show>
                    </div>
                  </div>
                </Show>
              </Show>
            </Show>
          </ErrorBoundary>
        </div>
      </SidePanel.Layout>
    </DocumentBlockContainer>
  );
};

function DatabaseSkeleton() {
  return (
    <div
      class="flex min-h-0 flex-1 flex-col gap-3 px-6 py-3"
      aria-busy="true"
      aria-label="Loading database"
    >
      <div class="mb-3 flex gap-2">
        <For each={[0, 1]}>
          {() => <div class="h-7 w-24 animate-pulse rounded-md bg-hover" />}
        </For>
      </div>
      <For each={[0, 1, 2, 3, 4, 5]}>
        {() => <div class="h-9 animate-pulse rounded-md bg-hover" />}
      </For>
    </div>
  );
}

const DatabaseBlock: Component = () => {
  const flag = useFeatureFlag(enableDatabases);
  return (
    <Show
      when={flag().enabled}
      fallback={
        <div class="grid size-full place-items-center p-6 text-sm text-ink-muted">
          Databases are not enabled for this account.
        </div>
      }
    >
      <Block />
    </Show>
  );
};

export default DatabaseBlock;
