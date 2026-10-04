import { ForwardToChannel } from '@core/component/ForwardToChannel';
import { HotkeyScope } from '@core/hotkey/HotkeyScope';
import { queryClient } from '@queries/client';
import { sharingKeys } from '@queries/sharing/keys';
import {
  invalidateSharePermissions,
  type SharePermissions,
  type SharePermissionsTarget,
  useSharePermissionsQuery,
} from '@queries/sharing/share-permissions';
import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { err, ok } from 'neverthrow';
import { createRoot, createSignal, For, type JSX, Show } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Permissions } from '../SharePermissions';
import { ShareModal, ShareOptions, ShareTrigger } from './ShareButton';

const ME = 'macro|me@example.com';
const SOMEONE_ELSE = 'macro|someone-else@example.com';

const mocks = vi.hoisted(() => ({
  registerHotkey: vi.fn(),
  sendToChannel: vi.fn(),
  sendToUsers: vi.fn(),
  mobile: false,
  hasTeam: false,
  getAgentPermissions: vi.fn(),
  updateAgentPermissions: vi.fn(),
  getInitiativePermissions: vi.fn(),
  updateInitiativePermissions: vi.fn(),
  getDocumentPermissions: vi.fn(),
  getDatabasePermissions: vi.fn(),
  updateDatabasePermissions: vi.fn(),
  getChatPermissions: vi.fn(),
  updateChatPermissions: vi.fn(),
  fetchCallSharePermission: vi.fn(),
  updateCallTeamShare: vi.fn(),
  setCallRecordTeamShareCache: vi.fn(),
  callRecordShared: true,
  callRecordChannelId: 'channel-1' as string | null,
  callRecordQuerySuccess: true,
  getProjectPermissions: vi.fn(),
  editProject: vi.fn(),
  editDocument: vi.fn(),
  copyLink: vi.fn(),
  sharePermissionsRead: vi.fn(),
  inBlock: true,
}));
vi.mock('@queries/client', async () => {
  const { QueryClient } = await import('@tanstack/solid-query');
  return {
    queryClient: new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: Infinity,
          refetchOnWindowFocus: false,
        },
      },
    }),
  };
});
vi.mock('@queries/storage/databases', () => ({
  getDatabaseSharePermissions: mocks.getDatabasePermissions,
  updateDatabaseSharePermissions: mocks.updateDatabasePermissions,
}));
vi.mock('@app/lib/analytics/analytics-context', () => ({
  useAnalytics: () => ({ track: vi.fn() }),
}));
vi.mock('@channel/Input', () => ({
  createConfiguredChannelMarkdownEditor: () => ({
    controls: { focus: vi.fn() },
  }),
}));
vi.mock('@core/auth', () => ({ useIsAuthenticated: () => () => true }));
vi.mock('@app/lib/constants/file-metadata', () => ({
  resolveBlockAlias: (name: string) =>
    ['task', 'snippet', 'skill'].includes(name) ? 'md' : name,
}));
vi.mock('@core/block', () => ({
  isInBlock: () => mocks.inBlock,
  useBlockAliasedName: () => 'agent',
  useBlockId: () => 'launcher-placeholder',
  useBlockName: () => 'md',
  useMaybeBlockName: () => 'md',
  useMaybeBlockAliasedName: () => 'md',
  useMaybeBlockId: () => 'launcher-placeholder',
}));
vi.mock('@core/component/CustomScrollbar', () => ({
  CustomScrollbar: () => null,
}));
vi.mock('@core/component/LexicalMarkdown/builder/MarkdownShell', () => ({
  MarkdownShell: () => <textarea aria-label="Optional message" />,
}));
vi.mock('@core/component/RecipientSelector', () => ({
  RecipientSelector: (props: {
    setSelectedOptions: (items: unknown[]) => void;
  }) => (
    <button
      onClick={() =>
        props.setSelectedOptions([{ kind: 'channel', id: 'channel-1' }])
      }
    >
      Select channel
    </button>
  ),
}));

