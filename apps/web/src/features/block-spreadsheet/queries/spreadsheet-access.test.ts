import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  isSpreadsheetEnabledForCurrentUser,
  waitForSpreadsheetRollout,
} from './spreadsheet-access';

const state = vi.hoisted(() => ({
  enabled: true,
  override: undefined as boolean | undefined,
  cached: undefined as boolean | undefined,
  fresh: undefined as boolean | undefined,
  callback: undefined as
    | undefined
    | ((
        _flags: string[],
        _variants: Record<string, boolean>,
        context?: { errorsLoading?: boolean }
      ) => void),
  unsubscribe: vi.fn(),
  subscribe: vi.fn(),
}));
vi.mock('@core/constant/featureFlags', () => ({
  enableSpreadsheets: {
    key: 'enable-spreadsheets',
    get override() {
      return state.override;
    },
  },
  isFeatureEnabled: () => state.enabled,
}));
vi.mock('@app/lib/analytics', () => ({
  analytics: {
    posthog: {
      isFeatureEnabled: (_key: string, options?: { fresh?: boolean }) =>
        options?.fresh ? state.fresh : state.cached,
      onFeatureFlags: state.subscribe,
    },
  },
}));

/** PostHog has answered `/flags` for this session. */
function answer(enabled: boolean, context?: { errorsLoading?: boolean }) {
  if (!context?.errorsLoading) {
    state.fresh = enabled;
    state.cached = enabled;
  }
  state.callback?.([], {}, context);
}

beforeEach(() => {
  vi.useFakeTimers();
  state.override = undefined;
  state.cached = undefined;
  state.fresh = undefined;
  state.callback = undefined;
  state.unsubscribe.mockReset();
  state.subscribe.mockReset().mockImplementation((callback) => {
    state.callback = callback;
    return state.unsubscribe;
  });
});
afterEach(() => vi.useRealTimers());

describe('imperative spreadsheet rollout guard', () => {
  it('follows the PostHog decision without a separate email restriction', () => {
    state.enabled = true;
    expect(isSpreadsheetEnabledForCurrentUser()).toBe(true);
    state.enabled = false;
    expect(isSpreadsheetEnabledForCurrentUser()).toBe(false);
  });
});

describe('spreadsheet open rollout readiness', () => {
  it.each([true, false])(
    'uses an explicit %s override without waiting on PostHog',
    async (enabled) => {
      state.override = enabled;
      expect(await waitForSpreadsheetRollout()).toBe(enabled);
      expect(state.subscribe).not.toHaveBeenCalled();
    }
  );

  it('opens immediately when the persisted snapshot already enables it', async () => {
    state.cached = true;
    expect(await waitForSpreadsheetRollout()).toBe(true);
    expect(state.subscribe).not.toHaveBeenCalled();
  });

  it('uses this session’s answer once it has arrived', async () => {
    state.cached = false;
    state.fresh = false;
    expect(await waitForSpreadsheetRollout()).toBe(false);
    expect(state.subscribe).not.toHaveBeenCalled();
  });

  it.each([
    ['no snapshot', undefined],
    ['a snapshot from before the rollout', false],
  ])(
    'waits for the remote decision with %s instead of rejecting a shared workbook',
    async (_label, cached) => {
      state.cached = cached;
      const result = waitForSpreadsheetRollout();
      expect(await Promise.race([result, Promise.resolve('pending')])).toBe(
        'pending'
      );
      answer(true);
      expect(await result).toBe(true);
      expect(state.unsubscribe).toHaveBeenCalledOnce();
      expect(vi.getTimerCount()).toBe(0);
    }
  );

  it('respects a remote decision that keeps the viewer out of the rollout', async () => {
    const result = waitForSpreadsheetRollout();
    answer(false);
    expect(await result).toBe(false);
  });

  it('falls back to the persisted snapshot when flags fail to load', async () => {
    state.cached = false;
    const result = waitForSpreadsheetRollout();
    answer(true, { errorsLoading: true });
    expect(await result).toBe(false);
    expect(state.unsubscribe).toHaveBeenCalledOnce();
  });

  it('does not hang navigation if a blocked PostHog request never calls back', async () => {
    const result = waitForSpreadsheetRollout();
    await vi.advanceTimersByTimeAsync(3000);
    expect(await result).toBe(false);
    expect(state.unsubscribe).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('cleans up when PostHog invokes its readiness callback synchronously', async () => {
    state.subscribe.mockImplementation((callback) => {
      state.fresh = true;
      callback([], {});
      return state.unsubscribe;
    });
    expect(await waitForSpreadsheetRollout()).toBe(true);
    expect(state.unsubscribe).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });
});
