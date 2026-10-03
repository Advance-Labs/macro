/** Entity renderer names accepted by the application, independent of legacy mounts. */
export const BlockRegistry = [
  'call',
  'calendar',
  'chat',
  'database',
  'write',
  'pdf',
  'md',
  'code',
  'image',
  'canvas',
  'spreadsheet',
  'channel',
  'project',
  'unknown',
  'video',
  'email',
  'contact',
  'company',
  'automation',
  'pr',
  'agent',
  // A task project (`project` is a folder).
  'initiative',
] as const;

/** Aliases for block types that share a concrete block implementation. */
export const BlockAliasRegistry = ['csv', 'task', 'snippet', 'skill'] as const;

export type BlockName = (typeof BlockRegistry)[number];
export type BlockAlias = (typeof BlockAliasRegistry)[number];
export type FileTypeString = string & {};
export type MimeType = string & {};

/** Compatibility names that resolve through another renderer. */
export const VirtualBlockRegistry = ['write'] as const;
export type ConcreteBlockName = Exclude<
  BlockName,
  (typeof VirtualBlockRegistry)[number]
>;
const virtualBlockNames = new Set<string>(VirtualBlockRegistry);
export const ConcreteBlockRegistry = BlockRegistry.filter(
  (name): name is ConcreteBlockName => !virtualBlockNames.has(name)
);
