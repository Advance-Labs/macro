import { DATABASE_MODEL } from '@core/component/AI/constant';
import { ResultAsync } from 'neverthrow';
import { cognitionApiServiceClient } from './client';
import {
  type DatabaseQuestionInput,
  databaseCompletionRequest,
} from './database-query-prompt';

/** A document question's structured answer, from read-only discovery tools. */
export function generateDatabaseQuery(input: DatabaseQuestionInput) {
  return new ResultAsync(
    cognitionApiServiceClient.structuredCompletion({
      model: DATABASE_MODEL,
      ...databaseCompletionRequest(input),
    })
  ).map((response) => response.result);
}
