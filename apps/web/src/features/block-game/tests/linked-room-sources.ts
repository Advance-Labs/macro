import { LoroDoc } from 'loro-crdt';
import { createSignal } from 'solid-js';
import type { GamePresence, GameRoomSource } from '../context/game-room-source';

/**
 * Two clients of one room: document updates are relayed between two LoroDocs
 * the way the sync service would, and each sees the other's presence.
 */
export function createLinkedRoomSources(users: readonly [string, string]) {
  const docs = [new LoroDoc(), new LoroDoc()] as const;
  docs[0].subscribeLocalUpdates((update) => docs[1].import(update));
  docs[1].subscribeLocalUpdates((update) => docs[0].import(update));
  const presences = [
    createSignal<GamePresence>(),
    createSignal<GamePresence>(),
  ] as const;

  const source = (index: 0 | 1): GameRoomSource => {
    const other = index === 0 ? 1 : 0;
    return {
      doc: () => docs[index],
      ready: () => true,
      error: () => undefined,
      status: () => 'connected',
      peers: () => {
        const presence = presences[other][0]();
        return presence
          ? [{ userId: users[other], color: 'currentColor', presence }]
          : [];
      },
      setPresence: (next) => presences[index][1](() => next),
    };
  };

  return {
    sources: [source(0), source(1)] as const,
    docs,
    presence: (index: 0 | 1) => presences[index][0](),
  };
}
