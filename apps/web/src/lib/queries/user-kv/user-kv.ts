import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import { storageServiceClient } from '@service-storage/client';
import type { UserKvEntry } from '@service-storage/generated/schemas/userKvEntry';
import type { UserKvEntryValue } from '@service-storage/generated/schemas/userKvEntryValue';
import { useMutation, useQuery } from '@tanstack/solid-query';
import { userKvKeys } from './keys';

/**
 * The caller's entries in one namespace of the per-user key-value store,
 * e.g. `tours`. Each use case validates its own values; the server only
 * guarantees they're JSON objects.
 *
 * Gate reads on `isSuccess` (see `queries/gate.ts`): reading `data` while
 * pending suspends the caller's `<Suspense>`.
 */
export function useUserKvQuery(namespace: () => string) {
  return useQuery(() => ({
    queryKey: userKvKeys.namespace(namespace()).queryKey,
    queryFn: async (): Promise<UserKvEntry[]> =>
      await throwOnErr(() =>
        storageServiceClient.listUserKv({ namespace: namespace() })
      ),
    // Writes update the cache directly, so the list only needs a refetch
    // when another device may have changed it.
    staleTime: 5 * 60 * 1000,
  }));
}

function replaceEntry(
  namespace: string,
  key: string,
  update: (current: UserKvEntry | undefined) => UserKvEntry
) {
  queryClient.setQueryData<UserKvEntry[]>(
    userKvKeys.namespace(namespace).queryKey,
    (entries) => {
      const list = entries ?? [];
      const current = list.find((entry) => entry.key === key);
      const next = update(current);
      return current
        ? list.map((entry) => (entry.key === key ? next : entry))
        : [...list, next].sort((a, b) => a.key.localeCompare(b.key));
    }
  );
}

/**
 * Create or replace an entry. The cached namespace updates immediately and
 * rolls back if the write fails.
 */
export function usePutUserKvMutation() {
  return useMutation(() => ({
    gcTime: 0,
    mutationFn: async (vars: {
      namespace: string;
      key: string;
      value: UserKvEntryValue;
    }): Promise<UserKvEntry> =>
      await throwOnErr(() => storageServiceClient.putUserKv(vars)),
    onMutate: async (vars) => {
      const queryKey = userKvKeys.namespace(vars.namespace).queryKey;
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<UserKvEntry[]>(queryKey);
      const now = new Date().toISOString();
      replaceEntry(vars.namespace, vars.key, (current) => ({
        namespace: vars.namespace,
        key: vars.key,
        value: vars.value,
        createdAt: current?.createdAt ?? now,
        updatedAt: now,
      }));
      return { previous };
    },
    onSuccess: (entry, vars) =>
      replaceEntry(vars.namespace, vars.key, () => entry),
    onError: (error, vars, context) => {
      console.error('failed to write user kv entry', error);
      queryClient.setQueryData(
        userKvKeys.namespace(vars.namespace).queryKey,
        context?.previous
      );
    },
  }));
}
