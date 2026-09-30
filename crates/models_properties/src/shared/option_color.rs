//! The colours new select options are given.

#[cfg(test)]
mod test;

/// The tag palette (`apps/web/src/features/property/tags/tagColors.ts`),
/// ordered so that neighbouring options differ in hue.
pub const OPTION_COLOR_CYCLE: [&str; 12] = [
    "#0091FF", // Blue
    "#46A758", // Green
    "#8E4EC6", // Purple
    "#F76B15", // Orange
    "#E93D82", // Pink
    "#12A594", // Teal
    "#FFB224", // Amber
    "#3E63DD", // Indigo
    "#E5484D", // Red
    "#F5D90A", // Yellow
    "#889096", // Gray
    "#E54D2E", // Tomato
];

/// The colour for the option at `position` among its definition's options.
pub fn option_color(position: usize) -> &'static str {
    OPTION_COLOR_CYCLE[position % OPTION_COLOR_CYCLE.len()]
}
