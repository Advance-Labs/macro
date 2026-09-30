import { useDatabaseQueryDefinition } from '@queries/storage/database-queries';
import {
  useDatabaseDetailQuery,
  useDatabasesQuery,
} from '@queries/storage/databases';
import type { DatabaseDetail } from '@service-storage/databases';
import { createSignal, For, type JSX, Match, Show, Switch } from 'solid-js';
import { AppAnswerDisplay } from './answer-display';
import { QueryDatabasePicker } from './components/query-database-picker';
import type { QueryCapabilities } from './context/query-context';
import {
  type QueryAnswer,
  type QueryDefinition,
  type QuerySchema,
  queryErrorMessage,
  type SavedQuestion,
} from './core/query';
import {
  queryCapabilities,
  runSavedQuery,
  saveQuestionSql,
  subscribeToQueryChanges,
  trackQueryDatabase,
} from './queries/app-query-source';
import { createLiveQuerySource, toQuerySchema } from './queries/query-source';
import { saveQuestion } from './queries/saved-question';
import { LiveQuestion } from './views/live-question';
import { QueryEditor } from './views/query-editor';

export function DatabaseQuestionPanel(props: {
  detail: DatabaseDetail;
  activeTableId?: string;
  initial?: QueryDefinition;
  onSave?: (definition: QueryDefinition, answer: QueryAnswer) => void;
  saveLabel?: string;
  saveHint?: string;
  sourcePicker?: JSX.Element;
  autoFocus?: boolean;
  capabilities?: QueryCapabilities;
  promptPlaceholder?: string;
}) {
  return (
    <AppAnswerDisplay>
      <QueryEditor
        autoFocus={props.autoFocus}
        schema={toQuerySchema(props.detail, props.activeTableId)}
        initial={
          props.initial ?? {
            databaseId: props.detail.database.id,
            sql: '',
            prompt: '',
            displayMode: 'scalar',
          }
        }
        capabilities={props.capabilities ?? queryCapabilities}
        promptPlaceholder={props.promptPlaceholder}
        onSave={props.onSave}
        saveLabel={props.saveLabel}
        saveHint={props.saveHint}
        sourcePicker={props.sourcePicker}
      />
    </AppAnswerDisplay>
  );
}

/** Source selection for a question inserted from the document slash menu. */
export function ChooseQuestionSource(props: {
  initial: QueryDefinition;
  onSave: (definition: QueryDefinition) => void;
}) {
  const databases = useDatabasesQuery();
  const [databaseId, setDatabaseId] = createSignal(props.initial.databaseId);
  const availableDatabases = () =>
    !databases.isPending ? (databases.data ?? []) : [];
  const detail = useDatabaseDetailQuery(databaseId);
  const loadedDetail = () => {
    if (detail.isPending || !databaseId()) return undefined;
    const current = detail.data;
    return current?.database.id === databaseId() ? current : undefined;
  };
  const schema = (): QuerySchema => {
    const current = loadedDetail();
    return current
      ? toQuerySchema(current)
      : {
          databaseId: databaseId(),
          name: databaseId()
            ? (availableDatabases().find(
                (entry) => entry.database.id === databaseId()
              )?.database.name ?? 'Database unavailable')
            : 'Automatic',
          tables: [],
        };
  };
  return (
    <AppAnswerDisplay>
      <QueryEditor
        autoFocus
        initial={{ ...props.initial, tableId: undefined }}
        schema={schema()}
        capabilities={queryCapabilities}
        sourceAvailable={!databaseId() || !!loadedDetail()}
        sourcePicker={(resolvedSchema) => (
          <QueryDatabasePicker
            databases={availableDatabases().map(({ database }) => ({
              id: database.id,
              name: database.name,
            }))}
            value={databaseId()}
            resolvedName={
              resolvedSchema().databaseId ? resolvedSchema().name : undefined
            }
            loading={databases.isPending}
            onChange={setDatabaseId}
          />
        )}
        onSave={props.onSave}
        saveLabel={props.initial.sql ? 'Save changes' : 'Insert'}
      />
      <Show when={databaseId() && detail.isError}>
        <p role="alert" class="px-4 pb-3 text-sm text-failure-ink">
          This database could not be opened. Choose Automatic or another source.
        </p>
      </Show>
      <Show when={databaseId() && detail.isPending}>
        <p role="status" class="px-4 pb-3 text-sm text-ink-muted">
          Loading database…
        </p>
      </Show>
    </AppAnswerDisplay>
  );
}

