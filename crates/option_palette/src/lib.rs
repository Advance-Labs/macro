#![deny(missing_docs)]
//! The palette select and tag options are coloured from: the colours the
//! web's tag components render (`apps/web/src/features/property/tags/tagColors.ts`),
//! each by name, with the hex value an option stores.

#[cfg(test)]
mod test;

use serde::{Deserialize, Serialize};

/// A colour select and tag options take, from the palette the tag picker
/// renders.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
    specta::Type,
    schemars::JsonSchema,
    strum::EnumIter,
)]
#[serde(rename_all = "snake_case")]
pub enum OptionColor {
    /// Red (`#E5484D`).
    Red,
    /// Tomato (`#E54D2E`).
    Tomato,
    /// Orange (`#F76B15`).
    Orange,
    /// Amber (`#FFB224`).
    Amber,
    /// Yellow (`#F5D90A`).
    Yellow,
    /// Green (`#46A758`).
    Green,
    /// Teal (`#12A594`).
    Teal,
    /// Blue (`#0091FF`).
    Blue,
    /// Indigo (`#3E63DD`).
    Indigo,
    /// Purple (`#8E4EC6`).
    Purple,
    /// Pink (`#E93D82`).
    Pink,
    /// Gray (`#889096`).
    Gray,
}

/// The order new options take their colours in, so that neighbouring
/// options differ in hue.
const NEW_OPTION_ORDER: [OptionColor; 12] = [
    OptionColor::Blue,
    OptionColor::Green,
    OptionColor::Purple,
    OptionColor::Orange,
    OptionColor::Pink,
    OptionColor::Teal,
    OptionColor::Amber,
    OptionColor::Indigo,
    OptionColor::Red,
    OptionColor::Yellow,
    OptionColor::Gray,
    OptionColor::Tomato,
];

impl OptionColor {
    /// The hex value an option of this colour stores.
    pub fn hex(self) -> &'static str {
        match self {
            OptionColor::Red => "#E5484D",
            OptionColor::Tomato => "#E54D2E",
            OptionColor::Orange => "#F76B15",
            OptionColor::Amber => "#FFB224",
            OptionColor::Yellow => "#F5D90A",
            OptionColor::Green => "#46A758",
            OptionColor::Teal => "#12A594",
            OptionColor::Blue => "#0091FF",
            OptionColor::Indigo => "#3E63DD",
            OptionColor::Purple => "#8E4EC6",
            OptionColor::Pink => "#E93D82",
            OptionColor::Gray => "#889096",
        }
    }

    /// The palette colour a stored hex value is, ignoring case; `None` for
    /// a colour outside the palette.
    pub fn from_hex(hex: &str) -> Option<OptionColor> {
        <OptionColor as strum::IntoEnumIterator>::iter()
            .find(|color| color.hex().eq_ignore_ascii_case(hex))
    }

    /// The colour of the option at `position` among its definition's
    /// options.
    pub fn for_position(position: usize) -> OptionColor {
        NEW_OPTION_ORDER[position % NEW_OPTION_ORDER.len()]
    }
}
