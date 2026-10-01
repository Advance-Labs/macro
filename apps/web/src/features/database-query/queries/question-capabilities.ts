import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import { errAsync, okAsync, type ResultAsync } from 'neverthrow';
import type { QueryCapabilities } from '../context/query-context';
import type {
  QueryAnswer,
  QueryFailure,
  QueryProposal,
  QuerySchema,
} from '../core/query';
import { toQuerySchema } from './query-source';

/** Resolve model-selected sources through the same permission-checked schema read as the UI. */
export function createQuestionCapabilities(
  input: QueryCapabilities & {
    describe: (databaseId: string) => ResultAsync<DatabaseDetail, QueryFailure>;
  }
): QueryCapabilities {
  return {
    read: (sql, context) =>
      input
        .read(sql)
        .andThen((answer): ResultAsync<QueryAnswer, QueryFailure> => {
          const databaseIds = answer.readDatabaseIds;
          if (
            context?.databaseId &&
            databaseIds.some((id) => id !== context.databaseId)
          )
            return errAsync({ kind: 'other-database' });
          const databaseId = databaseIds.includes(
            context?.source?.databaseId ?? ''
          )
            ? context?.source?.databaseId
            : databaseIds.toSorted()[0];
          if (!databaseId)
            return okAsync({
              ...answer,
              source: context?.databaseId
                ? context.source
                : { name: 'Automatic', tables: [] },
            });
          const knownSource = context?.source;
          const source: ResultAsync<QuerySchema, QueryFailure> =
            knownSource?.databaseId === databaseId &&
            (databaseIds.length > 1 ||
              answer.readTables.every((id) =>
                knownSource.tables.some((table) => table.id === id)
              ))
              ? okAsync(knownSource)
              : input
                  .describe(databaseId)
                  .map((detail) => toQuerySchema(detail));
          return source.andThen((verified) =>
            verified.databaseId !== databaseId ||
            (databaseIds.length === 1 &&
              answer.readTables.some(
                (id) => !verified.tables.some((table) => table.id === id)
              ))
              ? errAsync<QueryAnswer, QueryFailure>({
                  kind: 'unverified-source',
                })
              : okAsync({ ...answer, source: verified })
          );
        }),
    generate: (request) =>
      input
        .generate(request)
        .andThen((proposal): ResultAsync<QueryProposal, QueryFailure> => {
          const databaseId = proposal.databaseId ?? request.schema.databaseId;
          if (!databaseId) return okAsync(proposal);
          if (
            request.schema.databaseId &&
            databaseId !== request.schema.databaseId
          )
            return errAsync({ kind: 'other-database' });
          // The selected database already came from an authorized schema read. The
          // answer read below verifies live access/dependencies and refreshes the
          // schema if it encounters a new table; avoid another sequential fetch.
          if (
            request.schema.databaseId === databaseId &&
            request.schema.tables.some(
              (table) => !table.id.startsWith('platform:')
            )
          )
            return okAsync({ ...proposal, source: request.schema });
          return input.describe(databaseId).andThen((detail) =>
            detail.database.id !== databaseId
              ? errAsync<QueryProposal, QueryFailure>({
                  kind: 'unverified-source',
                })
              : okAsync({
                  ...proposal,
                  source: toQuerySchema(detail, request.schema.focusTableId),
                })
          );
        }),
  };
}
