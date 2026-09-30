import {
  ContextMenuContent,
  MenuItem,
  MenuSeparator,
} from '@core/component/ContextMenu';
import { ContextMenu } from '@kobalte/core/context-menu';
import { Tabs } from '@kobalte/core/tabs';
import ArrowLeftIcon from '@phosphor/arrow-left.svg';
import ArrowRightIcon from '@phosphor/arrow-right.svg';
import PlusIcon from '@phosphor/plus.svg';
import TableIcon from '@phosphor/table.svg';
import { Key } from '@solid-primitives/keyed';
import { createResizeObserver } from '@solid-primitives/resize-observer';
import {
  createDraggable,
  createDroppable,
  DragDropProvider,
  DragDropSensors,
  DragOverlay,
  useDragDropContext,
} from '@thisbeyond/solid-dnd';
import { Button } from '@ui/components/Button';
import { Tooltip } from '@ui/components/Tooltip';
import {
  createEffect,
  createSignal,
  createUniqueId,
  type JSX,
  on,
  onCleanup,
  Show,
} from 'solid-js';
import { createDragAutoScroll } from '../../../components/drag-drop/create-drag-auto-scroll';
import { isDatabaseNameTaken } from '../core/property-creation';
import type { CreateTable } from '../core/table-creation';
import { CreateTableDialog } from './create-table-dialog';