vi.mock('@core/hotkey/hotkeys', () => ({
  registerHotkey: mocks.registerHotkey,
  useHotkeyDOMScope: () => [vi.fn(), 'share-scope'],
}));
vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => mocks.mobile }));
vi.mock('@core/signal/useCombinedRecipient', () => ({
  useCombinedRecipients: () => ({ all: () => [] }),
}));
vi.mock('@core/util/channels', () => ({ useSendMessageToPeople: () => mocks }));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: {
    getBatchChannelPreviews: async () => ok({ previews: [] }),
    getDocumentPermissions: mocks.getDocumentPermissions,
    editDocument: mocks.editDocument,
    projects: {
      getPermissions: mocks.getProjectPermissions,
      edit: mocks.editProject,
    },
  },
  blockNameToItemType: (name: string) =>
    name === 'agent' ? 'agent_session' : 'document',
  itemTypeToReferenceEntityType: (type: string) => type,
}));
vi.mock('@queries/agent-session/share-permissions', () => ({
  fetchAgentSessionSharePermissions: (...args: unknown[]) =>
    mocks.getAgentPermissions(...args),
  updateAgentSessionSharePermissions: (...args: unknown[]) =>
    mocks.updateAgentPermissions(...args),
}));
vi.mock('@queries/initiative/share-permissions', () => ({
  fetchInitiativeSharePermissions: (...args: unknown[]) =>
    mocks.getInitiativePermissions(...args),
  updateInitiativeSharePermissions: (...args: unknown[]) =>
    mocks.updateInitiativePermissions(...args),
}));
vi.mock('@core/component/SharePermissions', () => ({
  Permissions: { OWNER: 'owner', CAN_VIEW: 'view' },
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { success: vi.fn(), failure: vi.fn(), alert: vi.fn() },
}));
vi.mock('@core/component/VerticalScrollIndicators', () => ({
  ScrollIndicators: () => null,
}));
vi.mock('@core/context/user', () => ({
  useUserId: () => () => ME,
  useReferralCode: () => () => undefined,
}));
vi.mock('@channel/use-channel-participants', () => ({
  useChannelParticipants: () => ({ users: () => [], ids: () => [] }),
}));
vi.mock('@core/component/EntityIcon', () => ({ EntityIcon: () => null }));
vi.mock('@core/component/UserIcon', () => ({ UserIcon: () => null }));
vi.mock('@core/component/Tabs', () => ({
  Tabs: (props: {
    list: { value: string; label: string }[];
    value?: string;
    onChange?: (value: string) => void;
  }) => (
    <div role="tablist">
      <For each={props.list}>
        {(tab) => (
          <button
            role="tab"
            aria-selected={props.value === tab.value}
            onClick={() => props.onChange?.(tab.value)}
          >
            {tab.label}
          </button>
        )}
      </For>
    </div>
  ),
}));
vi.mock('@core/signal/permissions', () => ({
  useGetPermissions: () => () => 'owner',
  useIsDocumentOwner: () => () => true,
}));
vi.mock('@core/user', () => ({ getDisplayName: (id: string) => id }));
vi.mock('@core/util/currentBlockDocumentName', () => ({
  useBlockDocumentName: () => () => '',
}));
vi.mock('@core/util/url', () => ({
  buildSimpleEntityUrl: ({ type, id }: { type: string; id: string }) =>
    `https://macro.com/app/${type}/${id}`,
}));
vi.mock('@service-cognition/client', () => ({
  cognitionApiServiceClient: {
    getChatPermissions: mocks.getChatPermissions,
    updateChatPermissions: mocks.updateChatPermissions,
  },
}));
vi.mock('@queries/call/call', () => ({
  fetchCallSharePermission: (...args: unknown[]) =>
    mocks.fetchCallSharePermission(...args),
  updateCallTeamShare: (...args: unknown[]) =>
    mocks.updateCallTeamShare(...args),
  setCallRecordTeamShareCache: (...args: unknown[]) =>
    mocks.setCallRecordTeamShareCache(...args),
  sharePermissionFromCallRecord: (record: {
    callId: string;
    createdBy: string;
    shareWithTeam: boolean;
  }) => ({
    id: record.callId,
    owner: record.createdBy,
    teamShareAccessLevel: record.shareWithTeam ? 'view' : null,
  }),
  useCallRecordQuery: () => ({
    get isSuccess() {
      return mocks.callRecordQuerySuccess;
    },
    get data() {
      return {
        callId: 'call-1',
        channelId: mocks.callRecordChannelId,
        createdBy: ME,
        shareWithTeam: mocks.callRecordShared,
      };
    },
  }),
}));
vi.mock('@queries/team/teams', () => ({
  useCurrentTeamQuery: () => ({
    isSuccess: mocks.hasTeam,
    data: mocks.hasTeam ? { id: 'team-1' } : undefined,
  }),
}));
vi.mock('@solidjs/router', () => ({ useNavigate: () => vi.fn() }));
vi.mock('./LoginButton', () => ({ openLoginModal: vi.fn() }));
vi.mock('@kobalte/core/dialog', () => {
  const Container = (props: { children?: JSX.Element }) => props.children;
  return {
    Dialog: Object.assign(Container, {
      Portal: Container,
      Overlay: () => null,
      Content: Container,
      Title: Container,
    }),
  };
});
vi.mock('@components/app/mobile/MobileDrawer', () => {
  const Container = (props: { children?: JSX.Element }) => props.children;
  return {
    MobileDrawer: Object.assign(Container, {
      Portal: Container,
      Overlay: () => null,
      Content: Container,
    }),
  };
});
vi.mock('@ui', async () => {
  const { createContext, useContext } = await import('solid-js');
  const RadioContext = createContext<{
    label: string;
    onChange?: (value: string) => void;
  }>();
  const Container = (props: { children?: JSX.Element }) => props.children;
  const Button = (props: {
    children?: JSX.Element;
    disabled?: boolean;
    tooltip?: string;
    onClick?: () => void;
  }) => (
    <button
      disabled={props.disabled}
      onClick={props.onClick}
      aria-label={props.tooltip}
    >
      {props.children}
    </button>
  );
  return {
    Button,
    CopyButton: Button,
    Panel: Object.assign(Container, { Header: Container, Body: Container }),
    // The owner row draws an unknown owner's avatar.
    Avatar: Object.assign(Container, { Fallback: Container }),
    Tooltip: Container,
    Dropdown: Object.assign(Container, {
      Trigger: Container,
      Content: Container,
      Item: Container,
      // Use only rendered options, so the mock cannot invent unsupported grants.
      RadioGroup: (props: {
        children?: JSX.Element;
        value?: string;
        'aria-label'?: string;
        onChange?: (value: string) => void;
      }) => {
        const label = props['aria-label'] ?? 'option';
        return (
          <div role="group" aria-label={label} data-value={props.value}>
            <RadioContext.Provider value={{ label, onChange: props.onChange }}>
              {props.children}
            </RadioContext.Provider>
          </div>
        );
      },
      RadioItem: (props: { value: string; children?: JSX.Element }) => {
        const group = useContext(RadioContext);
        return (
          <button
            aria-label={`Set ${group?.label} ${props.value}`}
            onClick={() => group?.onChange?.(props.value)}
          >
            {props.children}
          </button>
        );
      },
      ItemIndicator: Container,
      Group: Container,
    }),
    ButtonGroup: Object.assign(Container, { Divider: () => null }),
    SegmentedControl: (props: {
      'aria-label'?: string;
      onChange?: (value: string) => void;
      options: { value: string; disabled?: boolean }[];
    }) => (
      <div role="group" aria-label={props['aria-label']}>
        <For each={props.options}>
          {(option) => (
            <button
              disabled={option.disabled}
              onClick={() => props.onChange?.(option.value)}
            >
              Set link {option.value}
            </button>
          )}
        </For>
      </div>
    ),
    cn: (...values: unknown[]) => values.filter(Boolean).join(' '),
    Hotkey: () => null,
  };
});
beforeEach(() => {
  vi.clearAllMocks();
  queryClient.clear();
  mocks.registerHotkey.mockImplementation(() => ({ dispose: vi.fn() }));
  mocks.inBlock = true;
  mocks.mobile = false;
  mocks.hasTeam = false;
  mocks.callRecordShared = true;
  mocks.callRecordChannelId = 'channel-1';
  mocks.callRecordQuerySuccess = true;
  mocks.getAgentPermissions.mockResolvedValue(
    ok({
      id: 'session-permissions',
      owner: ME,
      channelSharePermissions: [],
    })
  );
  mocks.updateAgentPermissions.mockResolvedValue(ok({}));
  mocks.getInitiativePermissions.mockResolvedValue(
    ok({ id: 'project-permissions', owner: ME })
  );
  mocks.updateInitiativePermissions.mockResolvedValue(ok({}));
  mocks.getDocumentPermissions.mockResolvedValue(
    ok({ id: 'document-permissions', owner: ME, channelSharePermissions: [] })
  );
  mocks.updateChatPermissions.mockResolvedValue({ isErr: () => false });
  mocks.updateCallTeamShare.mockResolvedValue({ isErr: () => false });
  mocks.editProject.mockResolvedValue({ isErr: () => false });
  mocks.editDocument.mockResolvedValue({ isErr: () => false });
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: mocks.copyLink },
  });
  mocks.sendToChannel.mockResolvedValue({
    channelId: 'channel-1',
    navigateToChannel: vi.fn(),
  });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});
function mountShare(isOwner: boolean, sharePermissions?: SharePermissions) {
  const onOpenChange = vi.fn();
  const onCopyLink = mocks.copyLink;
  render(() => (
    <ShareModal
      id="persisted-session"
      name="Fix the menu"
      owner={isOwner ? ME : SOMEONE_ELSE}
      itemType="agent_session"
      blockAlias="agent"
      userPermissions={Permissions.OWNER}
      sharePermissions={sharePermissions}
      open
      onOpenChange={onOpenChange}
    />
  ));
  return { onOpenChange, onCopyLink };
}
const selectChannel = () =>
  fireEvent.click(screen.getByRole('button', { name: 'Select channel' }));
const share = () =>
  fireEvent.click(screen.getByRole('button', { name: 'Share' }));

