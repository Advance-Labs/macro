#![deny(missing_docs)]
//! The palette select and tag options are coloured from, each colour by name
//! with the hex value an option stores. The web's `tagColors.ts` copies it
//! and type-checks against the generated `OptionColor`.

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
    strum::EnumIter,
    strum::IntoStaticStr,
    strum::EnumString,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(ascii_case_insensitive)]
pub enum OptionColor {
    /// Red.
    #[strum(serialize = "#E5484D")]
    Red,
    /// Tomato.
    #[strum(serialize = "#E54D2E")]
    Tomato,
    /// Orange.
    #[strum(serialize = "#F76B15")]
    Orange,
    /// Amber.
    #[strum(serialize = "#FFB224")]
    Amber,
    /// Yellow.
    #[strum(serialize = "#F5D90A")]
    Yellow,
    /// Green.
    #[strum(serialize = "#46A758")]
    Green,
    /// Teal.
    #[strum(serialize = "#12A594")]
    Teal,
    /// Blue.
    #[strum(serialize = "#0091FF")]
    Blue,
    /// Indigo.
    #[strum(serialize = "#3E63DD")]
    Indigo,
    /// Purple.
    #[strum(serialize = "#8E4EC6")]
    Purple,
    /// Pink.
    #[strum(serialize = "#E93D82")]
    Pink,
    /// Gray.
    #[strum(serialize = "#889096")]
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
        self.into()
    }

    /// The palette colour a stored hex value is, ignoring case; `None` for
    /// a colour outside the palette.
    pub fn from_hex(hex: &str) -> Option<OptionColor> {
        hex.parse().ok()
    }

    /// The colour of the option at `position` among its definition's
    /// options.
    pub fn for_position(position: usize) -> OptionColor {
        NEW_OPTION_ORDER[position % NEW_OPTION_ORDER.len()]
    }
}