export function TableNavigation(props: {
  tables: { id: string; name: string }[];
  activeTableId: string | undefined;
  canCreate: boolean;
  onSelect: (tableId: string) => void;
  onCreate: CreateTable;
  onRename?: (
    tableId: string,
    name: string,
    previousName: string
  ) => Promise<void>;
  /** Persist a new tab order: every table id, left to right. */
  onReorder?: (tableIds: string[]) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [tabRail, setTabRail] = createSignal<HTMLDivElement>();
  const [renaming, setRenaming] = createSignal<{
    id: string;
    name: string;
    width: string;
    origin?: HTMLElement;
  }>();
  const [renameDraft, setRenameDraft] = createSignal('');
  const [renamePending, setRenamePending] = createSignal(false);
  const [renameError, setRenameError] = createSignal('');
  const renameErrorId = createUniqueId();
  const [menuTarget, setMenuTarget] = createSignal<{
    table: { id: string; name: string };
    origin: HTMLElement;
  }>();
  let menuTrigger: HTMLSpanElement | undefined;
  let renameInput: HTMLInputElement | undefined;
  let createButton: HTMLButtonElement | undefined;
  const canRename = () => props.canCreate && !!props.onRename;
  const canReorder = () =>
    props.canCreate && !!props.onReorder && props.tables.length > 1;
  const [tabDrop, setTabDrop] = createSignal<{
    targetId: string;
    edge: 'before' | 'after';
    left: number;
  }>();
  const [tabPreview, setTabPreview] = createSignal<HTMLElement>();
  let pointerOrigin: { x: number; y: number } | undefined;
  const moveTable = (
    tableId: string,
    targetId: string,
    edge: 'before' | 'after'
  ) => {
    const order = props.tables.map((table) => table.id);
    const remaining = order.filter((id) => id !== tableId);
    const target = remaining.indexOf(targetId);
    if (!order.includes(tableId) || target < 0) return;
    remaining.splice(target + Number(edge === 'after'), 0, tableId);
    if (remaining.join() === order.join()) return;
    props.onReorder?.(remaining);
  };
  const neighbour = (tableId: string | undefined, direction: -1 | 1) => {
    const index = props.tables.findIndex((table) => table.id === tableId);
    return index < 0 ? undefined : props.tables[index + direction];
  };
  const moveBy = (tableId: string, direction: -1 | 1) => {
    const target = neighbour(tableId, direction);
    if (target)
      moveTable(tableId, target.id, direction < 0 ? 'before' : 'after');
  };
  const rename = (
    table: { id: string; name: string },
    origin?: HTMLElement
  ) => {
    if (!canRename() || renamePending()) return;
    setRenameDraft(table.name);
    setRenameError('');
    const width = origin?.getBoundingClientRect().width;
    setRenaming({
      id: table.id,
      name: table.name,
      width: width
        ? `${width}px`
        : `${Math.max(10, Math.min(24, table.name.length + 5))}ch`,
      origin,
    });
    queueMicrotask(() => {
      renameInput?.focus();
      renameInput?.select();
    });
  };
  const finishRename = (restoreFocus: boolean) => {
    const origin = renaming()?.origin;
    setRenaming(undefined);
    setRenameError('');
    if (restoreFocus) queueMicrotask(() => origin?.focus());
  };
  const saveRename = async (restoreFocus: boolean) => {
    const target = renaming();
    if (!target || renamePending() || !props.onRename) return;
    const name = renameDraft().trim();
    if (!name) {
      setRenameError('Enter a table name.');
      return;
    }
    if (
      isDatabaseNameTaken(
        name,
        props.tables
          .filter((table) => table.id !== target.id)
          .map((table) => table.name)
      )
    ) {
      setRenameError(
        'A table with this name already exists. Try another name.'
      );
      return;
    }
    if (name === target.name) {
      finishRename(restoreFocus);
      return;
    }
    setRenamePending(true);
    setRenameError('');
    try {
      await props.onRename(target.id, name, target.name);
      finishRename(restoreFocus);
    } catch (error) {
      setRenameError(
        error instanceof Error
          ? error.message
          : 'Could not rename this table. Try again.'
      );
    } finally {
      setRenamePending(false);
    }
  };
  const openMenu = (
    table: { id: string; name: string },
    origin: HTMLElement,
    x: number,
    y: number
  ) => {
    if (!canRename() && !canReorder()) return;
    setMenuTarget({ table: { ...table }, origin });
    menuTrigger?.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: x,
        clientY: y,
      })
    );
  };
  const revealSelectedTable = () => {
    const rail = tabRail();
    const selected =
      rail?.querySelector<HTMLElement>('input[aria-label="Table name"]') ??
      rail?.querySelector<HTMLElement>('[data-selected]');
    if (!rail || !selected) return;
    const viewport = rail.getBoundingClientRect();
    const tab = selected.getBoundingClientRect();
    if (tab.left < viewport.left) rail.scrollLeft += tab.left - viewport.left;
    else if (tab.right > viewport.right)
      rail.scrollLeft += tab.right - viewport.right;
  };
  createResizeObserver(tabRail, revealSelectedTable);
  createEffect(
    on(
      [
        () => props.activeTableId,
        () => props.tables,
        () => renaming()?.id,
        tabRail,
      ],
      () => {
        // Kobalte applies the selected attribute while rendering the tab list.
        queueMicrotask(revealSelectedTable);
      }
    )
  );
  // Keep the menu outside Tabs because both primitives own a DOM collection.
  return (
    <ContextMenu>
      <ContextMenu.Trigger
        as="span"
        ref={menuTrigger}
        class="hidden"
        aria-hidden="true"
      />
      <div class="flex min-w-0 items-center gap-1.5">
        <span class="sr-only">Tables</span>
        <Show
          when={props.tables.length > 0}
          fallback={
            <span class="min-w-0 flex-1 text-xs text-ink-placeholder">
              No tables yet
            </span>
          }
        >
          <DragDropProvider
            collisionDetector={(draggable, droppables) => {
              const rail = tabRail();
              if (!pointerOrigin || !rail || !canReorder()) {
                setTabDrop(undefined);
                return null;
              }
              const x = pointerOrigin.x + draggable.transform.x;
              const y = pointerOrigin.y + draggable.transform.y;
              const viewport = rail.getBoundingClientRect();
              if (
                x < viewport.left ||
                x > viewport.right ||
                y < viewport.top - DROP_SLACK_PX ||
                y > viewport.bottom + DROP_SLACK_PX
              ) {
                setTabDrop(undefined);
                return null;
              }
              const tabs = droppables
                .map((droppable) => ({
                  droppable,
                  bounds: droppable.node.getBoundingClientRect(),
                }))
                .sort((a, b) => a.bounds.left - b.bounds.left);
              const target = (
                tabs.find(({ bounds }) => x <= bounds.right) ?? tabs.at(-1)
              )?.droppable;
              if (!target) {
                setTabDrop(undefined);
                return null;
              }
              const bounds = target.node.getBoundingClientRect();
              const edge =
                x < bounds.left + bounds.width / 2 ? 'before' : 'after';
              const from = props.tables.findIndex(
                (table) => table.id === String(draggable.id)
              );
              const to = props.tables.findIndex(
                (table) => table.id === String(target.id)
              );
              const insertion = to + Number(edge === 'after');
              if (
                from < 0 ||
                to < 0 ||
                insertion === from ||
                insertion === from + 1
              ) {
                setTabDrop(undefined);
                return null;
              }
              setTabDrop({
                targetId: String(target.id),
                edge,
                left:
                  (edge === 'before' ? bounds.left : bounds.right) -
                  viewport.left +
                  rail.scrollLeft,
              });
              return target;
            }}
            onDragStart={({ draggable }) => {
              const bounds = draggable.node.getBoundingClientRect();
              const copy = draggable.node.cloneNode(true) as HTMLElement;
              copy.removeAttribute('id');
              copy
                .querySelectorAll('[id]')
                .forEach((node) => node.removeAttribute('id'));
              copy.style.width = `${bounds.width}px`;
              copy.style.height = `${bounds.height}px`;
              copy.inert = true;
              copy.setAttribute('aria-hidden', 'true');
              copy.setAttribute('data-tab-drag-preview', '');
              setTabPreview(copy);
            }}
            onDragEnd={({ draggable }) => {
              const drop = tabDrop();
              setTabDrop(undefined);
              setTabPreview(undefined);
              pointerOrigin = undefined;
              if (canReorder() && drop)
                moveTable(String(draggable.id), drop.targetId, drop.edge);
            }}
          >
            <TabDragSensors
              onCancel={() => setTabDrop(undefined)}
              rail={tabRail}
            />
            <Tabs
              value={props.activeTableId ?? ''}
              onChange={props.onSelect}
              activationMode="manual"
              class="min-w-0"
            >
              <Tabs.List
                ref={setTabRail}
                aria-label="Database tables"
                class="relative flex min-h-8 items-center gap-0.5 overflow-x-auto"
              >
                <Key each={props.tables} by="id">
                  {(table) => (
                    <DraggableTab
                      id={table().id}
                      canDrag={canReorder() && renaming()?.id !== table().id}
                      onDragPointerDown={(event) => {
                        pointerOrigin = { x: event.clientX, y: event.clientY };
                      }}
                      style={{
                        width:
                          renaming()?.id === table().id
                            ? renaming()?.width
                            : undefined,
                      }}
                    >
                      <Tooltip label={table().name}>
                        <Tabs.Trigger
                          value={table().id}
                          aria-haspopup={
                            canRename() || canReorder() ? 'menu' : undefined
                          }
                          aria-keyshortcuts={
                            canRename() ? 'F2 Shift+F10' : undefined
                          }
                          onDblClick={(event) => {
                            if (!canRename()) return;
                            event.preventDefault();
                            event.stopPropagation();
                            rename(table(), event.currentTarget);
                          }}
                          onContextMenu={(event) => {
                            if (!canRename() && !canReorder()) return;
                            event.preventDefault();
                            event.stopPropagation();
                            openMenu(
                              table(),
                              event.currentTarget,
                              event.clientX,
                              event.clientY
                            );
                          }}
                          onKeyDown={(event) => {
                            if (!canRename() && !canReorder()) return;
                            if (event.key === 'F2' && canRename()) {
                              event.preventDefault();
                              event.stopPropagation();
                              rename(table(), event.currentTarget);
                            } else if (
                              event.key === 'ContextMenu' ||
                              (event.shiftKey && event.key === 'F10')
                            ) {
                              event.preventDefault();
                              event.stopPropagation();
                              const bounds =
                                event.currentTarget.getBoundingClientRect();
                              openMenu(
                                table(),
                                event.currentTarget,
                                bounds.left,
                                bounds.bottom
                              );
                            }
                          }}
                          class="relative flex h-8 max-w-40 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-xs text-ink-muted outline-none hover:bg-hover hover:text-ink focus-visible:ring-2 focus-visible:ring-ink/50 data-selected:bg-hover data-selected:font-medium data-selected:text-ink"
                          classList={{ hidden: renaming()?.id === table().id }}
                        >
                          <TableIcon class="size-3.5 shrink-0" />
                          <span class="truncate">{table().name}</span>
                        </Tabs.Trigger>
                      </Tooltip>
                      <Show when={renaming()?.id === table().id}>
                        <input
                          ref={renameInput}
                          aria-label="Table name"
                          aria-invalid={!!renameError()}
                          aria-describedby={
                            renameError() ? renameErrorId : undefined
                          }
                          aria-busy={renamePending()}
                          maxlength={200}
                          value={renameDraft()}
                          readOnly={renamePending()}
                          onFocusIn={(event) => event.stopPropagation()}
                          onMouseDown={(event) => event.stopPropagation()}
                          class="h-8 w-full min-w-0 rounded-md border border-ink/40 bg-input px-2 text-xs text-ink outline-none"
                          onInput={(event) => {
                            setRenameDraft(event.currentTarget.value);
                            setRenameError('');
                          }}
                          onBlur={(event) => {
                            event.stopPropagation();
                            void saveRename(false);
                          }}
                          onKeyDown={(event) => {
                            event.stopPropagation();
                            if (event.isComposing || event.keyCode === 229)
                              return;
                            if (event.key === 'Enter') {
                              event.preventDefault();
                              void saveRename(true);
                            } else if (event.key === 'Escape') {
                              event.preventDefault();
                              if (!renamePending()) finishRename(true);
                            }
                          }}
                        />
                      </Show>
                    </DraggableTab>
                  )}
                </Key>
                <Show when={tabDrop()}>
                  {(drop) => (
                    <div
                      aria-hidden="true"
                      data-tab-drop-indicator
                      data-drop-target={drop().targetId}
                      data-drop-edge={drop().edge}
                      class="pointer-events-none absolute inset-y-1 z-2 w-0.5 -translate-x-1/2 bg-accent"
                      style={{ left: `${drop().left}px` }}
                    />
                  )}
                </Show>
              </Tabs.List>
            </Tabs>
            <DragOverlay
              class="pointer-events-none select-none rounded-md bg-panel shadow-md"
              style={{ 'z-index': 1000 }}
            >
              {tabPreview()}
            </DragOverlay>
          </DragDropProvider>
        </Show>
        <Show when={props.canCreate}>
          <Button
            ref={createButton}
            type="button"
            size="sm"
            variant={props.tables.length ? 'ghost' : 'strong'}
            class="shrink-0 gap-1.5 text-xs focus-visible:ring-2 focus-visible:ring-ink/50"
            onClick={() => setOpen(true)}
          >
            <PlusIcon class="size-3.5" />
            New table
          </Button>
        </Show>
      </div>
      <Show when={renameError()}>
        <p id={renameErrorId} role="alert" class="mt-1 text-xs text-failure">
          {renameError()}
        </p>
      </Show>
      <ContextMenu.Portal>
        <ContextMenuContent
          class="min-w-44"
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            if (!renaming()) menuTarget()?.origin.focus();
          }}
        >
          <MenuItem
            text="Rename table"
            closeOnSelect
            shortcut="F2"
            disabled={!canRename()}
            onClick={() => {
              const target = menuTarget();
              if (target) rename(target.table, target.origin);
            }}
          />
          <Show when={props.onReorder}>
            <MenuSeparator />
            <MenuItem
              text="Move left"
              icon={ArrowLeftIcon}
              closeOnSelect
              disabled={
                !canReorder() || !neighbour(menuTarget()?.table.id, -1)
              }
              onClick={() => {
                const target = menuTarget();
                if (target) moveBy(target.table.id, -1);
              }}
            />
            <MenuItem
              text="Move right"
              icon={ArrowRightIcon}
              closeOnSelect
              disabled={
                !canReorder() || !neighbour(menuTarget()?.table.id, 1)
              }
              onClick={() => {
                const target = menuTarget();
                if (target) moveBy(target.table.id, 1);
              }}
            />
          </Show>
        </ContextMenuContent>
      </ContextMenu.Portal>
      <Show when={open()}>
        <CreateTableDialog
          existingNames={props.tables.map((table) => table.name)}
          onCreate={props.onCreate}
          onOpenTable={props.onSelect}
          onClose={() => setOpen(false)}
          returnFocus={createButton}
        />
      </Show>
    </ContextMenu>
  );
}

