import {
  $applyNodeReplacement,
  $createParagraphNode,
  DecoratorNode,
  type EditorConfig,
  type EditorThemeClasses,
  type LexicalEditor,
  type LexicalNode,
  type NodeKey,
  type SerializedLexicalNode,
  type Spread,
} from 'lexical';
import { type DecoratorComponent, getDecorator } from '../decoratorRegistry';
import { $applyIdFromSerialized } from '../plugins/nodeIdPlugin';
import { $createUnknownMentionNode } from './UnknownMentionNode';

const DATABASE_QUERY_TAG = 'm-db-query';
const DISPLAY_MODES = [
  'scalar',
  'table',
  'bar',
  'line',
  'area',
  'scatter',
  'pie',
] as const;
const MAX_Y_COLUMNS = 5;
export type DatabaseQueryDisplayMode = (typeof DISPLAY_MODES)[number];
/** Column aliases and plain choices; never renderer options. */
export type DatabaseQueryChart = {
  x: string;
  y: string[];
  title?: string;
  /** A column whose values split the one `y` series into groups. */
  color?: string;
  /** Stack bar or area series. */
  stack?: boolean;
};
/** Points at an immutable saved query; the SQL lives on the server. */
export type DatabaseQueryData = {
  /** Empty while the question is still a draft. */
  queryId: string;
  databaseId?: string;
  tableId?: string;
  prompt: string;
  title?: string;
  displayMode: DatabaseQueryDisplayMode;
  chart?: DatabaseQueryChart;
};
export type DatabaseQueryDecoratorProps = DatabaseQueryData & {
  key: NodeKey;
  theme: EditorThemeClasses;
};
export type SerializedDatabaseQueryNode = Spread<
  DatabaseQueryData,
  SerializedLexicalNode
>;

const isOptionalString = (value: unknown): value is string | undefined =>
  value === undefined || typeof value === 'string';

const isNonEmptyName = (name: unknown): name is string =>
  typeof name === 'string' && !!name.trim();

const isDisplayMode = (value: unknown): value is DatabaseQueryDisplayMode =>
  DISPLAY_MODES.some((mode) => mode === value);

export function parseDatabaseQueryChart(
  value: unknown
): DatabaseQueryChart | undefined {
  if (!value || typeof value !== 'object') return;
  const { x, y, title, color, stack } = value as Record<string, unknown>;
  if (
    !isNonEmptyName(x) ||
    !Array.isArray(y) ||
    !y.length ||
    y.length > MAX_Y_COLUMNS ||
    !y.every(isNonEmptyName) ||
    new Set(y).size !== y.length ||
    y.includes(x) ||
    !isOptionalString(title)
  )
    return;
  if (
    color !== undefined &&
    (!isNonEmptyName(color) ||
      color === x ||
      y.length !== 1 ||
      y.includes(color))
  )
    return;
  if (stack !== undefined && typeof stack !== 'boolean') return;
  return {
    x,
    y: [...y],
    ...(title ? { title } : {}),
    ...(color ? { color } : {}),
    ...(stack ? { stack: true } : {}),
  };
}

/** Only query source is serialized. Results belong to the current viewer. */
export function parseDatabaseQueryData(
  value: unknown
): DatabaseQueryData | undefined {
  if (!value || typeof value !== 'object') return;
  const { queryId, databaseId, tableId, prompt, title, displayMode, chart } =
    value as Record<string, unknown>;
  if (
    typeof queryId !== 'string' ||
    typeof prompt !== 'string' ||
    !isOptionalString(databaseId) ||
    !isOptionalString(tableId) ||
    !isOptionalString(title) ||
    !isDisplayMode(displayMode)
  )
    return;
  const parsedChart =
    chart === undefined ? undefined : parseDatabaseQueryChart(chart);
  if (chart !== undefined && !parsedChart) return;
  return {
    queryId,
    ...(databaseId ? { databaseId } : {}),
    ...(tableId ? { tableId } : {}),
    ...(title ? { title } : {}),
    prompt,
    displayMode,
    ...(parsedChart ? { chart: parsedChart } : {}),
  };
}

/** An `<m-db-query>` tag's payload; `undefined` when malformed. */
export function parseDatabaseQueryJson(
  json: string
): DatabaseQueryData | undefined {
  try {
    return parseDatabaseQueryData(JSON.parse(json));
  } catch {
    return;
  }
}

