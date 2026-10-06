use super::*;

fn e164(input: &str) -> String {
    PhoneNumber::parse(input)
        .unwrap_or_else(|error| panic!("{input:?} should parse: {error}"))
        .as_str()
        .to_string()
}

#[test]
fn national_north_american_numbers_get_the_plus_one_country_code() {
    for input in [
        "(555) 234-5678",
        "555-234-5678",
        "555.234.5678",
        "5552345678",
        "1 (555) 234-5678",
        "1-555-234-5678",
        "  555 234 5678  ",
    ] {
        assert_eq!(e164(input), "+15552345678", "{input}");
    }
}

#[test]
fn international_numbers_keep_their_country_code() {
    assert_eq!(e164("+44 20 7946 0958"), "+442079460958");
    assert_eq!(e164("+44 (0)20 7946 0958"), "+442079460958");
    assert_eq!(e164("0044 20 7946 0958"), "+442079460958");
    assert_eq!(e164("011 44 20 7946 0958"), "+442079460958");
    assert_eq!(e164("+683 4002"), "+6834002");
    assert_eq!(e164("+1 555 234 5678"), "+15552345678");
}

#[test]
fn tel_uris_and_unicode_dashes_are_accepted() {
    assert_eq!(e164("tel:+1-555-234-5678"), "+15552345678");
    assert_eq!(e164("TEL:+44 20 7946 0958"), "+442079460958");
    assert_eq!(e164("555\u{2013}234\u{2013}5678"), "+15552345678");
}

#[test]
fn national_numbers_outside_north_america_need_a_country_code() {
    assert_eq!(
        PhoneNumber::parse("020 7946 0958"),
        Err(PhoneNumberError::MissingCountryCode)
    );
    assert_eq!(
        PhoneNumber::parse("234 5678"),
        Err(PhoneNumberError::MissingCountryCode)
    );
}

#[test]
fn malformed_numbers_are_rejected_with_a_reason() {
    assert_eq!(PhoneNumber::parse("   "), Err(PhoneNumberError::Empty));
    assert_eq!(PhoneNumber::parse("+"), Err(PhoneNumberError::Empty));
    assert_eq!(
        PhoneNumber::parse("1-800-FLOWERS"),
        Err(PhoneNumberError::InvalidCharacter)
    );
    assert_eq!(
        PhoneNumber::parse("+12345"),
        Err(PhoneNumberError::InvalidLength)
    );
    assert_eq!(
        PhoneNumber::parse("+4412345678901234"),
        Err(PhoneNumberError::InvalidLength)
    );
    assert_eq!(
        PhoneNumber::parse("+0123456789"),
        Err(PhoneNumberError::InvalidCountryCode)
    );
}

#[test]
fn north_american_service_codes_and_invalid_exchanges_cannot_be_dialed() {
    for input in [
        "911",
        "+1 911 555 0100",
        "(411) 555-0100",
        "(155) 234-5678",
        "(555) 134-5678",
        "+1 555 234 567",
    ] {
        assert!(
            PhoneNumber::parse(input).is_err(),
            "{input} should not parse"
        );
    }
    assert_eq!(
        PhoneNumber::parse("+1 911 555 0100"),
        Err(PhoneNumberError::InvalidNorthAmericanNumber)
    );
}

#[test]
fn extensions_are_split_off_for_dialing() {
    for input in [
        "+1 555 234 5678 ext. 89",
        "+1 555 234 5678 ext 89",
        "+1 555 234 5678 extension 89",
        "+1 555 234 5678 x89",
        "+1 555 234 5678 X 89",
        "+1 555 234 5678#89",
        "+1 555 234 5678,89",
        "tel:+1-555-234-5678;ext=89",
    ] {
        let dialable = DialablePhoneNumber::parse(input).unwrap();
        assert_eq!(dialable.number.as_str(), "+15552345678", "{input}");
        assert_eq!(
            dialable.extension.as_ref().map(Extension::as_str),
            Some("89"),
            "{input}"
        );
    }
    assert_eq!(
        DialablePhoneNumber::parse("+1 555 234 5678")
            .unwrap()
            .extension,
        None
    );
}

#[test]
fn bare_number_parsing_rejects_extensions() {
    assert_eq!(
        PhoneNumber::parse("+1 555 234 5678 x89"),
        Err(PhoneNumberError::InvalidExtension)
    );
    assert_eq!(
        DialablePhoneNumber::parse("+1 555 234 5678 x"),
        Err(PhoneNumberError::InvalidExtension)
    );
    assert_eq!(
        DialablePhoneNumber::parse("+1 555 234 5678 x12345678901"),
        Err(PhoneNumberError::InvalidExtension)
    );
}

#[test]
fn e164_is_strict_for_stored_and_carrier_values() {
    assert_eq!(
        PhoneNumber::from_e164("+15552345678").unwrap().as_str(),
        "+15552345678"
    );
    assert!(PhoneNumber::from_e164("15552345678").is_err());
    assert!(PhoneNumber::from_e164("+1 555 234 5678").is_err());
    assert!(PhoneNumber::from_e164("+1911555010").is_err());
}

#[test]
fn serde_round_trips_e164_only() {
    let number = PhoneNumber::parse("(555) 234-5678").unwrap();
    let json = serde_json::to_value(&number).unwrap();
    assert_eq!(json, serde_json::json!("+15552345678"));
    assert_eq!(serde_json::from_value::<PhoneNumber>(json).unwrap(), number);
    assert!(serde_json::from_value::<PhoneNumber>(serde_json::json!("(555) 234-5678")).is_err());
}

#[test]
fn prefixes_identify_countries_and_ranges() {
    let us = PhoneNumber::parse("+1 900 234 5678").unwrap();
    assert!(us.is_north_american());
    assert!(us.starts_with_digits("1"));
    assert!(us.starts_with_digits("1900"));
    let uk = PhoneNumber::parse("+44 20 7946 0958").unwrap();
    assert!(!uk.is_north_american());
    assert!(uk.starts_with_digits("44"));
    assert_eq!(uk.digits(), "442079460958");
}

#[test]
fn display_groups_north_american_numbers_only() {
    assert_eq!(
        PhoneNumber::parse("5552345678").unwrap().display(),
        "+1 (555) 234-5678"
    );
    assert_eq!(
        PhoneNumber::parse("+44 20 7946 0958").unwrap().display(),
        "+442079460958"
    );
}
