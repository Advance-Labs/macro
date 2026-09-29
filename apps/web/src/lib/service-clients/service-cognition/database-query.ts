import { DEFAULT_MODEL } from '@core/component/AI/constant';
import { parseQueryProposal } from '../../../features/database-query/core/query';
import { cognitionApiServiceClient } from './client';
import {
  type DatabaseAssistantInput,
  databaseCompletionRequest,
} from './database-query-prompt';

/**
 * Propose a read-only answer query from the supplied schema.
 *
 * TODO(databases): the fable-yolo branch also has `runDatabaseAssistant`, a
 * write-capable completion whose server-authored tool receipts become an
 * `actionSummary`, used by the database page's AI. It needs the databases AI
 * toolset, which is not on this branch yet.
 */
export async function generateDatabaseQuery(input: DatabaseAssistantInput) {
  const result = await cognitionApiServiceClient.structuredCompletion({
    model: DEFAULT_MODEL,
    ...databaseCompletionRequest(input),
  });
  if (result.isErr())
    throw new Error(
      result.error[0]?.message ?? 'AI could not answer. Please try again.'
    );
  return parseQueryProposal(result.value.result);
}
