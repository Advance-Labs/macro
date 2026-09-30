//! nom combinators over the token stream, one function per grammar rule.
//!
//! Errors: a parser that fails before consuming anything returns
//! `nom::Err::Error` so an `alt` can try the next branch; once a rule has
//! committed (seen its keyword), everything after is wrapped in [`cut`] so a
//! failure is final and carries the message written for that spot. The
//! message text is the product for the agent reading it, so every leaf
//! names what it expected.

use nom::branch::alt;
use nom::combinator::{cut, opt};
use nom::multi::separated_list1;
use nom::sequence::{delimited, preceded};
use nom::{IResult, Input, Parser};

use super::ParseError;
use super::ast::*;
use super::lexer::{Tok, Token};

/// The input: the statement's tokens, always ending in [`Tok::End`]. A
/// newtype because nom implements [`Input`] only for bytes and `&str`.
#[derive(Debug, Clone, Copy)]
pub struct Tokens<'a>(&'a [Token]);

impl<'a> std::ops::Deref for Tokens<'a> {
    type Target = [Token];
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> Input for Tokens<'a> {
    type Item = &'a Token;
    type Iter = std::slice::Iter<'a, Token>;
    type IterIndices = std::iter::Enumerate<std::slice::Iter<'a, Token>>;

    fn input_len(&self) -> usize {
        self.0.len()
    }
    fn take(&self, index: usize) -> Self {
        Tokens(&self.0[..index])
    }
    fn take_from(&self, index: usize) -> Self {
        Tokens(&self.0[index..])
    }
    fn take_split(&self, index: usize) -> (Self, Self) {
        let (head, tail) = self.0.split_at(index);
        (Tokens(tail), Tokens(head))
    }
    fn position<P: Fn(Self::Item) -> bool>(&self, predicate: P) -> Option<usize> {
        self.0.iter().position(predicate)
    }
    fn iter_elements(&self) -> Self::Iter {
        self.0.iter()
    }
    fn iter_indices(&self) -> Self::IterIndices {
        self.0.iter().enumerate()
    }
    fn slice_index(&self, count: usize) -> Result<usize, nom::Needed> {
        if count <= self.0.len() {
            Ok(count)
        } else {
            Err(nom::Needed::new(count - self.0.len()))
        }
    }
}

type In<'a> = Tokens<'a>;
type R<'a, O> = IResult<In<'a>, O, ParseError>;

impl nom::error::ParseError<Tokens<'_>> for ParseError {
    fn from_error_kind(input: Tokens<'_>, kind: nom::error::ErrorKind) -> Self {
        // Only reached through combinators we never leave a message on; the
        // leaves below always say what they expected.
        at(input, &format!("something else ({kind:?})"))
    }

    fn append(_: Tokens<'_>, _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

/// Parse one statement from its tokens.
pub fn statement(tokens: &[Token]) -> Result<Statement, ParseError> {
    match statement_rule(Tokens(tokens)) {
        Ok((_, statement)) => Ok(statement),
        Err(nom::Err::Error(error) | nom::Err::Failure(error)) => Err(error),
        Err(nom::Err::Incomplete(_)) => unreachable!("the token stream is complete"),
    }
}

// ---- errors ------------------------------------------------------------

/// `expected …, found …` at the next token.
fn at(input: In<'_>, expected: &str) -> ParseError {
    let token = input.first().expect("the End token is always there");
    ParseError {
        span: token.span.clone(),
        message: format!("expected {expected}, found {}", token.kind.describe()),
    }
}

/// A message that stands on its own (not `expected …, found …`) at the next
/// token.
fn message_at(input: In<'_>, message: &str) -> ParseError {
    let token = input.first().expect("the End token is always there");
    ParseError {
        span: token.span.clone(),
        message: message.into(),
    }
}

/// A parser that fails, recoverably, with `expected` at the next token.
fn fail<'a, O>(input: In<'a>, expected: &str) -> R<'a, O> {
    Err(nom::Err::Error(at(input, expected)))
}

