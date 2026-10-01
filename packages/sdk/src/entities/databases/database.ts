import type {
  CardPosition,
  ChangeColumnTypeRequest,
  ColumnCast,
  ColumnDetail,
  ColumnSchemaOutcome,
  CreateColumnRequest,
  DatabaseDetail,
  DatabaseOp,
  DataType,
  ImportTable,
  InferColumnTypeOutcome,
  InferColumnTypeRequest,
  OpResult,
  SharePermissionV2,
  TableVersion,
  UpdateSharePermissionRequestV2,
} from '../../../generated/storage/types.gen';
import { MacroError, unwrap } from '../../utils';
import type { MacroClient } from '../../utils/client';
import { MacroEntity } from '../entity';
import type { PropertyDefinition } from '../properties/property-definition';
import { User } from '../users/user';
import { DatabaseColumn } from './column';
import { DatabaseTable } from './table';
import type { DatabaseView } from './view';

/** How a new column obtains the property definition behind it. */
export type ColumnBinding =
  | {
      /** Display name of the column, and of the definition created for it. */
      name: string;
      /** The value type the column holds. */
      dataType: DataType;
      /** Whether the column holds multiple values. Defaults to false. */
      multiSelect?: boolean;
      /**
       * For a select or tag column, the labels it accepts. Without any it
       * accepts nothing until {@link DatabaseColumn.addOptions} adds some.
       */
      options?: string[];
    }
  | {
      /** An existing property definition to bind the column to. */
      property: PropertyDefinition;
    };

/** Options for {@link Database.addColumn}. */
export type AddColumnOptions = ColumnBinding & {
  /** Allow a newly owned, empty text column to infer its first value's type. */
  inferType?: boolean;
  /**
   * Make this a link column pointing at another table (many-to-many). The
   * target may live in any database the caller can reach.
   */
  linkTo?: DatabaseTable;
};

