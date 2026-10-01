import { DatabaseMentionPlaceholder } from '@app/features/block-database/components/database-mention-label';
import type { DatabaseEntityType } from '@app/features/block-database/core/column-inference';
import { markdownToPlainText } from '@macro-inc/lexical-core/utils/parsers';
import { type Accessor, createContext, type JSX, useContext } from 'solid-js';
import { type ReferenceNames, unknownNames } from '../core/answer-cell';
import type { QueryAnswer } from '../core/query';

/**
 * What an answer is drawn with. The app supplies the names of the people
 * and rows it references and the database grid's mention and text
 * renderers; without them values keep their plain form.
 */
export type AnswerDisplay = {
  /** Names what an answer references, as far as they are known. */
  names: (
    answer: Accessor<QueryAnswer | undefined>
  ) => Accessor<ReferenceNames>;
  mention: (id: string, entityType: DatabaseEntityType) => JSX.Element;
  text: (markdown: string) => JSX.Element;
};

const plainDisplay: AnswerDisplay = {
  names: () => () => unknownNames,
  mention: (_, entityType) => (
    <DatabaseMentionPlaceholder entityType={entityType} />
  ),
  text: (markdown) => markdownToPlainText(markdown),
};

const AnswerDisplayContext = createContext<AnswerDisplay>(plainDisplay);

export const AnswerDisplayProvider = AnswerDisplayContext.Provider;

export function useAnswerDisplay(): AnswerDisplay {
  return useContext(AnswerDisplayContext);
}

export function useAnswerNames(
  answer: Accessor<QueryAnswer | undefined>
): Accessor<ReferenceNames> {
  return useAnswerDisplay().names(answer);
}
