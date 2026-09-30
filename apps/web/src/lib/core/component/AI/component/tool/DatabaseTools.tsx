/**
 * Tool renderers for the Macro Databases toolset
 * (`crates/databases/src/inbound/toolset`).
 */

import { databaseViewKeys } from '@app/features/block-database/queries/keys';
import { ToolQueryResults } from '@app/features/database-query/components/tool-query-results';
import { globalSplitManager } from '@app/signal/splitLayout';
import { useGlobalBlockOrchestrator } from '@components/app/GlobalAppState';
import {
  StaticMarkdown,
  StaticMarkdownContext,
} from '@core/component/LexicalMarkdown/component/core/StaticMarkdown';
import DatabaseIcon from '@phosphor/database.svg';
import TableIcon from '@phosphor/table.svg';
import { queryClient } from '@queries/client';
import type { NamedTool } from '@service-cognition/generated/tools/tool';
import { createSignal, For, Show } from 'solid-js';
import { BaseTool } from './BaseTool';
import { Tool } from './Tool';
import { createToolRenderer } from './ToolRenderer';

type DatabaseSchema = NamedTool<'DescribeDatabase', 'response'>['data'];

function SchemaTableList(props: { schema: DatabaseSchema }) {
  return (
    <Tool.List>
      <For each={props.schema.tables}>
        {(table) => (
          <Tool.ListItem icon={<TableIcon class="size-3" />}>
            <div class="min-w-0 truncate text-ink text-xs">
              {table.name}
              <span class="pl-1.5 text-ink-extra-muted">
                {table.columns.map((column) => column.name).join(', ')}
              </span>
            </div>
          </Tool.ListItem>
        )}
      </For>
    </Tool.List>
  );
}

export const listDatabasesHandler = createToolRenderer({
  name: 'ListDatabases',
  render: (ctx) => {
    const [expanded, setExpanded] = createSignal(false);
    const databases = () => ctx.response?.data.databases ?? [];

    return (
      <BaseTool
        icon={DatabaseIcon}
        renderContext={ctx.renderContext}
        type="call"
        response={
          expanded() && databases().length > 0 ? (
            <Tool.List>
              <For each={databases()}>
                {(database) => (
                  <Tool.ListItem icon={<TableIcon class="size-3" />}>
                    <div class="min-w-0 text-ink text-xs">
                      <span class="block truncate">{database.name}</span>
                      <span class="block truncate text-ink-muted">
                        {database.tables?.map((table) => table.name).join(', ')}
                      </span>
                    </div>
                  </Tool.ListItem>
                )}
              </For>
            </Tool.List>
          ) : undefined
        }
      >
        <div class="flex min-w-0 flex-1 items-center justify-between gap-3">
          <span class="min-w-0 truncate">Find database tables</span>
          <Tool.ResultToggle
            expanded={expanded()}
            onToggle={() => setExpanded((open) => !open)}
            showToggle={databases().length > 0}
            status={
              ctx.response
                ? `${databases().length} database${databases().length === 1 ? '' : 's'}`
                : undefined
            }
          />
        </div>
      </BaseTool>
    );
  },
});

export const describeDatabaseHandler = createToolRenderer({
  name: 'DescribeDatabase',
  render: (ctx) => {
    const [expanded, setExpanded] = createSignal(false);
    const schema = () => ctx.response?.data;

    return (
      <BaseTool
        icon={DatabaseIcon}
        renderContext={ctx.renderContext}
        type="call"
        response={
          expanded() && schema() ? (
            <SchemaTableList schema={schema()!} />
          ) : undefined
        }
      >
        <div class="flex min-w-0 flex-1 items-center justify-between gap-3">
          <span class="min-w-0 truncate">
            Read table{' '}
            <span class="text-ink">
              {schema()?.name ?? ctx.tool.data.databaseId}
            </span>
          </span>
          <Tool.ResultToggle
            expanded={expanded()}
            onToggle={() => setExpanded((open) => !open)}
            showToggle={!!schema()}
            status={
              schema() ? `${schema()?.tables.length ?? 0} tabs` : undefined
            }
          />
        </div>
      </BaseTool>
    );
  },
});

