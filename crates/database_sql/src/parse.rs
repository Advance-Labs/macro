//! Stage one: SQL text to the subset AST.
//!
//! ```text
//! statement := select | insert | update | delete
//! select    := SELECT items FROM table [WHERE cond] [GROUP BY ident]
//!              [ORDER BY order {, order}]
//! items     := '*' | item {, item}
//! item      := ident | agg
//! agg       := COUNT '(' '*' ')' | (COUNT|SUM|AVG|MIN|MAX) '(' ident ')'
//! table     := [ident '.'] ident
//! order     := (ident | agg | int) [ASC | DESC]
//! cond      := and {OR and}
//! and       := term {AND term}
//! term      := '(' cond ')' | atom
//! atom      := ident cmp lit
//!            | ident [NOT] IN '(' lit {, lit} ')'
//!            | ident [NOT] HAS lit
//!            | ident IS [NOT] NULL
//!            | ident [NOT] LIKE string
//! lit       := string | number | TRUE | FALSE | NULL
//! insert    := INSERT INTO table '(' ident {, ident} ')' VALUES row {, row}
//! row       := '(' lit {, lit} ')'
//! update    := UPDATE table SET ident '=' lit {, ident '=' lit} WHERE row_id '=' string
//! delete    := DELETE FROM table WHERE row_id '=' string
//! ```
//!
//! Keywords are case-insensitive; identifiers keep their case. A trailing
//! `;` is allowed. Everything else SQL has (joins, subqueries, aliases,
//! arithmetic, functions beyond the five aggregates, `LIMIT`, `HAVING`, an
//! `UPDATE`/`DELETE` over anything but one row id) is a
//! parse error with a span and a message written for the agent that sent it.

pub mod ast;
mod lexer;
mod parser;
#[cfg(test)]
mod test;

use std::ops::Range;

pub use ast::*;

/// Why a statement could not be parsed, with the byte range it points at.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message} at {span:?}")]
pub struct ParseError {
    /// Byte range in the source the message is about. Empty at end of input.
    pub span: Range<usize>,
    /// What was expected and what was found, in words an agent can act on.
    pub message: String,
}

/// Parse one statement.
pub fn parse(sql: &str) -> Result<Statement, ParseError> {
    let tokens = lexer::lex(sql)?;
    parser::Parser::new(sql, tokens).statement()
}
