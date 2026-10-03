import { describe, expect, it } from 'vitest';
import { LegacyBlockRegistry } from './legacyBlockRegistry';

const definitionFiles = import.meta.glob(
  '../../../features/block-*/definition.ts',
  {
    eager: true,
    import: 'default',
    query: '?raw',
  }
);

describe('block definition discovery', () => {
  it('has one definition file for every remaining legacy renderer', () => {
    const discoveredNames = Object.keys(definitionFiles).map((path) =>
      path
        .split('/')
        .at(-2)
        ?.replace(/^block-/, '')
    );

    expect(discoveredNames.sort()).toEqual([...LegacyBlockRegistry].sort());
  });
});
