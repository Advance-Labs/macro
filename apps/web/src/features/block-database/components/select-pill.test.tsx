import { cleanup, render, screen } from '@solidjs/testing-library';
import { afterEach, expect, it } from 'vitest';
import type { DatabaseViewColumn } from '../core/database-view';
import { SelectPill } from './select-pill';

afterEach(cleanup);

const column = (
  dataType: string,
  optionColors: Record<string, string> = {}
): DatabaseViewColumn => ({
  id: 'status',
  name: 'Status',
  dataType,
  isMultiSelect: false,
  options: ['Done', 'Blocked'],
  optionColors,
  writable: true,
});

it('draws a stored option colour instead of guessing one from the label', () => {
  render(() => (
    <SelectPill
      label="Blocked"
      column={column('SELECT_STRING', { Blocked: '#123456' })}
    />
  ));
  const dot = screen
    .getByTitle('Blocked')
    .querySelector<HTMLElement>('[data-slot="tag-dot"]');
  expect(dot?.style.backgroundColor).toBe('rgb(18, 52, 86)');
});

it('leaves an uncoloured select option as a plain label, like a task', () => {
  render(() => <SelectPill label="Done" column={column('SELECT_STRING')} />);
  expect(
    screen.getByTitle('Done').querySelector('[data-slot="tag-dot"]')
  ).toBeNull();
});

it('gives an uncoloured tag the default tag dot', () => {
  render(() => <SelectPill label="Done" column={column('TAG')} />);
  expect(
    screen.getByTitle('Done').querySelector('[data-slot="tag-dot"]')
  ).toBeTruthy();
});
