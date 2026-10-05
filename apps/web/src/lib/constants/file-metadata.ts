import type { SubType } from '@entity';
import type { ItemType } from '@service-storage/client';
import type { BasicDocumentSubTypeProperty } from '@service-storage/generated/schemas';
import type { BasicDocumentFileType } from '@service-storage/generated/schemas/basicDocumentFileType';
import {
  ENABLE_DOCX_TO_PDF,
  enableDocxEditor,
  isFeatureEnabled,
} from '../core/constant/featureFlags';
import { DefaultFilename } from '../core/constant/filename';
import { type BlockMetadata, blockMetadata } from './block-metadata';
import {
  type BlockAlias,
  type BlockName,
  BlockRegistry,
  type ConcreteBlockName,
  type FileTypeString,
  type MimeType,
} from './block-registry';

const metadataEntries = Object.entries(blockMetadata) as Array<
  [ConcreteBlockName, BlockMetadata]
>;
const blockNames = new Set<string>(BlockRegistry);

export const blockAcceptedMimetypeToFileExtension: Record<
  MimeType,
  FileTypeString
> = {};
export const blockAcceptedFileExtensionToMimeType: Record<
  FileTypeString,
  MimeType
> = {};
export const blockAcceptedFileExtensionSet = new Set<string>();

const fileTypeToBlockName_: Record<string, BlockName> = {};
const aliasToBlockName_: Partial<Record<string, BlockName>> = {};
const blockNameToDefaultFilename: Partial<
  Record<BlockName | BlockAlias, string>
> = {};

// Virtual names have no accepted files, just as with legacy definition discovery.
export const blockNameToFileExtensionSet = Object.fromEntries(
  metadataEntries.map(([name]) => [name, new Set<FileTypeString>()])
) as Record<BlockName, Set<FileTypeString>>;
const blockNameToMimeTypeSet = Object.fromEntries(
  metadataEntries.map(([name]) => [name, new Set<MimeType>()])
) as Record<BlockName, Set<MimeType>>;

for (const [name, metadata] of metadataEntries) {
  for (const alias of metadata.aliases ?? []) {
    aliasToBlockName_[alias.name] = name;
    if (alias.defaultFileName) {
      blockNameToDefaultFilename[alias.name] = alias.defaultFileName;
    }
  }

  for (const [fileExtension, mimeType] of Object.entries(metadata.accepted)) {
    fileTypeToBlockName_[fileExtension] = name;
    blockAcceptedFileExtensionSet.add(fileExtension);
    blockNameToFileExtensionSet[name].add(fileExtension);
    if (!mimeType) continue;
    // Preserve first-match MIME/extension precedence from legacy discovery.
    blockAcceptedMimetypeToFileExtension[mimeType] ??= fileExtension;
    blockNameToMimeTypeSet[name].add(mimeType);
    blockAcceptedFileExtensionToMimeType[fileExtension] ??= mimeType;
  }
  if (metadata.defaultFilename) {
    blockNameToDefaultFilename[name] = metadata.defaultFilename;
  }
}

export function blockAcceptsFileExtension(
  blockName: BlockName,
  fileExtension: string
) {
  return blockNameToFileExtensionSet[blockName].has(fileExtension);
}

export const blockNameToFileExtensions = Object.fromEntries(
  Object.entries(blockNameToFileExtensionSet).map(([name, extensions]) => [
    name,
    Array.from(extensions),
  ])
) as Record<BlockName, string[]>;

export const blockNameToMimeTypes = Object.fromEntries(
  Object.entries(blockNameToMimeTypeSet).map(([name, mimeTypes]) => [
    name,
    Array.from(mimeTypes),
  ])
) as Record<BlockName, string[]>;

export function isBlockAlias(name: string): name is BlockAlias {
  return name in aliasToBlockName_;
}

/** Return an alias's base renderer, or the original renderer name. */
export function resolveBlockAlias(name: BlockName | BlockAlias): BlockName {
  return aliasToBlockName_[name] || (name as BlockName);
}

