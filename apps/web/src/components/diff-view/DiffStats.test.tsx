import { render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { describe, expect, it } from 'vitest';
import { DiffStats } from './DiffStats';

function expectSquares(success: number, failure: number, neutral: number) {
  const image = screen.getByRole('img');
  const hidden = image.querySelectorAll('[aria-hidden="true"]');
  expect(hidden).toHaveLength(2);
  expect(hidden[1].classList.contains('gap-0.5')).toBe(true);
  const squares = Array.from(hidden[1].children);
  expect(squares).toHaveLength(5);
  expect(squares.every((square) => square.classList.contains('size-2.5'))).toBe(
    true
  );
  expect(
    squares.filter((square) => square.classList.contains('bg-success'))
  ).toHaveLength(success);
  expect(
    squares.filter((square) => square.classList.contains('bg-failure'))
  ).toHaveLength(failure);
  expect(
    squares.filter((square) => square.classList.contains('bg-edge-muted'))
  ).toHaveLength(neutral);
  expect(
    squares
      .slice(0, success)
      .every((square) => square.classList.contains('bg-success'))
  ).toBe(true);
}

describe('DiffStats', () => {
  it.each([
    { additions: 1, deletions: 1, success: 3, failure: 2 },
    { additions: 2, deletions: 3, success: 2, failure: 3 },
    { additions: 1, deletions: 99, success: 1, failure: 4 },
    { additions: 99, deletions: 1, success: 4, failure: 1 },
  ])(
    'shows a proportional mixed strip for $additions additions and $deletions deletions',
    ({ additions, deletions, success, failure }) => {
      render(() => <DiffStats additions={additions} deletions={deletions} />);
      expectSquares(success, failure, 0);
    }
  );

  it('shows only addition squares and a singular addition label', () => {
    render(() => <DiffStats additions={1} deletions={0} />);
    expect(
      screen.getByRole('img', { name: '1 addition, 0 deletions' })
    ).toBeTruthy();
    expect(screen.getByText('+1')).toBeTruthy();
    expectSquares(5, 0, 0);
  });

  it('shows only deletion squares and a singular deletion label', () => {
    render(() => <DiffStats additions={0} deletions={1} />);
    expect(
      screen.getByRole('img', { name: '0 additions, 1 deletion' })
    ).toBeTruthy();
    expect(screen.getByText('−1')).toBeTruthy();
    expectSquares(0, 5, 0);
  });

  it('keeps larger squares textured and static', () => {
    render(() => <DiffStats additions={3} deletions={2} />);
    const squares = screen.getByRole('img').querySelectorAll('.size-2\\.5');
    expect(squares).toHaveLength(5);
    for (const square of squares) {
      expect(square.classList.contains('shrink-0')).toBe(true);
      expect(square.classList.contains('rounded-sm')).toBe(true);
      expect(square.classList.contains('ring-1')).toBe(true);
      expect(square.classList.contains('ring-inset')).toBe(true);
      expect(square.classList.contains('ring-ink/20')).toBe(true);
      expect(square.className).toContain('repeating-linear-gradient');
      expect(square.className).toContain('var(--color-ink)');
      expect(square.className).not.toContain('animate-');
    }
  });
  it('shows five neutral squares with zero totals', () => {
    render(() => <DiffStats additions={0} deletions={0} />);
    expect(
      screen.getByRole('img', { name: '0 additions, 0 deletions' })
    ).toBeTruthy();
    expectSquares(0, 0, 5);
  });

  it('updates the totals, accessible label, and squares when props change', () => {
    const [counts, setCounts] = createSignal({ additions: 1, deletions: 1 });
    render(() => (
      <DiffStats
        additions={counts().additions}
        deletions={counts().deletions}
      />
    ));
    expect(
      screen.getByRole('img', { name: '1 addition, 1 deletion' })
    ).toBeTruthy();
    expect(screen.getByText('+1')).toBeTruthy();
    expect(screen.getByText('−1')).toBeTruthy();
    expectSquares(3, 2, 0);

    setCounts({ additions: 0, deletions: 5 });
    expect(
      screen.getByRole('img', { name: '0 additions, 5 deletions' })
    ).toBeTruthy();
    expect(screen.queryByText('+1')).toBeNull();
    expect(screen.getByText('−5')).toBeTruthy();
    expectSquares(0, 5, 0);

    setCounts({ additions: 0, deletions: 0 });
    expect(
      screen.getByRole('img', { name: '0 additions, 0 deletions' })
    ).toBeTruthy();
    expect(screen.queryByText('−5')).toBeNull();
    expectSquares(0, 0, 5);

    setCounts({ additions: 10, deletions: 0 });
    expect(
      screen.getByRole('img', { name: '10 additions, 0 deletions' })
    ).toBeTruthy();
    expect(screen.getByText('+10')).toBeTruthy();
    expectSquares(5, 0, 0);
  });
});
