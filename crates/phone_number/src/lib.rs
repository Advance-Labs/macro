#![deny(missing_docs)]

//! E.164 phone numbers shared by telephony and CRM.
//!
//! [`PhoneNumber`] is always stored, compared, and sent to carriers in E.164
//! form (`+15551234567`). People rarely type that form, so
//! [`DialablePhoneNumber::parse`] also accepts the ways numbers are usually
//! written or pasted: formatted national numbers (`(555) 123-4567`), the
//! `00` and `011` international prefixes, `tel:` URIs, and a trailing
//! extension (`+1 555 123 4567 ext. 89`).
//!
//! National numbers without a country code are read as North American
//! Numbering Plan numbers (`+1`), since there is no other locale to infer
//! one from. Everything else must carry its country code.

use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod test;

/// Fewest digits in an E.164 number, country code included (e.g. Niue `+683 4002`).
const MIN_DIGITS: usize = 7;
/// Most digits an E.164 number may have, country code included.
const MAX_DIGITS: usize = 15;
/// Most digits accepted in an extension.
const MAX_EXTENSION_DIGITS: usize = 10;
/// Country calling code of the North American Numbering Plan.
const NANP_COUNTRY_CODE: char = '1';
/// Digits in a NANP number after its country code: area code, exchange, line.
const NANP_NATIONAL_DIGITS: usize = 10;
/// The NANP international dialing prefix (`011 44 …` means `+44 …`).
const NANP_INTERNATIONAL_PREFIX: &str = "011";
/// The ITU international dialing prefix used outside North America.
const ITU_INTERNATIONAL_PREFIX: &str = "00";
/// How international formats mark a national trunk prefix to skip, as in `+44 (0)20 …`.
const NATIONAL_TRUNK_PREFIX_MARK: &str = "(0)";
/// Scheme of RFC 3966 telephone URIs.
const TEL_URI_SCHEME: &str = "tel:";
/// Extension markers, longest first so `ext.` wins over `x` inside `ext`.
const EXTENSION_MARKERS: [&str; 7] = [";ext=", "extension", "ext.", "ext", "x", "#", ","];

/// Why text could not be read as a phone number. The messages are written
/// for the person who typed the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PhoneNumberError {
    /// Nothing but whitespace was supplied.
    #[error("Enter a phone number")]
    Empty,
    /// A character that never appears in a phone number.
    #[error("Phone numbers can only contain digits, spaces, and + ( ) - . /")]
    InvalidCharacter,
    /// A national number that is not a valid North American number.
    #[error("Include the country code, for example +44 20 7946 0958")]
    MissingCountryCode,
    /// Too few or too many digits for any E.164 number.
    #[error("Phone numbers have 7 to 15 digits, including the country code")]
    InvalidLength,
    /// The digits after `+` start with 0, which no country code does.
    #[error("Country codes never start with 0")]
    InvalidCountryCode,
    /// A `+1` number whose area code or exchange cannot be dialed, such as
    /// the `N11` service codes (`911`).
    #[error("This isn't a valid North American phone number")]
    InvalidNorthAmericanNumber,
    /// An extension with no digits, non-digits, or too many digits.
    #[error("Extensions can only contain up to 10 digits")]
    InvalidExtension,
}

/// A validated E.164 phone number, such as `+15551234567`.
///
/// Serializes as the E.164 string. Deserialization only accepts E.164; use
/// [`DialablePhoneNumber::parse`] for numbers typed by people.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schema", schema(example = "+15552345678"))]
#[serde(try_from = "String", into = "String")]
pub struct PhoneNumber(String);

