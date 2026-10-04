import type { SharePermissions } from '@queries/sharing/share-permissions';
import { render, screen } from '@solidjs/testing-library';
import { ImperativeDialogHost } from '@ui';
import { createSignal, Show } from 'solid-js';
import { describe, expect, it, vi } from 'vitest';
import { Permissions } from '../SharePermissions';
import { useShareModal } from './shareModal';

const mocks = vi.hoisted(() => ({
  useMetadataQuery: vi.fn(),
  useAccessLevelQuery: vi.fn(),
}));

vi.mock('./ShareButton', () => ({
  ShareModal: (props: {
    name: string;
    sharePermissions?: { id: string };
    onOpenChange: (open: boolean) => void;
  }) => (
    <button
      type="button"
      data-testid="share-modal"
      data-permissions-id={props.sharePermissions?.id}
      onClick={() => props.onOpenChange(false)}
    >
      {props.name}
    </button>
  ),
}));
vi.mock('@queries/storage/document-metadata', () => ({
  useDocumentAccessLevelQuery: mocks.useAccessLevelQuery,
  useDocumentMetadataQuery: mocks.useMetadataQuery,
}));

describe('useShareModal', () => {
  it('forwards host grants and reactive updates through the dialog host', async () => {
    let openShare!: () => void;
    const [grants, setGrants] = createSignal<SharePermissions>();
    render(() => {
      openShare = useShareModal(() => ({
        id: 'doc-1',
        blockAlias: 'md',
        itemType: 'document',
        name: 'Plan',
        userPermissions: Permissions.OWNER,
        sharePermissions: grants(),
      }));
      return <ImperativeDialogHost />;
    });
    openShare();
    const modal = await screen.findByTestId('share-modal');
    expect(modal.getAttribute('data-permissions-id')).toBeNull();
    setGrants({
      id: 'host-grants',
      owner: 'owner',
      channelSharePermissions: [],
    });
    expect(modal.getAttribute('data-permissions-id')).toBe('host-grants');
  });
  it('waits for share data before opening', async () => {
    const [ready, setReady] = createSignal(false);
    let openShare!: () => void;
    render(() => {
      openShare = useShareModal(() =>
        ready()
          ? {
              id: 'doc-1',
              blockAlias: 'md',
              itemType: 'document',
              name: 'Plan',
              userPermissions: Permissions.OWNER,
            }
          : undefined
      );
      return <ImperativeDialogHost />;
    });

    openShare();
    expect(screen.queryByTestId('share-modal')).toBeNull();

    setReady(true);
    expect((await screen.findByTestId('share-modal')).textContent).toBe('Plan');
  });

  it('reports when the person closes an opened modal', async () => {
    const onClose = vi.fn();
    const [owned, setOwned] = createSignal(true);
    let openShare!: () => void;
    function Opener() {
      openShare = useShareModal(
        () => ({
          id: 'doc-1',
          blockAlias: 'md',
          itemType: 'document',
          name: 'Plan',
          userPermissions: Permissions.OWNER,
        }),
        { onClose }
      );
      return null;
    }
    render(() => (
      <>
        <Show when={owned()}>
          <Opener />
        </Show>
        <ImperativeDialogHost />
      </>
    ));

    openShare();
    (await screen.findByTestId('share-modal')).click();
    await vi.waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(screen.queryByTestId('share-modal')).toBeNull();

    // Unmounting the owner closes the modal without reporting it.
    openShare();
    await screen.findByTestId('share-modal');
    setOwned(false);
    await vi.waitFor(() =>
      expect(screen.queryByTestId('share-modal')).toBeNull()
    );
    await Promise.resolve();
    expect(onClose).toHaveBeenCalledOnce();
  });
});