export const queryDatabaseHandler = createToolRenderer({
  name: 'QueryDatabase',
  render: (ctx) => {
    const [expanded, setExpanded] = createSignal(true);
    const answer = () => {
      const data = ctx.response?.data;
      if (!data?.results.length) return undefined;
      return {
        results: data.results.map((result) => ({
          columns: result.columns.map((column) => ({
            name: column.name,
            entity_type: column.entityType ?? null,
          })),
          rows: result.rows.map((row) =>
            row.map((value) =>
              value == null
                ? null
                : typeof value === 'number' || typeof value === 'string'
                  ? value
                  : typeof value === 'boolean'
                    ? String(value)
                    : JSON.stringify(value)
            )
          ),
        })),
        read_tables: data.readVersions.map((table) => table.tableId),
        read_versions: Object.fromEntries(
          data.readVersions.map((table) => [table.tableId, table.version])
        ),
        truncated_tables: data.truncatedTables ?? [],
      };
    };
    return (
      <BaseTool
        icon={DatabaseIcon}
        renderContext={ctx.renderContext}
        type="call"
        response={
          expanded() && answer() ? (
            <ToolQueryResults
              answer={answer()!}
              sql={ctx.tool.data.sql}
              preferredDisplay={ctx.tool.data.display ?? undefined}
            />
          ) : undefined
        }
      >
        <div class="flex min-w-0 flex-1 items-center justify-between gap-3">
          <span class="min-w-0 truncate">
            {ctx.response?.data.summary || 'Query database'}
          </span>
          <Tool.ResultToggle
            expanded={expanded()}
            onToggle={() => setExpanded((open) => !open)}
            showToggle={!!answer()}
          />
        </div>
      </BaseTool>
    );
  },
});

