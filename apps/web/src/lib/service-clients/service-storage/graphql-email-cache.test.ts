import type { CacheHost } from '@graphql-cache/host/types';
import { INITIAL_CACHE_REVISION } from '@graphql-cache/protocol';
import { print } from 'graphql';
import { describe, expect, it, vi } from 'vitest';
import {
  EmailThreadPageDocument,
  SoupBackfillDocument,
} from './graphql/generated/graphql';
import { withEmailCacheCompatibility } from './graphql-email-cache';

const query = print(EmailThreadPageDocument);
const variables = { threadId: 'thread-1', offset: 0, limit: 20 };
const page = {
  user: {
    id: 'user-1',
    emailThread: {
      id: 'thread-1',
      messages: [{ id: 'message-1', bodyText: 'Cached body' }],
    },
  },
};
const emptySelection = { revision: INITIAL_CACHE_REVISION, records: [] };
const written = {
  revision: INITIAL_CACHE_REVISION,
  revisionAdvanced: false,
  changed: [],
  affectedOps: [],
  reset: false,
};

function fixture(native: boolean, supportsInvitations = false) {
  const host = {
    readQuery: vi.fn<CacheHost['readQuery']>(async ({ query }) =>
      query.includes('calendarInvitations')
        ? { kind: 'miss' as const }
        : { kind: 'hit' as const, data: page }
    ),
    readRecordsByKeys: vi.fn<CacheHost['readRecordsByKeys']>(async () => {
      if (!supportsInvitations)
        throw new Error(
          'unknown field `GraphqlSoupEmailMessage.calendarInvitations`'
        );
      return emptySelection;
    }),
    writeQuery: vi.fn<CacheHost['writeQuery']>(async () => written),
    hydrateQuery: vi.fn<CacheHost['hydrateQuery']>(async () => ({
      ...written,
      kind: 'data',
      data: { user: { soup: { nextCursor: null } } },
    })),
    enqueueOptimisticMutation: vi.fn(),
    commitOptimisticWrite: vi.fn(),
  };
  return {
    raw: host,
    host: withEmailCacheCompatibility(host as unknown as CacheHost, native),
  };
}

