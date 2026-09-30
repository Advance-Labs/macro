import { describe, expect, it } from 'vitest';
import { defaultBoardGroupColumn } from './board-grouping';

describe('default board grouping', () => {
  it('prefers the first multi-select column over earlier single selects and checkboxes', () => {
    expect(
      defaultBoardGroupColumn([
        {
          id: 'name',
          name: 'Name',
          dataType: 'STRING',
          isMultiSelect: false,
          options: [],
          writable: true,
        },
        {
          id: 'done',
          name: 'Done',
          dataType: 'BOOLEAN',
          isMultiSelect: false,
          options: [],
          writable: true,
        },
        {
          id: 'status',
          name: 'Status',
          dataType: 'SELECT_STRING',
          isMultiSelect: false,
          options: ['To do', 'Done'],
          writable: true,
        },
        {
          id: 'tags',
          name: 'Tags',
          dataType: 'SELECT_STRING',
          isMultiSelect: true,
          options: ['Bug', 'Feature'],
          writable: true,
        },
        {
          id: 'priorities',
          name: 'Priorities',
          dataType: 'SELECT_NUMBER',
          isMultiSelect: true,
          options: [1, 2],
          writable: true,
        },
      ])?.id
    ).toBe('tags');
  });

  it('skips multi-value relations and non-select multi-value fields', () => {
    expect(
      defaultBoardGroupColumn([
        {
          id: 'owners',
          name: 'Owners',
          dataType: 'ENTITY',
          isMultiSelect: true,
          options: [],
          writable: true,
        },
        {
          id: 'projects',
          name: 'Projects',
          dataType: 'SELECT_STRING',
          isMultiSelect: true,
          options: [],
          writable: true,
          relation: { databaseId: 'database', tableId: 'projects' },
        },
        {
          id: 'status',
          name: 'Status',
          dataType: 'SELECT_STRING',
          isMultiSelect: false,
          options: ['To do', 'Done'],
          writable: true,
        },
      ])?.id
    ).toBe('status');
  });

  it('falls back to the first single select when there is no multi-select', () => {
    expect(
      defaultBoardGroupColumn([
        {
          id: 'done',
          name: 'Done',
          dataType: 'BOOLEAN',
          isMultiSelect: false,
          options: [],
          writable: true,
        },
        {
          id: 'priority',
          name: 'Priority',
          dataType: 'SELECT_NUMBER',
          isMultiSelect: false,
          options: [1, 2],
          writable: true,
        },
        {
          id: 'status',
          name: 'Status',
          dataType: 'SELECT_STRING',
          isMultiSelect: false,
          options: ['To do', 'Done'],
          writable: true,
        },
      ])?.id
    ).toBe('priority');
  });

  it('keeps grouping by the first checkbox when there is no select column', () => {
    expect(
      defaultBoardGroupColumn([
        {
          id: 'name',
          name: 'Name',
          dataType: 'STRING',
          isMultiSelect: false,
          options: [],
          writable: true,
        },
        {
          id: 'done',
          name: 'Done',
          dataType: 'BOOLEAN',
          isMultiSelect: false,
          options: [],
          writable: true,
        },
      ])?.id
    ).toBe('done');
  });

  it('has no default when no column can group a board', () => {
    expect(
      defaultBoardGroupColumn([
        {
          id: 'name',
          name: 'Name',
          dataType: 'STRING',
          isMultiSelect: false,
          options: [],
          writable: true,
        },
      ])
    ).toBeUndefined();
  });
});
