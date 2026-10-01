import { describe, expect, it } from 'vitest';
import type { DatabaseViewColumn } from './database-view';
import {
  layoutColumns,
  withLaneHidden,
  withLaneOrder,
  withLayoutColumn,
  withLayoutOrder,
  withoutColumn,
} from './views';

const name: DatabaseViewColumn = {
  id: 'name',
  name: 'Name',
  dataType: 'STRING',
  isMultiSelect: false,
  options: [],
  writable: true,
};
const rsvp: DatabaseViewColumn = {
  id: 'rsvp',
  name: 'RSVP',
  dataType: 'SELECT_STRING',
  isMultiSelect: false,
  options: [],
  writable: true,
};
const guests: DatabaseViewColumn = {
  id: 'guests',
  name: 'Guests',
  dataType: 'NUMBER',
  isMultiSelect: false,
  options: [],
  writable: true,
};

describe('table layouts', () => {
  it('shows listed columns first, then the rest in table order', () => {
    expect(
      layoutColumns(
        {
          kind: 'table',
          columns: [{ column: 'guests', width: 120, hidden: false }],
        },
        [name, rsvp, guests]
      )
    ).toEqual([
      { column: guests, width: 120, hidden: false },
      { column: name, width: null, hidden: false },
      { column: rsvp, width: null, hidden: false },
    ]);
  });

  it('writes every column when one is hidden or resized', () => {
    expect(
      withLayoutColumn({ kind: 'table', columns: [] }, [name, rsvp], 'rsvp', {
        hidden: true,
      })
    ).toEqual({
      kind: 'table',
      columns: [
        { column: 'name', width: null, hidden: false },
        { column: 'rsvp', width: null, hidden: true },
      ],
    });
    expect(
      withLayoutColumn(
        {
          kind: 'table',
          columns: [{ column: 'rsvp', width: null, hidden: true }],
        },
        [name, rsvp],
        'name',
        { width: 240 }
      )
    ).toEqual({
      kind: 'table',
      columns: [
        { column: 'rsvp', width: null, hidden: true },
        { column: 'name', width: 240, hidden: false },
      ],
    });
  });

  it('reorders columns, keeping each one width and visibility', () => {
    expect(
      withLayoutOrder(
        {
          kind: 'table',
          columns: [{ column: 'name', width: 200, hidden: false }],
        },
        [name, rsvp, guests],
        ['rsvp', 'name']
      )
    ).toEqual({
      kind: 'table',
      columns: [
        { column: 'rsvp', width: null, hidden: false },
        { column: 'name', width: 200, hidden: false },
        { column: 'guests', width: null, hidden: false },
      ],
    });
  });
});

const board = {
  kind: 'board' as const,
  groupBy: 'rsvp',
  lanes: [{ option: 'yes', hidden: true }],
  cardFields: ['guests'],
  hideEmptyLanes: false,
};

describe('board lanes', () => {
  it('reorders every lane, keeping a hidden lane hidden', () => {
    expect(withLaneOrder(board, [null, 'no', 'yes'])).toEqual({
      ...board,
      lanes: [
        { option: null, hidden: false },
        { option: 'no', hidden: false },
        { option: 'yes', hidden: true },
      ],
    });
  });

  it('hides a lane it does not list yet, and shows a listed one again', () => {
    expect(withLaneHidden(board, null, true).lanes).toEqual([
      { option: 'yes', hidden: true },
      { option: null, hidden: true },
    ]);
    expect(withLaneHidden(board, 'yes', false).lanes).toEqual([
      { option: 'yes', hidden: false },
    ]);
  });
});

describe('a removed column', () => {
  it('leaves the conditions, sort and card fields that named it', () => {
    expect(
      withoutColumn(
        {
          id: 'view',
          databaseId: 'database',
          tableId: 'table',
          name: 'Board',
          position: 'a0',
          createdAt: '2026-10-01T00:00:00Z',
          updatedAt: '2026-10-01T00:00:00Z',
          query: {
            filter: {
              conjunction: 'and',
              conditions: [
                {
                  kind: 'group',
                  conjunction: 'or',
                  conditions: [
                    {
                      kind: 'condition',
                      column: 'guests',
                      test: { kind: 'presence', operator: 'isEmpty' },
                    },
                  ],
                },
                {
                  kind: 'condition',
                  column: 'name',
                  test: { kind: 'presence', operator: 'isEmpty' },
                },
              ],
            },
            sort: [{ column: 'guests', direction: 'ascending' }],
          },
          layout: board,
        },
        'guests'
      )
    ).toEqual({
      id: 'view',
      databaseId: 'database',
      tableId: 'table',
      name: 'Board',
      position: 'a0',
      createdAt: '2026-10-01T00:00:00Z',
      updatedAt: '2026-10-01T00:00:00Z',
      query: {
        filter: {
          conjunction: 'and',
          conditions: [
            {
              kind: 'condition',
              column: 'name',
              test: { kind: 'presence', operator: 'isEmpty' },
            },
          ],
        },
        sort: [],
      },
      layout: { ...board, cardFields: [] },
    });
  });
});
