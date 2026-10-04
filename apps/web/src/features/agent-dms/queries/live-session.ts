import { AgentSession } from '@core/agent-session/AgentSession';
import type {
  FoldedMessage,
  FoldedStreamEvent,
  SessionMetadata,
} from '@service-agent-fold/generated/types';
import type { AgentAction } from '@service-agent-harness/generated/schemas';
import {
  type Accessor,
  batch,
  createMemo,
  createSignal,
  onCleanup,
} from 'solid-js';

/** One shared session subscription for the open DM; text paints at most 4 Hz. */
export function createDmLiveSession(sessionId: Accessor<string | undefined>) {
  const [messages, setMessages] = createSignal<FoldedMessage[]>([]);
  const [metadata, setMetadata] = createSignal<SessionMetadata>();
  const [failed, setFailed] = createSignal(false);
  const [retry, setRetry] = createSignal(0);
  const live = createMemo(() => {
    retry();
    const id = sessionId();
    batch(() => {
      setMessages([]);
      setMetadata(undefined);
      setFailed(false);
    });
    if (!id) return undefined;
    const session = AgentSession.acquire(id);
    const rows = new Map<string, FoldedMessage>();
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const key = (row: FoldedMessage) => `${row.turn}:${row.author.kind}`;
    const flush = () => {
      timer = undefined;
      if (!disposed) setMessages([...rows.values()]);
    };
    const apply = (events: FoldedStreamEvent[]) => {
      for (const event of events) {
        if (event.kind === 'metadata') setMetadata(event.metadata);
        else if (event.kind === 'replace') {
          rows.clear();
          for (const row of event.messages) rows.set(key(row), row);
        } else rows.set(key(event.message), event.message);
      }
      timer ??= setTimeout(flush, 250);
    };
    const unsubscribe = session.subscribe(apply);
    const load = async () => {
      try {
        await session.load();
        const snapshot = await session.snapshot();
        if (disposed) return;
        apply([
          { kind: 'replace', messages: snapshot.messages },
          { kind: 'metadata', metadata: snapshot.metadata },
        ]);
        if (timer) clearTimeout(timer);
        flush();
      } catch {
        if (!disposed) setFailed(true);
      }
    };
    void load();
    onCleanup(() => {
      disposed = true;
      if (timer) clearTimeout(timer);
      unsubscribe();
      session.release();
    });
    return session;
  });
  return {
    messages,
    metadata,
    failed,
    retry: () => setRetry((value) => value + 1),
    issue: (action: AgentAction) => live()?.issue(action),
  };
}
