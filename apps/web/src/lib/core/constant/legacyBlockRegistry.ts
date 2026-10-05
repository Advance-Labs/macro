import type { ConcreteBlockName } from '../../constants/block-registry';

// Only these renderers still require legacy definitions. Remove entries as hosts migrate;
// do not derive this list from the application's supported names or metadata.
export const LegacyBlockRegistry = [
  'agent',
  'routine',
  'calendar',
  'call',
  'canvas',
  'channel',
  'chat',
  'code',
  'company',
  'contact',
  'database',
  'email',
  'fig',
  'initiative',
  'md',
  'pdf',
  'pptx',
  'pr',
  'project',
  'spreadsheet',
  'unknown',
  'video',
  'write',
] as const satisfies readonly ConcreteBlockName[];
