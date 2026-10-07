import { createRoot, createSignal } from 'solid-js';
import { describe, expect, it, vi } from 'vitest';
import {
  useSpreadsheetAccess,
  useSpreadsheetAccessLoading,
} from './use-spreadsheet-access';

const state = vi.hoisted(() => ({
  flag: (): boolean => false,
  loading: (): boolean => false,
}));
vi.mock('@app/lib/analytics/posthog', () => ({
  useFeatureFlag: () => () => ({
    enabled: state.flag(),
    loading: state.loading(),
  }),
}));
vi.mock('@core/constant/featureFlags', () => ({
  enableSpreadsheets: { key: 'enable-spreadsheets' },
}));
describe('spreadsheet internal rollout', () => {
  it('reacts to the PostHog rollout decision', () => {
    createRoot((dispose) => {
      const [flag, setFlag] = createSignal(false);
      state.flag = flag;
      const access = useSpreadsheetAccess();
      expect(access()).toBe(false);
      setFlag(true);
      expect(access()).toBe(true);
      setFlag(false);
      expect(access()).toBe(false);
      dispose();
    });
  });

  it('reports while the rollout answer is pending', () => {
    createRoot((dispose) => {
      const [loading, setLoading] = createSignal(true);
      state.loading = loading;
      const pending = useSpreadsheetAccessLoading();
      expect(pending()).toBe(true);
      setLoading(false);
      expect(pending()).toBe(false);
      dispose();
    });
  });
});
