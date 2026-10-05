import { type Accessor, createEffect, on, onCleanup } from 'solid-js';

export type DatabaseTarget = {
  databaseId?: string;
  tableId?: string;
  viewId?: string;
  rowId?: string;
  seek?: string;
};

type Grid = {
  tableId: string;
  openRecord: (rowId: string, isCurrent?: () => boolean) => void;
};

/** Route validity belongs to navigation; row delivery belongs to the grid. */
export function createDatabaseNavigation(input: {
  databaseId: Accessor<string>;
  target: Accessor<DatabaseTarget>;
  select: (target: { tableId: string; viewId?: string }) => void;
}) {
  let generation = 0;
  let queued:
    | { tableId: string; rowId: string; generation: number }
    | undefined;
  let grid: Grid | undefined;
  function deliver() {
    const row = queued;
    const receiver = grid;
    if (!row || !receiver || receiver.tableId !== row.tableId) return;
    queued = undefined;
    receiver.openRecord(
      row.rowId,
      () => generation === row.generation && grid === receiver
    );
  }
  function request(target: DatabaseTarget) {
    const current = ++generation;
    queued = undefined;
    if (
      !target.tableId ||
      (target.databaseId && target.databaseId !== input.databaseId())
    )
      return;
    queued = target.rowId
      ? { tableId: target.tableId, rowId: target.rowId, generation: current }
      : undefined;
    input.select({ tableId: target.tableId, viewId: target.viewId });
    deliver();
  }
  createEffect(
    on(
      () => {
        const target = input.target();
        return [
          target.databaseId,
          target.tableId,
          target.viewId,
          target.rowId,
          target.seek,
          input.databaseId(),
        ] as const;
      },
      (target, previous) => {
        // Search subscriptions also wake for other namespaces. Only a changed
        // target or seek token is a new route request.
        if (
          previous &&
          target.every((value, index) => value === previous[index])
        )
          return;
        const [databaseId, tableId, viewId, rowId] = target;
        request({ databaseId, tableId, viewId, rowId });
      }
    )
  );
  function register(entry: Grid) {
    grid = entry;
    deliver();
    onCleanup(() => {
      if (grid === entry) grid = undefined;
    });
  }
  onCleanup(() => {
    generation++;
    queued = undefined;
    grid = undefined;
  });
  return { request, register };
}