impl PhoneNumber {
    /// Accept only canonical E.164 text: `+` followed by digits. Use this
    /// for values Macro stored or a carrier reported.
    pub fn from_e164(value: &str) -> Result<Self, PhoneNumberError> {
        let digits = value
            .strip_prefix('+')
            .ok_or(PhoneNumberError::MissingCountryCode)?;
        if !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(PhoneNumberError::InvalidCharacter);
        }
        Self::from_international_digits(digits)
    }

    /// Read user-entered text that must be a bare number; extensions are
    /// rejected. See [`DialablePhoneNumber::parse`] for the accepted formats.
    pub fn parse(input: &str) -> Result<Self, PhoneNumberError> {
        let dialable = DialablePhoneNumber::parse(input)?;
        match dialable.extension {
            Some(_) => Err(PhoneNumberError::InvalidExtension),
            None => Ok(dialable.number),
        }
    }

    /// The E.164 form, e.g. `+15551234567`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The digits after the leading `+`, country code first.
    pub fn digits(&self) -> &str {
        &self.0[1..]
    }

    /// Whether the number's digits start with `prefix`, e.g. a country
    /// calling code (`"44"`) or a code plus area code (`"1900"`). E.164
    /// country codes are prefix-free, so this is an exact country test.
    pub fn starts_with_digits(&self, prefix: &str) -> bool {
        self.digits().starts_with(prefix)
    }

    /// Whether this is a North American Numbering Plan (`+1`) number.
    pub fn is_north_american(&self) -> bool {
        self.digits().starts_with(NANP_COUNTRY_CODE)
    }

    /// A readable rendering: `+1 (555) 123-4567` for North American
    /// numbers, the E.164 text otherwise (grouping other countries needs
    /// per-country metadata this crate deliberately does not carry).
    pub fn display(&self) -> String {
        if !self.is_north_american() {
            return self.0.clone();
        }
        let national = &self.digits()[1..];
        format!(
            "+1 ({}) {}-{}",
            &national[..3],
            &national[3..6],
            &national[6..]
        )
    }

    /// Validate digits that already include the country code.
    fn from_international_digits(digits: &str) -> Result<Self, PhoneNumberError> {
        if !(MIN_DIGITS..=MAX_DIGITS).contains(&digits.len()) {
            return Err(PhoneNumberError::InvalidLength);
        }
        if digits.starts_with('0') {
            return Err(PhoneNumberError::InvalidCountryCode);
        }
        if let Some(national) = digits.strip_prefix(NANP_COUNTRY_CODE) {
            validate_nanp_national(national)?;
        }
        Ok(Self(format!("+{digits}")))
    }
}

impl fmt::Display for PhoneNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for PhoneNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhoneNumber({})", self.0)
    }
}

impl TryFrom<String> for PhoneNumber {
    type Error = PhoneNumberError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_e164(&value)
    }
}

impl From<PhoneNumber> for String {
    fn from(value: PhoneNumber) -> Self {
        value.0
    }
}

/// Digits dialed after a call connects to reach a person behind a switchboard.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Extension(String);

impl Extension {
    /// The extension digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Extension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Extension({})", self.0)
    }
}

/// A number to dial, plus the extension to key in once it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialablePhoneNumber {
    /// The E.164 number the carrier connects to.
    pub number: PhoneNumber,
    /// Digits to send as DTMF tones after the call connects, if any.
    pub extension: Option<Extension>,
}

impl DialablePhoneNumber {
    /// Read a phone number the way people write it.
    ///
    /// Accepts E.164 (`+44 20 7946 0958`), the `00` and `011` international
    /// prefixes, ten- or eleven-digit North American numbers
    /// (`(555) 123-4567`, `1-555-123-4567`), `tel:` URIs, and an extension
    /// introduced by `ext`, `ext.`, `extension`, `x`, `#`, `,`, or `;ext=`.
    /// Spaces and `( ) - . /` are ignored.
    pub fn parse(input: &str) -> Result<Self, PhoneNumberError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(PhoneNumberError::Empty);
        }
        let without_scheme = strip_tel_scheme(trimmed);
        let (number_text, extension_text) = split_extension(without_scheme);
        let number = parse_number(number_text)?;
        let extension = extension_text.map(parse_extension).transpose()?;
        Ok(Self { number, extension })
    }
}

