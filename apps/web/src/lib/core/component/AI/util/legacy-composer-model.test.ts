import { beforeEach, describe, expect, it } from 'vitest';
import { Model } from '../constant/model';
import { legacyComposerModelChoice } from './legacy-composer-model';

beforeEach(() => window.localStorage.clear());

describe('legacy composer model choice', () => {
  it('prefers the in-memory model saved for this user', () => {
    window.localStorage.setItem(
      'agents-view-inmem-model-v1:user-a',
      'fireworks/kimi-k3'
    );
    localStorage.setItem(
      'soup-chat-input-model',
      JSON.stringify(Model.sonnet55)
    );
    expect(legacyComposerModelChoice('user-a')).toBe('fireworks/kimi-k3');
    expect(legacyComposerModelChoice('user-b')).toBe(Model.sonnet55);
  });

  it('ignores Gemini and blank values left by the free plan', () => {
    window.localStorage.setItem(
      'agents-view-inmem-model-v1:user-a',
      'google/gemini-3.8-flash'
    );
    expect(legacyComposerModelChoice('user-a')).toBeUndefined();
    window.localStorage.setItem('agents-view-inmem-model-v1:user-a', '  ');
    localStorage.setItem(
      'soup-chat-input-model',
      JSON.stringify(Model.gemini38Flash)
    );
    expect(legacyComposerModelChoice('user-a')).toBeUndefined();
  });
});
