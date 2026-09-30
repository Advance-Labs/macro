import { createClient, fetchExchange } from '@urql/core';
import { Kind, parse, print, visit } from 'graphql';
import {
  EmailThreadPageDocument,
  type EmailThreadPageFieldsFragment,
  EmailThreadPageFieldsFragmentDoc,
} from '../../../service-clients/service-storage/graphql/generated/graphql';
import { withEmailCacheCompatibility } from '../../../service-clients/service-storage/graphql-email-cache';
import { entityFromArgument } from '../../exchange/entity-resolvers';
import { normalizedCacheExchange } from '../../exchange/normalized-cache-exchange';
import { createWorkerCacheHost } from '../../host/worker-host';

const status = document.querySelector<HTMLParagraphElement>('#status')!;
const email = document.querySelector<HTMLElement>('#email')!;
const raw = createWorkerCacheHost({
  scope: `email-offline-${crypto.randomUUID()}`,
});
const host = withEmailCacheCompatibility(
  raw,
  new URLSearchParams(location.search).has('native')
);
const client = createClient({
  url: `${location.origin}/offline-email-api`,
  exchanges: [
    normalizedCacheExchange(host, {
      entityResolvers: {
        GraphqlUser: {
          emailThread: entityFromArgument('GraphqlSoupEmailThread', [
            'input',
            'threadId',
          ]),
        },
      },
    }),
    fetchExchange,
  ],
});
const variables = (threadId: string) => ({ threadId, offset: 0, limit: 20 });
const timestamp = '2026-09-01T00:00:00Z';
function thread(id: string): EmailThreadPageFieldsFragment {
  return {
    __typename: 'GraphqlSoupEmailThread',
    id,
    providerId: id,
    linkId: 'link',
    inboxVisible: true,
    isRead: false,
    projectId: null,
    latestInboundMessageTs: timestamp,
    createdAt: timestamp,
    updatedAt: timestamp,
    viewerPermission: {
      __typename: 'GraphqlAccessLevelPermission',
      accessLevel: 'OWNER',
    },
    labels: [],
    messages: [
      {
        __typename: 'GraphqlSoupEmailMessage',
        id: `message-${id}`,
        providerId: id,
        threadId: id,
        replyingToId: null,
        linkId: 'link',
        subject: id,
        snippet: id,
        internalDateTs: timestamp,
        sentAt: timestamp,
        isRead: false,
        isStarred: false,
        isSent: false,
        isDraft: false,
        hasAttachments: false,
        scheduledSendTime: null,
        from: null,
        to: [],
        cc: [],
        bcc: [],
        labels: [],
        bodyText: `${id} cached body`,
        bodyHtmlSanitized: `<p>${id} cached body</p>`,
        bodyMacro: null,
        bodyReplyless: null,
        calendarInvitations: [],
        attachments: [],
        attachmentsDraft: [],
        attachmentsForwarded: [],
        createdAt: timestamp,
        updatedAt: timestamp,
      },
    ],
  };
}
const legacy = (query: string) =>
  print(
    visit(parse(query), {
      Field: (field) =>
        field.name.value === 'calendarInvitations' ? null : undefined,
    })
  );
let unsubscribe: (() => void) | undefined;
function open(id: string) {
  unsubscribe?.();
  email.textContent = 'Loading';
  const subscription = client
    .query(EmailThreadPageDocument, variables(id), {
      requestPolicy: 'cache-and-network',
    })
    .subscribe((result) => {
      if (result.data?.user.emailThread) {
        email.textContent = result.data.user.emailThread.messages[0].bodyText;
      } else if (result.error) {
        email.textContent = 'Email unavailable offline';
      }
    });
  unsubscribe = () => subscription.unsubscribe();
}
async function prepare() {
  await raw.writeQuery({
    query: legacy(print(EmailThreadPageDocument)),
    variables: variables('visited'),
    data: { user: { id: 'viewer', emailThread: thread('visited') } },
  });
  // Hydrate an entity through Soup only: there has never been a direct
  // emailThread query for this ID. Opening it requires the entity resolver.
  const fragments = print({
    kind: Kind.DOCUMENT,
    definitions: EmailThreadPageFieldsFragmentDoc.definitions,
  });
  await raw.hydrateQuery({
    query: legacy(
      `query SeedEmail($offset: Int!, $limit: Int!) { user { id @cacheOnly soup(input: {initial: {limit: 5, emailView: ALL}}) { items @cacheOnly { ...EmailThreadPageFields } nextCursor } } } ${fragments}`
    ),
    variables: { offset: 0, limit: 20 },
    data: {
      user: {
        id: 'viewer',
        soup: { items: [thread('preloaded')], nextCursor: null },
      },
    },
  });
  const before = await raw.readQuery({
    query: print(EmailThreadPageDocument),
    variables: variables('visited'),
  });
  if (before.kind !== 'miss')
    throw new Error('Fixture must miss the newly selected field');
  for (const id of ['visited', 'preloaded', 'missing']) {
    const button = document.querySelector<HTMLButtonElement>(`#${id}`)!;
    button.disabled = false;
    button.addEventListener('click', () => open(id));
  }
  status.textContent = 'Legacy cache ready';
}
try {
  await prepare();
} catch (error) {
  status.textContent = String(error);
}
