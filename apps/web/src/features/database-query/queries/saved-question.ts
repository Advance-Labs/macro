import type { QueryDefinition, SavedQuestion } from '../core/query';

export type SaveQuestionSql = (input: {
  sql: string;
  databaseId?: string;
}) => Promise<string>;

/**
 * Turn an edited definition into what the document stores. Saved queries are
 * immutable, so changed SQL or a changed source becomes a new saved query;
 * presentation-only edits keep pointing at the current one.
 */
export async function saveQuestion(input: {
  definition: QueryDefinition;
  previous?: { queryId: string; sql: string; databaseId?: string };
  save: SaveQuestionSql;
}): Promise<SavedQuestion> {
  const { sql, ...presentation } = input.definition;
  const reusable =
    !!input.previous?.queryId &&
    input.previous.sql === sql &&
    input.previous.databaseId === input.definition.databaseId;
  const queryId =
    reusable && input.previous
      ? input.previous.queryId
      : await input.save({
          sql,
          ...(input.definition.databaseId
            ? { databaseId: input.definition.databaseId }
            : {}),
        });
  return { ...presentation, queryId };
}
