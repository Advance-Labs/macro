import type { OptionColor } from '@service-storage/generated/schemas/optionColor';

/**
 * The palette tags and select options are coloured from, in picker order:
 * each colour by the name the server knows it by (`crates/option_palette`)
 * and the hex value an option stores.
 */
export const TAG_COLOR_OPTIONS = [
  { value: 'red', name: 'Red', color: '#E5484D' },
  { value: 'tomato', name: 'Tomato', color: '#E54D2E' },
  { value: 'orange', name: 'Orange', color: '#F76B15' },
  { value: 'amber', name: 'Amber', color: '#FFB224' },
  { value: 'yellow', name: 'Yellow', color: '#F5D90A' },
  { value: 'green', name: 'Green', color: '#46A758' },
  { value: 'teal', name: 'Teal', color: '#12A594' },
  { value: 'blue', name: 'Blue', color: '#0091FF' },
  { value: 'indigo', name: 'Indigo', color: '#3E63DD' },
  { value: 'purple', name: 'Purple', color: '#8E4EC6' },
  { value: 'pink', name: 'Pink', color: '#E93D82' },
  { value: 'gray', name: 'Gray', color: '#889096' },
] as const satisfies readonly {
  value: OptionColor;
  name: string;
  color: string;
}[];

export type TagColorOption = (typeof TAG_COLOR_OPTIONS)[number];

/** Every server colour has a swatch: a new one fails to type-check here. */
const everyColorListed: Exclude<
  OptionColor,
  TagColorOption['value']
> extends never
  ? true
  : never = true;
void everyColorListed;

export const DEFAULT_TAG_COLOR: string = TAG_COLOR_OPTIONS[11].color;

/** The palette colour an option's stored hex value is, matched without regard to case. */
export function optionColorOf(hex: string | null | undefined) {
  const wanted = hex?.toUpperCase();
  return TAG_COLOR_OPTIONS.find((option) => option.color === wanted);
}
