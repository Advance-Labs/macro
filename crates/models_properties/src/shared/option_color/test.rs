use option_palette::OptionColor;
use strum::IntoEnumIterator;

use super::*;
use crate::api::is_valid_hex_color;

#[test]
fn the_first_options_take_the_palette_in_cycle_order() {
    assert_eq!(option_color(0), "#0091FF");
    assert_eq!(option_color(1), "#46A758");
    assert_eq!(option_color(2), "#8E4EC6");
}

#[test]
fn every_colour_is_a_storable_hex_string() {
    for color in OptionColor::iter() {
        assert!(is_valid_hex_color(color.hex()), "{color:?}");
    }
}
