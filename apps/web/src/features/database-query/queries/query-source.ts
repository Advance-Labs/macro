import { databaseQueryKeys } from '@queries/storage/keys';
import type { DatabaseDetail, ExecOutcome } from '@service-storage/databases';
import { useQuery } from '@tanstack/solid-query';
import { onCleanup } from 'solid-js';
import type { QuerySchema } from '../core/query';

export function toQuerySchema(
  detail: DatabaseDetail,
  activeTableId?: string
): QuerySchema {
  // The one platform table the dialect exposes: `macro.people`, every
  // person the viewer can see, keyed by entity id so entity columns join to it.
  const platformTables: QuerySchema['tables'] = [
    {
      id: 'platform:people',
      name: 'People in your teams',
      sqlName: 'macro.people',
      primaryKey: 'id',
      columns: ['id', 'name', 'email'].map((name) => ({
        name,
        sqlName: `"${name}"`,
        type: 'String',
        multiple: false,
        options: [],
      })),
    },
  ];
  return {
    databaseId: detail.database.id,
    name: detail.database.name,
    focusTableId: detail.tables.find(({ table }) => table.id === activeTableId)
      ?.table.id,
    tables: [
      ...detail.tables.map((table) => ({
        id: table.table.id,
        name: table.table.name,
        sqlName: table.sql_name,
        primaryKey: 'row_id',
        columns: table.columns.map((column) => ({
          name:
            column.column.display_name ??
            column.definition.definition.display_name,
          sqlName: column.sql_name,
          type: column.definition.definition.data_type,
          multiple:
            column.definition.definition.is_multi_select ||
            column.column.config?.kind === 'link',
          relation:
            column.column.config?.kind === 'link'
              ? {
                  databaseId: column.column.config.database_id,
                  tableId: column.column.config.table_id,
                  writable: column.writable,
                }
              : undefined,
          options: column.definition.property_options.map((option) =>
            String(option.value.value)
          ),
        })),
      })),
      ...platformTables,
    ],
  };
}

export function createLiveQuerySource(input: {
  sql: () => string;
  read: (sql: string) => Promise<ExecOutcome>;
  subscribe: (onChange: (tableId: string, version: number) => void) => void;
}) {
  const query = useQuery(() => {
    const statement = input.sql();
    return {
      queryKey: databaseQueryKeys.answer(statement).queryKey,
      queryFn: () => input.read(statement),
      enabled: !!statement.trim(),
      staleTime: 30_000,
      retry: false,
      refetchOnWindowFocus: true,
    };
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  input.subscribe((tableId, version) => {
    // A failed refresh hides the answer, but its last successful dependency
    // versions still let later table events recover the query automatically.
    const result = !query.isPending ? query.data : undefined;
    const readVersion = result?.read_versions[tableId];
    if (readVersion === undefined || version <= readVersion) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      void query.refetch();
    }, 300);
  });
  onCleanup(() => clearTimeout(timer));
  return query;
}
