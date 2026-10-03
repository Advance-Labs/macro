import type { ConcreteBlockName } from '../../constants/block-registry';

// Only these renderers still require legacy definitions. Remove entries as hosts migrate;
// do not derive this list from the application's supported names or metadata.
export const LegacyBlockRegistry = [
  'agent',
  'automation',
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
  'image',
  'initiative',
  'md',
  'pdf',
  'pr',
  'project',
  'spreadsheet',
  'unknown',
  'video',
] as const satisfies readonly ConcreteBlockName[];