/** Settle an empty inferred column using the table version the caller read. */
export type InferColumnTypeOptions = {
  dataType: 'STRING' | 'NUMBER' | 'ENTITY';
  specificEntityType?: InferColumnTypeRequest['specific_entity_type'];
  baseVersion: TableVersion;
};

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
   * table with its columns and SQL names. Cached until the next write.
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

  /** Create a table in the database. */
  async createTable(options: { name: string }): Promise<DatabaseTable> {
    const table = await this.mutate((client) =>
      client.storage.createDatabaseTable({
        path: { id: this.id },
        body: { name: options.name },
      }),
    );
    return DatabaseTable.byId(this, table.id);
  }

  /**
   * Apply ops to the database's tables in one transaction: insert, update,
   * or delete rows; change a column's type; edit or delete select options;
   * create, change, reorder, or delete views; move board cards. A refused op
   * leaves the whole batch unwritten; a column type change goes in a batch
   * of its own. Returns one result per op, in the order sent.
   */
  async applyOps(ops: DatabaseOp[]): Promise<OpResult[]> {
    const { results } = await this.mutate((client) =>
      client.storage.applyDatabaseOps({
        path: { id: this.id },
        body: { ops },
      }),
    );
    return results;
  }

  /**
   * Persist a new tab order. Pass every table of the database exactly once.
   * Returns the tables in their new order.
   */
  async reorderTables(tables: DatabaseTable[]): Promise<DatabaseTable[]> {
    for (const table of tables)
      this.assertOwns(`table ${table.id}`, table.database);
    const ordered = await this.mutate((client) =>
      client.storage.reorderDatabaseTables({
        path: { id: this.id },
        body: { tableIds: tables.map((table) => table.id) },
      }),
    );
    return ordered.map((table) => DatabaseTable.byId(this, table.id));
  }

  /**
   * Delete one of the database's tables with its columns, rows, and views.
   * A database keeps at least one table, and a table another table's
   * relation column points at cannot be deleted.
   */
  async deleteTable(table: DatabaseTable): Promise<void> {
    this.assertOwns(`table ${table.id}`, table.database);
    await this.mutate((client) =>
      client.storage.deleteDatabaseTable({
        path: { id: this.id, table_id: table.id },
      }),
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

  /** Change one placement's type only when all existing values convert safely. */
  async changeColumnType(
    column: DatabaseColumn,
    request: ChangeColumnTypeRequest,
  ): Promise<ColumnSchemaOutcome> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    return this.mutate((client) =>
      client.storage.changeDatabaseColumnType({
        path: { id: this.id, table_id: column.table.id, column_id: column.id },
        body: request,
      }),
    );
  }

  /** Remove a column and its cells, guarded by the table version last read. */
  async deleteColumn(
    column: DatabaseColumn,
    baseVersion: TableVersion,
  ): Promise<ColumnSchemaOutcome> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    return this.mutate((client) =>
      client.storage.deleteDatabaseColumn({
        path: { id: this.id, table_id: column.table.id, column_id: column.id },
        body: { baseVersion },
      }),
    );
  }

  /** Persist a complete column order, including currently hidden columns. */
  async reorderColumns(
    table: DatabaseTable,
    columnIds: string[],
    baseVersion: TableVersion,
  ): Promise<ColumnSchemaOutcome> {
    this.assertOwns(`table ${table.id}`, table.database);
    return this.mutate((client) =>
      client.storage.reorderDatabaseColumns({
        path: { id: this.id, table_id: table.id },
        body: { columnIds, baseVersion },
      }),
    );
  }

  /** Rename a table only if its last-read name is still current. */
  async renameTable(table: DatabaseTable, name: string): Promise<void> {
    this.assertOwns(`table ${table.id}`, table.database);
    const previousName = await table.name();
    await this.mutate((client) =>
      client.storage.renameDatabaseTable({
        path: { id: this.id, table_id: table.id },
        body: { name, previousName },
      }),
    );
  }

  /** Rename this column placement without changing shared definitions or SQL names. */
  async renameColumn(column: DatabaseColumn, name: string): Promise<void> {
    this.assertOwns(`column ${column.id}`, column.table.database);
    const previousName = await column.name();
    await this.mutate((client) =>
      client.storage.renameDatabaseColumn({
        path: { id: this.id, table_id: column.table.id, column_id: column.id },
        body: { name, previousName },
      }),
    );
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
          data_type: options.dataType,
          base_version: options.baseVersion,
          ...(options.specificEntityType !== undefined
            ? { specific_entity_type: options.specificEntityType }
            : {}),
        },
      }),
    );
  }

  /**
   * Add a column to one of the database's tables, either creating a property
   * definition for it or binding an existing one.
   */
  async addColumn(
    table: DatabaseTable,
    options: AddColumnOptions,
  ): Promise<DatabaseColumn> {
    this.assertOwns(`table ${table.id}`, table.database);
    const binding: CreateColumnRequest['binding'] =
      'property' in options
        ? { kind: 'existing', property_definition_id: options.property.id }
        : {
            kind: 'new',
            name: options.name,
            data_type: options.dataType,
            ...(options.multiSelect !== undefined
              ? { is_multi_select: options.multiSelect }
              : {}),
            ...(options.options !== undefined
              ? { options: options.options }
              : {}),
          };
    const { columnId } = await this.mutate((client) =>
      client.storage.createDatabaseColumn({
        path: { id: this.id, table_id: table.id },
        body: {
          binding,
          ...(options.inferType !== undefined
            ? { inferType: options.inferType }
            : {}),
          ...(options.linkTo !== undefined
            ? {
                linkToTableId: options.linkTo.id,
                linkToDatabaseId: options.linkTo.database.id,
              }
            : {}),
        },
      }),
    );
    return DatabaseColumn.byId(table, columnId);
  }

  /**
   * Add select options to one of the database's columns. Labels the column
   * already has are ignored, so the call is safe to repeat. Only select and
   * tag columns accept options.
   */
  async addColumnOptions(
    column: DatabaseColumn,
    labels: string[],
  ): Promise<ColumnDetail> {
    const table = column.table;
    this.assertOwns(`column ${column.id}`, table.database);
    return this.mutate((client) =>
      client.storage.addDatabaseColumnOptions({
        path: { id: this.id, table_id: table.id, column_id: column.id },
        body: { labels },
      }),
    );
  }
}
