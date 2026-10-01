/**
 * Series colors, assigned in this order and never cycled. They are the tag
 * palette's hues as theme tokens, ordered so neighbours stay apart under
 * color-vision deficiency (worst adjacent deutan ΔE 9.8). Amber and yellow
 * are left out: too light to read as marks on a light panel.
 */
export const CHART_PALETTE = [
  'var(--color-blue)',
  'var(--color-orange)',
  'var(--color-teal)',
  'var(--color-purple)',
  'var(--color-red)',
  'var(--color-cyan)',
  'var(--color-pink)',
  'var(--color-green)',
  'var(--color-violet)',
] as const;
