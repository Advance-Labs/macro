use filter_ast::Expr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The possible literal values in a database row filter AST.
///
/// Rows are **off by default**: a query that says nothing about them gets
/// none, so Home, Search and Recent never list them. Naming a table or a row
/// is the opt-in.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum DatabaseRowLiteral {
    /// Rows of this table.
    #[serde(rename = "t")]
    TableId(Uuid),
    /// This row.
    #[serde(rename = "id")]
    Id(Uuid),
}

/// Whether the expression asks for rows at all: some table or row is named
/// outside a `NOT`. A missing filter or a solely negative one keeps rows out.
pub fn database_rows_requested(filter: Option<&Expr<DatabaseRowLiteral>>) -> bool {
    match filter {
        Some(Expr::Literal(_)) => true,
        Some(Expr::And(a, b) | Expr::Or(a, b)) => {
            database_rows_requested(Some(a)) || database_rows_requested(Some(b))
        }
        Some(Expr::Not(_)) | None => false,
    }
}
