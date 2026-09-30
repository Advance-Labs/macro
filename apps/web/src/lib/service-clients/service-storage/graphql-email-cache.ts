import type { CacheHost } from '@graphql-cache/host/types';
import { parse, print, visit } from 'graphql';

const FIELD = 'calendarInvitations';
const UNKNOWN_FIELD =
  'unknown field `GraphqlSoupEmailMessage.calendarInvitations`';
const PROBE_FRAGMENT = 'EmailCacheCapabilities';

/**
 * Calendar snapshots are additive to email bodies. Older native schemas cannot
 * store them, and older persisted messages lack them even on a current engine.
 * Keep that optional enrichment from making the whole thread unavailable.
 * Live network reads still request snapshots; cache reads may omit them.
 */
export function withEmailCacheCompatibility(
  host: CacheHost,
  native: boolean
): CacheHost {
  const legacyDocuments = new Map<string, string>();
  function withoutInvitations(document: string): string {
    if (!document.includes(FIELD)) return document;
    const cached = legacyDocuments.get(document);
    if (cached) return cached;
    const projected = print(
      visit(parse(document), {
        Field: (field) => (field.name.value === FIELD ? null : undefined),
      })
    );
    legacyDocuments.set(document, projected);
    return projected;
  }

  let supportsInvitations: Promise<boolean> | undefined;
  async function probeInvitations(): Promise<boolean> {
    try {
      // Fragment validation runs even with no keys, so this works offline and
      // with an empty database. Probe before writes, never retry an enqueue.
      await host.readRecordsByKeys({
        document: `fragment ${PROBE_FRAGMENT} on GraphqlSoupEmailMessage { id ${FIELD} }`,
        fragmentName: PROBE_FRAGMENT,
        keys: [],
      });
      return true;
    } catch (error) {
      if (
        error instanceof Error &&
        (error.message === UNKNOWN_FIELD ||
          error.message ===
            'Command graphql_cache_read_records_by_keys not found')
      )
        return false;
      // A transient failure is not evidence about schema support.
      supportsInvitations = undefined;
      throw error;
    }
  }
  async function compatibleDocument(document: string): Promise<string> {
    if (!native || !document.includes(FIELD)) return document;
    supportsInvitations ??= probeInvitations();
    return (await supportsInvitations)
      ? document
      : withoutInvitations(document);
  }

  return {
    ...host,
    get disabled() {
      return host.disabled;
    },
    async readQuery(args) {
      const query = await compatibleDocument(args.query);
      const result = await host.readQuery({ ...args, query });
      if (result.kind === 'hit') return result;
      const fallback = withoutInvitations(query);
      return fallback === query
        ? result
        : host.readQuery({ ...args, query: fallback });
    },
    async readRecordsByKeys(args) {
      const document = await compatibleDocument(args.document);
      const result = await host.readRecordsByKeys({ ...args, document });
      const fallback = withoutInvitations(document);
      if (result.records.length === args.keys.length || fallback === document)
        return result;
      const legacy = await host.readRecordsByKeys({
        ...args,
        document: fallback,
      });
      // Never combine snapshots from different revisions. Prefer the complete
      // current fallback in that case; optional snapshots can arrive next read.
      if (legacy.revision !== result.revision) return legacy;
      const complete = new Map(
        result.records.map((entry) => [entry.recordKey, entry])
      );
      return {
        ...legacy,
        records: legacy.records.map(
          (entry) => complete.get(entry.recordKey) ?? entry
        ),
      };
    },
    async writeQuery(args) {
      return host.writeQuery({
        ...args,
        query: await compatibleDocument(args.query),
      });
    },
    async hydrateQuery(args) {
      return host.hydrateQuery({
        ...args,
        query: await compatibleDocument(args.query),
      });
    },
    async enqueueOptimisticMutation(args, claim) {
      return host.enqueueOptimisticMutation(
        { ...args, query: await compatibleDocument(args.query) },
        claim
      );
    },
    async commitOptimisticWrite(transactionId, claim, args) {
      return host.commitOptimisticWrite(transactionId, claim, {
        ...args,
        query: await compatibleDocument(args.query),
      });
    },
    async inspectQuery(args) {
      return host.inspectQuery({
        ...args,
        query: await compatibleDocument(args.query),
      });
    },
    async inspectQueryVariants(args) {
      return host.inspectQueryVariants({
        ...args,
        query: await compatibleDocument(args.query),
      });
    },
  };
}
