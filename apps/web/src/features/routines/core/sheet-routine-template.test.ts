import { describe, expect, it } from 'vitest';
import { parseRoutineSeed } from './draft';
import { validateRoutineDraft } from './routine-draft';
import { sheetRoutineSeed } from './sheet-routine-template';

const sheet = {
  documentId: '01a113c4-d644-7b84-a3ac-08d2643e60d4',
  documentName: 'Billing',
  sheetId: 'sheet1',
  sheetName: 'Invoices',
  range: 'B2:D9',
};

describe('sheet routine template', () => {
  it('targets the sheet and selection through a spreadsheet mention', () => {
    const seed = sheetRoutineSeed(sheet);
    expect(seed.name).toBe('Update Billing');
    const mention = seed.prompt.match(
      /<m-document-mention>(.*?)<\/m-document-mention>/
    );
    expect(JSON.parse(mention?.[1] ?? '{}')).toMatchObject({
      documentId: sheet.documentId,
      documentName: 'Billing',
      blockName: 'spreadsheet',
      blockParams: { sheetId: 'sheet1', sheetName: 'Invoices', range: 'B2:D9' },
    });
    expect(seed.prompt).toContain('"Invoices" sheet around B2:D9');
  });

  it('starts with a weekly schedule that can be created as is', () => {
    const seed = sheetRoutineSeed(sheet);
    expect(seed.triggers).toEqual([
      expect.objectContaining({
        kind: 'schedule',
        frequency: 'week',
        time: '09:00',
        daysOfWeek: ['2'],
      }),
    ]);
    expect(
      validateRoutineDraft(
        {
          ...seed,
          frequency: 'week',
          time: '09:00',
          daysOfWeek: ['2'],
          dayOfMonth: '1',
          target: { kind: 'model', model: 'claude-sonnet-4-6' },
        },
        true
      )
    ).toBeNull();
  });

  it('names an untitled workbook generically', () => {
    expect(sheetRoutineSeed({ ...sheet, documentName: ' ' }).name).toBe(
      'Update spreadsheet'
    );
  });

  it('passes the split-param check only with a complete seed', () => {
    const seed = sheetRoutineSeed(sheet);
    expect(parseRoutineSeed(seed)).toEqual(seed);
    expect(parseRoutineSeed({ name: 'Update Billing' })).toBeUndefined();
    expect(parseRoutineSeed([seed])).toBeUndefined();
    expect(
      parseRoutineSeed({ ...seed, triggers: [{ kind: 'schedule' }] })
    ).toBeUndefined();
  });
});
