//! Recursive descent over the token stream, one function per grammar rule.
//! Every error message is written here, deliberately, for the agent reading
//! it.

use std::ops::Range;

use super::ParseError;
use super::ast::*;
use super::lexer::{Tok, Token};

pub struct Parser<'a> {
    sql: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(sql: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            sql,
            tokens,
            pos: 0,
        }
    }

    // ---- token access -------------------------------------------------

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn peek_at(&self, offset: usize) -> Option<&Tok> {
        self.tokens.get(self.pos + offset).map(|t| &t.kind)
    }

    fn bump(&mut self) -> Option<Tok> {
        let token = self.tokens.get(self.pos).map(|t| t.kind.clone());
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    /// Consume the token if it is `kind`.
    fn eat(&mut self, kind: Tok) -> bool {
        if self.peek() == Some(&kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Consume `kind` or fail with `expected`.
    fn expect(&mut self, kind: Tok, expected: &str) -> Result<(), ParseError> {
        if self.eat(kind) {
            Ok(())
        } else {
            Err(self.error(expected))
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Span of the current token, or the empty range at end of input.
    fn current_span(&self) -> Range<usize> {
        match self.tokens.get(self.pos) {
            Some(token) => token.span.clone(),
            None => self.sql.len()..self.sql.len(),
        }
    }

    /// `expected …, found …` at the current token.
    fn error(&self, expected: &str) -> ParseError {
        let found = match self.peek() {
            Some(token) => token.describe(),
            None => "end of statement".into(),
        };
        ParseError {
            span: self.current_span(),
            message: format!("expected {expected}, found {found}"),
        }
    }

    // ---- statements ---------------------------------------------------

    pub fn statement(mut self) -> Result<Statement, ParseError> {
        let statement = match self.peek() {
            Some(Tok::Select) => Statement::Select(self.select()?),
            Some(Tok::Insert) => Statement::Insert(self.insert()?),
            _ => return Err(self.error("SELECT or INSERT")),
        };
        self.eat(Tok::Semi);
        if !self.at_end() {
            return Err(self.error("end of statement"));
        }
        Ok(statement)
    }

    fn select(&mut self) -> Result<Select, ParseError> {
        self.expect(Tok::Select, "SELECT")?;
        let items = self.items()?;
        self.expect(Tok::From, "FROM")?;
        let table = self.table()?;
        let where_ = if self.eat(Tok::Where) {
            Some(self.cond()?)
        } else {
            None
        };
        let group_by = if self.eat(Tok::Group) {
            self.expect(Tok::By, "BY after GROUP")?;
            Some(self.ident("a column name after GROUP BY")?)
        } else {
            None
        };
        let order_by = if self.eat(Tok::Order) {
            self.expect(Tok::By, "BY after ORDER")?;
            self.order_list()?
        } else {
            Vec::new()
        };
        Ok(Select {
            items,
            table,
            where_,
            group_by,
            order_by,
        })
    }

    fn items(&mut self) -> Result<Vec<Item>, ParseError> {
        if self.eat(Tok::Star) {
            return Ok(vec![Item::Star]);
        }
        let mut items = vec![self.item()?];
        while self.eat(Tok::Comma) {
            items.push(self.item()?);
        }
        Ok(items)
    }

    fn item(&mut self) -> Result<Item, ParseError> {
        if self.is_agg_start() {
            return Ok(Item::Agg(self.agg()?));
        }
        Ok(Item::Column(self.ident(
            "a column name, an aggregate like COUNT(*) or SUM(column), or *",
        )?))
    }

    fn is_agg_start(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Count | Tok::Sum | Tok::Avg | Tok::Min | Tok::Max)
        ) && self.peek_at(1) == Some(&Tok::LParen)
    }

    fn agg(&mut self) -> Result<Agg, ParseError> {
        let func = match self.bump() {
            Some(Tok::Count) => AggFn::Count,
            Some(Tok::Sum) => AggFn::Sum,
            Some(Tok::Avg) => AggFn::Avg,
            Some(Tok::Min) => AggFn::Min,
            Some(Tok::Max) => AggFn::Max,
            _ => unreachable!("agg() is only called after is_agg_start()"),
        };
        self.expect(Tok::LParen, "( after the aggregate name")?;
        let arg = if func == AggFn::Count && self.eat(Tok::Star) {
            None
        } else {
            Some(self.ident(if func == AggFn::Count {
                "* or a column name inside COUNT(…)"
            } else {
                "a column name inside the aggregate"
            })?)
        };
        self.expect(Tok::RParen, ") to close the aggregate")?;
        Ok(Agg { func, arg })
    }

    fn table(&mut self) -> Result<TableName, ParseError> {
        let first = self.ident("a table name after FROM, like database.table")?;
        if self.eat(Tok::Dot) {
            let table = self.ident("a table name after the .")?;
            Ok(TableName {
                database: Some(first),
                table,
            })
        } else {
            Ok(TableName {
                database: None,
                table: first,
            })
        }
    }

    fn order_list(&mut self) -> Result<Vec<OrderBy>, ParseError> {
        let mut keys = vec![self.order_by()?];
        while self.eat(Tok::Comma) {
            keys.push(self.order_by()?);
        }
        Ok(keys)
    }

    fn order_by(&mut self) -> Result<OrderBy, ParseError> {
        let key =
            if self.is_agg_start() {
                OrderKey::Agg(self.agg()?)
            } else if let Some(Tok::Num(n)) = self.peek() {
                let n = *n;
                if n.fract() != 0.0 || n < 1.0 {
                    return Err(self
                        .error("a column name or a 1-based select-list position after ORDER BY"));
                }
                self.bump();
                OrderKey::Position(n as u32)
            } else {
                OrderKey::Column(self.ident(
                    "a column name, an aggregate, or a select-list position after ORDER BY",
                )?)
            };
        let dir = if self.eat(Tok::Desc) {
            Dir::Desc
        } else {
            self.eat(Tok::Asc);
            Dir::Asc
        };
        Ok(OrderBy { key, dir })
    }

    // ---- conditions ---------------------------------------------------

    fn cond(&mut self) -> Result<Cond, ParseError> {
        let mut alternatives = vec![self.and_chain()?];
        while self.eat(Tok::Or) {
            alternatives.push(self.and_chain()?);
        }
        Ok(flatten(alternatives, Cond::Or))
    }

    fn and_chain(&mut self) -> Result<Cond, ParseError> {
        let mut conjuncts = vec![self.term()?];
        while self.eat(Tok::And) {
            conjuncts.push(self.term()?);
        }
        Ok(flatten(conjuncts, Cond::And))
    }

    fn term(&mut self) -> Result<Cond, ParseError> {
        if self.eat(Tok::LParen) {
            let inner = self.cond()?;
            self.expect(Tok::RParen, ") to close the condition")?;
            return Ok(inner);
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Cond, ParseError> {
        let column = self.ident("a column name to compare")?;
        let negated = self.eat(Tok::Not);
        match self.peek() {
            Some(Tok::In) => {
                self.bump();
                self.expect(Tok::LParen, "( after IN")?;
                let mut values = vec![self.lit()?];
                while self.eat(Tok::Comma) {
                    values.push(self.lit()?);
                }
                self.expect(Tok::RParen, ", or ) in the IN list")?;
                Ok(Cond::In {
                    column,
                    values,
                    negated,
                })
            }
            Some(Tok::Has) => {
                self.bump();
                let value = self.lit()?;
                Ok(Cond::Has {
                    column,
                    value,
                    negated,
                })
            }
            Some(Tok::Like) => {
                self.bump();
                let pattern = match self.bump() {
                    Some(Tok::Str(pattern)) => pattern,
                    _ => {
                        self.pos -= 1;
                        return Err(self.error("a quoted pattern after LIKE"));
                    }
                };
                Ok(Cond::Like {
                    column,
                    pattern,
                    negated,
                })
            }
            Some(Tok::Is) if !negated => {
                self.bump();
                let negated = self.eat(Tok::Not);
                self.expect(Tok::Null, "NULL after IS")?;
                Ok(Cond::IsNull { column, negated })
            }
            _ if negated => Err(self.error(&format!("IN, HAS or LIKE after \"{}\" NOT", column.0))),
            Some(op @ (Tok::Eq | Tok::Ne | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge)) => {
                let op = match op {
                    Tok::Eq => CmpOp::Eq,
                    Tok::Ne => CmpOp::Ne,
                    Tok::Lt => CmpOp::Lt,
                    Tok::Le => CmpOp::Le,
                    Tok::Gt => CmpOp::Gt,
                    _ => CmpOp::Ge,
                };
                self.bump();
                let value = self.lit()?;
                Ok(Cond::Cmp { column, op, value })
            }
            _ => Err(self.error(&format!(
                "a comparison operator, IN, HAS, IS or LIKE after \"{}\"",
                column.0
            ))),
        }
    }

    // ---- leaves -------------------------------------------------------

    fn ident(&mut self, expected: &str) -> Result<Ident, ParseError> {
        match self.peek() {
            Some(Tok::Ident(name) | Tok::QuotedIdent(name)) => {
                let name = name.clone();
                self.bump();
                Ok(Ident(name))
            }
            _ => Err(self.error(expected)),
        }
    }

    fn lit(&mut self) -> Result<Lit, ParseError> {
        match self.peek() {
            Some(Tok::Str(s)) => {
                let s = s.clone();
                self.bump();
                Ok(Lit::Str(s))
            }
            Some(Tok::Num(n)) => {
                let n = *n;
                self.bump();
                Ok(Lit::Num(n))
            }
            Some(Tok::Minus) => {
                self.bump();
                match self.peek() {
                    Some(Tok::Num(n)) => {
                        let n = -*n;
                        self.bump();
                        Ok(Lit::Num(n))
                    }
                    _ => Err(self.error("a number after -")),
                }
            }
            Some(Tok::True) => {
                self.bump();
                Ok(Lit::Bool(true))
            }
            Some(Tok::False) => {
                self.bump();
                Ok(Lit::Bool(false))
            }
            Some(Tok::Null) => {
                self.bump();
                Ok(Lit::Null)
            }
            _ => Err(self.error("a value: 'text', a number, TRUE, FALSE or NULL")),
        }
    }

    // ---- insert -------------------------------------------------------

    fn insert(&mut self) -> Result<Insert, ParseError> {
        self.expect(Tok::Insert, "INSERT")?;
        self.expect(Tok::Into, "INTO after INSERT")?;
        let table = self.table()?;
        self.expect(Tok::LParen, "( and the column list after the table name")?;
        let mut columns = vec![self.ident("a column name in the column list")?];
        while self.eat(Tok::Comma) {
            columns.push(self.ident("a column name after the ,")?);
        }
        self.expect(Tok::RParen, ", or ) in the column list")?;
        self.expect(Tok::Values, "VALUES after the column list")?;
        let mut rows = vec![self.row(columns.len(), 1)?];
        while self.eat(Tok::Comma) {
            rows.push(self.row(columns.len(), rows.len() + 1)?);
        }
        Ok(Insert {
            table,
            columns,
            rows,
        })
    }

    fn row(&mut self, width: usize, number: usize) -> Result<Vec<Lit>, ParseError> {
        let start = self.current_span().start;
        self.expect(Tok::LParen, "( to start a row of values")?;
        let mut values = vec![self.lit()?];
        while self.eat(Tok::Comma) {
            values.push(self.lit()?);
        }
        self.expect(Tok::RParen, ", or ) in the row of values")?;
        if values.len() != width {
            let end = self.tokens[self.pos - 1].span.end;
            return Err(ParseError {
                span: start..end,
                message: format!(
                    "row {number} has {} values but {width} columns were listed",
                    values.len()
                ),
            });
        }
        Ok(values)
    }
}

/// One element stays itself; several become the combining node.
fn flatten(mut parts: Vec<Cond>, combine: fn(Vec<Cond>) -> Cond) -> Cond {
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        combine(parts)
    }
}