/** How far above or below the tab strip a drag may stray and still drop. */
const DROP_SLACK_PX = 24;

function DraggableTab(props: {
  id: string;
  canDrag: boolean;
  onDragPointerDown: (event: MouseEvent) => void;
  style: JSX.CSSProperties;
  children: JSX.Element;
}) {
  const draggable = createDraggable(props.id);
  const droppable = createDroppable(props.id);
  return (
    <div
      ref={(element) => {
        draggable.ref(element);
        droppable.ref(element);
      }}
      class="flex shrink-0 items-center"
      classList={{ 'opacity-40': draggable.isActiveDraggable }}
      style={props.style}
      onMouseDown={(event) => {
        if (
          !props.canDrag ||
          event.button !== 0 ||
          (event.target instanceof Element && event.target.closest('input'))
        )
          return;
        props.onDragPointerDown(event);
        draggable.dragActivators.onmousedown?.(event);
      }}
    >
      {props.children}
    </div>
  );
}

function TabDragSensors(props: {
  onCancel: () => void;
  rail: () => HTMLElement | undefined;
}) {
  const context = useDragDropContext();
  if (!context) throw new Error('TabDragSensors requires DragDropProvider');
  const [state, actions] = context;
  createDragAutoScroll({ getViewport: props.rail, axis: 'x' });
  const cancelDrag = () => {
    if (!state.active.draggable) return;
    props.onCancel();
    actions.dragEnd();
  };
  const cancel = (event: KeyboardEvent) => {
    if (event.key !== 'Escape' || !state.active.draggable) return;
    event.preventDefault();
    event.stopPropagation();
    cancelDrag();
  };
  document.addEventListener('keydown', cancel, true);
  window.addEventListener('blur', cancelDrag);
  onCleanup(() => {
    document.removeEventListener('keydown', cancel, true);
    window.removeEventListener('blur', cancelDrag);
  });
  return <DragDropSensors />;
}
