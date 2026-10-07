import { buildMentionMarkdownString } from '@macro-inc/lexical-core/utils/mentions';
import type { RoutineSeed } from './draft';
import { newScheduleTrigger } from './routine-triggers';

export type SheetRoutineTarget = {
  documentId: string;
  documentName: string;
  sheetId: string;
  sheetName: string;
  range: string;
};

/** Starter routine that keeps a sheet up to date: placeholders mark what the user fills in. */
export function sheetRoutineSeed(sheet: SheetRoutineTarget): RoutineSeed {
  const mention = buildMentionMarkdownString({
    type: 'document',
    documentId: sheet.documentId,
    documentName: sheet.documentName,
    blockName: 'spreadsheet',
    blockParams: {
      sheetId: sheet.sheetId,
      sheetName: sheet.sheetName,
      range: sheet.range,
    },
  });
  const title = sheet.documentName.trim() || 'spreadsheet';
  return {
    name: `Update ${title}`,
    prompt: [
      `Keep ${mention} up to date.`,
      `1. Read the "${sheet.sheetName}" sheet around ${sheet.range}, including its headers, to learn the layout.`,
      '2. Gather the latest data from [emails, channels, documents, or a connected tool such as Brex].',
      '3. Write it into [the cells, columns, or new rows to update]. Append new rows rather than overwriting history, and keep existing formulas and formatting.',
      '4. Send a short summary of what changed to [a person or channel], or delete this step.',
    ].join('\n\n'),
    triggers: [newScheduleTrigger('week')],
  };
}