describe('email cache compatibility', () => {
  it.each([false, true])(
    'opens an older cached message without invitation snapshots (native=%s)',
    async (native) => {
      const { raw, host } = fixture(native, true);
      expect(await host.readQuery({ opKey: 42, query, variables })).toEqual({
        kind: 'hit',
        data: page,
      });
      expect(raw.readQuery).toHaveBeenCalledTimes(2);
      expect(raw.readQuery.mock.calls[1][0]).toMatchObject({
        opKey: 42,
        variables,
      });
      expect(raw.readQuery.mock.calls[1][0].query).not.toContain(
        'calendarInvitations'
      );
      expect(raw.readQuery.mock.calls[1][0].query).toContain(
        'bodyHtmlSanitized'
      );
      expect(raw.readQuery.mock.calls[1][0].query).toContain(
        'viewerPermission'
      );
      if (!native) expect(raw.readRecordsByKeys).not.toHaveBeenCalled();
    }
  );

  it('probes once before native writes and hydrates bodies on older binaries', async () => {
    const { raw, host } = fixture(true);
    await host.hydrateQuery({
      query: print(SoupBackfillDocument),
      data: {},
      identity: 'user-1',
    });
    await host.writeQuery({ query, variables, data: page, identity: 'user-1' });
    expect(await host.readQuery({ query, variables })).toEqual({
      kind: 'hit',
      data: page,
    });
    expect(raw.readRecordsByKeys).toHaveBeenCalledOnce();
    expect(raw.hydrateQuery).toHaveBeenCalledWith(
      expect.objectContaining({
        identity: 'user-1',
        query: expect.not.stringContaining('calendarInvitations'),
      })
    );
    expect(raw.writeQuery).toHaveBeenCalledWith(
      expect.objectContaining({
        data: page,
        query: expect.not.stringContaining('calendarInvitations'),
      })
    );
    expect(raw.readQuery).toHaveBeenCalledOnce();
  });

  it('keeps current invitation data and writes on supported native engines', async () => {
    const { raw, host } = fixture(true, true);
    const full = {
      kind: 'hit' as const,
      data: { calendarInvitations: [{ title: 'Meeting' }] },
    };
    raw.readQuery.mockResolvedValueOnce(full);
    expect(await host.readQuery({ query, variables })).toBe(full);
    await host.writeQuery({ query, data: full.data });
    expect(raw.readQuery).toHaveBeenCalledOnce();
    expect(raw.writeQuery).toHaveBeenCalledWith({ query, data: full.data });
  });

  it('does not reinterpret cache failures as missing invitation fields', async () => {
    const { raw, host } = fixture(false);
    raw.readQuery.mockRejectedValueOnce(new Error('storage failed'));
    await expect(host.readQuery({ query })).rejects.toThrow('storage failed');
    expect(raw.readQuery).toHaveBeenCalledOnce();
  });

  it('keeps a cache miss when the body itself is unavailable', async () => {
    const { raw, host } = fixture(false);
    raw.readQuery.mockResolvedValue({ kind: 'miss' });
    expect(await host.readQuery({ query, variables })).toEqual({
      kind: 'miss',
    });
  });

  it('reads older draft records without dropping snapshots from complete records', async () => {
    const { raw, host } = fixture(false);
    const complete = {
      recordKey: 'GraphqlSoupEmailMessage:new',
      record: { id: 'new', calendarInvitations: ['snapshot'] },
    };
    const legacy = {
      recordKey: 'GraphqlSoupEmailMessage:old',
      record: { id: 'old' },
    };
    raw.readRecordsByKeys
      .mockResolvedValueOnce({ ...emptySelection, records: [complete] })
      .mockResolvedValueOnce({
        ...emptySelection,
        records: [legacy, { ...complete, record: { id: 'new' } }],
      });
    const result = await host.readRecordsByKeys({
      document:
        'fragment Message on GraphqlSoupEmailMessage { id calendarInvitations }',
      fragmentName: 'Message',
      keys: [legacy.recordKey, complete.recordKey],
    });
    expect(result.records).toEqual([legacy, complete]);
  });

  it('strips unsupported snapshots before a single durable enqueue and settlement', async () => {
    const { raw, host } = fixture(true);
    const args = { uuid: 'draft-1', query, data: page };
    const claim = { owner: 'owner', nowMs: 1, leaseExpiresAtMs: 100 };
    await host.enqueueOptimisticMutation(args, claim);
    await host.commitOptimisticWrite(
      'tx-1',
      { owner: 'owner', generation: '1' },
      args
    );
    expect(raw.enqueueOptimisticMutation).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        uuid: 'draft-1',
        query: expect.not.stringContaining('calendarInvitations'),
      }),
      claim
    );
    expect(raw.commitOptimisticWrite).toHaveBeenCalledExactlyOnceWith(
      'tx-1',
      { owner: 'owner', generation: '1' },
      expect.objectContaining({
        query: expect.not.stringContaining('calendarInvitations'),
      })
    );
  });

  it('does not enqueue anything on a failed capability probe and retries the probe later', async () => {
    const { raw, host } = fixture(true, true);
    raw.readRecordsByKeys.mockRejectedValueOnce(new Error('IPC timeout'));
    const claim = { owner: 'owner', nowMs: 1, leaseExpiresAtMs: 100 };
    const args = { uuid: 'draft-1', query, data: page };
    await expect(host.enqueueOptimisticMutation(args, claim)).rejects.toThrow(
      'IPC timeout'
    );
    expect(raw.enqueueOptimisticMutation).not.toHaveBeenCalled();
    await host.enqueueOptimisticMutation(args, claim);
    expect(raw.enqueueOptimisticMutation).toHaveBeenCalledExactlyOnceWith(
      args,
      claim
    );
  });

  it('does not change queries unrelated to email invitations', async () => {
    const { raw, host } = fixture(true);
    await host.readQuery({ query: '{ user { id } }' });
    expect(raw.readRecordsByKeys).not.toHaveBeenCalled();
    expect(raw.readQuery).toHaveBeenCalledExactlyOnceWith({
      query: '{ user { id } }',
    });
  });
});