/** Map entity types, file extensions, and aliases to their renderer or icon. */
export function fileTypeToBlockName(
  blockOrFiletype?: string | null,
  icon?: boolean
): BlockName | BlockAlias {
  if (!blockOrFiletype) return 'unknown';
  if (blockOrFiletype === 'channel_message') return 'channel';
  if (blockOrFiletype === 'agent_session') return 'agent';
  if (blockOrFiletype === 'calendar_event') return 'calendar';
  if (blockOrFiletype === 'automation') return 'routine';
  if (blockOrFiletype === 'crm_company') return 'company';
  if (blockOrFiletype === 'crm_contact') return 'contact';

  if (blockOrFiletype === 'docx' || blockOrFiletype === 'write') {
    if (isFeatureEnabled(enableDocxEditor)) return 'write';
    if (ENABLE_DOCX_TO_PDF) return icon ? 'write' : 'pdf';
  }
  if (isBlockAlias(blockOrFiletype)) return blockOrFiletype;
  if (blockNames.has(blockOrFiletype)) return blockOrFiletype as BlockName;
  return fileTypeToBlockName_[blockOrFiletype] ?? 'unknown';
}

export function fileTypeToResolvedBlockName(
  blockOrFiletype?: string | null
): BlockName {
  return resolveBlockAlias(fileTypeToBlockName(blockOrFiletype));
}

/** Default display name for an unnamed file or entity. */
export function blockNameToDefaultFile(block?: BlockName | string | null) {
  if (!block) return DefaultFilename;
  return (
    blockNameToDefaultFilename[block as BlockName | BlockAlias] ||
    DefaultFilename
  );
}

export type ItemLike = {
  type:
    | ItemType
    | 'agent'
    | 'initiative'
    | 'call'
    | 'crm_company'
    | 'reminder'
    | 'calendar_event'
    | 'database';
  fileType?: BasicDocumentFileType;
  subType?: SubType | BasicDocumentSubTypeProperty;
  name?: string;
  /** A reminder borrows its referenced entity's icon. */
  referencedEntity?: { type: string; fileType?: string; subType?: string };
};

export function itemToBlockName(
  item: ItemLike,
  icon?: boolean
): BlockName | BlockAlias {
  const subTypeName =
    item.subType && 'type' in item.subType
      ? (item.subType.type as string)
      : undefined;
  if (subTypeName && isBlockAlias(subTypeName)) return subTypeName;
  if (item.fileType) return fileTypeToBlockName(item.fileType, icon);
  if (item.type === 'agent_session') return 'agent';
  if (item.type === 'channel_thread') return 'channel';
  if (item.type === 'reminder') {
    const referenced = item.referencedEntity;
    if (referenced?.subType && isBlockAlias(referenced.subType))
      return referenced.subType;
    return fileTypeToBlockName(referenced?.fileType ?? referenced?.type, icon);
  }
  return fileTypeToBlockName(item.type, icon);
}

export function itemToResolvedBlockName(item: ItemLike) {
  return resolveBlockAlias(itemToBlockName(item));
}

export function itemToSafeName(item: ItemLike): string {
  if (typeof item.name === 'string' && item.name.length > 0) return item.name;
  return blockNameToDefaultFile(itemToBlockName(item) || 'unknown');
}

/** Validate compatibility names without consulting mounted renderer definitions. */
export function verifyBlockName(
  name: string | undefined
): BlockName | BlockAlias {
  if (!name) return 'unknown';
  if (name === 'automation') return 'routine';
  if (name === 'write') {
    if (isFeatureEnabled(enableDocxEditor)) return 'write';
    if (ENABLE_DOCX_TO_PDF) return 'pdf';
  }
  if (isBlockAlias(name)) return name;
  if (name in blockMetadata) return name as ConcreteBlockName;
  return 'unknown';
}