export function databaseQueryMarkdown(data: DatabaseQueryData): string {
  const source = parseDatabaseQueryData(data);
  if (!source) throw new Error('Invalid database query');
  // Escaping '<' prevents user-authored titles or prompts from closing the XML tag.
  const json = JSON.stringify(source).replaceAll('<', '\\u003c');
  return `<${DATABASE_QUERY_TAG}>${json}</${DATABASE_QUERY_TAG}>`;
}

export class DatabaseQueryNode extends DecoratorNode<
  DecoratorComponent<DatabaseQueryDecoratorProps> | undefined
> {
  __queryId: string;
  __databaseId?: string;
  __tableId?: string;
  __prompt: string;
  __title?: string;
  __displayMode: DatabaseQueryDisplayMode;
  __chart?: DatabaseQueryChart;

  static getType() {
    return 'database-query';
  }
  static clone(node: DatabaseQueryNode) {
    return new DatabaseQueryNode(node.exportComponentProps(), node.__key);
  }
  constructor(data: DatabaseQueryData, key?: NodeKey) {
    super(key);
    this.__queryId = data.queryId;
    this.__databaseId = data.databaseId;
    this.__tableId = data.tableId;
    this.__prompt = data.prompt;
    this.__title = data.title;
    this.__displayMode = data.displayMode;
    this.__chart = data.chart;
  }
  isInline() {
    return this.__displayMode === 'scalar';
  }
  isKeyboardSelectable() {
    return true;
  }
  static importJSON(serialized: SerializedDatabaseQueryNode): LexicalNode {
    const data = parseDatabaseQueryData(serialized);
    // An unreadable answer degrades like its markdown form instead of failing the document.
    if (!data) {
      const fallback = $createUnknownMentionNode({
        name: 'Unavailable database question',
      });
      return serialized.displayMode === 'scalar'
        ? fallback
        : $createParagraphNode().append(fallback);
    }
    const node = $createDatabaseQueryNode(data);
    $applyIdFromSerialized(node, serialized);
    return node;
  }
  exportJSON(): SerializedDatabaseQueryNode {
    return {
      ...super.exportJSON(),
      ...this.exportComponentProps(),
      type: DatabaseQueryNode.getType(),
      version: 2,
    };
  }
  exportComponentProps(): DatabaseQueryData {
    return {
      queryId: this.__queryId,
      ...(this.__databaseId ? { databaseId: this.__databaseId } : {}),
      ...(this.__tableId ? { tableId: this.__tableId } : {}),
      ...(this.__title ? { title: this.__title } : {}),
      prompt: this.__prompt,
      displayMode: this.__displayMode,
      ...(this.__chart ? { chart: this.__chart } : {}),
    };
  }
  setQuery(data: DatabaseQueryData) {
    const writable = this.getWritable();
    writable.__queryId = data.queryId;
    writable.__databaseId = data.databaseId;
    writable.__tableId = data.tableId;
    writable.__prompt = data.prompt;
    writable.__title = data.title;
    writable.__displayMode = data.displayMode;
    writable.__chart = data.chart;
  }
  createDOM(): HTMLElement {
    const element = document.createElement(this.isInline() ? 'span' : 'div');
    element.setAttribute('data-database-query', 'true');
    return element;
  }
  updateDOM(previous: DatabaseQueryNode) {
    return previous.__displayMode !== this.__displayMode;
  }
  getTextContent() {
    return this.__title || this.__prompt || 'Database answer';
  }
  exportDOM() {
    const element = this.createDOM();
    element.textContent = this.getTextContent();
    return { element };
  }
  static importDOM() {
    return null;
  }
  decorate(_: LexicalEditor, config: EditorConfig) {
    const decorator =
      getDecorator<DatabaseQueryDecoratorProps>(DatabaseQueryNode);
    if (decorator)
      return () =>
        decorator({
          ...this.exportComponentProps(),
          key: this.getKey(),
          theme: config.theme,
        });
  }
}

export function $createDatabaseQueryNode(
  data: DatabaseQueryData
): DatabaseQueryNode {
  return $applyNodeReplacement(new DatabaseQueryNode(data));
}
export function $isDatabaseQueryNode(
  node: LexicalNode | null | undefined
): node is DatabaseQueryNode {
  return node instanceof DatabaseQueryNode;
}
