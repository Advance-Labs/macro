import { match, P } from 'ts-pattern';
import { v7 as uuidv7 } from 'uuid';
import type {
  ApplyOpsRequest,
  CardPosition,
  ColumnCast,
  ColumnKind,
  DatabaseDetail,
  DatabaseOp,
  ImportTable,
  InferColumnTypeOutcome,
  InferColumnTypeRequest,
  NewColumn,
  NewOption,
  OpRefusalResponse,
  OpResult,
  SharePermissionV2,
  TableVersion,
  TakenId,
  UpdateSharePermissionRequestV2,
} from '../../../generated/storage/types.gen';
import { MacroApiError, MacroError, unwrap } from '../../utils';
import type { MacroClient } from '../../utils/client';
import { MacroEntity } from '../entity';
import type { PropertyDefinition } from '../properties/property-definition';
import { User } from '../users/user';
import { DatabaseColumn } from './column';
import { DatabaseTable } from './table';
import type { DatabaseView } from './view';

/** The result of the op kind `Kind`. */
export type OpResultOf<Kind extends OpResult['kind']> = Extract<
  OpResult,
  { kind: Kind }
>;

/**
 * A type a column can have. A relation names the related table by handle;
 * the table may live in any database the caller can reach.
 */
export type ColumnType =
  | Exclude<ColumnKind, { type: 'relation' }>
  | { type: 'relation'; table: DatabaseTable };

/** Options for {@link Database.addColumn}. */
export type AddColumnOptions = (
  | {
      /** Display name of the column, unique within the table ignoring case. */
      name: string;
      /** What the column holds. */
      type: ColumnType;
      /**
       * For a select or tag column, the labels it starts with. Without any it
       * accepts nothing until {@link DatabaseColumn.addOptions} adds some.
       */
      options?: string[];
      /** Let a plain, empty text column settle its type from its first value. */
      inferType?: boolean;
    }
  | {
      /** An existing property definition to bind the column to. */
      property: PropertyDefinition;
    }
) & {
  /** The column the new one goes right after; by default, the last one. */
  after?: DatabaseColumn;
};

/** Options for {@link Database.changeColumnType}. */
export type ChangeColumnTypeOptions = {
  /** The type to change the column to. */
  to: ColumnType;
  /**
   * Empty the values that do not fit the new type instead of refusing the
   * change; a cell with several values keeps its first.
   */
  clearInvalid?: boolean;
};

/** Options for {@link Database.applyOps}. */
export type ApplyOpsOptions = {
  /**
   * The version each table must still be at, as the caller read it. A table
   * that moved refuses the whole batch with a 409, so an edit made against
   * what the caller saw does not overwrite another's.
   */
  baseVersions?: { table: DatabaseTable; version: TableVersion }[];
};

/** Settle an empty inferred column using the table version the caller read. */
export type InferColumnTypeOptions = {
  dataType: 'STRING' | 'NUMBER' | 'ENTITY';
  specificEntityType?: InferColumnTypeRequest['specificEntityType'];
  baseVersion: TableVersion;
};

/**
 * A batch of ops was refused (HTTP 400): nothing in it was written. Names the
 * op at fault and, when the batch minted an id that already names something
 * (a retry of a batch that committed, or an id minted twice), that id.
 */
export class MacroOpRefusedError extends MacroApiError {
  /** The refused op's index in the batch. */
  readonly op: number;
  /** The row's index within the op, when one row is at fault. */
  readonly row: number | null;
  /** The column placement at fault, when one is. */
  readonly column: string | null;
  /** The minted id that is already taken, when that is the reason. */
  readonly taken: TakenId | null;

  constructor(status: number, refusal: OpRefusalResponse) {
    super(status, refusal);
    this.name = 'MacroOpRefusedError';
    this.message = refusal.message;
    this.op = refusal.op;
    this.row = refusal.row;
    this.column = refusal.column;
    this.taken = refusal.taken;
  }
}

function columnKind(type: ColumnType): ColumnKind {
  return match(type)
    .with({ type: 'relation' }, ({ table }) => ({
      type: 'relation' as const,
      database: table.database.id,
      table: table.id,
    }))
    .with({ type: P.not('relation') }, (plain) => plain)
    .exhaustive();
}

function newOptions(labels: string[]): NewOption[] {
  return labels.map((label) => ({ id: uuidv7(), label }));
}

