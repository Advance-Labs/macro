/**
 * A table's typed views and a board's card places, written as ops. Each
 * change shows in the cached detail before the server answers; a refusal
 * reads the detail again.
 */
import type {
  CardPosition,
  DatabaseView,
  NewView,
  ViewLayout,
  ViewQuery,
} from '@core/database-sql/generated/types';
import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import { storageServiceClient } from '@service-storage/client';
import { useQuery } from '@tanstack/solid-query';
import { Mutex } from 'async-mutex';
import { ResultAsync } from 'neverthrow';
import type { Accessor } from 'solid-js';
import type { CardMove } from '../core/board-moves';
import type { DatabaseOpFailure } from '../core/write-failure';
import { applyOp, patchViews } from './detail-cache';
import { databaseViewKeys } from './keys';

const viewWrites = new Map<string, Mutex>();

/** One view's writes reach the server in the order they were made. */
function inOrder<Value>(
  viewId: string,
  write: () => ResultAsync<Value, DatabaseOpFailure>
): ResultAsync<Value, DatabaseOpFailure> {
  let mutex = viewWrites.get(viewId);
  if (!mutex) {
    mutex = new Mutex();
    viewWrites.set(viewId, mutex);
  }
  return new ResultAsync(mutex.runExclusive(async () => await write()));
}

export function createDatabaseView(
  databaseId: string,
  tableId: string,
  view: NewView
): ResultAsync<DatabaseView, DatabaseOpFailure> {
  return applyOp(
    databaseId,
    tableId,
    { kind: 'create_view', table: tableId, view },
    'view_written'
  ).map(({ view: created }) => {
    patchViews(databaseId, tableId, (views) => [
      ...views.filter((existing) => existing.id !== created.id),
      created,
    ]);
    return created;
  });
}

export type ViewChange = {
  name?: string;
  query?: ViewQuery;
  layout?: ViewLayout;
};

/** Change a view; a later change made before this one answers keeps its place. */
export function updateDatabaseView(
  view: DatabaseView,
  change: ViewChange
): ResultAsync<DatabaseView, DatabaseOpFailure> {
  const shown = { ...view, ...change };
  patchViews(view.databaseId, view.tableId, (views) =>
    views.map((existing) => (existing.id === view.id ? shown : existing))
  );
  return inOrder(view.id, () =>
    applyOp(
      view.databaseId,
      view.tableId,
      { kind: 'update_view', table: view.tableId, view: view.id, ...change },
      'view_written'
    )
  ).map(({ view: stored }) => {
    patchViews(view.databaseId, view.tableId, (views) =>
      views.map((existing) => (existing === shown ? stored : existing))
    );
    return stored;
  });
}

export function deleteDatabaseView(
  view: DatabaseView
): ResultAsync<void, DatabaseOpFailure> {
  patchViews(view.databaseId, view.tableId, (views) =>
    views.filter((existing) => existing.id !== view.id)
  );
  return applyOp(
    view.databaseId,
    view.tableId,
    { kind: 'delete_view', table: view.tableId, view: view.id },
    'view_deleted'
  ).map(() => undefined);
}

/** Put a table's views in `order`, which names every one of them once. */
export function reorderDatabaseViews(
  databaseId: string,
  tableId: string,
  order: string[]
): ResultAsync<void, DatabaseOpFailure> {
  const ordered = (views: DatabaseView[]) =>
    order.flatMap((id) => views.filter((view) => view.id === id));
  patchViews(databaseId, tableId, ordered);
  return applyOp(
    databaseId,
    tableId,
    { kind: 'reorder_views', table: tableId, order },
    'views_reordered'
  ).map(({ positions }) => {
    patchViews(databaseId, tableId, (views) =>
      ordered(views).map((view) => ({
        ...view,
        position:
          positions.find((entry) => entry.view === view.id)?.position ??
          view.position,
      }))
    );
  });
}

/** Where a board's cards sit, read again when its table changes. */
export function useCardPositions(
  databaseId: string,
  viewId: Accessor<string | undefined>
) {
  return useQuery(() => {
    const id = viewId();
    return {
      queryKey: databaseViewKeys.positions(databaseId, id ?? '').queryKey,
      queryFn: async () => {
        const { positions } = await throwOnErr(() =>
          storageServiceClient.databases.viewPositions({
            id: databaseId,
            viewId: id ?? '',
          })
        );
        return positions;
      },
      enabled: !!id,
    };
  });
}

export function setCardPositions(
  databaseId: string,
  viewId: string,
  change: (positions: CardPosition[]) => CardPosition[]
) {
  queryClient.setQueryData<CardPosition[]>(
    databaseViewKeys.positions(databaseId, viewId).queryKey,
    (previous) => change(previous ?? [])
  );
}

export function refreshCardPositions(databaseId: string, viewId: string) {
  return queryClient.invalidateQueries({
    queryKey: databaseViewKeys.positions(databaseId, viewId).queryKey,
  });
}

/**
 * Move a board's card, which also sets its row's grouping cell. The places
 * the server wrote replace the ones shown ahead of its answer.
 */
export function moveDatabaseCard(
  view: DatabaseView,
  move: CardMove
): ResultAsync<CardPosition[], DatabaseOpFailure> {
  return inOrder(view.id, () =>
    applyOp(
      view.databaseId,
      view.tableId,
      {
        kind: 'move_card',
        table: view.tableId,
        view: view.id,
        row: move.row,
        lane: move.lane,
        before: move.before,
        after: move.after,
      },
      'card_moved'
    )
  )
    .map(({ positions }) => positions)
    .mapErr((failure) => {
      void refreshCardPositions(view.databaseId, view.id);
      return failure;
    });
}
