import { useHasPaidAccess } from '@core/auth/license';
import { legacyComposerModelChoice } from '@core/component/AI/util/legacy-composer-model';
import { useUserId } from '@core/context/user';
import { throwOnErr } from '@core/util/result';
import { cognitionApiServiceClient } from '@service-cognition/client';
import { useQuery } from '@tanstack/solid-query';
import { createEffect, on } from 'solid-js';
import { queryClient } from './client';

export type ComposerModelPreference = {
  modelId: string;
  explicit: boolean;
};

const migrated = new Set<string>();

function queryKey(userId: string | undefined) {
  return ['composer-model', userId] as const;
}

/** The signed-in user's saved composer model, loaded from the server. */
export function useComposerModelPreference() {
  const userId = useUserId();
  const paid = useHasPaidAccess();
  const query = useQuery(() => {
    const id = userId();
    return {
      queryKey: queryKey(id),
      enabled: Boolean(id),
      queryFn: () =>
        throwOnErr(() => cognitionApiServiceClient.getSelectedModel()),
    };
  });

  const remember = async (modelId: string) => {
    const id = userId();
    if (!id) return;
    const result = await cognitionApiServiceClient.putSelectedModel({
      modelId,
    });
    if (result.isErr()) return;
    queryClient.setQueryData(queryKey(id), result.value);
  };

  // Copy a choice this browser saved before the server kept it.
  createEffect(
    on(
      () => [userId(), paid(), query.isSuccess, query.data] as const,
      ([id, isPaid, loaded, data]) => {
        if (!id || !isPaid || !loaded || !data || data.explicit) return;
        if (migrated.has(id)) return;
        const legacy = legacyComposerModelChoice(id);
        if (!legacy) return;
        migrated.add(id);
        void remember(legacy);
      }
    )
  );

  return {
    loaded: () => query.isSuccess,
    modelId: () => query.data?.modelId,
    explicit: () => query.data?.explicit === true,
    remember,
  };
}
