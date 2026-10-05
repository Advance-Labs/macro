import { optimisticContextOf } from '@graphql-cache/exchange/optimistic';
import { CombinedError, createClient, type Operation } from '@urql/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { map, pipe } from 'wonka';
import type { GraphqlSaveEmailDraftArgs } from './graphql/draft';
import {
  cancelEmailSendQueued,
  type EmailSendIntent,
  emailSendLocked,
  emailSendQueueSelected,
  restoreCancelledEmailSend,
  sendEmailQueued,
} from './send-queue';

const mocks = vi.hoisted(() => ({
  intents: vi.fn(),
  client: vi.fn(),
  cached: vi.fn(),
  save: vi.fn(),
  error: undefined as CombinedError | undefined,
  cacheEnabled: vi.fn(),
  rolloutEnabled: vi.fn(),
}));
vi.mock('@core/constant/featureFlags', () => ({
  enableGraphqlSoup: 'graphql',
  isFeatureEnabled: () => true,
}));
vi.mock('@graphql-cache/rollout', () => ({
  getBrowserTursoCacheRolloutDecision: () => ({
    enabled: mocks.rolloutEnabled(),
  }),
}));
vi.mock('@service-storage/graphql-soup', () => ({
  getGraphqlSoupClient: () => mocks.client(),
  getGraphqlCacheHost: () => ({ durableMutationIntents: mocks.intents }),
  graphqlCacheEnabled: () => mocks.cacheEnabled(),
}));
vi.mock('../soup/graphql/active-queries', () => ({
  getActiveGraphqlSoupRevalidations: () => [],
}));
vi.mock('./draft-queue', () => ({
  readCachedDraftAndThread: (...args: unknown[]) => mocks.cached(...args),
  saveEmailDraftQueued: (...args: unknown[]) => mocks.save(...args),
}));

const draft: GraphqlSaveEmailDraftArgs = {
  draftId: '00000000-0000-4000-8000-000000000001',
  threadDbId: '00000000-0000-4000-8000-000000000002',
  senderLinkId: '00000000-0000-4000-8000-000000000003',
  senderEmail: 'sender@example.com',
  subject: 'Approved subject',
  to: [{ email: 'to@example.com' }],
  bodyText: 'Approved body',
  bodyHtml: 'PHA-QXBwcm92ZWQgYm9keTwvcD4',
  optimisticBodyHtml: '<p>Approved body</p>',
  newThreadOwnerId: 'macro|sender@example.com',
};
let operations: Operation[];
beforeEach(() => {
  mocks.cacheEnabled.mockReturnValue(true);
  mocks.rolloutEnabled.mockReturnValue(true);
  operations = [];
  mocks.error = undefined;
  mocks.intents.mockResolvedValue([]);
  mocks.cached.mockResolvedValue({
    draftId: draft.draftId,
    threadDbId: draft.threadDbId,
  });
  mocks.save.mockResolvedValue({ kind: 'queued' });
  const client = createClient({
    url: 'http://test/graphql',
    exchanges: [
      () => (source) =>
        pipe(
          source,
          map((operation) => {
            operations.push(operation);
            return {
              operation,
              error: mocks.error,
              stale: false,
              hasNext: false,
              extensions: {
                normalizedCacheMutationDisposition: {
                  kind: 'queued',
                  transactionId: '1',
                },
              },
            };
          })
        ),
    ],
  });
  mocks.client.mockReturnValue(client);
});

it('retains legacy sending when GraphQL is enabled without the durable cache rollout', () => {
  mocks.cacheEnabled.mockReturnValue(false);
  mocks.rolloutEnabled.mockReturnValue(false);
  expect(emailSendQueueSelected('graphql')).toBe(false);
});

it('requires durable storage when an enabled cache fails to initialize', () => {
  mocks.cacheEnabled.mockReturnValue(false);
  expect(emailSendQueueSelected('graphql')).toBe(true);
  expect(emailSendQueueSelected('rest')).toBe(false);
});

