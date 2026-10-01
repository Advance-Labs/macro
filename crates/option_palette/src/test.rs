use strum::IntoEnumIterator;

use super::*;

#[test]
fn a_colour_crosses_the_wire_by_its_name() {
    assert_eq!(
        serde_json::to_value(OptionColor::Tomato).unwrap(),
        serde_json::json!("tomato")
    );
    assert_eq!(
        serde_json::from_value::<OptionColor>(serde_json::json!("gray")).unwrap(),
        OptionColor::Gray
    );
    assert!(serde_json::from_value::<OptionColor>(serde_json::json!("#E5484D")).is_err());
}

#[test]
fn a_stored_hex_reads_back_as_its_colour_whatever_its_case() {
    assert_eq!(OptionColor::from_hex("#E5484D"), Some(OptionColor::Red));
    assert_eq!(OptionColor::from_hex("#e5484d"), Some(OptionColor::Red));
    assert_eq!(OptionColor::from_hex("#123456"), None);
    for color in OptionColor::iter() {
        assert_eq!(OptionColor::from_hex(color.hex()), Some(color));
    }
}

#[test]
fn new_options_cycle_through_every_colour_once_then_start_again() {
    assert_eq!(OptionColor::for_position(0), OptionColor::Blue);
    assert_eq!(OptionColor::for_position(1), OptionColor::Green);
    assert_eq!(OptionColor::for_position(2), OptionColor::Purple);
    let mut cycle: Vec<OptionColor> = (0..12).map(OptionColor::for_position).collect();
    cycle.sort_by_key(|color| color.hex());
    cycle.dedup();
    assert_eq!(cycle.len(), 12);
    assert_eq!(OptionColor::for_position(12), OptionColor::Blue);
}
