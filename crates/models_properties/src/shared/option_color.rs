//! The colours new select options are given.

#[cfg(test)]
mod test;

use option_palette::OptionColor;

/// The colour for the option at `position` among its definition's options,
/// as the hex value an option stores.
pub fn option_color(position: usize) -> &'static str {
    OptionColor::for_position(position).hex()
}