function isResultOf<Kind extends OpResult['kind']>(
  result: OpResult | undefined,
  kind: Kind,
): result is OpResultOf<Kind> {
  return result?.kind === kind;
}

/** The one result of a one-op batch, which must be of the kind the op answers. */
function soleResult<Kind extends OpResult['kind']>(
  results: OpResult[],
  kind: Kind,
): OpResultOf<Kind> {
  const [result] = results;
  if (results.length !== 1 || !isResultOf(result, kind))
    throw new MacroError(
      `expected one ${kind} result, got ${results.map((each) => each.kind).join(', ') || 'none'}`,
    );
  return result;
}

/**
 * A Macro database: a named collection of tables, owned and shared as one
 * entity.
 *
 * A free-to-construct `(client, id)` handle like any other entity — the schema
 * loads lazily on first field access and is dropped after any mutation.
 */
export class Database extends MacroEntity<DatabaseDetail> {
  protected async fetch(): Promise<DatabaseDetail> {
    return unwrap(
      await this.client.storage.getDatabase({ path: { id: this.id } }),
    );
  }

  private assertOwns(part: string, owner: Database): void {
    if (owner.id !== this.id)
      throw new MacroError(`${part} does not belong to database ${this.id}`);
  }

  /** A handle to a database by id. Details load on first access. */
  static byId(client: MacroClient, id: string): Database {
    return new Database(client, id);
  }

  /** Create a database owned by the caller. */
  static async create(
    client: MacroClient,
    options: { name: string },
  ): Promise<Database> {
    const record = unwrap(
      await client.storage.createDatabase({ body: { name: options.name } }),
    );
    return new Database(client, record.id);
  }

  /** The databases the caller can see, each with the caller's access level. */
  static async list(client: MacroClient): Promise<Database[]> {
    const listed = unwrap(await client.storage.listDatabases());
    return listed.map((entry) => new Database(client, entry.database.id));
  }

  /**
   * The full schema: the database record, the caller's access, and every
   * table with its columns and views. Cached until the next write.
   */
  schema(): Promise<DatabaseDetail> {
    return this.detail.get();
  }

  /** The database's display name. */
  readonly name = this.mappedField('database', (record) => record.name);

  /** The user who owns the database. */
  readonly owner = this.mappedField('database', (record) =>
    User.byId(this.client, record.owner_id),
  );

  /** When the database was created. */
  readonly createdAt = this.mappedField(
    'database',
    (record) => record.created_at,
  );

  /** When the database was trashed, if it has been. */
  readonly trashedAt = this.mappedField(
    'database',
    (record) => record.trashed_at ?? undefined,
  );

  /** The caller's access on the database. */
  readonly grant = this.field('grant');

  /** The database's tables, in tab order. */
  readonly tables = this.mappedField('tables', (tables) =>
    tables.map((table) => DatabaseTable.byId(this, table.table.id)),
  );

  /** The table with the given display name, or `undefined` if there is none. */
  async table(name: string): Promise<DatabaseTable | undefined> {
    const { tables } = await this.schema();
    const found = tables.find((table) => table.table.name === name);
    return found ? DatabaseTable.byId(this, found.table.id) : undefined;
  }

  /** Create a table in the database, under an id minted here. */
  async createTable(options: { name: string }): Promise<DatabaseTable> {
    const { table } = soleResult(
      await this.applyOps([
        { kind: 'create_table', id: uuidv7(), name: options.name },
      ]),
      'table_created',
    );
    return DatabaseTable.byId(this, table);
  }

  /**
   * Apply ops to the database in one transaction: create, rename, reorder,
   * or delete tables and columns; change a column's type; add, edit, or
   * delete select options; insert, update, or delete rows; create, change,
   * reorder, or delete views; move board cards. Later ops see what earlier
   * ones did, so a table or column created by one op (under an id the caller
   * mints, as a UUIDv7) can be named by the next. A refused op leaves the
   * whole batch unwritten and throws {@link MacroOpRefusedError}. Returns one
   * result per op, in the order sent.
   */
  async applyOps(
    ops: DatabaseOp[],
    options: ApplyOpsOptions = {},
  ): Promise<OpResult[]> {
    const body: ApplyOpsRequest = { ops };
    if (options.baseVersions !== undefined) {
      for (const { table } of options.baseVersions)
        this.assertOwns(`table ${table.id}`, table.database);
      body.baseVersions = Object.fromEntries(
        options.baseVersions.map(({ table, version }) => [table.id, version]),
      );
    }
    const { results } = await this.mutate(async (client) => {
      const outcome = await client.storage.applyDatabaseOps({
        path: { id: this.id },
        body,
      });
      if (outcome.error !== undefined && 'op' in outcome.error)
        throw new MacroOpRefusedError(
          outcome.response?.status ?? 0,
          outcome.error,
        );
      return outcome;
    });
    return results;
  }

