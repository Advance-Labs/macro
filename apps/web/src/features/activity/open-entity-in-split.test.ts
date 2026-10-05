import { beforeEach, describe, expect, it, vi } from 'vitest';
import { openEntityInSplit } from './open-entity-in-split';

const mocks = vi.hoisted(() => ({
  projectsEnabled: false,
  openWithSplit: vi.fn(),
  openDocument: vi.fn(),
  findOpenView: vi.fn(),
  alert: vi.fn(),
}));

vi.mock('@app/signal/splitLayout', () => ({
  globalSplitManager: () => ({
    openWithSplit: mocks.openWithSplit,
    findOpenView: mocks.findOpenView,
    activeSplitId: () => 'source',
  }),
}));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanel: () => undefined,
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { alert: mocks.alert },
}));
vi.mock('@core/component/LexicalMarkdown/component/core/BlockLink', () => ({
  openDocument: mocks.openDocument,
}));
vi.mock('@core/constant/featureFlags', () => ({
  enableProjects: { key: 'enable-projects' },
  isFeatureEnabled: () => mocks.projectsEnabled,
}));

beforeEach(() => {
  mocks.projectsEnabled = false;
  vi.clearAllMocks();
  mocks.findOpenView.mockReset();
  mocks.openDocument.mockReset();
});

describe('activity project navigation', () => {
  it('does not open native projects when the rollout is disabled', () => {
    openEntityInSplit({ block: 'initiative', id: 'launch', newSplit: false });
    expect(mocks.openWithSplit).not.toHaveBeenCalled();
    expect(mocks.openDocument).not.toHaveBeenCalled();
  });

  it('opens native project discussions when the rollout is enabled', () => {
    mocks.projectsEnabled = true;
    openEntityInSplit({
      block: 'initiative',
      id: 'launch',
      params: { discussion_id: 'discussion' },
      newSplit: true,
    });
    expect(mocks.openWithSplit).toHaveBeenCalledWith(
      { type: 'component', id: 'initiative-view~launch~overview~discussion' },
      { preferNewSplit: true }
    );
  });

  it('keeps legacy folder navigation available when projects are disabled', () => {
    openEntityInSplit({ block: 'project', id: 'folder', newSplit: false });
    expect(mocks.openDocument).toHaveBeenCalledWith(
      'project',
      'folder',
      undefined,
      false,
      expect.any(Function)
    );
  });
});

describe('activity reused-owner notices', () => {
  it('waits for route application and reports reuse once', () => {
    mocks.findOpenView.mockReturnValue({ owner: 'other' });
    openEntityInSplit({ block: 'md', id: 'document', newSplit: false });
    expect(mocks.alert).not.toHaveBeenCalled();
    const applied = mocks.openDocument.mock.calls[0][4];
    applied();
    applied();
    expect(mocks.alert).toHaveBeenCalledExactlyOnceWith('Content already open');
  });

  it('does not report navigation within the current owner as reuse', () => {
    mocks.findOpenView.mockReturnValue({ owner: 'source' });
    openEntityInSplit({ block: 'md', id: 'document', newSplit: false });
    mocks.openDocument.mock.calls[0][4]();
    expect(mocks.alert).not.toHaveBeenCalled();
  });

  it('deduplicates synchronous reuse and its applied callback', () => {
    mocks.findOpenView.mockReturnValue({ owner: 'other' });
    mocks.openDocument.mockImplementation(
      (_block, _id, _params, _newSplit, applied) => {
        applied();
        return { status: 'reused', owner: 'other', sourceOwner: 'source' };
      }
    );
    openEntityInSplit({ block: 'md', id: 'document', newSplit: false });
    expect(mocks.alert).toHaveBeenCalledExactlyOnceWith('Content already open');
  });
});