describe('agent session sharing', () => {
  it.each([false, true])(
    'offers Edit for forwarding, people, and links without block context (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      mocks.inBlock = false;
      mountShare(true, {
        id: 'session-permissions',
        owner: ME,
        linkShare: 'PUBLIC',
        linkShareAccessLevel: 'view',
        channelSharePermissions: [
          { channel_id: 'shared-channel', access_level: 'view' },
        ],
      });
      const editOptions = () =>
        screen.getAllByRole('button', { name: 'Set option edit' });
      await vi.waitFor(() =>
        expect(editOptions()).toHaveLength(mobile ? 1 : 3)
      );

      fireEvent.click(editOptions()[0]);
      selectChannel();
      share();
      await vi.waitFor(() =>
        expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
          'persisted-session',
          {
            channelSharePermissions: [
              {
                operation: 'replace',
                accessLevel: 'edit',
                channelId: 'channel-1',
              },
            ],
          }
        )
      );

      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'People' }));
      fireEvent.click(editOptions()[mobile ? 0 : 1]);
      await vi.waitFor(() =>
        expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
          'persisted-session',
          {
            channelSharePermissions: [
              {
                operation: 'replace',
                accessLevel: 'edit',
                channelId: 'shared-channel',
              },
            ],
          }
        )
      );

      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));
      fireEvent.click(editOptions()[mobile ? 0 : 2]);
      await vi.waitFor(() =>
        expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
          'persisted-session',
          { linkShare: 'PUBLIC', linkShareAccessLevel: 'edit' }
        )
      );
    }
  );

  it.each([false, true])(
    'updates public links and team access through session permissions (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      mocks.hasTeam = true;
      mountShare(true, {
        id: 'session-permissions',
        owner: ME,
        channelSharePermissions: [],
      });
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));

      fireEvent.click(screen.getByRole('button', { name: 'Set link PUBLIC' }));
      await vi.waitFor(() =>
        expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
          'persisted-session',
          { linkShare: 'PUBLIC', linkShareAccessLevel: 'view' }
        )
      );
      fireEvent.click(screen.getByRole('button', { name: 'Set link NONE' }));
      await vi.waitFor(() =>
        expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
          'persisted-session',
          { linkShare: null, linkShareAccessLevel: null }
        )
      );
      fireEvent.click(
        screen.getByRole('button', { name: 'Set Team access level edit' })
      );
      await vi.waitFor(() =>
        expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
          'persisted-session',
          { teamShareAccessLevel: 'edit' }
        )
      );
      expect(mocks.editDocument).not.toHaveBeenCalled();
      expect(mocks.updateChatPermissions).not.toHaveBeenCalled();
    }
  );

  it('lists the owner in the mobile People tab', () => {
    mocks.mobile = true;
    mountShare(true);
    fireEvent.click(screen.getByRole('tab', { name: 'People' }));
    expect(screen.getByText('Me')).toBeTruthy();
    expect(screen.getByText('Owner')).toBeTruthy();
  });

  it.each([false, true])(
    'shows the standard share form for owners (mobile: %s)',
    (mobile) => {
      mocks.mobile = mobile;
      mountShare(true);
      expect(
        screen.getByRole('button', { name: 'Select channel' })
      ).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Share' })).toBeTruthy();
      if (mobile) {
        expect(screen.getByRole('tab', { name: 'People' })).toBeTruthy();
        expect(screen.getByRole('tab', { name: 'Link' })).toBeTruthy();
      } else {
        expect(
          screen.getByText('People with access to this agent session')
        ).toBeTruthy();
        expect(
          screen.getByRole('group', { name: 'Link sharing scope' })
        ).toBeTruthy();
      }
      expect(
        screen.queryByText(
          'Recipients can view and control this agent session.'
        )
      ).toBeNull();
    }
  );

  it('uses explicit identity outside a block', () => {
    mocks.inBlock = false;
    render(() => (
      <ShareTrigger onClick={vi.fn()} id="task-1" blockType="task" />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Copy Share Link' }));
    expect(mocks.copyLink).toHaveBeenCalledWith(
      'https://macro.com/app/task/task-1'
    );
  });

  it('opens sharing through the provided handler', () => {
    mocks.inBlock = false;
    const onClick = vi.fn();
    render(() => (
      <ShareTrigger onClick={onClick} id="task-1" blockType="task" />
    ));

    fireEvent.click(screen.getByRole('button', { name: 'Share' }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('uses a host view contextual link when provided', () => {
    mocks.inBlock = false;
    const copyContextLink = vi.fn();
    render(() => (
      <ShareTrigger
        onClick={vi.fn()}
        id="task-1"
        blockType="task"
        copyLink={copyContextLink}
      />
    ));

    fireEvent.click(screen.getByRole('button', { name: 'Copy Share Link' }));
    expect(copyContextLink).toHaveBeenCalledOnce();
    expect(mocks.copyLink).not.toHaveBeenCalled();
  });

  it('uses the host view contextual link inside the share modal', () => {
    const copyContextLink = vi.fn();
    render(() => (
      <ShareModal
        id="persisted-session"
        name="Fix the menu"
        owner={SOMEONE_ELSE}
        itemType="agent_session"
        blockAlias="agent"
        userPermissions={Permissions.CAN_VIEW}
        open
        onOpenChange={vi.fn()}
        copyLink={copyContextLink}
      />
    ));

    fireEvent.click(screen.getByRole('button', { name: 'Copy Link' }));
    expect(copyContextLink).toHaveBeenCalledOnce();
    expect(mocks.copyLink).not.toHaveBeenCalled();
  });

  it('copies the saved session link from the shared header trigger', () => {
    const [id, setId] = createSignal('saved-session');
    render(() => (
      <ShareTrigger onClick={vi.fn()} id={id()} blockType="agent" />
    ));
    setId('current-session');
    fireEvent.click(screen.getByRole('button', { name: 'Copy Share Link' }));
    expect(mocks.copyLink).toHaveBeenCalledWith(
      'https://macro.com/app/agent/current-session'
    );
  });
  it('shares the persisted session instead of its enclosing launcher identity', async () => {
    const { onOpenChange } = mountShare(true);
    expect(
      screen.getByText('People with access to this agent session')
    ).toBeTruthy();
    expect(mocks.sharePermissionsRead).not.toHaveBeenCalled();
    expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
    expect(mocks.getChatPermissions).not.toHaveBeenCalled();
    expect(mocks.getProjectPermissions).not.toHaveBeenCalled();
    expect(mocks.getAgentPermissions).toHaveBeenCalledWith('persisted-session');
    selectChannel();
    share();
    expect(mocks.sendToChannel).toHaveBeenCalledWith({
      attachments: [
        { entity_type: 'agent_session', entity_id: 'persisted-session' },
      ],
      content: '',
      channelId: 'channel-1',
      mentions: [],
    });
    await vi.waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(mocks.updateAgentPermissions).toHaveBeenCalledWith(
      'persisted-session',
      {
        channelSharePermissions: [
          { operation: 'replace', accessLevel: 'view', channelId: 'channel-1' },
        ],
      }
    );
  });
  it.each([false, true])(
    'lets participants copy a link without exposing a grant action (mobile: %s)',
    (mobile) => {
      mocks.mobile = mobile;
      const { onCopyLink } = mountShare(false);
      expect(
        screen.queryByRole('button', { name: 'Select channel' })
      ).toBeNull();
      expect(screen.queryByRole('button', { name: 'Share' })).toBeNull();
      expect(
        screen.queryByRole('group', { name: 'Link sharing scope' })
      ).toBeNull();
      expect(screen.queryByRole('tab', { name: 'Link' })).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: 'Copy Link' }));
      expect(onCopyLink).toHaveBeenCalledWith(
        'https://macro.com/app/agent/persisted-session'
      );
      expect(mocks.sendToChannel).not.toHaveBeenCalled();
      expect(mocks.updateAgentPermissions).not.toHaveBeenCalled();
    }
  );
  it('cancels an owner draft without sharing', () => {
    const { onOpenChange } = mountShare(true);
    selectChannel();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(mocks.sendToChannel).not.toHaveBeenCalled();
  });
  it('provides a working Share action on mobile', () => {
    mocks.mobile = true;
    mountShare(true);
    const button = screen.getByRole('button', { name: 'Share' });
    expect(button.hasAttribute('disabled')).toBe(true);
    selectChannel();
    expect(button.hasAttribute('disabled')).toBe(false);
    share();
    expect(mocks.sendToChannel).toHaveBeenCalledOnce();
  });
  it('forwards the explicit identity without an enclosing block', () => {
    render(() => (
      <ForwardToChannel
        name="Document"
        blockId="document-1"
        blockName="md"
        hideAccessLevelSelector
      />
    ));
    selectChannel();
    share();
    expect(mocks.sendToChannel).toHaveBeenCalledWith(
      expect.objectContaining({
        attachments: [{ entity_type: 'document', entity_id: 'document-1' }],
      })
    );
  });
  it('uses the current explicit identity if it changes while mounted', () => {
    const [id, setId] = createSignal('old-session');
    render(() => (
      <ForwardToChannel
        name="Session"
        blockName="agent"
        blockId={id()}
        hideAccessLevelSelector
      />
    ));
    setId('new-session');
    selectChannel();
    share();
    expect(mocks.sendToChannel).toHaveBeenCalledWith(
      expect.objectContaining({
        attachments: [
          { entity_type: 'agent_session', entity_id: 'new-session' },
        ],
      })
    );
  });
});

describe('share edit availability', () => {
  it.each([
    { edit: undefined, expected: true },
    { edit: true, expected: true },
    { edit: false, expected: false },
  ])('honors explicit grant options: %j', ({ edit, expected }) => {
    render(() => (
      <ShareOptions permissionOptions={{ edit }} setPermissions={vi.fn()} />
    ));
    expect(
      screen.queryByRole('button', { name: 'Set option edit' }) !== null
    ).toBe(expected);
  });
});

function mountChatShare() {
  mocks.sharePermissionsRead.mockReturnValue({
    id: 'perm-1',
    owner: ME,
    linkShare: null,
    linkShareAccessLevel: null,
    teamShareAccessLevel: 'view',
    channelSharePermissions: [],
  });
  render(() => (
    <ShareModal
      sharePermissions={mocks.sharePermissionsRead()}
      id="chat-1"
      name="Planning chat"
      owner={ME}
      itemType="chat"
      blockAlias="chat"
      userPermissions={Permissions.OWNER}
      open
      onOpenChange={vi.fn()}
    />
  ));
}

function mountCallShare() {
  mocks.sharePermissionsRead.mockReturnValue({
    id: 'perm-call',
    owner: ME,
    linkShare: null,
    linkShareAccessLevel: null,
    teamShareAccessLevel: 'view',
    channelSharePermissions: [],
  });
  render(() => (
    <ShareModal
      sharePermissions={mocks.sharePermissionsRead()}
      id="call-1"
      name="Weekly sync"
      owner={ME}
      itemType="call"
      blockAlias="call"
      userPermissions={Permissions.OWNER}
      open
      onOpenChange={vi.fn()}
    />
  ));
}

describe('call team sharing', () => {
  it('hides team access for a standalone call even when stale permissions claim it is shared', () => {
    mocks.hasTeam = true;
    mocks.callRecordChannelId = null;
    mountCallShare();

    expect(screen.queryByText('Team access')).toBeNull();
    expect(
      screen.queryByRole('group', { name: 'Team access level' })
    ).toBeNull();
    expect(screen.getByRole('button', { name: 'Share' })).toBeTruthy();
    expect(mocks.updateCallTeamShare).not.toHaveBeenCalled();
  });

  it('does not offer team access until the call channel is known', () => {
    mocks.hasTeam = true;
    mocks.callRecordQuerySuccess = false;
    mountCallShare();
    expect(screen.queryByText('Team access')).toBeNull();
  });

  it('lets the owner share the call with their team at view', async () => {
    mocks.hasTeam = true;
    mountCallShare();

    expect(screen.getByText('Team access')).toBeTruthy();
    expect(
      screen.getByText("Share this call directly with the owner's team.")
    ).toBeTruthy();
    expect(
      screen
        .getByRole('group', { name: 'Team access level' })
        .getAttribute('data-value')
    ).toBe('view');

    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level view' })
    );

    await vi.waitFor(() =>
      expect(mocks.updateCallTeamShare).toHaveBeenCalledWith('call-1', true)
    );
    expect(mocks.setCallRecordTeamShareCache).toHaveBeenCalledWith(
      'call-1',
      true
    );
    expect(mocks.updateChatPermissions).not.toHaveBeenCalled();
    expect(mocks.editDocument).not.toHaveBeenCalled();
    expect(mocks.editProject).not.toHaveBeenCalled();
  });

  it('clears call team access with an explicit null', async () => {
    mocks.hasTeam = true;
    mountCallShare();

    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level NONE' })
    );

    await vi.waitFor(() =>
      expect(mocks.updateCallTeamShare).toHaveBeenCalledWith('call-1', false)
    );
    expect(mocks.setCallRecordTeamShareCache).toHaveBeenCalledWith(
      'call-1',
      false
    );
  });

  it('shows team access from the call record cache after the checkbox writes it', () => {
    mocks.hasTeam = true;
    mocks.callRecordShared = false;
    mountCallShare();

    expect(
      screen
        .getByRole('group', { name: 'Team access level' })
        .getAttribute('data-value')
    ).toBe('NONE');
  });

  it('hides call team access when the owner has no team', () => {
    mocks.hasTeam = false;
    mountCallShare();

    expect(screen.queryByText('Team access')).toBeNull();
    expect(
      screen.queryByRole('group', { name: 'Team access level' })
    ).toBeNull();
    expect(mocks.updateCallTeamShare).not.toHaveBeenCalled();
  });

  it('loads team access from the call record outside a block', () => {
    mocks.inBlock = false;
    mocks.hasTeam = true;
    mountCallShare();

    expect(
      screen
        .getByRole('group', { name: 'Team access level' })
        .getAttribute('data-value')
    ).toBe('view');
    expect(mocks.fetchCallSharePermission).not.toHaveBeenCalled();
  });
});

