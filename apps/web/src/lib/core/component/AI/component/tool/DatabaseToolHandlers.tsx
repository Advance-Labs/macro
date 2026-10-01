/**
 * Database tools render as a placeholder until the databases UI ships.
 */
import type { ToolName } from '@service-cognition/generated/tools/tool';
import type {
  RenderContext,
  ToolHandler,
  ToolHandlerMap,
} from './ToolRenderer';

const DATABASE_TOOL_NAMES = [
  'ListDatabases',
  'DescribeDatabase',
  'QueryDatabase',
  'CreateDatabase',
  'CreateTable',
  'RenameTable',
  'ReorderTables',
  'AddColumn',
  'AddColumnOptions',
  'SaveDatabaseView',
  'RenameDatabase',
  'DeleteTable',
  'RenameColumn',
  'ChangeColumnType',
  'DeleteColumn',
  'ReorderColumns',
  'SaveDatabaseQuery',
] as const satisfies readonly ToolName[];

type DatabaseToolName = (typeof DATABASE_TOOL_NAMES)[number];

type DatabaseToolHandlerMap = Pick<
  ToolHandlerMap<RenderContext>,
  DatabaseToolName
>;

function DatabaseToolPlaceholder() {
  return <span class="text-xs text-ink-muted">Database tool</span>;
}

function placeholderHandler<Name extends DatabaseToolName>(
  _name: Name
): ToolHandler<Name, RenderContext> {
  return { render: DatabaseToolPlaceholder };
}

export const databaseToolPlaceholders = Object.fromEntries(
  DATABASE_TOOL_NAMES.map((name) => [name, placeholderHandler(name)])
) as DatabaseToolHandlerMap;