/// Replace a recoverable failure's message with `expected`, keeping the
/// position. For an `alt` whose branches each know only their own keyword.
fn expecting<'a, O>(
    expected: &'static str,
    mut parser: impl Parser<In<'a>, Output = O, Error = ParseError>,
) -> impl FnMut(In<'a>) -> R<'a, O> {
    move |input| match parser.parse(input) {
        Err(nom::Err::Error(_)) => fail(input, expected),
        other => other,
    }
}

// ---- tokens ------------------------------------------------------------

/// The token `kind`, or a recoverable failure that names `expected`.
fn tok<'a>(kind: Tok, expected: &'static str) -> impl Fn(In<'a>) -> R<'a, ()> {
    move |input| match input.first() {
        Some(token) if token.kind == kind => Ok((input.take_from(1), ())),
        _ => fail(input, expected),
    }
}

/// The token `kind` as a branch discriminator; the message is never shown
/// because the enclosing `alt` supplies its own.
fn kw<'a>(kind: Tok) -> impl Fn(In<'a>) -> R<'a, ()> {
    tok(kind, "")
}

fn ident<'a>(expected: &'static str) -> impl Fn(In<'a>) -> R<'a, Ident> {
    move |input| match input.first().map(|token| &token.kind) {
        Some(Tok::Ident(name) | Tok::QuotedIdent(name)) => {
            Ok((input.take_from(1), Ident(name.clone())))
        }
        _ => fail(input, expected),
    }
}

fn string<'a>(expected: &'static str) -> impl Fn(In<'a>) -> R<'a, String> {
    move |input| match input.first().map(|token| &token.kind) {
        Some(Tok::Str(text)) => Ok((input.take_from(1), text.clone())),
        _ => fail(input, expected),
    }
}

fn lit(input: In<'_>) -> R<'_, Lit> {
    let rest = input.take_from(1);
    match input.first().map(|token| &token.kind) {
        Some(Tok::Str(text)) => Ok((rest, Lit::Str(text.clone()))),
        Some(Tok::Num(n)) => Ok((rest, Lit::Num(*n))),
        Some(Tok::True) => Ok((rest, Lit::Bool(true))),
        Some(Tok::False) => Ok((rest, Lit::Bool(false))),
        Some(Tok::Null) => Ok((rest, Lit::Null)),
        Some(Tok::Minus) => match rest.first().map(|token| &token.kind) {
            Some(Tok::Num(n)) => Ok((rest.take_from(1), Lit::Num(-n))),
            _ => Err(nom::Err::Failure(at(rest, "a number after -"))),
        },
        Some(Tok::Select) => Err(nom::Err::Failure(message_at(
            input,
            "subqueries are not supported: run the inner SELECT on its own first and use the values it returns",
        ))),
        Some(Tok::LParen) if rest.first().map(|token| &token.kind) == Some(&Tok::Select) => {
            Err(nom::Err::Failure(message_at(
                rest,
                "subqueries are not supported: run the inner SELECT on its own first and use the values it returns",
            )))
        }
        _ => fail(input, "a value: 'text', a number, TRUE, FALSE or NULL"),
    }
}

/// A literal, or `[lit, …]` for a multi-valued cell.
fn value(input: In<'_>) -> R<'_, Lit> {
    alt((
        delimited(
            kw(Tok::LBracket),
            cut(separated_list1(comma, lit)),
            cut(tok(Tok::RBracket, ", or ] in the list")),
        )
        .map(Lit::List),
        lit,
    ))
    .parse(input)
}

/// `column` or `alias.column`.
fn column_ref<'a>(expected: &'static str) -> impl Fn(In<'a>) -> R<'a, ColumnRef> {
    move |input| {
        let (input, first) = ident(expected)(input)?;
        match opt(preceded(
            kw(Tok::Dot),
            cut(ident("a column name after the .")),
        ))
        .parse(input)?
        {
            (input, Some(column)) => Ok((
                input,
                ColumnRef {
                    table: Some(first),
                    column,
                },
            )),
            (input, None) => Ok((
                input,
                ColumnRef {
                    table: None,
                    column: first,
                },
            )),
        }
    }
}

/// `[database.]table`.
fn table<'a>(expected: &'static str) -> impl Fn(In<'a>) -> R<'a, TableName> {
    move |input| {
        let (input, first) = ident(expected)(input)?;
        match opt(preceded(
            kw(Tok::Dot),
            cut(ident("a table name after the .")),
        ))
        .parse(input)?
        {
            (input, Some(table)) => Ok((
                input,
                TableName {
                    database: Some(first),
                    table,
                },
            )),
            (input, None) => Ok((
                input,
                TableName {
                    database: None,
                    table: first,
                },
            )),
        }
    }
}

fn comma(input: In<'_>) -> R<'_, ()> {
    kw(Tok::Comma)(input)
}

// ---- statements --------------------------------------------------------

fn statement_rule(input: In<'_>) -> R<'_, Statement> {
    let (input, statement) = expecting(
        "SELECT, INSERT, UPDATE or DELETE",
        alt((
            select.map(Statement::Select),
            insert.map(Statement::Insert),
            update.map(Statement::Update),
            delete.map(Statement::Delete),
        )),
    )(input)?;
    let (input, _) = opt(kw(Tok::Semi)).parse(input)?;
    let (input, _) = cut(tok(Tok::End, "end of statement")).parse(input)?;
    Ok((input, statement))
}

fn select(input: In<'_>) -> R<'_, Select> {
    let (input, _) = kw(Tok::Select)(input)?;
    let (input, distinct) = opt(kw(Tok::Distinct)).parse(input)?;
    let (input, (items, aliases)) = cut(items).parse(input)?;
    let (input, _) = cut(tok(Tok::From, "FROM")).parse(input)?;
    let (input, from) =
        cut(table_item("a table name after FROM, like database.table")).parse(input)?;
    let (input, joins) = nom::multi::many0(join).parse(input)?;
    if input.first().map(|token| &token.kind) == Some(&Tok::Comma) {
        return Err(nom::Err::Failure(message_at(
            input,
            "tables are combined with JOIN … ON a.column = b.row_id, not a comma",
        )));
    }
    let (input, where_) = opt(preceded(kw(Tok::Where), cut(cond))).parse(input)?;
    let (input, group_by) = opt(preceded(
        kw(Tok::Group),
        cut(preceded(
            tok(Tok::By, "BY after GROUP"),
            column_ref("a column name after GROUP BY"),
        )),
    ))
    .parse(input)?;
    let (input, order_by) = opt(preceded(
        kw(Tok::Order),
        cut(preceded(
            tok(Tok::By, "BY after ORDER"),
            separated_list1(comma, order_by),
        )),
    ))
    .parse(input)?;
    let (input, limit) = opt(preceded(
        kw(Tok::Limit),
        cut(count("a row count after LIMIT")),
    ))
    .parse(input)?;
    let (input, offset) = opt(preceded(
        kw(Tok::Offset),
        cut(count("a row count after OFFSET")),
    ))
    .parse(input)?;
    Ok((
        input,
        Select {
            distinct: distinct.is_some(),
            items,
            aliases,
            from,
            joins,
            where_,
            group_by,
            order_by: order_by.unwrap_or_default(),
            limit,
            offset,
        },
    ))
}

/// `table [[AS] alias]`.
fn table_item<'a>(expected: &'static str) -> impl Fn(In<'a>) -> R<'a, FromItem> {
    move |input| {
        let (input, table) = table(expected)(input)?;
        let (input, alias) = alt((
            preceded(kw(Tok::As), cut(ident("an alias after AS"))).map(Some),
            ident("an alias").map(Some),
            nom::combinator::success(None),
        ))
        .parse(input)?;
        Ok((input, FromItem { table, alias }))
    }
}

/// `[INNER] JOIN table ON …` or `LEFT [OUTER] JOIN table ON …`.
fn join(input: In<'_>) -> R<'_, Join> {
    let (input, kind) = alt((
        kw(Tok::Join).map(|()| JoinKind::Inner),
        preceded(kw(Tok::Inner), cut(tok(Tok::Join, "JOIN after INNER"))).map(|()| JoinKind::Inner),
        preceded(
            kw(Tok::Left),
            cut(preceded(
                opt(kw(Tok::Outer)),
                tok(Tok::Join, "JOIN after LEFT"),
            )),
        )
        .map(|()| JoinKind::Left),
    ))
    .parse(input)?;
    let (input, table) =
        cut(table_item("a table name after JOIN, like database.table")).parse(input)?;
    let (input, _) = cut(tok(Tok::On, "ON after the joined table")).parse(input)?;
    let (input, on) = cut(separated_list1(kw(Tok::And), join_equality)).parse(input)?;
    Ok((input, Join { kind, table, on }))
}

/// `column = column`, the only condition a join accepts. `column HAS column`
/// says the same thing about a multi-valued column: a join already matches
/// any one of a cell's values.
fn join_equality(input: In<'_>) -> R<'_, (ColumnRef, ColumnRef)> {
    let (input, left) = column_ref("a column to join on, like alias.column")(input)?;
    let (input, _) = cut(expecting(
        "= between the two join columns",
        alt((kw(Tok::Eq), kw(Tok::Has))),
    ))
    .parse(input)?;
    let (input, right) = cut(column_ref("a column of the other table after =")).parse(input)?;
    Ok((input, (left, right)))
}

/// The select list and the aliases some of its items were given.
type SelectList = (Vec<Item>, Vec<(usize, Ident)>);

fn items(input: In<'_>) -> R<'_, SelectList> {
    alt((
        kw(Tok::Star).map(|()| (vec![Item::Star], Vec::new())),
        separated_list1(comma, aliased_item).map(|entries| {
            let mut items = Vec::with_capacity(entries.len());
            let mut aliases = Vec::new();
            for (index, (item, alias)) in entries.into_iter().enumerate() {
                items.push(item);
                if let Some(alias) = alias {
                    aliases.push((index, alias));
                }
            }
            (items, aliases)
        }),
    ))
    .parse(input)
}

/// The name after `AS`: an identifier, or a keyword such as `count` when the
/// select list goes on after it (`,` or `FROM` follows), so `AS FROM` still
/// reads as a missing name.
fn alias_name(input: In<'_>) -> R<'_, Ident> {
    let continues = matches!(
        input.get(1).map(|token| &token.kind),
        Some(Tok::Comma | Tok::From)
    );
    match input.first().and_then(|token| token.kind.keyword_name()) {
        Some(name) if continues => Ok((input.take_from(1), Ident(name))),
        _ => ident("a name for the column after AS")(input),
    }
}

/// `item [[AS] name]`.
fn aliased_item(input: In<'_>) -> R<'_, (Item, Option<Ident>)> {
    let (input, item) = item(input)?;
    let (input, alias) = alt((
        preceded(kw(Tok::As), cut(alias_name)).map(Some),
        opt(ident("an alias")),
    ))
    .parse(input)?;
    Ok((input, (item, alias)))
}

fn item(input: In<'_>) -> R<'_, Item> {
    alt((
        agg.map(Item::Agg),
        column_ref("a column name, an aggregate like COUNT(*) or SUM(column), or *")
            .map(Item::Column),
    ))
    .parse(input)
}

/// `COUNT(*)` or `FUNC(column)`. Only a branch if the name is followed by
/// `(`: `count` alone is a column.
fn agg(input: In<'_>) -> R<'_, Agg> {
    let func = match input.first().map(|token| &token.kind) {
        Some(Tok::Count) => AggFn::Count,
        Some(Tok::Sum) => AggFn::Sum,
        Some(Tok::Avg) => AggFn::Avg,
        Some(Tok::Min) => AggFn::Min,
        Some(Tok::Max) => AggFn::Max,
        _ => return fail(input, "an aggregate"),
    };
    if input.get(1).map(|token| &token.kind) != Some(&Tok::LParen) {
        return fail(input, "an aggregate");
    }
    let input = input.take_from(2);
    let (input, arg) = if func == AggFn::Count {
        alt((
            kw(Tok::Star).map(|()| None),
            column_ref("* or a column name inside COUNT(…)").map(Some),
        ))
        .parse(input)?
    } else {
        column_ref("a column name inside the aggregate")
            .map(Some)
            .parse(input)?
    };
    let (input, _) = cut(tok(Tok::RParen, ") to close the aggregate")).parse(input)?;
    Ok((input, Agg { func, arg }))
}

fn order_by(input: In<'_>) -> R<'_, OrderBy> {
    let (input, key) = match input.first().map(|token| &token.kind) {
        Some(Tok::Num(n)) => {
            if n.fract() != 0.0 || *n < 1.0 {
                return fail(
                    input,
                    "a column name or a 1-based select-list position after ORDER BY",
                );
            }
            (input.take_from(1), OrderKey::Position(*n as u32))
        }
        _ => alt((
            agg.map(OrderKey::Agg),
            column_ref("a column name, an aggregate, or a select-list position after ORDER BY")
                .map(OrderKey::Column),
        ))
        .parse(input)?,
    };
    let (input, dir) = alt((
        kw(Tok::Desc).map(|()| Dir::Desc),
        opt(kw(Tok::Asc)).map(|_| Dir::Asc),
    ))
    .parse(input)?;
    Ok((input, OrderBy { key, dir }))
}

// ---- conditions --------------------------------------------------------

fn cond(input: In<'_>) -> R<'_, Cond> {
    separated_list1(kw(Tok::Or), and_chain)
        .map(|parts| flatten(parts, Cond::Or))
        .parse(input)
}

fn and_chain(input: In<'_>) -> R<'_, Cond> {
    separated_list1(kw(Tok::And), term)
        .map(|parts| flatten(parts, Cond::And))
        .parse(input)
}

fn term(input: In<'_>) -> R<'_, Cond> {
    alt((
        delimited(
            kw(Tok::LParen),
            cut(cond),
            cut(tok(Tok::RParen, ") to close the condition")),
        ),
        atom,
    ))
    .parse(input)
}

fn atom(input: In<'_>) -> R<'_, Cond> {
    let (input, column) = column_ref("a column name to compare")(input)?;
    let (input, negated) = opt(kw(Tok::Not)).map(|not| not.is_some()).parse(input)?;

    // The forms that take NOT: `NOT IN`, `NOT HAS`, `NOT LIKE`.
    let negatable = |column: &ColumnRef| {
        let column = column.clone();
        alt((
            preceded(
                kw(Tok::In),
                cut(delimited(
                    tok(Tok::LParen, "( after IN"),
                    separated_list1(comma, lit),
                    tok(Tok::RParen, ", or ) in the IN list"),
                )),
            )
            .map({
                let column = column.clone();
                move |values| Cond::In {
                    column: column.clone(),
                    values,
                    negated,
                }
            }),
            preceded(kw(Tok::Has), cut(lit)).map({
                let column = column.clone();
                move |value| Cond::Has {
                    column: column.clone(),
                    value,
                    negated,
                }
            }),
            preceded(kw(Tok::Like), cut(string("a quoted pattern after LIKE"))).map(
                move |pattern| Cond::Like {
                    column: column.clone(),
                    pattern,
                    negated,
                },
            ),
        ))
    };

    if negated {
        let expected = format!("IN, HAS or LIKE after \"{}\" NOT", column.column.0);
        return match negatable(&column).parse(input) {
            Err(nom::Err::Error(_)) => Err(nom::Err::Failure(at(input, &expected))),
            other => other,
        };
    }

    let is_null = preceded(
        kw(Tok::Is),
        cut((opt(kw(Tok::Not)), tok(Tok::Null, "NULL after IS"))),
    )
    .map({
        let column = column.clone();
        move |(not, ())| Cond::IsNull {
            column: column.clone(),
            negated: not.is_some(),
        }
    });
    let compare = (cmp_op, cut(lit)).map({
        let column = column.clone();
        move |(op, value)| Cond::Cmp {
            column: column.clone(),
            op,
            value,
        }
    });
    let expected = format!(
        "a comparison operator, IN, HAS, IS or LIKE after \"{}\"",
        column.column.0
    );
    match alt((negatable(&column), is_null, compare)).parse(input) {
        Err(nom::Err::Error(_)) => Err(nom::Err::Failure(at(input, &expected))),
        other => other,
    }
}

/// A non-negative whole number.
fn count<'a>(expected: &'static str) -> impl Fn(In<'a>) -> R<'a, u32> {
    move |input| match input.first().map(|token| &token.kind) {
        Some(Tok::Num(n)) if n.fract() == 0.0 && *n >= 0.0 => Ok((input.take_from(1), *n as u32)),
        _ => fail(input, expected),
    }
}

fn cmp_op(input: In<'_>) -> R<'_, CmpOp> {
    let op = match input.first().map(|token| &token.kind) {
        Some(Tok::Eq) => CmpOp::Eq,
        Some(Tok::Ne) => CmpOp::Ne,
        Some(Tok::Lt) => CmpOp::Lt,
        Some(Tok::Le) => CmpOp::Le,
        Some(Tok::Gt) => CmpOp::Gt,
        Some(Tok::Ge) => CmpOp::Ge,
        _ => return fail(input, "a comparison operator"),
    };
    Ok((input.take_from(1), op))
}

/// One element stays itself; several become the combining node.
fn flatten(mut parts: Vec<Cond>, combine: fn(Vec<Cond>) -> Cond) -> Cond {
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        combine(parts)
    }
}

// ---- writes ------------------------------------------------------------

fn insert(input: In<'_>) -> R<'_, Insert> {
    let (input, _) = kw(Tok::Insert)(input)?;
    let (input, _) = cut(tok(Tok::Into, "INTO after INSERT")).parse(input)?;
    let (input, table) = cut(table("a table name after INTO, like database.table")).parse(input)?;
    if let Ok((input, ())) = kw(Tok::Default)(input) {
        let (input, ()) = cut(tok(Tok::Values, "VALUES after DEFAULT")).parse(input)?;
        return Ok((
            input,
            Insert {
                table,
                columns: Vec::new(),
                rows: vec![Vec::new()],
            },
        ));
    }
    let (input, columns) = cut(delimited(
        tok(
            Tok::LParen,
            "( and the column list, or DEFAULT VALUES, after the table name",
        ),
        separated_list1(comma, ident("a column name in the column list")),
        tok(Tok::RParen, ", or ) in the column list"),
    ))
    .parse(input)?;
    let (input, _) = cut(tok(Tok::Values, "VALUES after the column list")).parse(input)?;
    let mut rows = Vec::new();
    let mut input = input;
    loop {
        let (rest, row) = cut(|i| row(i, columns.len(), rows.len() + 1)).parse(input)?;
        rows.push(row);
        match comma(rest) {
            Ok((rest, ())) => input = rest,
            Err(_) => {
                input = rest;
                break;
            }
        }
    }
    Ok((
        input,
        Insert {
            table,
            columns,
            rows,
        },
    ))
}

/// One `(v, …)` row, which must be as wide as the column list.
fn row(input: In<'_>, width: usize, number: usize) -> R<'_, Vec<Lit>> {
    let start = input
        .first()
        .map(|token| token.span.start)
        .unwrap_or_default();
    let (rest, values) = delimited(
        tok(Tok::LParen, "( to start a row of values"),
        cut(separated_list1(comma, value)),
        cut(tok(Tok::RParen, ", or ) in the row of values")),
    )
    .parse(input)?;
    if values.len() != width {
        let consumed = input.len() - rest.len();
        let end = input[consumed - 1].span.end;
        return Err(nom::Err::Failure(ParseError {
            span: start..end,
            message: format!(
                "row {number} has {} values but {width} columns were listed",
                values.len()
            ),
        }));
    }
    Ok((rest, values))
}

fn update(input: In<'_>) -> R<'_, Update> {
    let (input, _) = kw(Tok::Update)(input)?;
    let (input, table) =
        cut(table("a table name after UPDATE, like database.table")).parse(input)?;
    let (input, _) = cut(tok(Tok::Set, "SET after the table name")).parse(input)?;
    let (input, assignments) = cut(separated_list1(comma, assignment)).parse(input)?;
    let (input, row_id) = cut(|i| row_id_clause(i, "UPDATE")).parse(input)?;
    Ok((
        input,
        Update {
            table,
            assignments,
            row_id,
        },
    ))
}

fn assignment(input: In<'_>) -> R<'_, (Ident, Lit)> {
    let (input, column) = ident("a column name to set")(input)?;
    let (input, _) = match kw(Tok::Eq)(input) {
        Ok(ok) => ok,
        Err(_) => {
            return Err(nom::Err::Failure(at(
                input,
                &format!("= after \"{}\"", column.0),
            )));
        }
    };
    let (input, value) = cut(value).parse(input)?;
    Ok((input, (column, value)))
}

fn delete(input: In<'_>) -> R<'_, Delete> {
    let (input, _) = kw(Tok::Delete)(input)?;
    let (input, _) = cut(tok(Tok::From, "FROM after DELETE")).parse(input)?;
    let (input, table) = cut(table("a table name after FROM, like database.table")).parse(input)?;
    let (input, row_id) = cut(|i| row_id_clause(i, "DELETE")).parse(input)?;
    Ok((input, Delete { table, row_id }))
}

/// `WHERE row_id = 'id'`, the only condition a write accepts.
fn row_id_clause<'a>(input: In<'a>, statement: &str) -> R<'a, String> {
    let (input, _) = match kw(Tok::Where)(input) {
        Ok(ok) => ok,
        Err(_) => {
            return Err(nom::Err::Failure(at(
                input,
                &format!("WHERE row_id = '<id>' ({statement} changes one row at a time)"),
            )));
        }
    };
    let input = match input.first().map(|token| &token.kind) {
        Some(Tok::Ident(name)) if name.eq_ignore_ascii_case("row_id") => input.take_from(1),
        _ => {
            return Err(nom::Err::Failure(at(
                input,
                &format!("row_id ({statement} changes one row at a time)"),
            )));
        }
    };
    let (input, _) = cut(tok(Tok::Eq, "= after row_id")).parse(input)?;
    cut(string("a quoted row id")).parse(input)
}
