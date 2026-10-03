import type { AnyBlockDefinition } from '@core/block';
import type { BlockName } from '../../constants/block-registry';
import { LegacyBlockRegistry } from './legacyBlockRegistry';

// Compatibility exports only. Metadata consumers use the pure module directly.
export {
  blockAcceptedFileExtensionSet,
  blockAcceptedFileExtensionToMimeType,
  blockAcceptedMimetypeToFileExtension,
  blockAcceptsFileExtension,
  blockNameToDefaultFile,
  blockNameToFileExtensionSet,
  blockNameToFileExtensions,
  blockNameToMimeTypes,
  fileTypeToBlockName,
  fileTypeToResolvedBlockName,
  type ItemLike,
  isBlockAlias,
  itemToBlockName,
  itemToResolvedBlockName,
  itemToSafeName,
  resolveBlockAlias,
  verifyBlockName,
} from '../../constants/file-metadata';

const discoveredBlockDefinitions = Object.values<AnyBlockDefinition>(
  import.meta.glob('../../../features/block-*/definition.ts', {
    eager: true,
    import: 'definition',
  })
);

const definitionNames = discoveredBlockDefinitions.map(
  (definition) => definition.name
);
const duplicateDefinitionNames = definitionNames.filter(
  (name, index) => definitionNames.indexOf(name) !== index
);
if (duplicateDefinitionNames.length > 0) {
  throw new Error(
    `Duplicate block definitions discovered: ${[...new Set(duplicateDefinitionNames)].join(', ')}`
  );
}

const missingBlockDefinitions = LegacyBlockRegistry.filter(
  (name) => !definitionNames.includes(name)
);
if (missingBlockDefinitions.length > 0) {
  throw new Error(
    `Missing block definitions: ${missingBlockDefinitions.join(', ')}`
  );
}

export const blocks = Object.fromEntries(
  discoveredBlockDefinitions.map((definition) => [definition.name, definition])
) as Readonly<Record<BlockName, Readonly<AnyBlockDefinition>>>;
