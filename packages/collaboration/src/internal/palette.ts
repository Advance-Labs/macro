const palette = [
  'red',
  'orange',
  'yellow',
  'lime',
  'green',
  'teal',
  'cyan',
  'blue',
  'violet',
  'purple',
  'pink',
] as const;

/** Pick a collaboration cursor color from Macro's shared accent palette. */
export function getRandomPaletteColor(): string {
  return palette[Math.floor(Math.random() * palette.length)] ?? palette[0];
}

/**
 * The same palette color for the same key on every client, for presence that
 * must match across viewers (the random cursor color is per session).
 */
export function paletteColorForKey(key: string): string {
  let hash = 0;
  for (const character of key) {
    hash = (hash * 31 + (character.codePointAt(0) ?? 0)) >>> 0;
  }
  return palette[hash % palette.length] ?? palette[0];
}
