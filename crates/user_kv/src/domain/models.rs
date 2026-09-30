//! Domain models for the per-user key-value store.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Maximum length of a namespace or key.
pub const MAX_SLUG_LEN: usize = 64;

/// A JSON object stored under a key. Values are always objects so every use
/// case can add fields later without changing the value's type.
pub type KvValue = serde_json::Map<String, serde_json::Value>;

/// Whether `value` is a slug: 1 to [MAX_SLUG_LEN] characters of `a-z`, `0-9`,
/// `_`, `.`, or `-`, starting with a letter or digit. Matches the table's
/// check constraints.
fn is_slug(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= MAX_SLUG_LEN
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
}

macro_rules! slug_type {
    ($(#[$doc:meta])* $name:ident, $label:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parse a slug, rejecting anything the table would refuse.
            pub fn parse(value: impl Into<String>) -> Result<Self, UserKvError> {
                let value = value.into();
                if is_slug(&value) {
                    Ok(Self(value))
                } else {
                    Err(UserKvError::BadRequest(format!(
                        concat!(
                            "invalid ",
                            $label,
                            " {:?}: use 1-{} characters of a-z, 0-9, '_', '.', or '-', starting with a letter or digit"
                        ),
                        value, MAX_SLUG_LEN
                    )))
                }
            }

            /// The slug as a string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = UserKvError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

slug_type!(
    /// Groups the entries of one use case, e.g. `tours`.
    KvNamespace,
    "namespace"
);

slug_type!(
    /// Identifies an entry within its namespace, e.g. `calendar`.
    KvKey,
    "key"
);

/// One stored entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct UserKvEntry {
    /// The entry's namespace.
    pub namespace: KvNamespace,
    /// The entry's key within its namespace.
    pub key: KvKey,
    /// The stored JSON object.
    #[cfg_attr(feature = "inbound", schema(value_type = Object))]
    pub value: KvValue,
    /// When the entry was first written.
    pub created_at: DateTime<Utc>,
    /// When the entry was last written.
    pub updated_at: DateTime<Utc>,
}

/// Errors returned by the user key-value service.
#[derive(Debug, thiserror::Error)]
pub enum UserKvError {
    /// No entry exists for this namespace and key.
    #[error("entry not found")]
    NotFound,
    /// The namespace, key, or value was invalid.
    #[error("{0}")]
    BadRequest(String),
    /// The value is larger than the service allows.
    #[error("value is {size} bytes; the limit is {limit} bytes")]
    ValueTooLarge {
        /// Size of the rejected value, as compact JSON.
        size: usize,
        /// The limit it exceeded.
        limit: usize,
    },
    /// Writing a new key would exceed the per-user entry limit.
    #[error("cannot store more than {limit} entries")]
    EntryLimitReached {
        /// The per-user entry limit.
        limit: usize,
    },
    /// Any other internal error.
    #[error("internal user kv error: {0:?}")]
    Internal(rootcause::Report),
}

impl From<rootcause::Report> for UserKvError {
    fn from(report: rootcause::Report) -> Self {
        UserKvError::Internal(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_accept_lowercase_digits_and_separators() {
        for ok in ["tours", "calendar", "a", "0", "view.tours-v2_x"] {
            assert!(KvKey::parse(ok).is_ok(), "{ok} should parse");
        }
    }

    #[test]
    fn slugs_reject_empty_uppercase_leading_separators_and_long_values() {
        let long = "a".repeat(MAX_SLUG_LEN + 1);
        for bad in [
            "",
            "Tours",
            "-tours",
            ".x",
            "has space",
            "slash/key",
            long.as_str(),
        ] {
            assert!(
                matches!(KvNamespace::parse(bad), Err(UserKvError::BadRequest(_))),
                "{bad:?} should be rejected"
            );
        }
        assert!(KvNamespace::parse("a".repeat(MAX_SLUG_LEN)).is_ok());
    }

    #[test]
    fn slugs_validate_when_deserialized() {
        assert!(serde_json::from_str::<KvKey>("\"calendar\"").is_ok());
        assert!(serde_json::from_str::<KvKey>("\"Calendar\"").is_err());
    }
}
