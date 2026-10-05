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
  // PowerPoint presentations, edited in the browser.
  'pptx',
  // Figma files, viewed in the browser.
  'fig',
  'channel',
  'project',
  'unknown',
  'video',
  'email',
  'contact',
  'company',
  'routine',
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

/** Compatibility-only names without a concrete renderer. */
export const VirtualBlockRegistry = [] as const;
export type ConcreteBlockName = Exclude<
  BlockName,
  (typeof VirtualBlockRegistry)[number]
>;
const virtualBlockNames = new Set<string>(VirtualBlockRegistry);
export const ConcreteBlockRegistry = BlockRegistry.filter(
  (name): name is ConcreteBlockName => !virtualBlockNames.has(name)
);