describe('chat team sharing', () => {
  it('lets the owner share the chat with their team through the chat permissions endpoint', async () => {
    mocks.hasTeam = true;
    mountChatShare();

    expect(screen.getByText('Team access')).toBeTruthy();
    expect(
      screen.getByText("Share this chat directly with the owner's team.")
    ).toBeTruthy();
    expect(
      screen.getByRole('group', { name: 'Link sharing scope' })
    ).toBeTruthy();
    expect(
      screen
        .getByRole('group', { name: 'Team access level' })
        .getAttribute('data-value')
    ).toBe('view');

    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level edit' })
    );

    await vi.waitFor(() =>
      expect(mocks.updateChatPermissions).toHaveBeenCalledWith({
        chat_id: 'chat-1',
        sharePermission: { teamShareAccessLevel: 'edit' },
      })
    );
    expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
  });

  it('clears team access with an explicit null', async () => {
    mocks.hasTeam = true;
    mountChatShare();

    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level NONE' })
    );

    await vi.waitFor(() =>
      expect(mocks.updateChatPermissions).toHaveBeenCalledWith({
        chat_id: 'chat-1',
        sharePermission: { teamShareAccessLevel: null },
      })
    );
  });

  it('hides team access when the owner has no team', () => {
    mocks.hasTeam = false;
    mountChatShare();

    expect(screen.queryByText('Team access')).toBeNull();
    expect(
      screen.queryByRole('group', { name: 'Team access level' })
    ).toBeNull();
    expect(mocks.updateChatPermissions).not.toHaveBeenCalled();
  });
});

function mountProjectShare() {
  mocks.sharePermissionsRead.mockReturnValue({
    id: 'perm-project',
    owner: ME,
    linkShare: null,
    linkShareAccessLevel: null,
    teamShareAccessLevel: 'view',
    channelSharePermissions: [],
  });
  render(() => (
    <ShareModal
      sharePermissions={mocks.sharePermissionsRead()}
      id="project-1"
      name="Launch folder"
      owner={ME}
      itemType="project"
      blockAlias="project"
      userPermissions={Permissions.OWNER}
      open
      onOpenChange={vi.fn()}
    />
  ));
}

describe('project team sharing', () => {
  it('lets the owner share the folder with their team without a link sharing card', async () => {
    mocks.hasTeam = true;
    mountProjectShare();

    expect(screen.getByText('Team access')).toBeTruthy();
    expect(
      screen.getByText("Share this folder directly with the owner's team.")
    ).toBeTruthy();
    expect(
      screen.queryByRole('group', { name: 'Link sharing scope' })
    ).toBeNull();
    expect(screen.queryByText('Link sharing off')).toBeNull();
    expect(
      screen
        .getByRole('group', { name: 'Team access level' })
        .getAttribute('data-value')
    ).toBe('view');

    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level edit' })
    );

    await vi.waitFor(() =>
      expect(mocks.editProject).toHaveBeenCalledWith({
        id: 'project-1',
        sharePermission: { teamShareAccessLevel: 'edit' },
      })
    );
    expect(mocks.updateChatPermissions).not.toHaveBeenCalled();
    expect(mocks.editDocument).not.toHaveBeenCalled();
  });

  it('clears folder team access with an explicit null', async () => {
    mocks.hasTeam = true;
    mountProjectShare();

    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level NONE' })
    );

    await vi.waitFor(() =>
      expect(mocks.editProject).toHaveBeenCalledWith({
        id: 'project-1',
        sharePermission: { teamShareAccessLevel: null },
      })
    );
  });

  it('hides folder team access when the owner has no team', () => {
    mocks.hasTeam = false;
    mountProjectShare();

    expect(screen.queryByText('Team access')).toBeNull();
    expect(
      screen.queryByRole('group', { name: 'Team access level' })
    ).toBeNull();
    expect(
      screen.queryByRole('group', { name: 'Link sharing scope' })
    ).toBeNull();
    expect(mocks.editProject).not.toHaveBeenCalled();
  });

  it('puts folder team access on a Team tab and omits the Link tab', () => {
    mocks.mobile = true;
    mocks.hasTeam = true;
    mountProjectShare();

    expect(screen.queryByRole('tab', { name: 'Link' })).toBeNull();
    expect(screen.queryByText('Team access')).toBeNull();

    fireEvent.click(screen.getByRole('tab', { name: 'Team' }));

    expect(screen.getByText('Team access')).toBeTruthy();
    expect(
      screen.getByText("Share this folder directly with the owner's team.")
    ).toBeTruthy();
    expect(
      screen.queryByRole('group', { name: 'Link sharing scope' })
    ).toBeNull();
  });
});

