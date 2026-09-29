import type { AutomergeDoc } from '@macro-inc/automerge';
import { createAutomergeDoc } from './manager';
import type { RawUpdate } from './shared';

export function loroDocFromSnapshot(snapshot: RawUpdate): AutomergeDoc {
  const loroDoc = createAutomergeDoc();
  loroDoc.import(snapshot);
  return loroDoc;
}

export function compareLoroDocVersions(
  a: AutomergeDoc,
  b: AutomergeDoc
): number {
  const aVersion = a.version();
  return aVersion.compare(b.version()) ?? 0;
}
