import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * typed-document-node inlines every fragment a document spreads into that
 * document's JSON AST, so one fragment used by twenty operations ships twenty
 * times in the entry chunk. This rewrites the generated module so each
 * fragment definition is emitted once and every document references it. The
 * documents keep their exact DocumentNode shape.
 */
const DOCUMENT_LINE =
  /^export const (\w+) = (\{"kind":"Document".*\}) as unknown as (.+);$/;

type Definition = { kind: string; name?: { value: string } };

export function shareGraphqlFragments(source: string): string {
  const fragments = new Map<string, string>();
  const lines = source.split('\n').map((line) => {
    const match = line.match(DOCUMENT_LINE);
    if (!match) return line;
    const [, name, json, type] = match;
    const document = JSON.parse(json) as { definitions: Definition[] };
    const definitions = document.definitions.map((definition) => {
      if (definition.kind !== 'FragmentDefinition' || !definition.name) {
        return JSON.stringify(definition);
      }
      const fragmentName = definition.name.value;
      const text = JSON.stringify(definition);
      const shared = fragments.get(fragmentName);
      if (shared !== undefined && shared !== text) {
        throw new Error(`fragment ${fragmentName} differs between documents`);
      }
      fragments.set(fragmentName, text);
      return fragmentConstName(fragmentName);
    });
    return `export const ${name} = {"kind":"Document","definitions":[${definitions.join(',')}]} as unknown as ${type};`;
  });

  const firstDocument = lines.findIndex((line) => DOCUMENT_LINE.test(line));
  if (firstDocument === -1) return source;
  lines.splice(
    firstDocument,
    0,
    ...[...fragments].map(
      ([fragmentName, text]) =>
        `const ${fragmentConstName(fragmentName)} = ${text};`
    )
  );
  return lines.join('\n');
}

function fragmentConstName(fragmentName: string): string {
  return `fragment_${fragmentName}`;
}

if (import.meta.main) {
  const generated = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '../src/lib/service-clients/service-storage/graphql/generated/graphql.ts'
  );
  writeFileSync(
    generated,
    shareGraphqlFragments(readFileSync(generated, 'utf8'))
  );
}
