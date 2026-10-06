use super::*;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[test]
fn typed_numbers_are_normalized_and_deduplicated_in_order() {
    let parsed = parse_contact_phone_numbers(&strings(&[
        "(555) 234-5678",
        "+44 20 7946 0958",
        "5552345678",
    ]))
    .unwrap();
    assert_eq!(
        parsed.iter().map(PhoneNumber::as_str).collect::<Vec<_>>(),
        vec!["+15552345678", "+442079460958"]
    );
}

#[test]
fn invalid_numbers_name_the_offending_input() {
    let CrmError::InvalidRequest(message) =
        parse_contact_phone_numbers(&strings(&["+15552345678", "call me"])).unwrap_err()
    else {
        panic!("expected InvalidRequest");
    };
    assert!(message.contains("call me"), "{message}");
}

#[test]
fn contacts_have_a_bounded_number_of_phone_numbers() {
    let numbers: Vec<String> = (0..=MAX_CONTACT_PHONE_NUMBERS)
        .map(|index| format!("+1555234{index:04}"))
        .collect();
    assert!(parse_contact_phone_numbers(&numbers).is_err());
    assert!(parse_contact_phone_numbers(&numbers[..MAX_CONTACT_PHONE_NUMBERS]).is_ok());
    assert!(parse_contact_phone_numbers(&[]).unwrap().is_empty());
}