fn strip_tel_scheme(input: &str) -> &str {
    match input.get(..TEL_URI_SCHEME.len()) {
        Some(scheme) if scheme.eq_ignore_ascii_case(TEL_URI_SCHEME) => {
            &input[TEL_URI_SCHEME.len()..]
        }
        _ => input,
    }
}

/// Split text at the earliest extension marker. Markers are ASCII, so byte
/// offsets found in the lowercased copy are valid in the original.
fn split_extension(input: &str) -> (&str, Option<&str>) {
    let lowered = input.to_ascii_lowercase();
    let earliest = EXTENSION_MARKERS
        .iter()
        .filter_map(|marker| lowered.find(marker).map(|index| (index, marker.len())))
        .min_by_key(|(index, _)| *index);
    match earliest {
        Some((index, marker_len)) => (&input[..index], Some(&input[index + marker_len..])),
        None => (input, None),
    }
}

fn parse_number(text: &str) -> Result<PhoneNumber, PhoneNumberError> {
    let text = text.trim();
    let (has_plus, rest) = match text.strip_prefix('+') {
        // `+44 (0)20 …` marks the national trunk prefix, which is not dialed
        // internationally.
        Some(rest) => (true, rest.replace(NATIONAL_TRUNK_PREFIX_MARK, "")),
        None => (false, text.to_string()),
    };
    let mut digits = String::with_capacity(rest.len());
    for c in rest.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if !is_separator(c) {
            return Err(PhoneNumberError::InvalidCharacter);
        }
    }
    if digits.is_empty() {
        return Err(PhoneNumberError::Empty);
    }
    if has_plus {
        return PhoneNumber::from_international_digits(&digits);
    }
    if let Some(international) = digits
        .strip_prefix(NANP_INTERNATIONAL_PREFIX)
        .or_else(|| digits.strip_prefix(ITU_INTERNATIONAL_PREFIX))
    {
        return PhoneNumber::from_international_digits(international);
    }
    match digits.len() {
        NANP_NATIONAL_DIGITS => {
            validate_nanp_national(&digits)?;
            Ok(PhoneNumber(format!("+{NANP_COUNTRY_CODE}{digits}")))
        }
        len if len == NANP_NATIONAL_DIGITS + 1 && digits.starts_with(NANP_COUNTRY_CODE) => {
            PhoneNumber::from_international_digits(&digits)
        }
        _ => Err(PhoneNumberError::MissingCountryCode),
    }
}

fn parse_extension(text: &str) -> Result<Extension, PhoneNumberError> {
    let digits = text.trim().trim_start_matches(['.', ':', ' ']).trim();
    if digits.is_empty()
        || digits.len() > MAX_EXTENSION_DIGITS
        || !digits.chars().all(|c| c.is_ascii_digit())
    {
        return Err(PhoneNumberError::InvalidExtension);
    }
    Ok(Extension(digits.to_string()))
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '.' | '(' | ')' | '/' | '\u{2010}'..='\u{2015}')
}

/// Validate the ten national digits of a `+1` number: the area code and the
/// exchange both start with 2–9, and the area code is not an `N11` service
/// code such as 911.
fn validate_nanp_national(national: &str) -> Result<(), PhoneNumberError> {
    let bytes = national.as_bytes();
    if bytes.len() != NANP_NATIONAL_DIGITS {
        return Err(PhoneNumberError::InvalidNorthAmericanNumber);
    }
    let starts_valid = |digit: u8| (b'2'..=b'9').contains(&digit);
    let is_service_code = bytes[1] == b'1' && bytes[2] == b'1';
    if !starts_valid(bytes[0]) || !starts_valid(bytes[3]) || is_service_code {
        return Err(PhoneNumberError::InvalidNorthAmericanNumber);
    }
    Ok(())
}
