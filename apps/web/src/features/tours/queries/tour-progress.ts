import { usePutUserKvMutation, useUserKvQuery } from '@queries/user-kv/user-kv';
import {
  parseTourProgress,
  TOURS_NAMESPACE,
  type TourProgress,
} from '../core/progress';

/**
 * Progress for one tour, stored in the user's `tours` key-value namespace.
 * Every mounted tour shares one namespace query.
 *
 * `ready` is false until progress has loaded, and stays false if it can't
 * load, so a tour the user already finished never flashes up.
 *
 * With `localOnly`, nothing is read or saved: the tour starts fresh on every
 * mount, for iterating on tours locally.
 */
export function createTourProgress(props: {
  tourId: string;
  localOnly: boolean;
}) {
  const entries = useUserKvQuery(() => TOURS_NAMESPACE, {
    enabled: () => !props.localOnly,
  });
  const put = usePutUserKvMutation();

  const ready = () => props.localOnly || entries.isSuccess;
  const stored = () => {
    if (props.localOnly || !entries.isSuccess) return undefined;
    const entry = entries.data.find(({ key }) => key === props.tourId);
    return entry && parseTourProgress(entry.value);
  };

  const save = (progress: TourProgress) => {
    if (props.localOnly) return;
    put.mutate({
      namespace: TOURS_NAMESPACE,
      key: props.tourId,
      value: progress,
    });
  };

  return { ready, stored, save };
}
