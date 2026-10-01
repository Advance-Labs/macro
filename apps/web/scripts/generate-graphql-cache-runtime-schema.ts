import {
  type GraphQLCompositeType,
  assertValidSchema,
  buildSchema,
  isEnumType,
  isInterfaceType,
  isListType,
  isNonNullType,
  isObjectType,
  isScalarType,
  isUnionType,
} from 'graphql';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFile, writeFile } from 'node:fs/promises';

/** Cache metadata protocol 1 mirrors cache-core's flattened output types. */
export function deriveRuntimeCacheSchema(source: string) {
  const schema = buildSchema(source);
  assertValidSchema(schema);
  const roots = [
    schema.getQueryType(),
    schema.getMutationType(),
    schema.getSubscriptionType(),
  ];
  const types = Object.values(schema.getTypeMap())
    .filter(
      (type): type is GraphQLCompositeType =>
        !type.name.startsWith('__') &&
        (isObjectType(type) || isInterfaceType(type) || isUnionType(type))
    )
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((type) => {
      const fields = isUnionType(type)
        ? []
        : Object.values(type.getFields())
            .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
            .map((field) => {
              const nullable = !isNonNullType(field.type);
              const outer = isNonNullType(field.type)
                ? field.type.ofType
                : field.type;
              const list = isListType(outer);
              const item = isListType(outer) ? outer.ofType : outer;
              const item_nullable = list && !isNonNullType(item);
              const named = isNonNullType(item) ? item.ofType : item;
              if (isListType(named))
                throw new Error(
                  `nested list unsupported: ${type.name}.${field.name}`
                );
              const kind =
                isObjectType(named) ||
                isInterfaceType(named) ||
                isUnionType(named)
                  ? 'Composite'
                  : isEnumType(named) ||
                      (isScalarType(named) &&
                        ['Int', 'Float', 'String', 'Boolean', 'ID'].includes(
                          named.name
                        ))
                    ? 'Leaf'
                    : 'OpaqueScalar';
              return {
                name: field.name,
                ty: { name: named.name, kind, nullable, list, item_nullable },
              };
            });
      const id = fields.find((field) => field.name === 'id');
      if (
        id &&
        (id.ty.name !== 'ID' ||
          id.ty.nullable ||
          id.ty.list ||
          roots.some((root) => root?.name === type.name))
      ) {
        throw new Error(
          `invalid cache key: ${type.name}.id must be ID! on a non-root type`
        );
      }
      return {
        name: type.name,
        kind: isObjectType(type)
          ? 'Object'
          : isInterfaceType(type)
            ? 'Interface'
            : 'Union',
        key_fields: id ? ['id'] : null,
        fields,
        possible_types: isObjectType(type)
          ? []
          : schema
              .getPossibleTypes(type)
              .map((candidate) => candidate.name)
              .sort(),
      };
    });
  return {
    protocolVersion: 1,
    queryRoot: schema.getQueryType()?.name,
    mutationRoot: schema.getMutationType()?.name ?? null,
    subscriptionRoot: schema.getSubscriptionType()?.name ?? null,
    types,
  };
}

async function main() {
  const schemaPath = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '../../../static_assets/schema.graphql'
  );
  const outputPath = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '../src/lib/graphql-cache/generated/runtime-schema.json'
  );
  const output = `${JSON.stringify(deriveRuntimeCacheSchema(await readFile(schemaPath, 'utf8')))}\n`;
  if (process.argv.includes('--check')) {
    if ((await readFile(outputPath, 'utf8')) !== output) {
      throw new Error(
        'GraphQL cache runtime schema is stale; run `bun run gen-graphql-cache-schema`'
      );
    }
  } else {
    await writeFile(outputPath, output);
  }
}

if (import.meta.main) await main();
