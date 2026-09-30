import { describe, expect, it } from 'vitest';
import { resultCell, resultCellText } from './answer-cell';

describe('result cells without a known column', () => {
  it('shows a date-only value as the grid does, without time or ISO text', () => {
    expect(
      resultCell('2025-12-31T00:00:00+00:00', {
        name: 'Date',
        entity_type: null,
      })
    ).toEqual({ kind: 'text', text: 'Dec 31, 2025' });
    expect(
      resultCell('2025-07-19', { name: 'Day', entity_type: null })
    ).toEqual({ kind: 'text', text: 'Jul 19, 2025' });
  });

  it('keeps the time of a value that has one', () => {
    expect(
      resultCell('2025-12-31T15:30:00Z', { name: 'At', entity_type: null })
    ).toEqual({
      kind: 'text',
      text: `Dec 31, 2025, ${new Date('2025-12-31T15:30:00Z').toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', hour12: true })}`,
    });
  });

  it('formats numbers and leaves other text alone', () => {
    expect(resultCell(1234.5, { name: 'Total', entity_type: null })).toEqual({
      kind: 'text',
      text: '1,234.5',
    });
    expect(
      resultCell('Skyline Rooftop', { name: 'Location', entity_type: null })
    ).toEqual({ kind: 'text', text: 'Skyline Rooftop' });
    expect(resultCell('2025', { name: 'Year', entity_type: null })).toEqual({
      kind: 'text',
      text: '2025',
    });
  });

  it('shows an empty value as empty', () => {
    expect(resultCell(null, { name: 'Date', entity_type: null })).toEqual({
      kind: 'empty',
    });
    expect(resultCell('', { name: 'Name', entity_type: null })).toEqual({
      kind: 'empty',
    });
  });

  it('turns entity ids into mentions of the engine’s entity type', () => {
    expect(resultCell('usr_1', { name: 'Host', entity_type: 'user' })).toEqual({
      kind: 'mentions',
      entityType: 'USER',
      ids: ['usr_1'],
    });
    expect(
      resultCell('["doc_1","doc_2"]', {
        name: 'Documents',
        entity_type: 'document',
      })
    ).toEqual({
      kind: 'mentions',
      entityType: 'DOCUMENT',
      ids: ['doc_1', 'doc_2'],
    });
    expect(
      resultCell('thread_1', { name: 'Email', entity_type: 'email_thread' })
    ).toEqual({ kind: 'mentions', entityType: 'THREAD', ids: ['thread_1'] });
  });

  it('names linked rows instead of printing their ids', () => {
    expect(
      resultCell('01a0ef5c-0000-7000-8000-000000000000', {
        name: 'Party',
        entity_type: 'database_row',
      })
    ).toEqual({ kind: 'text', text: 'Linked record' });
  });
});

describe('result cells from a known database column', () => {
  const column = {
    id: 'column',
    name: 'Column',
    isMultiSelect: false,
    options: [],
    writable: true,
  };

  it('formats a date column as the grid does', () => {
    expect(
      resultCell(
        '2025-12-31T00:00:00+00:00',
        { name: 'Date', entity_type: null },
        { ...column, dataType: 'DATE' }
      )
    ).toEqual({ kind: 'text', text: 'Dec 31, 2025' });
  });

  it('shows a checkbox for a boolean column', () => {
    const boolean = { ...column, dataType: 'BOOLEAN' };
    expect(
      resultCell(1, { name: 'Plus One', entity_type: null }, boolean)
    ).toEqual({ kind: 'boolean', checked: true });
    expect(
      resultCell(0, { name: 'Plus One', entity_type: null }, boolean)
    ).toEqual({ kind: 'boolean', checked: false });
  });

  it('shows select values as options, one per chosen label', () => {
    expect(
      resultCell(
        'Maybe',
        { name: 'RSVP', entity_type: null },
        { ...column, dataType: 'SELECT_STRING' }
      )
    ).toEqual({ kind: 'options', labels: ['Maybe'] });
    expect(
      resultCell(
        '["Vegan","Nut free"]',
        { name: 'Dietary', entity_type: null },
        { ...column, dataType: 'SELECT_STRING', isMultiSelect: true }
      )
    ).toEqual({ kind: 'options', labels: ['Vegan', 'Nut free'] });
  });

  it('renders text columns as their markdown, so mentions and links work', () => {
    expect(
      resultCell(
        'See [the plan](https://macro.com)',
        { name: 'Notes', entity_type: null },
        { ...column, dataType: 'STRING' }
      )
    ).toEqual({
      kind: 'markdown',
      markdown: 'See [the plan](https://macro.com)',
    });
  });

  it('uses the column’s own entity type, so tasks are tasks', () => {
    expect(
      resultCell(
        'task_1',
        { name: 'Task', entity_type: 'document' },
        { ...column, dataType: 'ENTITY', specificEntityType: 'TASK' }
      )
    ).toEqual({ kind: 'mentions', entityType: 'TASK', ids: ['task_1'] });
  });

  it('formats numbers as the grid does', () => {
    expect(
      resultCell(
        3,
        { name: 'Extra Guests', entity_type: null },
        { ...column, dataType: 'NUMBER' }
      )
    ).toEqual({ kind: 'text', text: '3' });
  });
});

describe('result cell text', () => {
  it('spells each kind for labels and titles', () => {
    expect(resultCellText({ kind: 'empty' })).toBe('—');
    expect(resultCellText({ kind: 'text', text: 'Dec 31, 2025' })).toBe(
      'Dec 31, 2025'
    );
    expect(resultCellText({ kind: 'markdown', markdown: 'Plain notes' })).toBe(
      'Plain notes'
    );
    expect(resultCellText({ kind: 'boolean', checked: true })).toBe('True');
    expect(
      resultCellText({ kind: 'options', labels: ['Vegan', 'Nut free'] })
    ).toBe('Vegan, Nut free');
    expect(
      resultCellText({ kind: 'mentions', entityType: 'USER', ids: ['a', 'b'] })
    ).toBe('2 people');
    expect(
      resultCellText({ kind: 'mentions', entityType: 'DOCUMENT', ids: ['a'] })
    ).toBe('1 document');
  });
});
