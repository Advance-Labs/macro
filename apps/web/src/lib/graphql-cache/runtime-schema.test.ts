import { describe, expect, it } from 'vitest';
import { deriveRuntimeCacheSchema } from '../../../scripts/generate-graphql-cache-runtime-schema';
import {
  assertCacheSchemaAcknowledged,
  runtimeCacheSchema,
} from './runtime-schema';

describe('runtime cache schema', () => {
  it('carries the metadata needed to cache calendar invitations', () => {
    const schema = JSON.parse(runtimeCacheSchema);
    expect(schema.protocolVersion).toBe(1);
    const message = schema.types.find(
      (type: { name: string }) => type.name === 'GraphqlSoupEmailMessage'
    );
    expect(message.fields).toContainEqual({
      name: 'calendarInvitations',
      ty: {
        name: 'JSON',
        kind: 'OpaqueScalar',
        nullable: false,
        list: false,
        item_nullable: false,
      },
    });
  });
  it('derives wrapping, entity keys and abstract membership from SDL', () => {
    const metadata = deriveRuntimeCacheSchema(
      'scalar JSON\ninterface Node { id: ID! }\ntype Message implements Node { id: ID! invitations: JSON! }\ntype Query { nodes: [Node!]! }'
    );
    expect(
      metadata.types.find((type) => type.name === 'Message')?.key_fields
    ).toEqual(['id']);
    expect(
      metadata.types.find((type) => type.name === 'Node')?.possible_types
    ).toEqual(['Message']);
    expect(
      metadata.types.find((type) => type.name === 'Query')?.fields[0].ty
    ).toEqual({
      name: 'Node',
      kind: 'Composite',
      nullable: false,
      list: true,
      item_nullable: false,
    });
  });
  it.each([
    'type Query { values: [[String]] }',
    'type Query { message: Message } type Message { id: String! }',
  ])('rejects SDL requiring unsupported cache semantics: %s', (source) =>
    expect(() => deriveRuntimeCacheSchema(source)).toThrow()
  );
  it.each([
    null,
    undefined,
    {},
    { protocolVersion: 2, fingerprint: 'a'.repeat(64) },
    { protocolVersion: 1, fingerprint: 'untrusted' },
  ])('rejects invalid acknowledgement %j', (value) => {
    expect(() => assertCacheSchemaAcknowledged(value)).toThrow(
      'Update the app'
    );
  });
});
