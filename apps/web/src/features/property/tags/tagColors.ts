import type { OptionColor } from '@service-storage/generated/schemas/optionColor';

/**
 * The palette tags and select options are coloured from: each colour by the
 * name the server knows it by and the hex value an option stores. Copied from
 * `crates/option_palette`; a new server colour fails to type-check here.
 */
const TAG_COLORS = {
  red: { name: 'Red', color: '#E5484D' },
  tomato: { name: 'Tomato', color: '#E54D2E' },
  orange: { name: 'Orange', color: '#F76B15' },
  amber: { name: 'Amber', color: '#FFB224' },
  yellow: { name: 'Yellow', color: '#F5D90A' },
  green: { name: 'Green', color: '#46A758' },
  teal: { name: 'Teal', color: '#12A594' },
  blue: { name: 'Blue', color: '#0091FF' },
  indigo: { name: 'Indigo', color: '#3E63DD' },
  purple: { name: 'Purple', color: '#8E4EC6' },
  pink: { name: 'Pink', color: '#E93D82' },
  gray: { name: 'Gray', color: '#889096' },
} as const satisfies Record<OptionColor, { name: string; color: string }>;

/** The palette in picker order; `Object.entries` widens the keys back to string. */
export const TAG_COLOR_OPTIONS = Object.entries(TAG_COLORS).map(
  ([value, swatch]) => ({ value: value as OptionColor, ...swatch })
);

export type TagColorOption = (typeof TAG_COLOR_OPTIONS)[number];

export const DEFAULT_TAG_COLOR: string = TAG_COLORS.gray.color;

/** The palette colour an option's stored hex value is, matched without regard to case. */
export function optionColorOf(hex: string | null | undefined) {
  const wanted = hex?.toUpperCase();
  return TAG_COLOR_OPTIONS.find((option) => option.color === wanted);
}
