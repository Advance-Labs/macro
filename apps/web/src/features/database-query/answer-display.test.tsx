import { render } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import { AppAnswerDisplay } from './answer-display';
import { QueryResults } from './components/query-results';
import type { QueryAnswer } from './core/query';

vi.mock('@property/hooks/usePropertyEntityDisplay', () => ({
  usePropertyEntityDisplay: (id: () => string, type: () => string) => ({
    name: () => (type() === 'USER' ? 'Ada Lovelace' : 'Launch plan'),
    icon: () => <span data-testid={`${type()}-${id()}-icon`} />,
  }),
}));
vi.mock('@core/component/UserIcon', () => ({
  UserIcon: (props: { id: string }) => (
    <span data-testid="person-icon" data-id={props.id} />
  ),
}));
vi.mock(
  '@core/component/LexicalMarkdown/component/menu/MentionsMenu/MentionsMenu',
  () => ({ MentionsMenu: () => null })
);
vi.mock(
  '@core/component/LexicalMarkdown/component/menu/MentionsMenu/utils/entityUtils',
  () => ({ getBlockNameFromEntity: () => 'md' })
);
vi.mock(
  '@core/component/LexicalMarkdown/component/core/StaticMarkdown',
  () => ({
    StaticMarkdown: (props: { markdown: string }) => (
      <span data-testid="markdown">{props.markdown}</span>
    ),
  })
);
vi.mock('./queries/answer-names', () => ({
  answerNames: () => () => () => undefined,
}));

describe('answers in the app', () => {
  it('draws people and documents with the database grid’s mentions', () => {
    const answer: QueryAnswer = {
      columns: [
        {
          name: 'Host',
          kind: 'entity',
          source: {
            markdown: false,
            options: [],
            tag: false,
            target: 'USER',
            relatedTable: null,
          },
        },
        {
          name: 'Plan',
          kind: 'entity',
          source: {
            markdown: false,
            options: [],
            tag: false,
            target: 'DOCUMENT',
            relatedTable: null,
          },
        },
      ],
      rows: [
        [
          { type: 'entities', value: ['macro|ada@macro.com'] },
          { type: 'entities', value: ['doc-1'] },
        ],
      ],
      rowIds: [],
      readTables: [],
      readDatabaseIds: [],
      truncatedTables: [],
    };
    const result = render(() => (
      <AppAnswerDisplay>
        <QueryResults answer={answer} displayMode="table" />
      </AppAnswerDisplay>
    ));
    const cells = Array.from(result.getByRole('table').querySelectorAll('td'));
    expect(cells.map((cell) => cell.textContent)).toEqual([
      'Ada Lovelace',
      'Launch plan',
    ]);
    expect(
      cells[0]
        .querySelector('[data-testid="person-icon"]')
        ?.getAttribute('data-id')
    ).toBe('macro|ada@macro.com');
    expect(
      cells[1].querySelector('[data-testid="DOCUMENT-doc-1-icon"]')
    ).toBeTruthy();
    result.unmount();
  });
});
