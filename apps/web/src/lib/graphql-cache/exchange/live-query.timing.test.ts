// Opt-in timings: a full keyed reconcile vs one response-path patch on a
// Soup-like page. Mirrors crates/client/cache-core/tests/watch_query_timing.rs.
// LIVE_QUERY_TIMINGS=1 bunx vitest run --project graphql-cache \
//   --reporter=default live-query.timing
import { gql } from '@urql/core';
import { createComputed, createRoot } from 'solid-js';
import { describe, expect, it } from 'vitest';
import { LiveQuery } from './live-query';
import { applyQueryPatches } from './query-patches';
import { queryShape } from './query-shape';

const SAMPLES = 200;
const WARMUP = 20;
const SHAPE = queryShape(gql`
  query Page($input: SoupInput!) {
    user { id soup(input: $input) { items {
      __typename id frecencyScore
      ... on GraphqlSoupDocument {
        documentName: name ownerId fileType projectId createdAt updatedAt viewedAt deletedAt
        subType { __typename ... on GraphqlTaskSubType { isCompleted } }
        properties { id propertyDefinitionId displayName dataType isMultiSelect value {
          __typename
          ... on GraphqlSelectOptionPropertyValue { optionIds }
          ... on GraphqlStringPropertyValue { value }
        } }
      }
    } nextCursor } }
  }
`);

type PropertyValue = {
  __typename: string;
  optionIds?: string[];
  value?: string;
};

function page(rows: number) {
  const property = (id: string, definition: string, value: PropertyValue) => ({
    id,
    propertyDefinitionId: definition,
    displayName: definition,
    dataType: value.optionIds ? 'SELECT_STRING' : 'STRING',
    isMultiSelect: false,
    value,
  });
  const items = Array.from({ length: rows }, (_, i) => ({
    __typename: 'GraphqlSoupDocument',
    id: `doc-${i}`,
    frecencyScore: 0.5,
    documentName: `Document ${i}`,
    ownerId: 'macro|owner@example.com',
    fileType: 'md',
    projectId: i % 3 === 0 ? null : 'project-1',
    createdAt: '2026-10-01T00:00:00Z',
    updatedAt: '2026-10-02T00:00:00Z',
    viewedAt: '2026-10-03T00:00:00Z',
    deletedAt: null,
    subType: { __typename: 'GraphqlTaskSubType', isCompleted: false },
    properties: [
      property(`status-${i}`, 'status', {
        __typename: 'GraphqlSelectOptionPropertyValue',
        optionIds: ['todo'],
      }),
      property(`note-${i}`, 'note', {
        __typename: 'GraphqlStringPropertyValue',
        value: 'note',
      }),
    ],
  }));
  return { user: { id: 'viewer', soup: { items, nextCursor: null } } };
}
type Page = ReturnType<typeof page>;

/** Like a rendered `<For>`: the list tracks rows, each row its own fields. */
function mount(rows: number): LiveQuery {
  return createRoot(() => {
    const view = new LiveQuery(page(rows), SHAPE);
    const items = () => (view.data as Page).user.soup.items;
    createComputed(() => {
      for (const item of items()) void item.id;
    });
    for (const item of items()) {
      createComputed(() => {
        void item.documentName;
        void item.updatedAt;
        void item.subType.isCompleted;
        for (const { value } of item.properties)
          void (value.optionIds?.join(',') ?? value.value);
      });
    }
    return view;
  });
}

/** Median microseconds per call. */
function median(run: (sample: number) => void): number {
  for (let sample = 0; sample < WARMUP; sample++) run(sample);
  const times: number[] = [];
  for (let sample = WARMUP; sample < WARMUP + SAMPLES; sample++) {
    const start = performance.now();
    run(sample);
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  return times[times.length >> 1] * 1000;
}

describe.skipIf(!process.env.LIVE_QUERY_TIMINGS)(
  'live query update timings',
  () => {
    it('prints full reconcile and single-patch costs', () => {
      const lines = [
        '| rows | JSON.parse | structuredClone | full: parse + keyed reconcile | patch: apply + store update |',
        '|---:|---:|---:|---:|---:|',
      ];
      for (const rows of [100, 500]) {
        const serialized = JSON.stringify(page(rows));
        const parsed = JSON.parse(serialized) as Page;
        const full = mount(rows);
        const patched = mount(rows);
        const parse = median(() => JSON.parse(serialized));
        const clone = median(() => structuredClone(parsed));
        const reconcile = median((sample) => {
          const next = JSON.parse(serialized) as Page;
          next.user.soup.items[(sample * 7) % rows].documentName =
            `Renamed ${sample}`;
          full.replace(next);
        });
        const patch = median((sample) => {
          const path = [
            'user',
            'soup',
            'items',
            (sample * 7) % rows,
            'documentName',
          ];
          patched.replace(
            applyQueryPatches(patched.snapshot, [
              { path, value: `Renamed ${sample}` },
            ])
          );
        });
        const last = ((WARMUP + SAMPLES - 1) * 7) % rows;
        expect((full.data as Page).user.soup.items[last]).toEqual(
          (patched.data as Page).user.soup.items[last]
        );
        const us = (value: number) => `${value.toFixed(1)} µs`;
        lines.push(
          `| ${rows} | ${us(parse)} | ${us(clone)} | ${us(reconcile)} | ${us(patch)} |`
        );
      }
      console.log(lines.join('\n'));
    });
  }
);
