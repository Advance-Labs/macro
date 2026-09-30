import { describe, expect, it, vi } from 'vitest';
import { saveQuestion } from './saved-question';

describe('saving a document question', () => {
  it('saves new SQL as a new query and stores only its id', async () => {
    const save = vi.fn(async () => 'new-query');
    const question = await saveQuestion({
      definition: {
        databaseId: 'db',
        sql: 'SELECT COUNT(*) FROM "Projects"',
        prompt: 'How many projects?',
        title: 'Projects',
        displayMode: 'scalar',
      },
      save,
    });
    expect(save).toHaveBeenCalledExactlyOnceWith({
      sql: 'SELECT COUNT(*) FROM "Projects"',
      databaseId: 'db',
    });
    expect(question).toEqual({
      queryId: 'new-query',
      databaseId: 'db',
      prompt: 'How many projects?',
      title: 'Projects',
      displayMode: 'scalar',
    });
  });

  it('repoints an edited answer at a new query because saved queries are immutable', async () => {
    const save = vi.fn(async () => 'edited-query');
    const question = await saveQuestion({
      definition: {
        databaseId: 'db',
        sql: 'SELECT COUNT(*) FROM "Projects" WHERE "Status" = \'Open\'',
        prompt: 'How many open projects?',
        displayMode: 'scalar',
      },
      previous: {
        queryId: 'original-query',
        sql: 'SELECT COUNT(*) FROM "Projects"',
        databaseId: 'db',
      },
      save,
    });
    expect(save).toHaveBeenCalledOnce();
    expect(question.queryId).toBe('edited-query');
  });

  it('keeps the saved query when only the presentation changes', async () => {
    const save = vi.fn(async () => 'unused');
    const question = await saveQuestion({
      definition: {
        databaseId: 'db',
        sql: 'SELECT "Status", COUNT(*) FROM "Projects" GROUP BY "Status"',
        prompt: 'Projects by status',
        displayMode: 'bar',
        chart: { x: 'Status', y: ['COUNT(*)'] },
      },
      previous: {
        queryId: 'original-query',
        sql: 'SELECT "Status", COUNT(*) FROM "Projects" GROUP BY "Status"',
        databaseId: 'db',
      },
      save,
    });
    expect(save).not.toHaveBeenCalled();
    expect(question.queryId).toBe('original-query');
    expect(question.displayMode).toBe('bar');
  });

  it('saves a new query when the same SQL moves to another database', async () => {
    const save = vi.fn(async () => 'moved-query');
    const question = await saveQuestion({
      definition: {
        databaseId: 'other',
        sql: 'SELECT 1',
        prompt: 'One',
        displayMode: 'scalar',
      },
      previous: {
        queryId: 'original-query',
        sql: 'SELECT 1',
        databaseId: 'db',
      },
      save,
    });
    expect(save).toHaveBeenCalledExactlyOnceWith({
      sql: 'SELECT 1',
      databaseId: 'other',
    });
    expect(question.queryId).toBe('moved-query');
  });
});
