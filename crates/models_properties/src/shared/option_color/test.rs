use super::*;
use crate::api::is_valid_hex_color;

#[test]
fn the_first_options_take_the_palette_in_cycle_order() {
    assert_eq!(option_color(0), "#0091FF");
    assert_eq!(option_color(1), "#46A758");
    assert_eq!(option_color(2), "#8E4EC6");
}

#[test]
fn a_full_cycle_uses_every_colour_once_then_starts_again() {
    let cycle: Vec<&str> = (0..12).map(option_color).collect();
    let mut distinct = cycle.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 12);
    assert_eq!(option_color(12), "#0091FF");
    assert_eq!(option_color(13), "#46A758");
}

#[test]
fn every_colour_is_a_storable_hex_string() {
    for color in OPTION_COLOR_CYCLE {
        assert!(is_valid_hex_color(color), "{color}");
    }
}
