import { DatabaseMentionPlaceholder } from '@app/features/block-database/components/database-mention-label';
import type { DatabaseEntityType } from '@app/features/block-database/core/column-inference';
import type { DatabaseViewColumn } from '@app/features/block-database/core/database-view';
import { markdownToPlainText } from '@macro-inc/lexical-core/utils/parsers';
import { type Accessor, createContext, type JSX, useContext } from 'solid-js';
import type { ResultColumn } from '../core/answer-cell';
import type { QueryAnswer } from '../core/query';

/** The database column a result column was read from, when it is known. */
export type ResultColumnLookup = (
  column: ResultColumn
) => DatabaseViewColumn | undefined;

/**
 * What an answer is drawn with. The app supplies the database grid's own
 * schema lookup, mention and text renderers; without them values keep their
 * plain form.
 */
export type AnswerDisplay = {
  /** Resolves an answer's columns against the databases it read. */
  columns: (
    answer: Accessor<QueryAnswer | undefined>
  ) => Accessor<ResultColumnLookup>;
  mention: (id: string, entityType: DatabaseEntityType) => JSX.Element;
  text: (markdown: string) => JSX.Element;
};

const plainDisplay: AnswerDisplay = {
  columns: () => () => () => undefined,
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

export function useResultColumns(
  answer: Accessor<QueryAnswer | undefined>
): Accessor<ResultColumnLookup> {
  return useAnswerDisplay().columns(answer);
}
