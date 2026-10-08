import { FREE_DEFAULT_MODEL, Model, type TModel } from '../constant/model';

/**
 * Model a paid composer opens on when the user has never picked one.
 * The paid catalog default stays Sonnet; Opus is only this landing spot.
 */
export const UPGRADE_MODEL: TModel = Model.opus55;

/**
 * A stored id counts as a choice when the user picked it, or when it cannot
 * be the free plan's only model. Gemini saved while it was the only option
 * does not count.
 */
export function modelChoiceIsExplicit(
  model: string | undefined,
  explicitFlag: boolean
): boolean {
  if (!model) return false;
  if (explicitFlag) return true;
  return model !== FREE_DEFAULT_MODEL;
}

/**
 * The free catalog is Gemini alone, so picking that row is not a choice.
 * Any other advertised model means the picker offered a real selection.
 */
export function catalogOffersModelChoice(modelIds: readonly string[]): boolean {
  return modelIds.some((id) => id !== FREE_DEFAULT_MODEL);
}

/**
 * Model the Macro composer should run.
 *
 * A saved pick wins, including Gemini chosen once other models were
 * available. With no pick, a paid plan lands on {@link UPGRADE_MODEL}.
 * A free plan keeps the catalog's current model.
 */
export function resolveUpgradedModel(input: {
  paid: boolean;
  preferred?: string;
  explicit: boolean;
  catalog: readonly string[];
  currentModel?: string;
  /** Picker choice from this visit. Non-explicit picks are not passed. */
  sessionModel?: string;
  sessionExplicit?: boolean;
}): string | undefined {
  const listed = (id: string | undefined): id is string =>
    Boolean(id && input.catalog.includes(id));

  if (input.sessionExplicit && listed(input.sessionModel)) {
    return input.sessionModel;
  }
  if (input.explicit && listed(input.preferred)) return input.preferred;

  const noChoice = !input.sessionExplicit && !input.explicit;
  if (noChoice && input.paid && input.catalog.length > 0) {
    return UPGRADE_MODEL;
  }

  if (listed(input.sessionModel)) return input.sessionModel;
  if (listed(input.preferred)) return input.preferred;
  return input.currentModel;
}
