import { describe, expect, it } from 'vitest';
import { shareGraphqlFragments } from './share-graphql-fragments';

describe('shareGraphqlFragments', () => {
  it('emits each inlined fragment once and references it from every document', () => {
    const generated = [
      "import type { TypedDocumentNode as DocumentNode } from '@graphql-typed-document-node/core';",
      'export const ItemFieldsFragmentDoc = {"kind":"Document","definitions":[{"kind":"FragmentDefinition","name":{"kind":"Name","value":"ItemFields"}}]} as unknown as DocumentNode<ItemFieldsFragment, unknown>;',
      'export const ItemsDocument = {"kind":"Document","definitions":[{"kind":"OperationDefinition","operation":"query"},{"kind":"FragmentDefinition","name":{"kind":"Name","value":"ItemFields"}}]} as unknown as DocumentNode<ItemsQuery, ItemsQueryVariables>;',
    ].join('\n');

    expect(shareGraphqlFragments(generated)).toBe(
      [
        "import type { TypedDocumentNode as DocumentNode } from '@graphql-typed-document-node/core';",
        'const fragment_ItemFields = {"kind":"FragmentDefinition","name":{"kind":"Name","value":"ItemFields"}};',
        'export const ItemFieldsFragmentDoc = {"kind":"Document","definitions":[fragment_ItemFields]} as unknown as DocumentNode<ItemFieldsFragment, unknown>;',
        'export const ItemsDocument = {"kind":"Document","definitions":[{"kind":"OperationDefinition","operation":"query"},fragment_ItemFields]} as unknown as DocumentNode<ItemsQuery, ItemsQueryVariables>;',
      ].join('\n')
    );
  });
});
