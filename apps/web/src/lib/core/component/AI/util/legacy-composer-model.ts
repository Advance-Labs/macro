import { createUserScopedStorage } from '@core/util/userScopedStorage';
import { FREE_DEFAULT_MODEL } from '../constant/model';
import { getSoupInputStoredModel } from './storage';

/**
 * A model this browser saved before choices moved to the server.
 * Gemini does not count: on the free plan it was the only option.
 */
export function legacyComposerModelChoice(
  userId: string | undefined
): string | undefined {
  if (userId) {
    const stored = createUserScopedStorage('agents-view-inmem-model-v1')
      .read(userId)
      ?.trim();
    if (stored && stored !== FREE_DEFAULT_MODEL) return stored;
  }
  const soup = getSoupInputStoredModel();
  if (!soup || soup === FREE_DEFAULT_MODEL) return;
  return soup;
}
