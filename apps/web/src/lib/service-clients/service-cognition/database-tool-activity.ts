/** Receipts are produced by the server from executed tools, never model output. */
export function summarizeDatabaseActivity(
  activity: {
    name: string;
    success: boolean;
    changesApplied?: number | null;
  }[]
): string | undefined {
  const successful = activity.filter((entry) => entry.success);
  const count = (name: string) =>
    successful.filter((entry) => entry.name === name).length;
  const summaries: string[] = [];
  for (const [tool, verb, noun] of [
    ['CreateDatabase', 'Created', 'database'],
    ['CreateTable', 'Created', 'table'],
    ['AddColumn', 'Added', 'column'],
    ['SaveDatabaseView', 'Saved', 'view'],
    ['RenameTable', 'Renamed', 'table'],
    ['RenameColumn', 'Renamed', 'column'],
    ['ChangeColumnType', 'Changed the type of', 'column'],
    ['DeleteColumn', 'Deleted', 'column'],
    ['ReorderColumns', 'Reordered columns in', 'table'],
    ['DeleteTable', 'Deleted', 'table'],
    ['RenameDatabase', 'Renamed', 'database'],
  ]) {
    const total = count(tool);
    if (total)
      summaries.push(`${verb} ${total} ${noun}${total === 1 ? '' : 's'}`);
  }
  if (count('AddColumnOptions')) summaries.push('Added select options');
  const changed = successful.reduce(
    (total, entry) =>
      total +
      (entry.name === 'QueryDatabase' ? (entry.changesApplied ?? 0) : 0),
    0
  );
  if (changed)
    summaries.push(`Applied ${changed} row change${changed === 1 ? '' : 's'}`);
  return summaries.length ? `${summaries.join(' · ')}.` : undefined;
}