async function send(): Promise<EmailSendIntent> {
  await sendEmailQueued({
    draft,
    attachmentIds: ['uploaded'],
    forwardedAttachmentIds: ['forwarded'],
    restoreBodyText: 'Editable body',
  });
  const context = optimisticContextOf(operations[0])!;
  return {
    uuid: context.uuid,
    phase: 'pending',
    locallyCancelled: false,
    metadata: context.durableIntent as EmailSendIntent['metadata'],
  };
}

describe('durable email send intent', () => {
  it('persists the approved envelope, inbox, attachments and recovery body before acknowledging', async () => {
    const intent = await send();
    expect(intent.metadata.payload.input).toMatchObject({
      attempt: { attemptId: intent.uuid, linkId: draft.senderLinkId },
      message: {
        draftId: draft.draftId,
        subject: draft.subject,
        to: draft.to,
        bodyText: draft.bodyText,
      },
      attachmentIds: ['uploaded'],
      forwardedAttachmentIds: ['forwarded'],
      restoreBodyText: 'Editable body',
    });
    expect(operations[0].variables?.input).toEqual(
      intent.metadata.payload.input
    );
    expect(JSON.stringify(operations[0].variables)).not.toContain(
      'existingThread'
    );
    expect(emailSendLocked(intent)).toBe(true);
  });
  it('acknowledges a persisted send and cancellation despite a failed initial network attempt', async () => {
    mocks.error = new CombinedError({
      networkError: new Error('connection lost'),
    });
    const intent = await send();
    await expect(cancelEmailSendQueued(intent)).resolves.toBeUndefined();
    expect(operations).toHaveLength(2);
  });
  it('resolves queued draft handles before checking a canonical draft for duplicate sends', async () => {
    const intent = await send();
    mocks.intents.mockResolvedValue([intent]);
    mocks.cached.mockResolvedValue({
      draftId: 'server-message',
      threadDbId: 'server-thread',
    });
    await expect(
      sendEmailQueued({
        draft: { ...draft, draftId: 'server-message' },
        attachmentIds: [],
        forwardedAttachmentIds: [],
      })
    ).rejects.toThrow('already queued');
  });

  it('uses the same attempt UUID to atomically replace send with cancellation', async () => {
    const intent = await send();
    await cancelEmailSendQueued(intent);
    expect(operations[1].variables).toEqual({
      input: intent.metadata.payload.input.attempt,
    });
    expect(optimisticContextOf(operations[1])).toMatchObject({
      uuid: intent.uuid,
      durableIntent: { replace: true },
    });
    expect(
      emailSendLocked({
        ...intent,
        metadata: { ...intent.metadata, replace: true },
      })
    ).toBe(true);
    expect(emailSendLocked({ ...intent, locallyCancelled: true })).toBe(false);
  });
  it('refuses a second send for the same locked draft', async () => {
    const intent = await send();
    mocks.intents.mockResolvedValue([intent]);
    await expect(
      sendEmailQueued({ draft, attachmentIds: [], forwardedAttachmentIds: [] })
    ).rejects.toThrow('already queued');
    expect(operations).toHaveLength(1);
  });
  it('restores pre-send content under the original identity without dropping attachments', async () => {
    const intent = await send();
    await expect(restoreCancelledEmailSend(intent)).rejects.toThrow(
      'Confirm cancellation'
    );
    await restoreCancelledEmailSend({ ...intent, locallyCancelled: true });
    expect(mocks.save).toHaveBeenCalledWith({
      args: expect.objectContaining({
        draftId: draft.draftId,
        threadDbId: draft.threadDbId,
        bodyText: 'Editable body',
        mutationUuid: intent.uuid,
        durableIntent: expect.objectContaining({
          replace: true,
          payload: expect.objectContaining({ restoring: true }),
        }),
      }),
    });
  });
});
