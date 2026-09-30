import type { QueryDefinition, SavedQuestion } from '../core/query';

export type SaveQuestionSql = (input: {
  sql: string;
  databaseId?: string;
}) => Promise<string>;

/** Save a new question's SQL as a query and keep only its id with the presentation. */
export async function saveQuestion(input: {
  definition: QueryDefinition;
  save: SaveQuestionSql;
}): Promise<SavedQuestion> {
  const { sql, ...presentation } = input.definition;
  const queryId = await input.save({
    sql,
    ...(input.definition.databaseId
      ? { databaseId: input.definition.databaseId }
      : {}),
  });
  return { ...presentation, queryId };
}
