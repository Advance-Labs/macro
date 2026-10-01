import type { Client } from '@urql/core';
import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { trashDatabase } from './trash-database';

const cache = vi.hoisted(() => ({
  setQueryData: vi.fn(),
  invalidateQueries: vi.fn(),
}));
vi.mock('@queries/client', () => ({ queryClient: cache }));
beforeEach(() => vi.clearAllMocks());

function clientWith(response: unknown) {
  const mutation = vi.fn(() => ({ toPromise: async () => response }));
  return {
    client: { mutation } as unknown as Pick<Client, 'mutation'>,
    mutation,
  };
}

describe('trash database', () => {
  it('uses the database entity mutation without requesting Soup effects and removes only the confirmed item', async () => {
    const { client, mutation } = clientWith({
      data: {
        trashEntities: { results: [{ __typename: 'GraphqlMutationSuccess' }] },
      },
    });
    expect(await trashDatabase(client, 'db')).toEqual(ok(undefined));
    expect(mutation).toHaveBeenCalledWith(
      expect.not.stringContaining('effects'),
      { entities: [{ type: 'DATABASE', id: 'db' }] }
    );
    const update = cache.setQueryData.mock.calls[0][1];
    expect(
      update([{ database: { id: 'db' } }, { database: { id: 'other' } }])
    ).toEqual([{ database: { id: 'other' } }]);
    expect(cache.invalidateQueries).toHaveBeenCalledTimes(2);
  });
  it.each([
    {
      response: { error: new Error('Offline') },
      failure: { kind: 'unreachable' },
    },
    {
      response: {
        data: {
          trashEntities: {
            results: [
              {
                __typename: 'GraphqlMutationError',
                message: 'Owner access required',
              },
            ],
          },
        },
      },
      failure: { kind: 'refused', message: 'Owner access required' },
    },
    {
      response: { data: { trashEntities: { results: [] } } },
      failure: { kind: 'unreachable' },
    },
  ])(
    'leaves cached databases intact on a rejected or missing outcome',
    async ({ response, failure }) => {
      const { client } = clientWith(response);
      expect(await trashDatabase(client, 'db')).toEqual(err(failure));
      expect(cache.setQueryData).not.toHaveBeenCalled();
      expect(cache.invalidateQueries).not.toHaveBeenCalled();
    }
  );
});