function SavedQuestionSql(props: { queryId: string }) {
  const definition = useDatabaseQueryDefinition(() => props.queryId);
  return (
    <Switch fallback="Loading SQL…">
      <Match when={definition.isSuccess && definition.data}>
        {(saved) => saved().definition.query}
      </Match>
      <Match when={definition.isError}>SQL unavailable</Match>
    </Switch>
  );
}

/** Loads the saved SQL, then saves any edit as a new immutable query. */
function EditSavedQuestion(props: {
  source: SavedQuestion;
  onSave: (source: SavedQuestion) => void;
}) {
  const definition = useDatabaseQueryDefinition(() => props.source.queryId);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const savedSql = () =>
    definition.isSuccess ? definition.data.definition.query : '';
  const save = async (next: QueryDefinition) => {
    if (saving()) return;
    setSaving(true);
    setError();
    try {
      props.onSave(
        await saveQuestion({
          definition: next,
          previous: props.source.queryId
            ? {
                queryId: props.source.queryId,
                sql: savedSql(),
                databaseId: props.source.databaseId,
              }
            : undefined,
          save: saveQuestionSql,
        })
      );
    } catch (failure) {
      setError(queryErrorMessage(failure));
    } finally {
      setSaving(false);
    }
  };
  return (
    <Switch
      fallback={
        <p role="status" class="p-4 text-sm text-ink-muted">
          Loading question…
        </p>
      }
    >
      <Match when={props.source.queryId && definition.isError}>
        <p role="alert" class="p-4 text-sm text-failure-ink">
          {queryErrorMessage(definition.error)}
        </p>
      </Match>
      <Match when={!props.source.queryId || definition.isSuccess}>
        <ChooseQuestionSource
          initial={{ ...props.source, sql: savedSql() }}
          onSave={(next) => void save(next)}
        />
        <Show when={saving()}>
          <p role="status" class="px-4 pb-3 text-sm text-ink-muted">
            Saving question…
          </p>
        </Show>
        <Show when={error()}>
          <p role="alert" class="px-4 pb-3 text-sm text-failure-ink">
            {error()}
          </p>
        </Show>
      </Match>
    </Switch>
  );
}

/** Production wiring is loaded only when a document query enters the viewport. */
export function DatabaseLiveQuestion(props: {
  source: SavedQuestion;
  onSave?: (source: SavedQuestion) => void;
}) {
  const query = createLiveQuerySource({
    queryId: () => props.source.queryId,
    run: runSavedQuery,
    subscribe: subscribeToQueryChanges,
  });
  const trackingIds = () =>
    Array.from(
      new Set([
        ...(props.source.databaseId ? [props.source.databaseId] : []),
        ...(!query.isPending ? (query.data?.read_database_ids ?? []) : []),
      ])
    );
  return (
    <AppAnswerDisplay>
      <For each={trackingIds()}>
        {(id) => {
          trackQueryDatabase(id, () => {
            if (props.source.queryId) void query.refetch();
          });
          return null;
        }}
      </For>
      <LiveQuestion
        source={props.source}
        answer={query.isSuccess ? query.data : undefined}
        loading={query.isFetching}
        error={query.isError ? query.error : undefined}
        onRefresh={() => void query.refetch()}
        onRename={
          props.onSave
            ? (title) => props.onSave?.({ ...props.source, title })
            : undefined
        }
        sql={() => <SavedQuestionSql queryId={props.source.queryId} />}
        editor={
          props.onSave
            ? (onClose) => (
                <EditSavedQuestion
                  source={props.source}
                  onSave={(source) => {
                    props.onSave?.(source);
                    onClose();
                  }}
                />
              )
            : undefined
        }
      />
    </AppAnswerDisplay>
  );
}
