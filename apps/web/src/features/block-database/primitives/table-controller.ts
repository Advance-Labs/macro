import type { ResultError } from '@core/util/result';
import type { DatabaseSchemaErrorCode } from '@service-storage/databases';
import { Mutex } from 'async-mutex';
import { err, ok, type Result } from 'neverthrow';
import { batch, createMemo, createSignal, onCleanup } from 'solid-js';
import type {
  DatabaseRowsSource,
  DatabaseWriteResult,
} from '../context/table-source';
import {
  type DatabaseRow,
  type DatabaseRowMutation,
  optimisticRows,
} from '../core/table';
import type { DatabaseWriteFailure } from '../core/write-failure';

type PendingWrite = {
  id: number;
  mutation: DatabaseRowMutation;
  createIntentId?: string;
};
export type FailedWrite = {
  mutation: DatabaseRowMutation;
  label: string;
  option?: string;
  createIntentId?: string;
  failure: DatabaseWriteFailure;
};

/** A save that did not land: the write failed, or the table was closed. */
export type DatabaseSaveFailure = DatabaseWriteFailure | { kind: 'unmounted' };

export type DatabaseSave = Result<DatabaseWriteResult, DatabaseSaveFailure>;

export type AcceptedDraftWrites = {
  save: (
    mutation: DatabaseRowMutation,
    label?: string,
    option?: string,
    createIntentId?: string
  ) => Promise<DatabaseSave>;
};

const UNMOUNTED = { kind: 'unmounted' } as const;

