import type { BlockAlias, ConcreteBlockName, MimeType } from './block-registry';
import { supportedExtensions } from './code-file-types';
import { VIDEO_MIMES } from './video-file-types';

export type BlockMetadata = {
  description: string;
  accepted: Record<string, MimeType>;
  defaultFilename?: string;
  aliases?: Array<{ name: BlockAlias; defaultFileName?: string }>;
};

export const DEFAULT_CHAT_NAME = 'New Chat';

// Preserve the former glob order: MIME-to-extension lookup uses the first match.
// Metadata remains available after a legacy renderer definition is removed.
export const blockMetadata = {
  agent: {
    description: 'View an agent session',
    accepted: {},
  },
  automation: {
    description: 'view and edit a single automation',
    defaultFilename: 'Untitled automation',
    accepted: {},
  },
  calendar: {
    description: 'View calendar events',
    accepted: {},
  },
  call: {
    description: '',
    defaultFilename: 'Call',
    accepted: {},
  },
  canvas: {
    description: 'edit canvas',
    accepted: {
      canvas: 'application/x-macro-canvas',
    },
  },
  channel: {
    description: '',
    accepted: {},
  },
  chat: {
    description: '',
    defaultFilename: DEFAULT_CHAT_NAME,
    accepted: {},
  },
  code: {
    description: 'Edit code files with syntax highlighting and formatting',
    aliases: [
      {
        name: 'csv',
        defaultFileName: 'New CSV',
      },
    ],
    accepted: Object.fromEntries(
      supportedExtensions.map((extension) => [extension, 'text/plain'])
    ),
  },
  company: {
    description: 'View a CRM company',
    accepted: {},
  },
  contact: {
    description: 'View a CRM contact',
    accepted: {},
  },
  database: {
    description: 'View a table',
    accepted: {},
    defaultFilename: 'Untitled database',
  },
  email: {
    description: 'View and manage email threads',
    defaultFilename: '[No subject]',
    accepted: {},
  },
  image: {
    description: 'views images',
    accepted: {
      png: 'image/png',
      jpg: 'image/jpeg',
      jpeg: 'image/jpeg',
      gif: 'image/gif',
      svg: 'image/svg+xml',
      webp: 'image/webp',
    },
  },
  initiative: {
    description: 'View a task project',
    accepted: {},
  },
  md: {
    description: 'write markdown notes',
    defaultFilename: 'New Note',
    aliases: [
      {
        name: 'task',
        defaultFileName: 'New Task',
      },
      {
        name: 'snippet',
        defaultFileName: 'New Snippet',
      },
      {
        name: 'skill',
        defaultFileName: 'New Skill',
      },
    ],
    accepted: {
      md: 'text/markdown',
    },
  },
  pdf: {
    description: 'work with pdf files',
    accepted: {
      pdf: 'application/pdf',
      docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    },
  },
  pr: {
    description: 'View a GitHub pull request',
    accepted: {},
  },
  project: {
    description: 'View individual folders',
    accepted: {},
  },
  spreadsheet: {
    description: 'Calculate, organize, and collaborate in a spreadsheet',
    defaultFilename: 'New Spreadsheet',
    accepted: {
      spreadsheet: 'application/x-macro-spreadsheet',
    },
  },
  unknown: {
    description: 'fallback block for unknown files types',
    accepted: {},
  },
  video: {
    description: 'block for video file types',
    accepted: VIDEO_MIMES,
  },
} satisfies Record<ConcreteBlockName, BlockMetadata>;