describe('native project sharing', () => {
  function mountProject(
    options: {
      owner?: string;
      people?: () => JSX.Element;
      hasDirectShares?: boolean;
      sharePermissions?: SharePermissions;
    } = {}
  ) {
    mocks.inBlock = false;
    const owner = options.owner ?? ME;
    render(() => (
      <ShareModal
        id="initiative-1"
        name="Launch"
        owner={owner}
        itemType="initiative"
        blockAlias="initiative"
        userPermissions={
          owner === ME ? Permissions.OWNER : Permissions.CAN_VIEW
        }
        people={options.people}
        hasDirectShares={options.hasDirectShares}
        sharePermissions={options.sharePermissions}
        open
        onOpenChange={vi.fn()}
      />
    ));
  }

  it('grants the destination before forwarding the project itself', async () => {
    const order: string[] = [];
    mocks.updateInitiativePermissions.mockImplementation(async () => {
      order.push('grant');
      return ok({});
    });
    mocks.sendToChannel.mockImplementation(async (input) => {
      await input.beforeSend?.(input.channelId);
      order.push('message');
      return { channelId: input.channelId, navigateToChannel: vi.fn() };
    });
    mountProject();
    selectChannel();
    share();
    await vi.waitFor(() => expect(order).toEqual(['grant', 'message']));
    expect(mocks.sendToChannel.mock.calls[0][0].attachments).toEqual([
      { entity_type: 'initiative', entity_id: 'initiative-1' },
    ]);
    expect(mocks.updateInitiativePermissions).toHaveBeenCalledOnce();
    expect(mocks.updateInitiativePermissions).toHaveBeenCalledWith(
      'initiative-1',
      {
        channelSharePermissions: [
          { operation: 'replace', accessLevel: 'view', channelId: 'channel-1' },
        ],
      }
    );
    expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
    expect(mocks.sharePermissionsRead).not.toHaveBeenCalled();
  });

  it('does not post the project when its grant fails', async () => {
    mocks.updateInitiativePermissions.mockResolvedValue(
      err([{ code: 'FORBIDDEN', message: 'Not the owner' }])
    );
    const posted = vi.fn();
    mocks.sendToChannel.mockImplementation(async (input) => {
      await input.beforeSend?.(input.channelId);
      posted();
      return { channelId: input.channelId, navigateToChannel: vi.fn() };
    });
    mountProject();
    selectChannel();
    share();
    const { toast } = await import('@core/component/Toast/Toast');
    await vi.waitFor(() =>
      expect(toast.failure).toHaveBeenCalledWith('Message failed to send')
    );
    expect(posted).not.toHaveBeenCalled();
  });

  it('lets only the owner forward a project', () => {
    mountProject({ owner: SOMEONE_ELSE });
    expect(
      screen.getByText(
        'Only the owner can share access to this project. You can copy a link for people who already have access.'
      )
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Select channel' })).toBeNull();
  });

  it('changes team, link and channel grants through the project', async () => {
    mocks.hasTeam = true;
    mountProject({
      sharePermissions: {
        id: 'project-permissions',
        owner: ME,
        teamShareAccessLevel: 'view',
        channelSharePermissions: [
          { channel_id: 'channel-1', access_level: 'edit' },
        ],
      },
    });
    // The consumer supplies the project's loaded grants.
    await screen.findByRole('button', { name: 'Set option none' });
    fireEvent.click(
      screen.getByRole('button', { name: 'Set Team access level edit' })
    );
    await vi.waitFor(() =>
      expect(mocks.updateInitiativePermissions).toHaveBeenCalledWith(
        'initiative-1',
        { teamShareAccessLevel: 'edit' }
      )
    );
    fireEvent.click(screen.getByRole('button', { name: 'Set link PUBLIC' }));
    await vi.waitFor(() =>
      expect(mocks.updateInitiativePermissions).toHaveBeenCalledWith(
        'initiative-1',
        { linkShare: 'PUBLIC', linkShareAccessLevel: 'view' }
      )
    );
    fireEvent.click(screen.getByRole('button', { name: 'Set option none' }));
    await vi.waitFor(() =>
      expect(mocks.updateInitiativePermissions).toHaveBeenCalledWith(
        'initiative-1',
        {
          channelSharePermissions: [
            { operation: 'remove', channelId: 'channel-1' },
          ],
        }
      )
    );
    expect(mocks.editDocument).not.toHaveBeenCalled();
    expect(mocks.editProject).not.toHaveBeenCalled();
  });

  it.each([false, true])(
    'lists direct collaborators among the people with access (mobile: %s)',
    (mobile) => {
      mocks.mobile = mobile;
      mountProject({
        people: () => <div>Collaborator row</div>,
        hasDirectShares: true,
      });
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'People' }));
      else
        expect(
          screen.getByText('People with access to this project')
        ).toBeTruthy();
      expect(screen.getByText('Collaborator row')).toBeTruthy();
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));
      expect(screen.getByText('Shared')).toBeTruthy();
    }
  );

  it('copies the project route instead of a block URL', () => {
    mocks.inBlock = false;
    render(() => (
      <ShareTrigger
        onClick={vi.fn()}
        id="initiative-1"
        blockType="initiative"
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Copy Share Link' }));
    expect(mocks.copyLink).toHaveBeenCalledWith(
      'https://macro.com/app/component/initiative-view~initiative-1~overview'
    );
  });
});

it('database sharing uses its supplied permissions without another fetch and disables public links', () => {
  mocks.sharePermissionsRead.mockReturnValue({
    id: 'database-id',
    owner: 'owner',
    channelSharePermissions: [],
  });
  render(() => (
    <ShareModal
      sharePermissions={mocks.sharePermissionsRead()}
      id="database-id"
      itemType="database"
      blockAlias="database"
      owner="owner"
      name="Ideas"
      userPermissions={Permissions.OWNER}
      open
      onOpenChange={() => {}}
    />
  ));
  expect(mocks.sharePermissionsRead).toHaveBeenCalled();
  expect(mocks.getDatabasePermissions).not.toHaveBeenCalled();
  expect(screen.queryByText('Anyone with the link')).toBeNull();
});

