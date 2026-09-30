/**
 * The shapes that cross the wasm boundary, mirroring the `serde` forms of
 * `database_sql`'s `Step`, `Request`, `GqlQuery`, `Page`, `Row`, `Cell`,
 * `Bin`, `Outcome`, and `Catalog`. Keys are UUID strings. The engine's
 * transcripts in `crates/database_sql/fixtures/transcripts` are real values
 * of these shapes; the driver's tests replay them.
 */

import type { EntityType } from '@service-properties/generated/schemas/entityType';

export type Cell =
  | { type: 'text'; value: string }
  | { type: 'number'; value: number }
  | { type: 'bool'; value: boolean }
  | { type: 'date'; value: string }
  | { type: 'options'; value: string[] }
  | { type: 'entities'; value: string[] };

/** One fetched row: the entity id, its place in its table, and its cells by key. */
export interface Row {
  id: string;
  /** The row's fractional position; `people` rows have none. */
  position?: string;
  cells: Record<string, Cell>;
}

/** One page of a request. */
export interface Page {
  rows: Row[];
  next: string | null;
}

/** One `groupSoup` bin. */
export interface Bin {
  key: Cell | null;
  count: number;
}

/** The Soup `propf` expression, in its wire form. */
export type Propf =
  | { '&': [Propf, Propf] }
  | { '|': [Propf, Propf] }
  | { '!': Propf }
  | { l: { pd: string; et?: string; v: { so: string } | { er: string } } };

/** The values a joined relation is matched on. */
export interface KeyHint {
  column: string | null;
  values: Cell[];
}

export type GqlQuery =
  | {
      type: 'soup';
      table: string;
      propf: Propf | null;
      keyHint: KeyHint | null;
    }
  | { type: 'groupSoup'; table: string; propf: Propf | null; groupBy: string }
  | { type: 'people'; ids: string[] | null };

/** One fetch the engine wants. */
export interface Request {
  id: number;
  query: GqlQuery;
  needs: string[];
  cursor: string | null;
  limit: number;
}

export type OutcomeKind =
  | 'text'
  | 'number'
  | 'boolean'
  | 'date'
  | 'select'
  | 'entity';

export interface OutcomeColumn {
  name: string;
  column?: string;
  kind: OutcomeKind;
}

/** What a statement produced. */
export interface Outcome {
  columns: OutcomeColumn[];
  rows: (Cell | null)[][];
  rowIds: string[];
  readTables: string[];
  truncated: boolean;
  insertedRowIds: string[];
  changesApplied: number;
  failures: { row: number; message: string }[];
  /** The column an `ALTER COLUMN … TYPE` changed; the server runs those. */
  alteredColumn?: {
    table: string;
    column: string;
    to: string;
    clearedCells: number;
    trimmedCells: number;
  };
}

/** What the driver does next. */
export type Step =
  | ({ step: 'fetch' } & Request)
  | ({ step: 'bins' } & Request)
  | ({ step: 'done' } & Outcome);

export type ColumnKind =
  | { kind: 'text' }
  | { kind: 'number' }
  | { kind: 'boolean' }
  | { kind: 'date' }
  | { kind: 'link' }
  | { kind: 'select'; multi: boolean; options: { id: string; label: string }[] }
  | { kind: 'entity'; multi: boolean; target: EntityType };

export interface CatalogTable {
  id: string;
  database: string;
  name: string;
  columns: { id: string; name: string; kind: ColumnKind }[];
  source?: 'database' | 'people';
}

/** Every table a statement may name. */
export interface Catalog {
  tables: CatalogTable[];
}
