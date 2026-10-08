import { describe, expect, it } from 'vitest';
import { FREE_DEFAULT_MODEL, Model } from '../constant/model';
import {
  catalogOffersModelChoice,
  modelChoiceIsExplicit,
  resolveUpgradedModel,
  UPGRADE_MODEL,
} from './plan-model';

const paidCatalog = [
  Model.sonnet55,
  Model.opus55,
  Model.gemini38Flash,
  'fireworks/kimi-k3',
];

describe('plan model choice', () => {
  it('treats a stored non-free model as a choice and Gemini as one only when flagged', () => {
    expect(modelChoiceIsExplicit(Model.sonnet55, false)).toBe(true);
    expect(modelChoiceIsExplicit(FREE_DEFAULT_MODEL, false)).toBe(false);
    expect(modelChoiceIsExplicit(FREE_DEFAULT_MODEL, true)).toBe(true);
    expect(modelChoiceIsExplicit(undefined, true)).toBe(false);
    expect(catalogOffersModelChoice([FREE_DEFAULT_MODEL])).toBe(false);
    expect(catalogOffersModelChoice(paidCatalog)).toBe(true);
  });

  it('lands a paid plan with no pick on Opus', () => {
    expect(
      resolveUpgradedModel({
        paid: true,
        explicit: false,
        catalog: paidCatalog,
        currentModel: Model.sonnet55,
      })
    ).toBe(UPGRADE_MODEL);
  });

  it('keeps Gemini when it was chosen from a catalog that offered other models', () => {
    expect(
      resolveUpgradedModel({
        paid: true,
        preferred: FREE_DEFAULT_MODEL,
        explicit: true,
        catalog: paidCatalog,
        currentModel: Model.sonnet55,
      })
    ).toBe(FREE_DEFAULT_MODEL);
  });

  it('keeps any other chosen model', () => {
    expect(
      resolveUpgradedModel({
        paid: true,
        preferred: 'fireworks/kimi-k3',
        explicit: true,
        catalog: paidCatalog,
        currentModel: Model.sonnet55,
      })
    ).toBe('fireworks/kimi-k3');
  });

  it('keeps the free catalog default while the user is still on the free plan', () => {
    expect(
      resolveUpgradedModel({
        paid: false,
        preferred: Model.sonnet55,
        explicit: true,
        catalog: [FREE_DEFAULT_MODEL],
        currentModel: FREE_DEFAULT_MODEL,
      })
    ).toBe(FREE_DEFAULT_MODEL);
  });

  it('does not invent a model while the catalog is still empty', () => {
    expect(
      resolveUpgradedModel({
        paid: true,
        explicit: false,
        catalog: [],
        currentModel: undefined,
      })
    ).toBeUndefined();
  });
});
