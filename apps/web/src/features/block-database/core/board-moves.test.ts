import { describe, expect, it } from 'vitest';
import {
  cardMove,
  laneCards,
  placeCard,
  withMovedCards,
  withPositions,
} from './board-moves';

/** Spells out which bounds a key was asked between. */
const keyBetween = (before: string | null, after: string | null) =>
  `(${before ?? '-'}..${after ?? '-'})`;

const board = {
  lanes: [
    { option: null, hidden: false, cards: [] },
    { option: 'yes', hidden: false, cards: ['ann', 'bob', 'cat'] },
    { option: 'no', hidden: false, cards: ['dan'] },
  ],
};

describe('a card dropped on the board', () => {
  it('lands after the card above it and before the card below it', () => {
    expect(cardMove(board, 'dan', 'yes', 'bob')).toEqual({
      row: 'dan',
      lane: 'yes',
      before: 'ann',
      after: 'bob',
    });
  });

  it('dropped first in a lane has only a card below it', () => {
    expect(cardMove(board, 'dan', 'yes', 'ann')).toEqual({
      row: 'dan',
      lane: 'yes',
      before: null,
      after: 'ann',
    });
  });

  it('dropped at the end of a lane has only a card above it', () => {
    expect(cardMove(board, 'ann', 'no', undefined)).toEqual({
      row: 'ann',
      lane: 'no',
      before: 'dan',
      after: null,
    });
  });

  it('dropped in an empty lane has no neighbours', () => {
    expect(cardMove(board, 'ann', null, undefined)).toEqual({
      row: 'ann',
      lane: null,
      before: null,
      after: null,
    });
  });

  it('moved within its lane skips itself when finding neighbours', () => {
    expect(cardMove(board, 'ann', 'yes', undefined)).toEqual({
      row: 'ann',
      lane: 'yes',
      before: 'cat',
      after: null,
    });
    expect(cardMove(board, 'cat', 'yes', 'bob')).toEqual({
      row: 'cat',
      lane: 'yes',
      before: 'ann',
      after: 'bob',
    });
  });

  it('dropped back where it was is no move', () => {
    expect(cardMove(board, 'bob', 'yes', 'cat')).toBeUndefined();
    expect(cardMove(board, 'cat', 'yes', undefined)).toBeUndefined();
  });
});

describe('a moved card, placed before the server answers', () => {
  it('takes a key between its placed neighbours', () => {
    expect(
      placeCard(
        [
          { row: 'ann', position: 'a0' },
          { row: 'bob', position: 'a1' },
        ],
        { row: 'dan', lane: 'yes', before: 'ann', after: 'bob' },
        keyBetween
      )
    ).toEqual([{ row: 'dan', lane: 'yes', position: '(a0..a1)' }]);
  });

  it('takes a key before the first card, or after the last', () => {
    expect(
      placeCard(
        [{ row: 'ann', position: 'a0' }],
        { row: 'dan', lane: 'yes', before: null, after: 'ann' },
        keyBetween
      )
    ).toEqual([{ row: 'dan', lane: 'yes', position: '(-..a0)' }]);
    expect(
      placeCard(
        [{ row: 'ann', position: 'a0' }],
        { row: 'dan', lane: 'yes', before: 'ann', after: null },
        keyBetween
      )
    ).toEqual([{ row: 'dan', lane: 'yes', position: '(a0..-)' }]);
  });

  it('places the unplaced cards above it first, in their order, as the server does', () => {
    expect(
      placeCard(
        [
          { row: 'ann', position: 'a0' },
          { row: 'bob', position: null },
          { row: 'cat', position: null },
        ],
        { row: 'dan', lane: null, before: 'cat', after: null },
        keyBetween
      )
    ).toEqual([
      { row: 'bob', lane: null, position: '(a0..-)' },
      { row: 'cat', lane: null, position: '((a0..-)..-)' },
      { row: 'dan', lane: null, position: '(((a0..-)..-)..-)' },
    ]);
  });

  it('reads a lane in display order with the places stored for that lane only', () => {
    expect(
      laneCards(
        board,
        [
          { row: 'ann', lane: 'yes', position: 'a0' },
          { row: 'bob', lane: 'no', position: 'a5' },
          { row: 'cat', lane: 'yes', position: 'a1' },
        ],
        'yes',
        'cat'
      )
    ).toEqual([
      { row: 'ann', position: 'a0' },
      { row: 'bob', position: null },
    ]);
  });

  it('replaces the places of the cards it wrote and keeps the rest', () => {
    expect(
      withPositions(
        [
          { row: 'ann', lane: 'yes', position: 'a0' },
          { row: 'dan', lane: 'no', position: 'a0' },
        ],
        [{ row: 'dan', lane: 'yes', position: 'a1' }]
      )
    ).toEqual([
      { row: 'ann', lane: 'yes', position: 'a0' },
      { row: 'dan', lane: 'yes', position: 'a1' },
    ]);
  });
});

describe('a moved card, shown before its row is read again', () => {
  it('leaves its old lane for its new one, between its neighbours', () => {
    expect(
      withMovedCards(board, [
        { row: 'dan', lane: 'yes', before: 'ann', after: 'bob' },
      ])
    ).toEqual({
      lanes: [
        { option: null, hidden: false, cards: [] },
        { option: 'yes', hidden: false, cards: ['ann', 'dan', 'bob', 'cat'] },
        { option: 'no', hidden: false, cards: [] },
      ],
    });
  });

  it('goes before its lower neighbour when it has no upper one, and last with neither', () => {
    expect(
      withMovedCards(board, [
        { row: 'cat', lane: 'yes', before: null, after: 'ann' },
        { row: 'bob', lane: null, before: null, after: null },
      ])
    ).toEqual({
      lanes: [
        { option: null, hidden: false, cards: ['bob'] },
        { option: 'yes', hidden: false, cards: ['cat', 'ann'] },
        { option: 'no', hidden: false, cards: ['dan'] },
      ],
    });
  });
});
