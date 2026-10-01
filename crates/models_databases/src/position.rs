//! Positions: the fractional keys tables, columns, rows, views and cards are
//! ordered by. A key sorts as plain bytes, so text columns compare them under
//! `COLLATE "C"` and the browser compares them as strings. The server and the
//! browser (through the engine's wasm build) mint them with the same code.

#[cfg(test)]
mod test;

use fractional_index::FractionalIndex;

/// Why a key could not be minted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PositionError {
    /// A bound is not a key this module minted.
    #[error("`{0}` is not a position key")]
    NotAKey(String),
    /// The lower bound does not sort before the upper one.
    #[error("`{before}` does not sort before `{after}`")]
    OutOfOrder {
        /// The lower bound.
        before: String,
        /// The upper bound.
        after: String,
    },
}

/// A key that sorts after `before` and before `after`; either bound may be
/// left out to place it first or last, and with neither it is the first key
/// of an empty list.
pub fn key_between(before: Option<&str>, after: Option<&str>) -> Result<String, PositionError> {
    let lower = before.map(parse).transpose()?;
    let upper = after.map(parse).transpose()?;
    FractionalIndex::new(lower.as_ref(), upper.as_ref())
        .map(|key| key.to_string())
        .ok_or_else(|| PositionError::OutOfOrder {
            before: before.unwrap_or_default().to_string(),
            after: after.unwrap_or_default().to_string(),
        })
}

/// `count` keys in order between `before` and `after`, bisecting so their
/// length grows with the logarithm of `count` rather than with `count`.
pub fn keys_between(
    before: Option<&str>,
    after: Option<&str>,
    count: usize,
) -> Result<Vec<String>, PositionError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let middle = key_between(before, after)?;
    let lower_half = count / 2;
    let mut keys = keys_between(before, Some(&middle), lower_half)?;
    keys.push(middle.clone());
    keys.extend(keys_between(Some(&middle), after, count - lower_half - 1)?);
    Ok(keys)
}

fn parse(key: &str) -> Result<FractionalIndex, PositionError> {
    FractionalIndex::from_string(key).map_err(|_| PositionError::NotAKey(key.to_string()))
}
