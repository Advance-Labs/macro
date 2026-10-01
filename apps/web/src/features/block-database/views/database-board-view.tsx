import { toast } from '@core/component/Toast/Toast';
import { engineFailure } from '@core/database-sql/driver';
import type { Outcome } from '@core/database-sql/generated/types';
import { loadDatabaseSqlWasm } from '@core/database-sql/wasm-module';
import type { ResultError } from '@core/util/result';
import type { DatabaseSchemaErrorCode } from '@service-storage/databases';
import type { CardPosition } from '@service-storage/generated/schemas/cardPosition';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import type { ViewLayout } from '@service-storage/generated/schemas/viewLayout';
import { ConfirmDialog } from '@ui/components/ConfirmDialog';
import { Result, type ResultAsync } from 'neverthrow';
import {
  type Accessor,
  createMemo,
  createSignal,
  type JSX,
  Show,
} from 'solid-js';
import {
  DatabaseBoard,
  type DatabaseBoardControls,
} from '../components/database-board';
import type { DatabaseRowsSource } from '../context/table-source';
import {
  type CardMove,
  cardMove,
  laneCards,
  placeCard,
  withMovedCards,
  withPositions,
} from '../core/board-moves';
import type { DatabaseViewColumn } from '../core/database-view';
import type { DatabaseRow } from '../core/table';
import { withLaneHidden, withLaneOrder } from '../core/views';
import {
  type DatabaseOpFailure,
  databaseOpMessage,
} from '../core/write-failure';
import type { ViewChange } from '../queries/views';

type BoardLayout = Extract<ViewLayout, { kind: 'board' }>;

/** Where a board's card places come from, and how a move is written. */
export type BoardPositions = {
  positions: Accessor<CardPosition[] | undefined>;
  setPositions: (change: (positions: CardPosition[]) => CardPosition[]) => void;
  move: (
    view: DatabaseView,
    move: CardMove
  ) => ResultAsync<CardPosition[], DatabaseOpFailure>;
};

/**
 * A board laid out by the engine from the view's rows and card places. A move
 * shows at once and settles on the server's places; on a sorted board a drag first asks to remove the sort.
 */
