import { queryClient } from '@queries/client';
import { invalidateDatabase } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { RenameEntitiesMutationVariables } from '@service-storage/graphql/generated/graphql';
import type { Client } from '@urql/core';
import { err, errAsync, ok, type Result, ResultAsync } from 'neverthrow';
import type { DatabaseEntityFailure } from '../core/write-failure';

type RenameDatabaseResult = {
  renameEntities: {
    results: (
      | { __typename: 'GraphqlMutationSuccess' }
      | { __typename: 'GraphqlMutationError'; message: string }
    )[];
  };
};

// Databases are not Soup entities. Request the mutation result without the
// generic rename fragment's Soup effects, whose hydration would fail.
const RENAME_DATABASE = `mutation RenameDatabase($inputs: [RenameEntityInput!]!) {
  renameEntities(inputs: $inputs) {
    results {
      __typename
      ... on GraphqlMutationError { message }
    }
  }
}`;

export function renameDatabase(
  client: Pick<Client, 'mutation'>,
  databaseId: string,
  name: string
): ResultAsync<void, DatabaseEntityFailure> {
  const displayName = name.trim();
  if (!displayName) return errAsync({ kind: 'empty-name' });
  const renamed = async (): Promise<Result<void, DatabaseEntityFailure>> => {
    const response = await client
      .mutation<RenameDatabaseResult, RenameEntitiesMutationVariables>(
        RENAME_DATABASE,
        {
          inputs: [
            { entity: { type: 'DATABASE', id: databaseId }, displayName },
          ],
        }
      )
      .toPromise();
    const result = response.data?.renameEntities.results[0];
    if (response.error || !result) return err({ kind: 'unreachable' });
    if (result.__typename === 'GraphqlMutationError')
      return err({ kind: 'refused', message: result.message });
    queryClient.setQueryData(
      databasesKeys.detail(databaseId).queryKey,
      (previous: DatabaseDetail | undefined) =>
        previous
          ? {
              ...previous,
              database: { ...previous.database, name: displayName },
            }
          : previous
    );
    void queryClient.invalidateQueries({
      queryKey: databasesKeys.list.queryKey,
    });
    // Qualified SQL names include the database name; open reads rerun against
    // the reloaded catalog.
    await invalidateDatabase(databaseId);
    return ok(undefined);
  };
  return new ResultAsync(renamed());
}
