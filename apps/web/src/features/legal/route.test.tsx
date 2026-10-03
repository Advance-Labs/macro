import {
  createRoutesManifest,
  decodeRoute,
  encodeRoute,
  getRouteClaim,
} from '@app/lib/split-router/routes';
import { describe, expect, it, vi } from 'vitest';
import { legalRoute } from './route';

vi.mock('@components/app/split-layout/split-router/app-route-shell', () => ({
  withAuth: (value: unknown) => value,
  AppView: () => null,
}));
vi.mock('./views/editor', () => ({ Editor: () => null }));
vi.mock('./views/envelope-detail', () => ({ EnvelopeDetail: () => null }));
const manifest = createRoutesManifest({ definitions: [legalRoute] });
const id = '01a10238-683d-7910-9cfb-d03e79623d1c';

describe('Legal in the app split router', () => {
  it('restores an agreement within the Legal workspace and owns its split content', () => {
    const entry = decodeRoute(manifest, ['legal', id])!;
    expect(encodeRoute(manifest, entry)).toEqual(['legal', id]);
    expect(entry.location.route.matches.map((match) => match.id)).toEqual([
      'view-legal',
      'legal-envelope',
    ]);
    expect(getRouteClaim(manifest, entry.location.route)).toEqual({
      namespace: 'component',
      id: `legal-envelope:${id}`,
    });
  });
  it('resolves new envelope preparation as a child view, not an agreement ID', () => {
    const entry = decodeRoute(manifest, ['legal', 'new'])!;
    expect(entry.location.route.matches.map((match) => match.id)).toEqual([
      'view-legal',
      'legal-new-envelope',
    ]);
  });
  it('rejects malformed agreement IDs', () => {
    expect(
      decodeRoute(manifest, ['legal', 'invalid'])?.location.route.matches.at(-1)
        ?.id
    ).not.toBe('legal-envelope');
  });
});