export const createDatabaseHandler = createToolRenderer({
  name: 'CreateDatabase',
  render: (ctx) => (
    <BaseTool icon={DatabaseIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Create database <span class="text-ink">{ctx.tool.data.name}</span>
      </span>
    </BaseTool>
  ),
});

export const createTableHandler = createToolRenderer({
  name: 'CreateTable',
  render: (ctx) => (
    <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Create table <span class="text-ink">{ctx.tool.data.name}</span>
      </span>
    </BaseTool>
  ),
});

export const renameTableHandler = createToolRenderer({
  name: 'RenameTable',
  render: (ctx) => (
    <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Rename table to <span class="text-ink">{ctx.tool.data.name}</span>
      </span>
    </BaseTool>
  ),
});

export const addColumnHandler = createToolRenderer({
  name: 'AddColumn',
  render: (ctx) => {
    const options = () => ctx.tool.data.options ?? [];

    return (
      <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
        <span class="min-w-0 truncate">
          Add column <span class="text-ink">{ctx.tool.data.name}</span>
          <span class="pl-1.5 text-ink-extra-muted">
            {ctx.tool.data.dataType}
            <Show when={options().length > 0}>
              {' · '}
              {options().join(', ')}
            </Show>
          </span>
        </span>
      </BaseTool>
    );
  },
});

export const addColumnOptionsHandler = createToolRenderer({
  name: 'AddColumnOptions',
  render: (ctx) => (
    <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Add options{' '}
        <span class="text-ink">{ctx.tool.data.labels.join(', ')}</span>
        <Show when={ctx.response}>
          {(response) => (
            <span class="pl-1.5 text-ink-extra-muted">
              · now {response().data.options.length} options
            </span>
          )}
        </Show>
      </span>
    </BaseTool>
  ),
});

export const saveDatabaseViewHandler = createToolRenderer({
  name: 'SaveDatabaseView',
  handleResponse: async () => {
    await queryClient.invalidateQueries({
      queryKey: databaseViewKeys.saved.queryKey,
    });
  },
  render: (ctx) => {
    const orchestrator = useGlobalBlockOrchestrator();
    async function openView() {
      const result = ctx.response?.data;
      if (!result) return;
      globalSplitManager()?.openWithSplit(
        { type: 'database', id: ctx.tool.data.databaseId },
        { activate: true }
      );
      const handle = await orchestrator.getBlockHandle(
        ctx.tool.data.databaseId,
        'database'
      );
      await handle?.goToLocationFromParams({
        tableId: ctx.tool.data.tableId,
        viewId: result.viewId,
      });
    }
    return (
      <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
        <div class="flex min-w-0 flex-1 items-center justify-between gap-3">
          <span class="min-w-0 truncate">
            {ctx.response ? 'Saved' : 'Save'}{' '}
            {ctx.tool.data.view.layout === 'board' ? 'board' : 'view'}{' '}
            <span class="text-ink">
              {ctx.response?.data.name ?? ctx.tool.data.name}
            </span>
          </span>
          <Show when={ctx.response}>
            <button
              type="button"
              class="shrink-0 rounded px-2 py-1 text-xs text-ink hover:bg-hover"
              onClick={() => void openView()}
            >
              Open view
            </button>
          </Show>
        </div>
      </BaseTool>
    );
  },
});

function schemaTable(
  schema: DatabaseSchema | null | undefined,
  tableId: string
) {
  return schema?.tables.find((table) => table.id === tableId);
}

function schemaColumn(
  schema: DatabaseSchema | null | undefined,
  tableId: string,
  columnId: string
) {
  return schemaTable(schema, tableId)?.columns.find(
    (column) => column.id === columnId
  );
}

function Scope(props: { name: string | undefined; preposition: string }) {
  return (
    <Show when={props.name}>
      {(name) => (
        <>
          {' '}
          <span class="text-ink-extra-muted">
            {props.preposition} {name()}
          </span>
        </>
      )}
    </Show>
  );
}

export const renameDatabaseHandler = createToolRenderer({
  name: 'RenameDatabase',
  render: (ctx) => (
    <BaseTool icon={DatabaseIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Rename database to{' '}
        <span class="text-ink">
          {ctx.response?.data.name ?? ctx.tool.data.name}
        </span>
      </span>
    </BaseTool>
  ),
});

export const deleteTableHandler = createToolRenderer({
  name: 'DeleteTable',
  render: (ctx) => (
    <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Delete table
        <Scope name={ctx.response?.data.database?.name} preposition="from" />
      </span>
    </BaseTool>
  ),
});

export const renameColumnHandler = createToolRenderer({
  name: 'RenameColumn',
  render: (ctx) => (
    <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Rename column to{' '}
        <span class="text-ink">
          {ctx.response?.data.name ?? ctx.tool.data.name}
        </span>
        <Scope
          name={
            schemaTable(ctx.response?.data.database, ctx.tool.data.tableId)
              ?.name
          }
          preposition="in"
        />
      </span>
    </BaseTool>
  ),
});

export const changeColumnTypeHandler = createToolRenderer({
  name: 'ChangeColumnType',
  render: (ctx) => {
    const options = () => ctx.tool.data.options ?? [];
    const column = () =>
      schemaColumn(
        ctx.response?.data.database,
        ctx.tool.data.tableId,
        ctx.tool.data.columnId
      );
    return (
      <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
        <span class="min-w-0 truncate">
          <Show when={column()} fallback="Change column type to ">
            {(changed) => (
              <>
                Change <span class="text-ink">{changed().name}</span> to{' '}
              </>
            )}
          </Show>
          <span class="text-ink">{ctx.tool.data.dataType}</span>
          <span class="pl-1.5 text-ink-extra-muted">
            <Show when={ctx.tool.data.isMultiSelect}>multiple</Show>
            <Show when={options().length > 0}>
              {' · '}
              {options().join(', ')}
            </Show>
          </span>
        </span>
      </BaseTool>
    );
  },
});

export const deleteColumnHandler = createToolRenderer({
  name: 'DeleteColumn',
  render: (ctx) => (
    <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
      <span class="min-w-0 truncate">
        Delete column
        <Scope
          name={
            schemaTable(ctx.response?.data.database, ctx.tool.data.tableId)
              ?.name
          }
          preposition="from"
        />
      </span>
    </BaseTool>
  ),
});

export const reorderColumnsHandler = createToolRenderer({
  name: 'ReorderColumns',
  render: (ctx) => {
    const count = () => ctx.tool.data.columnIds.length;
    return (
      <BaseTool icon={TableIcon} renderContext={ctx.renderContext} type="call">
        <span class="min-w-0 truncate">
          Reorder{' '}
          <span class="text-ink">
            {count()} column{count() === 1 ? '' : 's'}
          </span>
          <Scope
            name={
              schemaTable(ctx.response?.data.database, ctx.tool.data.tableId)
                ?.name
            }
            preposition="in"
          />
        </span>
      </BaseTool>
    );
  },
});

export const saveDatabaseQueryHandler = createToolRenderer({
  name: 'SaveDatabaseQuery',
  render: (ctx) => {
    const [expanded, setExpanded] = createSignal(true);
    const markdown = () => ctx.response?.data.markdown;
    return (
      <BaseTool
        icon={DatabaseIcon}
        renderContext={ctx.renderContext}
        type="call"
        response={
          expanded() && markdown() ? (
            <div class="min-w-0 p-3">
              <StaticMarkdownContext>
                <StaticMarkdown
                  markdown={markdown()!}
                  target="internal"
                  lazy={false}
                />
              </StaticMarkdownContext>
            </div>
          ) : undefined
        }
      >
        <div class="flex min-w-0 flex-1 items-center justify-between gap-3">
          <span class="min-w-0 truncate">
            {ctx.response ? 'Saved' : 'Save'} question{' '}
            <span class="text-ink">{ctx.tool.data.title}</span>
          </span>
          <Tool.ResultToggle
            expanded={expanded()}
            onToggle={() => setExpanded((open) => !open)}
            showToggle={!!ctx.response}
          />
        </div>
      </BaseTool>
    );
  },
});