describe('explicit sharing ownership', () => {
  it.each([false, true])(
    'waits for fallback grants before showing or changing link access (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      let resolvePermissions!: (value: SharePermissions) => void;
      const permissions = new Promise<SharePermissions>((resolve) => {
        resolvePermissions = resolve;
      });
      mocks.getDocumentPermissions.mockImplementation(async () =>
        ok(await permissions)
      );
      render(() => (
        <ShareModal
          id="pending-document"
          blockAlias="md"
          itemType="document"
          name="Document"
          owner={ME}
          userPermissions={Permissions.OWNER}
          sharePermissions={undefined}
          open
          onOpenChange={vi.fn()}
        />
      ));
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));
      const publicButton = () =>
        screen.getByRole('button', { name: 'Set link PUBLIC' });
      expect(publicButton().hasAttribute('disabled')).toBe(true);
      expect(screen.getByText('Loading access')).toBeTruthy();
      expect(screen.queryByText('Just me')).toBeNull();
      fireEvent.click(publicButton());
      expect(mocks.editDocument).not.toHaveBeenCalled();

      resolvePermissions({
        id: 'grants',
        owner: ME,
        linkShare: 'PUBLIC',
        linkShareAccessLevel: 'edit',
      });
      await vi.waitFor(() =>
        expect(publicButton().hasAttribute('disabled')).toBe(false)
      );
      expect(screen.getByText('Public')).toBeTruthy();
      expect(screen.queryByText('Loading access')).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: 'Set link NONE' }));
      await vi.waitFor(() =>
        expect(mocks.editDocument).toHaveBeenCalledWith({
          documentId: 'pending-document',
          sharePermission: { linkShare: null, linkShareAccessLevel: null },
        })
      );
    }
  );

  it.each([false, true])(
    'uses cached fallback grants when the override is undefined (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      mocks.hasTeam = true;
      queryClient.setQueryData(
        sharingKeys.permissions('document', 'external-document').queryKey,
        { id: 'cached-grants', owner: ME, linkShare: 'PUBLIC' }
      );
      render(() => (
        <ShareModal
          id="external-document"
          blockAlias="md"
          itemType="document"
          name="Document"
          owner={ME}
          userPermissions={Permissions.OWNER}
          sharePermissions={undefined}
          open
          onOpenChange={vi.fn()}
        />
      ));
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));
      const button = screen.getByRole('button', { name: 'Set link PUBLIC' });
      expect(button.hasAttribute('disabled')).toBe(false);
      expect(screen.queryByText('Access unavailable')).toBeNull();
      expect(screen.queryByText('Just me')).toBeNull();
      expect(screen.getByText('Public')).toBeTruthy();
      expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
    }
  );

  it.each([false, true])(
    'does not present failed grant reads as private access (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      mocks.getDocumentPermissions.mockResolvedValue(
        err([{ code: 'FORBIDDEN', message: 'Access denied' }])
      );
      render(() => (
        <ShareModal
          id="denied-document"
          blockAlias="md"
          itemType="document"
          name="Document"
          owner={ME}
          userPermissions={Permissions.OWNER}
          open
          onOpenChange={vi.fn()}
        />
      ));
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));
      await vi.waitFor(() =>
        expect(screen.getByText('Access unavailable')).toBeTruthy()
      );
      expect(screen.queryByText('Just me')).toBeNull();
      const button = screen.getByRole('button', { name: 'Set link PUBLIC' });
      expect(button.hasAttribute('disabled')).toBe(true);
      fireEvent.click(button);
      expect(mocks.editDocument).not.toHaveBeenCalled();
    }
  );
  it('shares the UI-owned query between the trigger and modal without caller fetching', async () => {
    const [open, setOpen] = createSignal(false);
    render(() => (
      <>
        <ShareTrigger
          id="ui-owned-document"
          blockType="md"
          onClick={() => setOpen(true)}
        />
        <Show when={open()}>
          <ShareModal
            id="ui-owned-document"
            blockAlias="md"
            itemType="document"
            name="Document"
            owner={ME}
            userPermissions={Permissions.OWNER}
            open
            onOpenChange={setOpen}
          />
        </Show>
      </>
    ));
    await vi.waitFor(() =>
      expect(
        queryClient.getQueryState(
          sharingKeys.permissions('document', 'ui-owned-document').queryKey
        )?.status
      ).toBe('success')
    );
    setOpen(true);
    setOpen(false);
    setOpen(true);
    await Promise.resolve();
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
  });

  it('fetches trigger grants when the override is undefined', async () => {
    render(() => (
      <ShareTrigger
        id="externally-owned-document"
        blockType="md"
        sharePermissions={undefined}
        onClick={vi.fn()}
      />
    ));
    await vi.waitFor(() =>
      expect(mocks.getDocumentPermissions).toHaveBeenCalledWith({
        document_id: 'externally-owned-document',
      })
    );
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
  });

  it('registers sharing shortcuts on the nearest host scope without scope props', () => {
    const view = render(() => (
      <HotkeyScope scope="split-scope">
        <HotkeyScope scope="preview-scope">
          <ShareTrigger id="document" blockType="md" onClick={vi.fn()} />
        </HotkeyScope>
      </HotkeyScope>
    ));
    expect(mocks.registerHotkey).toHaveBeenCalledOnce();
    expect(mocks.registerHotkey).toHaveBeenCalledWith(
      expect.objectContaining({ scopeId: 'preview-scope' })
    );
    const registration = mocks.registerHotkey.mock.results[0].value;
    view.unmount();
    expect(registration.dispose).toHaveBeenCalledOnce();
  });

  it('uses an explicit shortcut scope instead of the inherited host scope', () => {
    render(() => (
      <HotkeyScope scope="split-scope">
        <ShareTrigger
          id="document"
          blockType="md"
          hotkeyScope="explicit-scope"
          onClick={vi.fn()}
        />
      </HotkeyScope>
    ));
    expect(mocks.registerHotkey).toHaveBeenCalledWith(
      expect.objectContaining({ scopeId: 'explicit-scope' })
    );
  });
  it('registers delayed and replaced hotkey scopes and disposes with the trigger', () => {
    const [scope, setScope] = createSignal<string>();
    const [id, setId] = createSignal('first-document');
    const view = render(() => (
      <ShareTrigger
        id={id()}
        blockType="md"
        hotkeyScope={scope()}
        sharePermissions={undefined}
        onClick={vi.fn()}
      />
    ));
    expect(mocks.registerHotkey).not.toHaveBeenCalled();
    setScope('first-scope');
    expect(mocks.registerHotkey).toHaveBeenCalledTimes(1);
    const first = mocks.registerHotkey.mock.results[0].value;
    setId('second-document');
    expect(mocks.registerHotkey).toHaveBeenCalledTimes(1);
    setScope('second-scope');
    expect(first.dispose).toHaveBeenCalledOnce();
    expect(mocks.registerHotkey).toHaveBeenCalledTimes(2);
    expect(mocks.registerHotkey).toHaveBeenLastCalledWith(
      expect.objectContaining({ scopeId: 'second-scope' })
    );
    const second = mocks.registerHotkey.mock.results[1].value;
    view.unmount();
    expect(second.dispose).toHaveBeenCalledOnce();
  });
  it('shares one host fetch across its trigger and repeated modal mounts', async () => {
    mocks.getDocumentPermissions.mockResolvedValue(
      ok({ id: 'grants', owner: ME, channelSharePermissions: [] })
    );
    const [open, setOpen] = createSignal(false);
    const host = render(() => {
      const permissionsQuery = useSharePermissionsQuery(
        () => ({
          id: 'document-1',
          itemType: 'document',
        }),
        { enabled: () => true }
      );
      return (
        <>
          <ShareTrigger
            id="document-1"
            blockType="md"
            sharePermissions={
              permissionsQuery.isSuccess ? permissionsQuery.data : undefined
            }
            onClick={() => setOpen(true)}
          />
          <Show when={open()}>
            <ShareModal
              id="document-1"
              blockAlias="md"
              itemType="document"
              name="Document"
              owner={ME}
              userPermissions={Permissions.OWNER}
              sharePermissions={
                permissionsQuery.isSuccess ? permissionsQuery.data : undefined
              }
              open
              onOpenChange={setOpen}
            />
          </Show>
        </>
      );
    });
    await vi.waitFor(() =>
      expect(
        queryClient.getQueryState(
          sharingKeys.permissions('document', 'document-1').queryKey
        )?.status
      ).toBe('success')
    );
    setOpen(true);
    setOpen(false);
    setOpen(true);
    await Promise.resolve();
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
    await invalidateSharePermissions({
      id: 'document-1',
      itemType: 'document',
    });
    await vi.waitFor(() =>
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2)
    );
    host.unmount();
    await invalidateSharePermissions({
      id: 'document-1',
      itemType: 'document',
    });
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
  });

  it('prefers supplied grants over cached grants and follows override updates', async () => {
    queryClient.setQueryData(
      sharingKeys.permissions('document', 'host-owned-document').queryKey,
      {
        id: 'cached-grants',
        owner: ME,
        channelSharePermissions: [
          { channel_id: 'cached-channel', access_level: 'view' },
        ],
      }
    );
    const [grants, setGrants] = createSignal<SharePermissions>({
      id: 'initial-host-grants',
      owner: ME,
      channelSharePermissions: [],
    });
    render(() => (
      <ShareModal
        id="host-owned-document"
        blockAlias="md"
        itemType="document"
        name="Document"
        owner={ME}
        userPermissions={Permissions.OWNER}
        sharePermissions={grants()}
        open
        onOpenChange={vi.fn()}
      />
    ));
    expect(
      screen.queryByRole('button', { name: 'Set option none' })
    ).toBeNull();
    expect(screen.getByRole('button', { name: 'Select channel' })).toBeTruthy();
    await invalidateSharePermissions({
      id: 'host-owned-document',
      itemType: 'document',
    });
    expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
    setGrants({
      id: 'host-grants',
      owner: ME,
      channelSharePermissions: [
        { channel_id: 'host-channel', access_level: 'view' },
      ],
    });
    expect(
      screen.getByRole('button', { name: 'Set option none' })
    ).toBeTruthy();
    expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
  });

  it('starts fallback fetching when the host clears its grants', async () => {
    const [grants, setGrants] = createSignal<SharePermissions | undefined>({
      id: 'host-grants',
      owner: ME,
      linkShare: 'PUBLIC',
    });
    render(() => (
      <ShareModal
        id="cleared-host-document"
        blockAlias="md"
        itemType="document"
        name="Document"
        owner={ME}
        userPermissions={Permissions.OWNER}
        sharePermissions={grants()}
        open
        onOpenChange={vi.fn()}
      />
    ));
    expect(screen.getByText('Public')).toBeTruthy();
    expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
    setGrants(undefined);
    await vi.waitFor(() =>
      expect(mocks.getDocumentPermissions).toHaveBeenCalledWith({
        document_id: 'cleared-host-document',
      })
    );
    await vi.waitFor(() => expect(screen.getByText('Just me')).toBeTruthy());
    expect(screen.queryByText('Public')).toBeNull();
  });

  it('does not fetch again while a supplied host query is pending', async () => {
    const grants = ok({ id: 'grants', owner: ME, channelSharePermissions: [] });
    let resolve!: (value: typeof grants) => void;
    mocks.getDocumentPermissions.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      })
    );
    render(() => {
      const permissionsQuery = useSharePermissionsQuery(
        () => ({
          id: 'pending-document',
          itemType: 'document',
        }),
        { enabled: () => true }
      );
      return (
        <ShareModal
          id="pending-document"
          blockAlias="md"
          itemType="document"
          name="Document"
          owner={ME}
          userPermissions={Permissions.OWNER}
          sharePermissions={
            permissionsQuery.isSuccess ? permissionsQuery.data : undefined
          }
          open
          onOpenChange={vi.fn()}
        />
      );
    });
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
    resolve(grants);
    await Promise.resolve();
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
  });

  it('fetches undefined non-owner grants across identity changes', async () => {
    const [id, setId] = createSignal('host-non-owner-document');
    render(() => (
      <ShareModal
        id={id()}
        blockAlias="md"
        itemType="document"
        name="Document"
        owner={SOMEONE_ELSE}
        userPermissions={Permissions.CAN_VIEW}
        sharePermissions={undefined}
        open
        onOpenChange={vi.fn()}
      />
    ));
    await vi.waitFor(() =>
      expect(mocks.getDocumentPermissions).toHaveBeenCalledWith({
        document_id: 'host-non-owner-document',
      })
    );
    setId('second-host-non-owner-document');
    await vi.waitFor(() =>
      expect(mocks.getDocumentPermissions).toHaveBeenCalledWith({
        document_id: 'second-host-non-owner-document',
      })
    );
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
  });

  it('fetches and invalidates a direct modal query when sharePermissions is omitted', async () => {
    mocks.getDocumentPermissions.mockResolvedValue(
      ok({ id: 'grants', owner: ME, channelSharePermissions: [] })
    );
    render(() => (
      <ShareModal
        id="direct-document"
        blockAlias="md"
        itemType="document"
        name="Document"
        owner={ME}
        userPermissions={Permissions.OWNER}
        open
        onOpenChange={vi.fn()}
      />
    ));
    await vi.waitFor(() =>
      expect(
        queryClient.getQueryState(
          sharingKeys.permissions('document', 'direct-document').queryKey
        )?.status
      ).toBe('success')
    );
    await invalidateSharePermissions({
      id: 'direct-document',
      itemType: 'document',
    });
    expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
  });

  it('fetches permissions for a direct non-owner modal', async () => {
    mocks.getDocumentPermissions.mockResolvedValue(
      ok({
        id: 'non-owner-grants',
        owner: SOMEONE_ELSE,
        channelSharePermissions: [],
      })
    );
    render(() => (
      <ShareModal
        id="non-owner-document"
        blockAlias="md"
        itemType="document"
        name="Document"
        owner={SOMEONE_ELSE}
        userPermissions={Permissions.CAN_VIEW}
        open
        onOpenChange={vi.fn()}
      />
    ));
    await vi.waitFor(() =>
      expect(mocks.getDocumentPermissions).toHaveBeenCalledWith({
        document_id: 'non-owner-document',
      })
    );
  });

  it.each([
    { id: 'call-1', itemType: 'call' },
    { id: 'email-1', itemType: 'email' },
    { id: 'channel-1', itemType: 'channel' },
    { id: '', itemType: 'agent_session' },
    { id: '', itemType: 'initiative' },
    { id: '', itemType: 'database' },
    { id: '', itemType: 'chat' },
    { id: '', itemType: 'document' },
    { id: '', itemType: 'project' },
    { id: 'trash', itemType: 'project' },
  ] satisfies SharePermissionsTarget[])(
    'disables ineligible permission targets with caller fetching enabled: %j',
    async (target) => {
      let query!: ReturnType<typeof useSharePermissionsQuery>;
      let dispose!: () => void;
      createRoot((cleanup) => {
        dispose = cleanup;
        query = useSharePermissionsQuery(() => target, {
          enabled: () => true,
        });
      });
      try {
        await invalidateSharePermissions(target);
        expect(query.isPending).toBe(true);
        expect(query.isFetching).toBe(false);
        for (const fetch of [
          mocks.getAgentPermissions,
          mocks.getInitiativePermissions,
          mocks.getDatabasePermissions,
          mocks.getChatPermissions,
          mocks.getDocumentPermissions,
          mocks.getProjectPermissions,
        ]) {
          expect(fetch).not.toHaveBeenCalled();
        }
      } finally {
        dispose();
      }
    }
  );

  it('reevaluates permission target eligibility when the ID or item type changes', async () => {
    mocks.getProjectPermissions.mockResolvedValue(
      ok({ id: 'folder-grants', owner: ME, channelSharePermissions: [] })
    );
    mocks.getDocumentPermissions.mockResolvedValue(
      ok({ id: 'document-grants', owner: ME, channelSharePermissions: [] })
    );
    const [target, setTarget] = createSignal<SharePermissionsTarget>({
      id: 'trash',
      itemType: 'project',
    });
    let query!: ReturnType<typeof useSharePermissionsQuery>;
    let dispose!: () => void;
    createRoot((cleanup) => {
      dispose = cleanup;
      query = useSharePermissionsQuery(target, { enabled: () => true });
    });
    try {
      expect(mocks.getProjectPermissions).not.toHaveBeenCalled();
      setTarget({ id: 'folder-1', itemType: 'project' });
      await vi.waitFor(() => expect(query.isSuccess).toBe(true));
      expect(mocks.getProjectPermissions).toHaveBeenCalledWith({
        id: 'folder-1',
      });
      setTarget({ id: '', itemType: 'project' });
      await invalidateSharePermissions(target());
      expect(mocks.getProjectPermissions).toHaveBeenCalledTimes(1);
      expect(query.isPending).toBe(true);
      setTarget({ id: 'trash', itemType: 'email' });
      await invalidateSharePermissions(target());
      expect(query.isPending).toBe(true);
      setTarget({ id: 'trash', itemType: 'document' });
      await vi.waitFor(() => expect(query.isSuccess).toBe(true));
      expect(mocks.getDocumentPermissions).toHaveBeenCalledWith({
        document_id: 'trash',
      });
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
    } finally {
      dispose();
    }
  });

  it('gates fetching on the supplied owner capability and hides stale grants', async () => {
    mocks.getDocumentPermissions.mockImplementation(
      async ({ document_id }: { document_id: string }) =>
        ok({
          id: document_id,
          owner: ME,
          channelSharePermissions: [],
        })
    );
    const [isOwner, setIsOwner] = createSignal(false);
    const [id, setId] = createSignal('first-document');
    let query!: ReturnType<typeof useSharePermissionsQuery>;
    let dispose!: () => void;
    createRoot((cleanup) => {
      dispose = cleanup;
      query = useSharePermissionsQuery(
        () => ({ id: id(), itemType: 'document' }),
        { enabled: isOwner }
      );
    });
    try {
      expect(mocks.getDocumentPermissions).not.toHaveBeenCalled();
      setIsOwner(true);
      await vi.waitFor(() =>
        expect(isOwner() && query.isSuccess ? query.data?.id : undefined).toBe(
          'first-document'
        )
      );
      setId('second-document');
      expect(
        isOwner() && query.isSuccess ? query.data : undefined
      ).toBeUndefined();
      await vi.waitFor(() =>
        expect(isOwner() && query.isSuccess ? query.data?.id : undefined).toBe(
          'second-document'
        )
      );
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
      setIsOwner(false);
      expect(
        isOwner() && query.isSuccess ? query.data : undefined
      ).toBeUndefined();
      await invalidateSharePermissions({
        id: 'second-document',
        itemType: 'document',
      });
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
    } finally {
      dispose();
    }
  });

  it('keeps two queries isolated and refetches only active owners', async () => {
    mocks.getDocumentPermissions.mockResolvedValue(
      ok({ id: 'grants', owner: ME, channelSharePermissions: [] })
    );
    const disposers: (() => void)[] = [];
    for (const id of ['first-document', 'second-document']) {
      createRoot((dispose) => {
        disposers.push(dispose);
        useSharePermissionsQuery(() => ({ id, itemType: 'document' }), {
          enabled: () => true,
        });
      });
    }
    try {
      await vi.waitFor(() => {
        for (const id of ['first-document', 'second-document']) {
          expect(
            queryClient.getQueryState(
              sharingKeys.permissions('document', id).queryKey
            )?.status
          ).toBe('success');
        }
      });
      disposers[0]();
      await invalidateSharePermissions({
        id: 'first-document',
        itemType: 'document',
      });
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
      await invalidateSharePermissions({
        id: 'second-document',
        itemType: 'document',
      });
      await vi.waitFor(() =>
        expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(3)
      );
      expect(mocks.getDocumentPermissions).toHaveBeenLastCalledWith({
        document_id: 'second-document',
      });
    } finally {
      for (const dispose of disposers) dispose();
    }
  });
  it('deduplicates separate owners of the same permissions query', async () => {
    mocks.getDocumentPermissions.mockResolvedValue(
      ok({ id: 'grants', owner: ME, channelSharePermissions: [] })
    );
    const disposers: (() => void)[] = [];
    const queries: ReturnType<typeof useSharePermissionsQuery>[] = [];
    for (let index = 0; index < 2; index++) {
      createRoot((dispose) => {
        disposers.push(dispose);
        queries.push(
          useSharePermissionsQuery(
            () => ({
              id: 'same-document',
              itemType: 'document',
            }),
            { enabled: () => true }
          )
        );
      });
    }
    try {
      await vi.waitFor(() => {
        expect(
          queries.every(
            (query) => query.isSuccess && query.data?.id === 'grants'
          )
        ).toBe(true);
      });
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(1);
      await invalidateSharePermissions({
        id: 'same-document',
        itemType: 'document',
      });
      expect(mocks.getDocumentPermissions).toHaveBeenCalledTimes(2);
    } finally {
      for (const dispose of disposers) dispose();
    }
  });

  it('keeps failed permission requests in query error state without exposing grants', async () => {
    mocks.getDocumentPermissions.mockResolvedValue(
      err([{ code: 'FORBIDDEN', message: 'Access denied' }])
    );
    let query!: ReturnType<typeof useSharePermissionsQuery>;
    let dispose!: () => void;
    createRoot((cleanup) => {
      dispose = cleanup;
      query = useSharePermissionsQuery(
        () => ({
          id: 'denied-document',
          itemType: 'document',
        }),
        { enabled: () => true }
      );
    });
    try {
      await vi.waitFor(() =>
        expect(
          queryClient.getQueryCache().find({
            queryKey: sharingKeys.permissions('document', 'denied-document')
              .queryKey,
          })?.state.status
        ).toBe('error')
      );
      expect(query.isSuccess).toBe(false);
      expect(mocks.getDocumentPermissions).toHaveBeenCalledOnce();
    } finally {
      dispose();
    }
  });

  it.each([false, true])(
    'applies combined grant options across sharing controls (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      render(() => (
        <ShareModal
          id="options-document"
          itemType="document"
          blockAlias="md"
          name="Document"
          owner={ME}
          userPermissions={Permissions.OWNER}
          permissionOptions={{ edit: false, comment: false }}
          sharePermissions={{
            id: 'grants',
            owner: ME,
            linkShare: 'PUBLIC',
            linkShareAccessLevel: 'view',
            channelSharePermissions: [
              { channel_id: 'shared-channel', access_level: 'view' },
            ],
          }}
          open
          onOpenChange={vi.fn()}
        />
      ));
      for (const tab of mobile ? ['Share', 'People', 'Link'] : ['Share']) {
        if (mobile) fireEvent.click(screen.getByRole('tab', { name: tab }));
        expect(
          screen.getAllByRole('button', { name: 'Set option view' }).length
        ).toBeGreaterThan(0);
        expect(
          screen.queryByRole('button', { name: 'Set option edit' })
        ).toBeNull();
        expect(
          screen.queryByRole('button', { name: 'Set option comment' })
        ).toBeNull();
      }
    }
  );

  it.each([false, true])(
    'matches disabled-comment grant labels in link-scope notifications (mobile: %s)',
    async (mobile) => {
      mocks.mobile = mobile;
      mocks.hasTeam = true;
      render(() => (
        <ShareModal
          id="comment-grant-document"
          itemType="document"
          blockAlias="md"
          name="Document"
          owner={ME}
          userPermissions={Permissions.OWNER}
          permissionOptions={{ comment: false }}
          sharePermissions={{
            id: 'comment-grants',
            owner: ME,
            linkShare: 'TEAM',
            linkShareAccessLevel: 'comment',
            channelSharePermissions: [],
          }}
          open
          onOpenChange={vi.fn()}
        />
      ));
      if (mobile) fireEvent.click(screen.getByRole('tab', { name: 'Link' }));
      expect(screen.getAllByText('View').length).toBeGreaterThan(0);
      fireEvent.click(screen.getByRole('button', { name: 'Set link PUBLIC' }));
      const { toast } = await import('@core/component/Toast/Toast');
      await vi.waitFor(() =>
        expect(toast.success).toHaveBeenCalledWith(
          'Updated Public link sharing',
          { subtext: 'Anyone with the link can view this document' }
        )
      );
      expect(mocks.editDocument).toHaveBeenCalledWith({
        documentId: 'comment-grant-document',
        sharePermission: {
          linkShare: 'PUBLIC',
          linkShareAccessLevel: 'comment',
        },
      });
    }
  );

  it('updates both grant options from one reactive permissionOptions prop', () => {
    const [options, setOptions] = createSignal({ edit: false, comment: false });
    render(() => (
      <ShareOptions permissionOptions={options()} setPermissions={vi.fn()} />
    ));
    expect(
      screen.queryByRole('button', { name: 'Set option edit' })
    ).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Set option comment' })
    ).toBeNull();
    setOptions({ edit: true, comment: true });
    expect(
      screen.getByRole('button', { name: 'Set option edit' })
    ).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Set option comment' })
    ).toBeTruthy();
  });
  it('uses explicit comment capability for options and existing comment labels', () => {
    render(() => (
      <ShareOptions
        permissionOptions={{ comment: false }}
        permissions="comment"
        setPermissions={vi.fn()}
      />
    ));
    expect(
      screen.queryByRole('button', { name: 'Set option comment' })
    ).toBeNull();
    expect(screen.getAllByText('View')).toHaveLength(2);
  });
});
