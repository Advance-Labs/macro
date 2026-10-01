import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import type { ListedDatabase } from '@service-storage/generated/schemas/listedDatabase';
import type { TrashEntitiesMutationVariables } from '@service-storage/graphql/generated/graphql';
import type { Client } from '@urql/core';
import { err, ok, type Result, ResultAsync } from 'neverthrow';
import type { DatabaseEntityFailure } from '../core/write-failure';

type TrashDatabaseResult = {
  trashEntities: {
    results: (
      | { __typename: 'GraphqlMutationSuccess' }
      | { __typename: 'GraphqlMutationError'; message: string }
    )[];
  };
};

// Like database rename, request the outcome without Soup effects: databases
// have their own catalog and cannot be hydrated as Soup entities.
const TRASH_DATABASE = `mutation TrashDatabase($entities: [EntityRefInput!]!) {
  trashEntities(entities: $entities) {
    results {
      __typename
      ... on GraphqlMutationError { message }
    }
  }
}`;

export function trashDatabase(
  client: Pick<Client, 'mutation'>,
  databaseId: string
): ResultAsync<void, DatabaseEntityFailure> {
  const trashed = async (): Promise<Result<void, DatabaseEntityFailure>> => {
    const response = await client
      .mutation<TrashDatabaseResult, TrashEntitiesMutationVariables>(
        TRASH_DATABASE,
        { entities: [{ type: 'DATABASE', id: databaseId }] }
      )
      .toPromise();
    const result = response.data?.trashEntities.results[0];
    if (response.error || !result) return err({ kind: 'unreachable' });
    if (result.__typename === 'GraphqlMutationError')
      return err({ kind: 'refused', message: result.message });
    queryClient.setQueryData(
      databasesKeys.list.queryKey,
      (previous: ListedDatabase[] | undefined) =>
        previous?.filter(({ database }) => database.id !== databaseId)
    );
    void queryClient.invalidateQueries({
      queryKey: databasesKeys.list.queryKey,
    });
    void queryClient.invalidateQueries({
      queryKey: databasesKeys.detail(databaseId).queryKey,
      refetchType: 'none',
    });
    return ok(undefined);
  };
  return new ResultAsync(trashed());
}
