//! Converting a column's cells to another type, one cell at a time, after
//! `database_sql::cast` has said the change can work at all. A cell that
//! does not fit is a [`Misfit`]: counted and quoted when the change is
//! refused, emptied when the caller asked to clear what does not fit.

use super::*;
use crate::domain::catalog::PropertyType;
use crate::domain::models::RowId;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_value::PropertyValue;

/// The most characters of a value quoted in a refusal.
const MAX_EXAMPLE_LEN: usize = 40;
/// The most values quoted per kind of misfit.
const MAX_EXAMPLES: usize = 3;

#[derive(Debug)]
pub(super) enum ConvertedCell {
    Value(PropertyValue),
    Options(Vec<String>),
}

/// Why one cell's value does not fit the new type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Misfit {
    NotText,
    NotNumber,
    NotDate,
    NotCheckbox,
    NotUrl,
    NotOption,
    OptionInOtherCase,
    OtherReference,
    SeveralValues,
}

impl Misfit {
    /// What is counted: values, or cells for a cell with several values.
    fn noun(self, one: bool) -> &'static str {
        match (self, one) {
            (Misfit::SeveralValues, true) => "cell",
            (Misfit::SeveralValues, false) => "cells",
            (_, true) => "value",
            (_, false) => "values",
        }
    }

    /// What is wrong with them.
    fn predicate(self, one: bool) -> &'static str {
        let (singular, plural) = match self {
            Misfit::NotText => ("can't be written as text", "can't be written as text"),
            Misfit::NotNumber => ("isn't a number", "aren't numbers"),
            Misfit::NotDate => ("isn't a date", "aren't dates"),
            Misfit::NotCheckbox => ("isn't true or false", "aren't true or false"),
            Misfit::NotUrl => ("isn't a complete URL", "aren't complete URLs"),
            Misfit::NotOption => ("can't be an option", "can't be options"),
            Misfit::OptionInOtherCase => (
                "differs from another only in capitalization",
                "differ from others only in capitalization",
            ),
            Misfit::OtherReference => (
                "points at a different kind of item",
                "point at a different kind of item",
            ),
            Misfit::SeveralValues => ("has more than one value", "have more than one value"),
        };
        if one { singular } else { plural }
    }
}

/// Converts the cells of one column to one target type.
pub(super) struct Converter<'a> {
    source: &'a PropertyDefinitionWithOptions,
    target: PropertyType,
    clear_invalid: bool,
    /// The converted cells, in the order they were pushed.
    pub cells: Vec<(RowId, ConvertedCell)>,
    /// Every option label the new column needs, first spelling first.
    pub labels: Vec<String>,
    /// Without clearing: each cell that did not fit, with its value as text.
    misfits: Vec<(Misfit, String)>,
    /// With clearing: cells emptied.
    pub cleared: usize,
    /// With clearing: cells cut down to their first value.
    pub trimmed: usize,
}

impl<'a> Converter<'a> {
    /// A converter to `target`. When both types take options, the source's
    /// options come along, used or not, as long as they fit.
    pub fn new(
        source: &'a PropertyDefinitionWithOptions,
        target: PropertyType,
        clear_invalid: bool,
    ) -> Self {
        let mut converter = Converter {
            source,
            target,
            clear_invalid,
            cells: Vec::new(),
            labels: Vec::new(),
            misfits: Vec::new(),
            cleared: 0,
            trimmed: 0,
        };
        if takes_options(source.definition.data_type) && takes_options(target.data_type) {
            for (_, label) in catalog::option_labels(source) {
                if let Ok(label) = converter.label(PropertyValue::Str(label)) {
                    converter.adopt(vec![label]);
                }
            }
        }
        converter
    }

    /// Convert one row's cell.
    pub fn push(&mut self, row: RowId, value: &PropertyValue) {
        if is_empty(value) {
            return;
        }
        match self.convert(value) {
            Ok((cell, trimmed)) => {
                if let ConvertedCell::Options(labels) = &cell {
                    self.adopt(labels.clone());
                }
                self.trimmed += usize::from(trimmed);
                self.cells.push((row, cell));
            }
            Err(_) if self.clear_invalid => self.cleared += 1,
            Err(misfit) => self.misfits.push((misfit, self.example(value))),
        }
    }

    /// How many cells did not fit.
    pub fn failures(&self) -> usize {
        self.misfits.len()
    }

    /// Up to three quoted values per kind of misfit, in the order they came.
    pub fn examples(&self) -> Vec<String> {
        self.groups()
            .into_iter()
            .flat_map(|(_, _, examples)| examples)
            .collect()
    }

