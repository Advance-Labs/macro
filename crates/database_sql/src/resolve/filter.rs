//! Typing literals against columns and checking each condition form against
//! the column's kind.

use chrono::{DateTime, NaiveDate, Utc};

use crate::catalog::{Column, ColumnKind};
use crate::parse::{CmpOp, Cond, Lit};

use super::names::Scope;
use super::{Filter, ResolveError, Value};

pub fn resolve(scope: &mut Scope<'_>, cond: Cond) -> Result<Filter, ResolveError> {
    match cond {
        Cond::And(parts) => parts
            .into_iter()
            .map(|part| resolve(scope, part))
            .collect::<Result<_, _>>()
            .map(Filter::And),
        Cond::Or(parts) => parts
            .into_iter()
            .map(|part| resolve(scope, part))
            .collect::<Result<_, _>>()
            .map(Filter::Or),
        Cond::Cmp { column, op, value } => {
            let bound = scope.column(&column)?;
            let column = &bound.column;
            if column.kind.is_multi() {
                return Err(ResolveError::EqualityOnMultiValued {
                    column: column.name.clone(),
                });
            }
            if value == Lit::Null {
                return Err(ResolveError::CompareToNull {
                    column: column.name.clone(),
                });
            }
            check_operator(column, op)?;
            Ok(Filter::Cmp {
                column: bound.key,
                op,
                value: typed(column, value)?,
            })
        }
        Cond::In {
            column,
            values,
            negated,
        } => {
            let bound = scope.column(&column)?;
            let column = &bound.column;
            if column.kind.is_multi() {
                return Err(ResolveError::EqualityOnMultiValued {
                    column: column.name.clone(),
                });
            }
            let values = values
                .into_iter()
                .map(|value| {
                    if value == Lit::Null {
                        return Err(ResolveError::CompareToNull {
                            column: column.name.clone(),
                        });
                    }
                    typed(column, value)
                })
                .collect::<Result<_, _>>()?;
            Ok(Filter::In {
                column: bound.key,
                values,
                negated,
            })
        }
        Cond::Has {
            column,
            value,
            negated,
        } => {
            let bound = scope.column(&column)?;
            let column = &bound.column;
            if !column.kind.is_multi() {
                return Err(ResolveError::HasOnSingleValued {
                    column: column.name.clone(),
                });
            }
            Ok(Filter::Has {
                column: bound.key,
                value: typed(column, value)?,
                negated,
            })
        }
        Cond::IsNull { column, negated } => {
            let bound = scope.column(&column)?;
            Ok(Filter::IsNull {
                column: bound.key,
                negated,
            })
        }
        Cond::Like {
            column,
            pattern,
            negated,
        } => {
            let bound = scope.column(&column)?;
            let column = &bound.column;
            match column.kind {
                ColumnKind::Text | ColumnKind::Link => Ok(Filter::Like {
                    column: bound.key,
                    pattern,
                    negated,
                }),
                _ => Err(ResolveError::OperatorNotSupported {
                    column: column.name.clone(),
                    op: "LIKE",
                    supported: "LIKE only applies to text columns",
                }),
            }
        }
    }
}

/// Which operators a column's kind defines.
fn check_operator(column: &Column, op: CmpOp) -> Result<(), ResolveError> {
    let ordered = matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge);
    let supported = match column.kind {
        ColumnKind::Text | ColumnKind::Link | ColumnKind::Number | ColumnKind::Date => {
            return Ok(());
        }
        ColumnKind::Boolean => "checkbox columns support = and !=",
        ColumnKind::Select { .. } => "select columns support =, != and IN",
        ColumnKind::Entity { .. } => "entity columns support =, != and IN",
    };
    if ordered {
        return Err(ResolveError::OperatorNotSupported {
            column: column.name.clone(),
            op: op.symbol(),
            supported,
        });
    }
    Ok(())
}

impl CmpOp {
    /// The operator as written.
    pub fn symbol(self) -> &'static str {
        self.into()
    }
}

/// Type a literal for the column it is compared to or stored in.
pub fn typed(column: &Column, lit: Lit) -> Result<Value, ResolveError> {
    let mismatch = |expected, hint| ResolveError::TypeMismatch {
        column: column.name.clone(),
        expected,
        hint,
    };
    match (&column.kind, lit) {
        (ColumnKind::Text | ColumnKind::Link, Lit::Str(text)) => Ok(Value::Text(text)),
        (ColumnKind::Text, _) => Err(mismatch("text", "compare it to quoted 'text'")),
        (ColumnKind::Link, _) => Err(mismatch("link", "compare it to a quoted 'URL'")),
        (ColumnKind::Number, Lit::Num(n)) => Ok(Value::Number(n)),
        (ColumnKind::Number, _) => Err(mismatch("number", "compare it to a number")),
        (ColumnKind::Boolean, Lit::Bool(b)) => Ok(Value::Bool(b)),
        (ColumnKind::Boolean, _) => Err(mismatch("checkbox", "compare it to TRUE or FALSE")),
        (ColumnKind::Date, Lit::Str(text)) => parse_date(&text).map(Value::Date).ok_or_else(|| {
            mismatch(
                "date",
                "compare it to an ISO date like '2026-09-01' or '2026-09-01T09:00:00Z'",
            )
        }),
        (ColumnKind::Date, _) => Err(mismatch(
            "date",
            "compare it to an ISO date like '2026-09-01' or '2026-09-01T09:00:00Z'",
        )),
        (ColumnKind::Select { options, .. }, Lit::Str(label)) => options
            .iter()
            .find(|option| option.label.eq_ignore_ascii_case(&label))
            .map(|option| Value::Option(option.id))
            .ok_or_else(|| ResolveError::UnknownOption {
                column: column.name.clone(),
                label,
                options: options.iter().map(|option| option.label.clone()).collect(),
            }),
        (ColumnKind::Select { .. }, _) => {
            Err(mismatch("select", "compare it to a quoted option label"))
        }
        (ColumnKind::Entity { .. }, Lit::Str(id)) if is_entity_id(&id) => Ok(Value::Entity(id)),
        (ColumnKind::Entity { .. }, _) => Err(mismatch(
            "entity",
            "give an id like 'macro|sam@example.com', not a name",
        )),
    }
}

/// Macro entity ids are either a UUID or `<kind>|<rest>` (`macro|sam@example.com`,
/// `bot|<uuid>`). A bare name is what an agent writes when it has not looked
/// the person up.
fn is_entity_id(text: &str) -> bool {
    uuid::Uuid::parse_str(text).is_ok()
        || text
            .split_once('|')
            .is_some_and(|(kind, rest)| !kind.is_empty() && !rest.is_empty())
}

/// `2026-09-01` (midnight UTC) or any RFC 3339 date-time.
fn parse_date(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(datetime) = DateTime::parse_from_rfc3339(text) {
        return Some(datetime.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|datetime| datetime.and_utc())
}