  /**
   * Persist a new tab order. Pass every table of the database exactly once.
   * Returns the tables in their new order.
   */
  async reorderTables(tables: DatabaseTable[]): Promise<DatabaseTable[]> {
    for (const table of tables)
      this.assertOwns(`table ${table.id}`, table.database);
    const reordered = soleResult(
      await this.applyOps([
        { kind: 'reorder_tables', order: tables.map((table) => table.id) },
      ]),
      'tables_reordered',
    );
    return reordered.tables.map(({ table }) => DatabaseTable.byId(this, table));
  }

  /**
   * Delete one of the database's tables with its columns, rows, and views.
   * A database keeps at least one table, and a table another table's
   * relation column points at cannot be deleted.
   */
  async deleteTable(table: DatabaseTable): Promise<void> {
    this.assertOwns(`table ${table.id}`, table.database);
    soleResult(
      await this.applyOps([{ kind: 'delete_table', table: table.id }]),
      'table_deleted',
    );
  }

  /**
   * What changing a column to each type would do to its values. Changes
   * nothing.
   */
  async columnCasts(column: DatabaseColumn): Promise<ColumnCast[]> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    return unwrap(
      await this.client.storage.listDatabaseColumnCasts({
        path: { id: this.id, table_id: column.table.id, column_id: column.id },
      }),
    );
  }

  /** Where a board view's cards sit: each placed card's lane and key. */
  async viewPositions(view: DatabaseView): Promise<CardPosition[]> {
    this.assertOwns(`view ${view.id}`, view.table.database);
    const { positions } = unwrap(
      await this.client.storage.getDatabaseViewPositions({
        path: { id: this.id, view_id: view.id },
      }),
    );
    return positions;
  }

  /** Import text rows atomically. Keep requestId unchanged when retrying. */
  async importTable(request: ImportTable): Promise<DatabaseTable> {
    const table = await this.mutate((client) =>
      client.storage.importDatabaseTable({
        path: { id: this.id },
        body: request,
      }),
    );
    return DatabaseTable.byId(this, table.id);
  }

  /** Read direct recipients. Only the database owner can manage sharing. */
  async sharePermissions(): Promise<SharePermissionV2> {
    return unwrap(
      await this.client.storage.getDatabasePermissions({
        path: { id: this.id },
      }),
    );
  }

  /** Add, replace, or remove recipient grants without transferring ownership. */
  updateSharePermissions(
    request: UpdateSharePermissionRequestV2,
  ): Promise<SharePermissionV2> {
    return this.mutate((client) =>
      client.storage.updateDatabasePermissions({
        path: { id: this.id },
        body: request,
      }),
    );
  }

  /**
   * Change a column's type at the table version last read. Values that do not
   * convert refuse the change unless `clearInvalid` empties them; the result
   * counts the cells it cleared and the multi-value cells it trimmed.
   */
  async changeColumnType(
    column: DatabaseColumn,
    options: ChangeColumnTypeOptions,
  ): Promise<OpResultOf<'column_typed'>> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    const version = await column.table.version();
    return soleResult(
      await this.applyOps(
        [
          {
            kind: 'change_column_type',
            table: column.table.id,
            column: column.id,
            to: columnKind(options.to),
            ...(options.clearInvalid !== undefined
              ? { clearInvalid: options.clearInvalid }
              : {}),
          },
        ],
        { baseVersions: [{ table: column.table, version }] },
      ),
      'column_typed',
    );
  }

  /**
   * Remove a column and its cells at the table version last read. Returns
   * the table's new version.
   */
  async deleteColumn(column: DatabaseColumn): Promise<TableVersion> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    const version = await column.table.version();
    const { tableVersion } = soleResult(
      await this.applyOps(
        [{ kind: 'delete_column', table: column.table.id, column: column.id }],
        { baseVersions: [{ table: column.table, version }] },
      ),
      'column_deleted',
    );
    return tableVersion;
  }

  /**
   * Persist a complete column order, including currently hidden columns, at
   * the table version last read. Pass every column of the table exactly once.
   * Returns the table's new version.
   */
  async reorderColumns(
    table: DatabaseTable,
    columns: DatabaseColumn[],
  ): Promise<TableVersion> {
    this.assertOwns(`table ${table.id}`, table.database);
    for (const column of columns)
      if (column.table.id !== table.id)
        throw new MacroError(
          `column ${column.id} does not belong to table ${table.id}`,
        );
    const version = await table.version();
    const { tableVersion } = soleResult(
      await this.applyOps(
        [
          {
            kind: 'reorder_columns',
            table: table.id,
            order: columns.map((column) => column.id),
          },
        ],
        { baseVersions: [{ table, version }] },
      ),
      'columns_reordered',
    );
    return tableVersion;
  }

  /**
   * Rename a table only if its last-read name is still current. Returns the
   * table's new version.
   */
  async renameTable(table: DatabaseTable, name: string): Promise<TableVersion> {
    this.assertOwns(`table ${table.id}`, table.database);
    const previousName = await table.name();
    const { tableVersion } = soleResult(
      await this.applyOps([
        { kind: 'rename_table', table: table.id, name, previousName },
      ]),
      'table_renamed',
    );
    return tableVersion;
  }

  /**
   * Rename this column placement, only if its last-read name is still
   * current, without changing its shared property definition. Returns the
   * table's new version.
   */
  async renameColumn(
    column: DatabaseColumn,
    name: string,
  ): Promise<TableVersion> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    const previousName = await column.name();
    const { tableVersion } = soleResult(
      await this.applyOps([
        {
          kind: 'rename_column',
          table: column.table.id,
          column: column.id,
          name,
          previousName,
        },
      ]),
      'column_renamed',
    );
    return tableVersion;
  }

  /** Adopt a first-value type only while the owned column is empty and inferable. */
  async inferColumnType(
    column: DatabaseColumn,
    options: InferColumnTypeOptions,
  ): Promise<InferColumnTypeOutcome> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    return this.mutate((client) =>
      client.storage.inferDatabaseColumnType({
        path: { id: this.id, table_id: column.table.id, column_id: column.id },
        body: {
          dataType: options.dataType,
          baseVersion: options.baseVersion,
          ...(options.specificEntityType !== undefined
            ? { specificEntityType: options.specificEntityType }
            : {}),
        },
      }),
    );
  }

  /**
   * Add a column to one of the database's tables, under an id minted here,
   * either creating a property definition for it or binding an existing one.
   */
  async addColumn(
    table: DatabaseTable,
    options: AddColumnOptions,
  ): Promise<DatabaseColumn> {
    this.assertOwns(`table ${table.id}`, table.database);
    if (options.after !== undefined && options.after.table.id !== table.id)
      throw new MacroError(
        `column ${options.after.id} does not belong to table ${table.id}`,
      );
    const definition: NewColumn =
      'property' in options
        ? { source: 'existing', property: options.property.id }
        : {
            source: 'new',
            name: options.name,
            type: columnKind(options.type),
            ...(options.options !== undefined
              ? { options: newOptions(options.options) }
              : {}),
            ...(options.inferType !== undefined
              ? { inferType: options.inferType }
              : {}),
          };
    const { column } = soleResult(
      await this.applyOps([
        {
          kind: 'create_column',
          table: table.id,
          id: uuidv7(),
          definition,
          ...(options.after !== undefined ? { after: options.after.id } : {}),
        },
      ]),
      'column_created',
    );
    return DatabaseColumn.byId(table, column);
  }

  /**
   * Add select options to one of the database's columns, each under an id
   * minted here. Labels the column already has are skipped, so the call is
   * safe to repeat; the result lists the options it did create. Only select
   * and tag columns accept options.
   */
  async addColumnOptions(
    column: DatabaseColumn,
    labels: string[],
  ): Promise<OpResultOf<'options_added'>> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    return soleResult(
      await this.applyOps([
        {
          kind: 'add_options',
          table: column.table.id,
          column: column.id,
          options: newOptions(labels),
        },
      ]),
      'options_added',
    );
  }
}