    /// `3 values aren't numbers`, for a menu; `None` when every cell fit.
    pub fn summary(&self) -> Option<String> {
        let groups = self.groups();
        (!groups.is_empty()).then(|| {
            groups
                .iter()
                .map(|(misfit, count, _)| {
                    let one = *count == 1;
                    format!("{count} {} {}", misfit.noun(one), misfit.predicate(one))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
    }

    /// Why the change is refused, naming the column, the misfits, and the
    /// way forward; `None` when every cell fit.
    pub fn refusal(&self, column: &str) -> Option<String> {
        let groups = self.groups();
        if groups.is_empty() {
            return None;
        }
        let mut sentences: Vec<String> = groups
            .iter()
            .map(|(misfit, count, examples)| {
                let one = *count == 1;
                let quoted: Vec<String> = examples
                    .iter()
                    .map(|example| format!("'{example}'"))
                    .collect();
                format!(
                    "{count} {} in \"{column}\" {}: {}.",
                    misfit.noun(one),
                    misfit.predicate(one),
                    quoted.join(", ")
                )
            })
            .collect();
        let one = self.misfits.len() == 1;
        let several =
            |(misfit, _, _): &(Misfit, usize, Vec<String>)| *misfit == Misfit::SeveralValues;
        sentences.push(
            match (groups.iter().any(several), groups.iter().all(several), one) {
                (false, _, true) => "Fix it, or convert with clearing to empty it.",
                (false, _, false) => "Fix them, or convert with clearing to empty them.",
                (true, true, true) => {
                    "Fix it, or convert with clearing to keep only its first value."
                }
                (true, true, false) => {
                    "Fix them, or convert with clearing to keep only their first values."
                }
                (true, false, _) => {
                    "Fix them, or convert with clearing: values that don't fit are emptied, and \
                     cells with several values keep their first."
                }
            }
            .to_owned(),
        );
        Some(sentences.join(" "))
    }

    /// The misfits by kind, in the order each kind first came: its count and
    /// up to three examples.
    fn groups(&self) -> Vec<(Misfit, usize, Vec<String>)> {
        let mut groups: Vec<(Misfit, usize, Vec<String>)> = Vec::new();
        for (misfit, example) in &self.misfits {
            let index = match groups.iter().position(|(kind, _, _)| kind == misfit) {
                Some(index) => index,
                None => {
                    groups.push((*misfit, 0, Vec::new()));
                    groups.len() - 1
                }
            };
            let (_, count, examples) = &mut groups[index];
            *count += 1;
            if examples.len() < MAX_EXAMPLES {
                examples.push(example.clone());
            }
        }
        groups
    }

    /// Keep the labels a converted cell needs.
    fn adopt(&mut self, labels: Vec<String>) {
        for label in labels {
            if !self.labels.contains(&label) {
                self.labels.push(label);
            }
        }
    }

    /// The converted cell, and whether it lost all but its first value.
    fn convert(&self, value: &PropertyValue) -> Result<(ConvertedCell, bool), Misfit> {
        if let PropertyValue::EntityRef(references) = value {
            return self.convert_references(references);
        }
        let mut values = self.values(value)?;
        let mut trimmed = false;
        if values.len() > 1 && !self.target_is_multi() {
            if !self.clear_invalid {
                return Err(Misfit::SeveralValues);
            }
            values.truncate(1);
            trimmed = true;
        }
        let cell = if takes_options(self.target.data_type) {
            ConvertedCell::Options(
                values
                    .into_iter()
                    .map(|value| self.label(value))
                    .collect::<Result<_, _>>()?,
            )
        } else if self.target.data_type == DataType::Link {
            ConvertedCell::Value(PropertyValue::Link(
                values.into_iter().map(url).collect::<Result<_, _>>()?,
            ))
        } else {
            let value = values.into_iter().next().ok_or(Misfit::NotOption)?;
            ConvertedCell::Value(self.scalar(value)?)
        };
        Ok((cell, trimmed))
    }

    fn convert_references(
        &self,
        references: &[models_properties::shared::EntityReference],
    ) -> Result<(ConvertedCell, bool), Misfit> {
        if self.target.data_type != DataType::Entity
            || references
                .iter()
                .any(|reference| Some(reference.entity_type) != self.target.specific_entity_type)
        {
            return Err(Misfit::OtherReference);
        }
        if references.len() > 1 && !self.target.is_multi_select {
            if !self.clear_invalid {
                return Err(Misfit::SeveralValues);
            }
            return Ok((
                ConvertedCell::Value(PropertyValue::EntityRef(references[..1].to_vec())),
                true,
            ));
        }
        Ok((
            ConvertedCell::Value(PropertyValue::EntityRef(references.to_vec())),
            false,
        ))
    }

    fn target_is_multi(&self) -> bool {
        self.target.is_multi_select
            && matches!(
                self.target.data_type,
                DataType::SelectString | DataType::SelectNumber | DataType::Tag | DataType::Link
            )
    }

    /// A cell's values one by one: an option cell's labels, a link cell's
    /// URLs, or the one value.
    fn values(&self, value: &PropertyValue) -> Result<Vec<PropertyValue>, Misfit> {
        Ok(match value {
            PropertyValue::SelectOption(ids) => ids
                .iter()
                .map(|id| {
                    let option = self
                        .source
                        .property_options
                        .iter()
                        .find(|option| option.id == *id)
                        .ok_or(Misfit::NotOption)?;
                    Ok(match &option.value {
                        PropertyOptionValue::String(value) => PropertyValue::Str(value.clone()),
                        PropertyOptionValue::Number(value) => PropertyValue::Num(*value),
                    })
                })
                .collect::<Result<_, Misfit>>()?,
            PropertyValue::Link(urls) => urls.iter().cloned().map(PropertyValue::Str).collect(),
            value => vec![value.clone()],
        })
    }

    /// A value as an option label of the target.
    fn label(&self, value: PropertyValue) -> Result<String, Misfit> {
        let label = if self.target.data_type == DataType::SelectNumber {
            number(value).map_err(|_| Misfit::NotOption)?.to_string()
        } else {
            text(value).ok_or(Misfit::NotOption)?
        };
        if label.trim() != label || label.is_empty() || label.chars().count() > MAX_OPTION_LABEL_LEN
        {
            return Err(Misfit::NotOption);
        }
        let taken = self
            .labels
            .iter()
            .any(|existing| existing != &label && option_key(existing) == option_key(&label));
        if taken {
            return Err(Misfit::OptionInOtherCase);
        }
        Ok(label)
    }

    /// One value as a text, number, checkbox or date cell.
    fn scalar(&self, value: PropertyValue) -> Result<PropertyValue, Misfit> {
        match self.target.data_type {
            DataType::String => text(value).map(PropertyValue::Str).ok_or(Misfit::NotText),
            DataType::Number => number(value).map(PropertyValue::Num),
            DataType::Boolean => match value {
                PropertyValue::Bool(value) => Ok(PropertyValue::Bool(value)),
                PropertyValue::Str(value) if value == "true" || value == "false" => {
                    Ok(PropertyValue::Bool(value == "true"))
                }
                _ => Err(Misfit::NotCheckbox),
            },
            DataType::Date => match value {
                PropertyValue::Date(value) => Ok(PropertyValue::Date(value)),
                PropertyValue::Str(value) => date(&value).map(PropertyValue::Date),
                _ => Err(Misfit::NotDate),
            },
            DataType::Entity => Err(Misfit::OtherReference),
            DataType::Link | DataType::SelectString | DataType::SelectNumber | DataType::Tag => {
                unreachable!("options and links are converted by convert")
            }
        }
    }

    /// A cell's value as a refusal quotes it.
    fn example(&self, value: &PropertyValue) -> String {
        let text = match value {
            PropertyValue::EntityRef(references) => references
                .iter()
                .map(|reference| reference.entity_id.clone())
                .collect::<Vec<_>>()
                .join(", "),
            value => self
                .values(value)
                .unwrap_or_default()
                .into_iter()
                .filter_map(text)
                .collect::<Vec<_>>()
                .join(", "),
        };
        match text.char_indices().nth(MAX_EXAMPLE_LEN) {
            Some((end, _)) => format!("{}…", &text[..end]),
            None => text,
        }
    }
}

/// Whether a stored value is an empty cell.
pub(super) fn is_empty(value: &PropertyValue) -> bool {
    matches!(value, PropertyValue::Str(value) if value.is_empty())
        || matches!(value, PropertyValue::SelectOption(value) if value.is_empty())
        || matches!(value, PropertyValue::EntityRef(value) if value.is_empty())
        || matches!(value, PropertyValue::Link(value) if value.is_empty())
}

/// A number, written exactly: no padding, rounding or leading zeros.
fn number(value: PropertyValue) -> Result<f64, Misfit> {
    match value {
        PropertyValue::Num(value) if value.is_finite() => Ok(value),
        PropertyValue::Str(value) => value
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite() && number.to_string() == value)
            .ok_or(Misfit::NotNumber),
        _ => Err(Misfit::NotNumber),
    }
}

/// A value as text. A date the grid shows as a calendar day (midnight
/// UTC) reads `YYYY-MM-DD`; one with a time keeps it.
fn text(value: PropertyValue) -> Option<String> {
    match value {
        PropertyValue::Str(value) => Some(value),
        PropertyValue::Num(value) if value.is_finite() => Some(value.to_string()),
        PropertyValue::Bool(value) => Some(value.to_string()),
        PropertyValue::Date(value) if value.time() == chrono::NaiveTime::MIN => {
            Some(value.date_naive().to_string())
        }
        PropertyValue::Date(value) => Some(value.to_rfc3339()),
        _ => None,
    }
}

/// `YYYY-MM-DD` or a complete ISO date and time.
fn date(value: &str) -> Result<chrono::DateTime<Utc>, Misfit> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .filter(|date| date.to_string() == value)
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|date| date.and_utc())
        })
        .ok_or(Misfit::NotDate)
}

/// A complete http or https URL.
fn url(value: PropertyValue) -> Result<String, Misfit> {
    let text = text(value).ok_or(Misfit::NotUrl)?;
    match url::Url::parse(&text) {
        Ok(url) if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() => Ok(text),
        _ => Err(Misfit::NotUrl),
    }
}
