import { toast } from '@core/component/Toast/Toast';
import { throwOnErr } from '@core/util/result';
import { queryClient } from '@queries/client';
import {
  type CallSettings,
  callServiceClient,
  type UpdateCallSettingsRequest,
  type UpdateTeamCallPolicyRequest,
} from '@service-call/client';
import { useMutation, useQuery } from '@tanstack/solid-query';
import { callKeys } from './keys';

/** The viewer's call settings and their team's call policy. */
export function useCallSettingsQuery() {
  return useQuery(() => ({
    queryKey: callKeys.settings.queryKey,
    queryFn: () => throwOnErr(() => callServiceClient.getCallSettings()),
  }));
}

/**
 * Shows a settings change at once, then replaces it with what the server
 * stored. A failed save restores the previous settings.
 */
function optimisticCallSettings<Args>(
  patch: (args: Args, settings: CallSettings) => CallSettings,
  failure: string
) {
  const key = callKeys.settings.queryKey;
  return {
    onMutate: async (args: Args) => {
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<CallSettings>(key);
      if (previous) queryClient.setQueryData(key, patch(args, previous));
      return { previous };
    },
    onError: (
      error: Error,
      _args: Args,
      context: { previous?: CallSettings } | undefined
    ) => {
      console.error(failure, error);
      if (context?.previous) queryClient.setQueryData(key, context.previous);
      toast.failure(failure);
    },
    onSuccess: (settings: CallSettings) => {
      queryClient.setQueryData(key, settings);
    },
  };
}

export function useUpdateCallSettingsMutation() {
  return useMutation(() => ({
    mutationFn: (body: UpdateCallSettingsRequest) =>
      throwOnErr(() => callServiceClient.updateCallSettings(body)),
    ...optimisticCallSettings<UpdateCallSettingsRequest>(
      (body, settings) => ({
        ...settings,
        recordByDefault: {
          ...settings.recordByDefault,
          ...definedFields(body.recordByDefault ?? {}),
        },
        shareHuddlesByDefault:
          body.shareHuddlesByDefault ?? settings.shareHuddlesByDefault,
        refuseOneOnOneRecording:
          body.refuseOneOnOneRecording ?? settings.refuseOneOnOneRecording,
      }),
      'Failed to update call settings'
    ),
  }));
}

export function useUpdateTeamCallPolicyMutation() {
  return useMutation(() => ({
    mutationFn: (body: UpdateTeamCallPolicyRequest) =>
      throwOnErr(() => callServiceClient.updateTeamCallPolicy(body)),
    ...optimisticCallSettings<UpdateTeamCallPolicyRequest>(
      (body, settings) =>
        settings.team
          ? {
              ...settings,
              team: {
                ...settings.team,
                recordingBlocked: {
                  ...settings.team.recordingBlocked,
                  ...definedFields(body.recordingBlocked ?? {}),
                },
                huddleSharingBlocked:
                  body.huddleSharingBlocked ??
                  settings.team.huddleSharingBlocked,
              },
            }
          : settings,
      'Failed to update team call policy'
    ),
  }));
}

/** The flags a patch sets; omitted ones stay as they are. */
function definedFields<T extends object>(patch: T) {
  return Object.fromEntries(
    Object.entries(patch).filter(
      (entry): entry is [string, boolean] => typeof entry[1] === 'boolean'
    )
  );
}
