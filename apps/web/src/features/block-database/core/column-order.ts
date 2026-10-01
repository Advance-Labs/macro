import { moveBeside } from './move-beside';

/** Insert at a visible header edge while preserving each hidden column's slot. */
export function reorderDatabaseColumns(
  order: readonly string[],
  hiddenColumns: readonly string[],
  columnId: string,
  targetId: string,
  edge: 'before' | 'after'
): string[] | undefined {
  const hidden = new Set(hiddenColumns);
  const visible = moveBeside(
    order.filter((id) => !hidden.has(id)),
    columnId,
    targetId,
    edge
  );
  if (!visible) return;
  const nextOrder = mergeDatabaseColumnOrder(order, visible);
  return nextOrder.some((id, index) => id !== order[index])
    ? nextOrder
    : undefined;
}

/** Apply a partial layout without moving omitted schema columns. */
export function mergeDatabaseColumnOrder(
  order: readonly string[],
  requestedOrder: readonly string[]
): string[] {
  const known = new Set(order);
  const requested = [...new Set(requestedOrder)].filter((id) => known.has(id));
  const included = new Set(requested);
  let index = 0;
  return order.map((id) => (included.has(id) ? requested[index++] : id));
}