/** One writer per mounted table. A queued edit always uses the latest completed read/write. */
export function createTableController(
  source: DatabaseRowsSource,
  onSaved?: (mutation: DatabaseRowMutation, result: DatabaseWriteResult) => void
) {
  const mutex = new Mutex();
  const [completedCreates, setCompletedCreates] = createSignal(
    new Map<string, DatabaseWriteResult>()
  );
  const [pending, setPending] = createSignal<PendingWrite[]>([]);
  const [failures, setFailures] = createSignal<FailedWrite[]>([]);
  const [committed, setCommitted] = createSignal<
    {
      version: number | undefined;
      mutation: DatabaseRowMutation;
      insertedRowIds: string[];
    }[]
  >([]);
  const [refreshWarning, setRefreshWarning] = createSignal(false);
  const [schemaPending, setSchemaPending] = createSignal(0);
  let sequence = 0;
  let lastWrittenVersion: number | undefined;
  let disposed = false;
  const uncertainCreates = new Map<string, FailedWrite>();
  const uncertainMutations = new WeakMap<DatabaseRowMutation, FailedWrite>();
  onCleanup(() => {
    disposed = true;
  });

  async function save(
    mutation: DatabaseRowMutation,
    label = 'change',
    option?: string,
    createIntentId?: string
  ): Promise<DatabaseSave> {
    const completed = createIntentId && completedCreates().get(createIntentId);
    if (completed) return ok(completed);
    const id = ++sequence;
    setPending((writes) => [...writes, { id, mutation, createIntentId }]);
    let result: DatabaseSave;
    let didWrite = false;
    try {
      result = await mutex.runExclusive(async (): Promise<DatabaseSave> => {
        // A new table version cannot prove whether this INSERT committed.
        // Keep the same draft blocked even after refresh or banner dismissal.
        const uncertain =
          (createIntentId && uncertainCreates.get(createIntentId)) ||
          uncertainMutations.get(mutation);
        if (uncertain) {
          setFailures((failed) =>
            failed.includes(uncertain) ? failed : [...failed, uncertain]
          );
          return err(uncertain.failure);
        }
        // Draft submit and the error-banner Retry can be queued together.
        const completed =
          createIntentId && completedCreates().get(createIntentId);
        if (completed) return ok(completed);
        const readVersion = source.snapshot()?.version;
        const version =
          readVersion === undefined
            ? lastWrittenVersion
            : lastWrittenVersion === undefined
              ? readVersion
              : Math.max(readVersion, lastWrittenVersion);
        // A new option is created by the write that first uses it.
        const written = await source.write(
          mutation,
          version,
          option !== undefined
        );
        if (written.isErr()) {
          const failure = written.error;
          const outcomeUnknown = failure.kind === 'outcome-unknown';
          if (outcomeUnknown && (await source.refresh()).isErr())
            setRefreshWarning(true);
          const failed: FailedWrite = {
            mutation,
            label,
            option,
            createIntentId,
            failure,
          };
          if (outcomeUnknown) {
            uncertainMutations.set(mutation, failed);
            if (createIntentId) uncertainCreates.set(createIntentId, failed);
          }
          setFailures((failures) => [
            ...failures.filter(
              (entry) =>
                entry.mutation !== mutation &&
                (createIntentId === undefined ||
                  entry.createIntentId !== createIntentId)
            ),
            failed,
          ]);
          return err(failure);
        }
        didWrite = true;
        const saved = written.value;
        batch(() => {
          if (createIntentId)
            setCompletedCreates((creates) =>
              new Map(creates).set(createIntentId, saved)
            );
          lastWrittenVersion = saved.version ?? lastWrittenVersion;
          // A later successful edit must not hide an earlier rejected edit.
          setFailures((failed) =>
            failed.filter(
              (entry) =>
                entry.mutation !== mutation &&
                (createIntentId === undefined ||
                  entry.createIntentId !== createIntentId)
            )
          );
          setCommitted((writes) => [
            ...writes,
            {
              mutation,
              version: saved.version,
              insertedRowIds: saved.insertedRowIds,
            },
          ]);
        });
        // A failed refresh cannot turn a committed write into a failed edit.
        await refresh();
        return ok(saved);
      });
    } finally {
      setPending((writes) => writes.filter((write) => write.id !== id));
    }
    if (result.isOk() && didWrite && !disposed)
      onSaved?.(mutation, result.value);
    return result;
  }

  async function retry() {
    if (disposed) return;
    const failed = failures()[0];
    if (!failed) return;
    if (failed.failure.kind === 'outcome-unknown') {
      await refresh();
      return;
    }
    await save(
      failed.mutation,
      failed.label,
      failed.option,
      failed.createIntentId
    );
  }

  function pruneCommitted() {
    const version = source.snapshot()?.version;
    // A successful read can resolve before its reactive snapshot is published.
    setCommitted((writes) =>
      writes.filter(
        (write) =>
          write.version !== undefined &&
          (version === undefined || write.version > version)
      )
    );
  }

  async function refresh() {
    const refreshed = await source.refresh();
    setRefreshWarning(refreshed.isErr());
    if (refreshed.isOk()) pruneCommitted();
  }

  /** Schema changes share the row-write queue because both advance the table version. */
  async function addGroup(
    columnId: string,
    label: string
  ): Promise<Result<void, ResultError<DatabaseSchemaErrorCode>[]>> {
    setSchemaPending((count) => count + 1);
    try {
      return await mutex.runExclusive(async () => {
        const added = await source.addOption(columnId, label);
        // The option is saved even if its subsequent rows refresh fails.
        if (added.isOk()) await refresh();
        return added;
      });
    } finally {
      setSchemaPending((count) => count - 1);
    }
  }

  const unreadWrites = createMemo(() => {
    const version = source.snapshot()?.version;
    return committed().filter(
      (write) =>
        write.version === undefined ||
        version === undefined ||
        write.version > version
    );
  });
  const mutations = () => [
    ...unreadWrites().map((write) => write.mutation),
    ...pending().map((write) => write.mutation),
  ];

  const rows = createMemo(() => {
    const read = source.snapshot()?.rows ?? [];
    // Acknowledged inserts remain openable when only the follow-up read failed.
    // Whether they match the view is unknown until then, so they stay on screen.
    const created: DatabaseRow[] = unreadWrites().flatMap((write) => {
      if (write.mutation.kind !== 'create') return [];
      const cells = write.mutation.values;
      return write.insertedRowIds
        .filter((rowId) => !read.some((row) => row.rowId === rowId))
        .map((rowId) => ({ rowId, cells }));
    });
    return optimisticRows([...read, ...created], mutations());
  });

  return {
    /** The rows the view's statement returned, with local writes applied until they are read back. */
    rows,
    /** The view's rows, then the rows it retains by id that it does not show. */
    knownRows: createMemo(() => {
      const shown = rows();
      const retained = (source.snapshot()?.retained ?? []).filter(
        (row) => !shown.some((known) => known.rowId === row.rowId)
      );
      return [...shown, ...optimisticRows(retained, mutations())];
    }),
    pending: () => pending().length > 0 || schemaPending() > 0,
    createPending: (intentId: string) =>
      pending().some((write) => write.createIntentId === intentId),
    createComplete: (intentId: string) => completedCreates().has(intentId),
    createResult: (intentId: string) => completedCreates().get(intentId),
    createUncertain: (intentId: string) => uncertainCreates.has(intentId),
    rowPending: (rowId: string) =>
      pending().some(
        (write) =>
          write.mutation.kind !== 'create' && write.mutation.rowId === rowId
      ),
    failure: () => failures()[0],
    refreshWarning,
    save: (...args: Parameters<typeof save>): Promise<DatabaseSave> =>
      disposed ? Promise.resolve(err(UNMOUNTED)) : save(...args),
    // A draft may need its inserted row ID before it can submit later fields.
    // Admit the whole drain while mounted so those accepted writes survive a tab switch.
    runDraftWrites: (
      drain: (writes: AcceptedDraftWrites) => Promise<boolean>
    ) => (disposed ? Promise.resolve(false) : drain({ save })),
    retry,
    refresh,
    addGroup: (...args: Parameters<typeof addGroup>) =>
      disposed ? Promise.resolve(ok(undefined)) : addGroup(...args),
    dismissFailure: () => setFailures((failed) => failed.slice(1)),
  };
}
