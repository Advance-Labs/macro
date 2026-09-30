import {
  DatabaseMentionValue,
  DatabaseTextValue,
} from '@app/features/block-database/database-mentions';
import type { JSX } from 'solid-js';
import {
  type AnswerDisplay,
  AnswerDisplayProvider,
} from './context/answer-display';
import { answerResultColumns } from './queries/answer-columns';

/** Answers drawn with the database grid's schema, mentions and text. */
export const appAnswerDisplay: AnswerDisplay = {
  columns: answerResultColumns,
  mention: (id, entityType) => (
    <DatabaseMentionValue id={id} entityType={entityType} />
  ),
  text: (markdown) => <DatabaseTextValue value={markdown} />,
};

export function AppAnswerDisplay(props: { children: JSX.Element }) {
  return (
    <AnswerDisplayProvider value={appAnswerDisplay}>
      {props.children}
    </AnswerDisplayProvider>
  );
}
