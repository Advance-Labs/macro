import type { DatabaseEntityType } from '@app/features/block-database/core/column-inference';
import { type Accessor, createContext, type JSX, useContext } from 'solid-js';
import type { ReferenceNames } from '../core/answer-cell';
import type { QueryAnswer } from '../core/query';

/** How a cell's mentions and markdown text are drawn. */
export type AnswerRenderers = {
  mention: (id: string, entityType: DatabaseEntityType) => JSX.Element;
  text: (markdown: string) => JSX.Element;
};

/** What an answer is drawn with, and the names of what it references. */
export type AnswerDisplay = AnswerRenderers & {
  /** Names what an answer references, as far as they are known. */
  names: (
    answer: Accessor<QueryAnswer | undefined>
  ) => Accessor<ReferenceNames>;
};

const AnswerDisplayContext = createContext<AnswerDisplay>();

export const AnswerDisplayProvider = AnswerDisplayContext.Provider;

export function useAnswerDisplay(): AnswerDisplay {
  const display = useContext(AnswerDisplayContext);
  if (!display)
    throw new Error(
      'useAnswerDisplay needs an AnswerDisplayProvider, such as AppAnswerDisplay.'
    );
  return display;
}
