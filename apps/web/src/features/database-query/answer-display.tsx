import {
  DatabaseMentionValue,
  DatabaseTextValue,
} from '@app/features/block-database/database-mentions';
import type { JSX } from 'solid-js';
import {
  type AnswerDisplay,
  AnswerDisplayProvider,
} from './context/answer-display';
import { answerNames } from './queries/answer-names';

/** Answers drawn with the database grid's names, mentions and text. */
const appAnswerDisplay: AnswerDisplay = {
  names: answerNames,
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