export function DatabaseBoardView(props: {
  view: DatabaseView;
  layout: BoardLayout;
  source: DatabaseRowsSource;
  rows: DatabaseRow[];
  columns: DatabaseViewColumn[];
  groupColumn: DatabaseViewColumn;
  positions: BoardPositions;
  canEdit: boolean;
  onViewChange?: (change: ViewChange) => void;
  rowPending: (rowId: string) => boolean;
  createPending: (intentId: string) => boolean;
  createComplete: (intentId: string) => boolean;
  onOpen: (rowId: string) => void;
  onCreate: (
    lane: string | null,
    title: string,
    intentId: string,
    options?: { open: true }
  ) => Promise<boolean>;
  onAddGroup?: (
    label: string
  ) => Promise<Result<void, ResultError<DatabaseSchemaErrorCode>[]>>;
  renderTextValue?: (value: string) => JSX.Element;
  controlsRef?: (controls: DatabaseBoardControls) => void;
}) {
  const [engine, setEngine] =
    createSignal<Awaited<ReturnType<typeof loadDatabaseSqlWasm>>>();
  void (async () => {
    const loaded = await loadDatabaseSqlWasm();
    setEngine(() => loaded);
  })();
  /** Moves shown ahead of the rows: until the first answer after the server took them. */
  const [moves, setMoves] = createSignal<
    { move: CardMove; takenBefore?: Outcome }[]
  >([]);
  /** A drop on a sorted board, held until the sort goes: where in its lane it landed. */
  const [sortedDrop, setSortedDrop] = createSignal<{
    row: string;
    lane: string | null;
    index: number;
  }>();
  /** The board as the engine lays it out, sorted as the view says or, with `unsorted`, by hand. */
  const layOut = (unsorted: boolean) => {
    const wasm = engine();
    const read = props.source.read();
    const positions = props.positions.positions();
    if (!wasm || !read || !positions) return undefined;
    const query = unsorted ? { ...read.view.query, sort: [] } : read.view.query;
    return Result.fromThrowable(wasm.board, engineFailure)(
      read.catalog,
      { ...read.view, query, layout: props.layout },
      read.outcome,
      positions
    );
  };
  const laidOut = createMemo(() => layOut(false));
  const board = () => {
    const result = laidOut();
    if (!result?.isOk()) return undefined;
    const outcome = props.source.read()?.outcome;
    const pending = moves().flatMap((entry) =>
      entry.takenBefore && entry.takenBefore !== outcome ? [] : [entry.move]
    );
    return withMovedCards(result.value, pending);
  };
  function forget(move: CardMove) {
    setMoves((current) => current.filter((entry) => entry.move !== move));
  }
  function taken(move: CardMove) {
    const outcome = props.source.read()?.outcome;
    setMoves((current) =>
      current.map((entry) =>
        entry.move === move ? { ...entry, takenBefore: outcome } : entry
      )
    );
  }
  function write(move: CardMove) {
    const wasm = engine();
    const shown = board();
    const positions = props.positions.positions();
    if (wasm && shown && positions)
      props.positions.setPositions((current) =>
        withPositions(
          current,
          placeCard(
            laneCards(shown, positions, move.lane, move.row),
            move,
            wasm.keyBetween
          )
        )
      );
    setMoves((current) => [...current, { move }]);
    void props.positions
      .move(props.view, move)
      .map((placed) => {
        props.positions.setPositions((current) =>
          withPositions(current, placed)
        );
        taken(move);
      })
      .mapErr((failure) => {
        forget(move);
        toast.failure(databaseOpMessage(failure, 'this card'));
      });
  }
  function drop(row: string, lane: string | null, next?: string) {
    const shown = board();
    const move = shown && cardMove(shown, row, lane, next);
    if (!move) return;
    if (!props.view.query.sort?.length) {
      write(move);
      return;
    }
    const others = (
      shown.lanes.find((entry) => entry.option === lane)?.cards ?? []
    ).filter((card) => card !== row);
    setSortedDrop({
      row,
      lane,
      index: next === undefined ? others.length : others.indexOf(next),
    });
  }
  /** Without the sort the lane shows its arranged order; the card lands at the index it was dropped at. */
  function removeSortAndMove() {
    const dropped = sortedDrop();
    setSortedDrop(undefined);
    if (!dropped) return;
    props.onViewChange?.({ query: { ...props.view.query, sort: [] } });
    const laid = layOut(true);
    if (!laid?.isOk()) return;
    const arranged = laid.value;
    const others = (
      arranged.lanes.find((entry) => entry.option === dropped.lane)?.cards ?? []
    ).filter((card) => card !== dropped.row);
    const move = cardMove(
      arranged,
      dropped.row,
      dropped.lane,
      others[dropped.index]
    );
    if (move) write(move);
  }
  return (
    <>
      <Show
        when={board()}
        fallback={
          <Show when={laidOut()?.isErr()}>
            <p role="alert" class="px-5 py-8 text-sm text-ink-muted">
              This board could not be laid out. Try refreshing the table.
            </p>
          </Show>
        }
      >
        {(shown) => (
          <DatabaseBoard
            rows={props.rows}
            columns={props.columns}
            board={shown()}
            layout={props.layout}
            groupColumn={props.groupColumn}
            renderTextValue={props.renderTextValue}
            canEdit={props.canEdit}
            rowPending={props.rowPending}
            createPending={props.createPending}
            createComplete={props.createComplete}
            onOpen={props.onOpen}
            onMove={drop}
            onLaneOrderChange={
              props.onViewChange
                ? (order) =>
                    props.onViewChange?.({
                      layout: withLaneOrder(props.layout, order),
                    })
                : undefined
            }
            onHideLane={
              props.onViewChange
                ? (lane) =>
                    props.onViewChange?.({
                      layout: withLaneHidden(props.layout, lane, true),
                    })
                : undefined
            }
            onHideEmptyLanes={
              props.onViewChange
                ? (hideEmptyLanes) =>
                    props.onViewChange?.({
                      layout: { ...props.layout, hideEmptyLanes },
                    })
                : undefined
            }
            onCreate={props.onCreate}
            onAddGroup={props.onAddGroup}
            controlsRef={props.controlsRef}
          />
        )}
      </Show>
      <ConfirmDialog
        open={!!sortedDrop()}
        onOpenChange={(open) => {
          if (!open) setSortedDrop(undefined);
        }}
        title="Remove sort to arrange cards manually?"
        confirmLabel="Remove sort"
        onConfirm={removeSortAndMove}
        body={
          <p>
            This board is sorted, so its cards keep the sort’s order. Without
            the sort, cards show in the order you arrange them, starting with
            this one where you dropped it.
          </p>
        }
      />
    </>
  );
}
