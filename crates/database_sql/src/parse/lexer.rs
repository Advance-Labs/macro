//! Tokens. Keywords win over identifiers; identifiers keep their case.

use std::ops::Range;

use logos::Logos;

use super::ParseError;

/// A token kind. String-carrying variants hold the decoded text (quotes and
/// escapes removed).
#[derive(Logos, Debug, Clone, PartialEq, strum::IntoStaticStr)]
#[logos(skip r"[ \t\r\n]+")]
#[strum(serialize_all = "UPPERCASE")]
pub enum Tok {
    #[regex("(?i)select")]
    Select,
    #[regex("(?i)distinct")]
    Distinct,
    #[regex("(?i)from")]
    From,
    #[regex("(?i)as")]
    As,
    #[regex("(?i)join")]
    Join,
    #[regex("(?i)inner")]
    Inner,
    #[regex("(?i)left")]
    Left,
    #[regex("(?i)outer")]
    Outer,
    #[regex("(?i)on")]
    On,
    // Reserved so it can never be read as a table alias; the parser rejects
    // it with a message the agent can act on.
    #[regex("(?i)limit")]
    Limit,
    #[regex("(?i)offset")]
    Offset,
    #[regex("(?i)default")]
    Default,
    #[regex("(?i)where")]
    Where,
    #[regex("(?i)group")]
    Group,
    #[regex("(?i)order")]
    Order,
    #[regex("(?i)by")]
    By,
    #[regex("(?i)asc")]
    Asc,
    #[regex("(?i)desc")]
    Desc,
    #[regex("(?i)and")]
    And,
    #[regex("(?i)or")]
    Or,
    #[regex("(?i)not")]
    Not,
    #[regex("(?i)in")]
    In,
    #[regex("(?i)has")]
    Has,
    #[regex("(?i)is")]
    Is,
    #[regex("(?i)null")]
    Null,
    #[regex("(?i)like")]
    Like,
    #[regex("(?i)true")]
    True,
    #[regex("(?i)false")]
    False,
    #[regex("(?i)insert")]
    Insert,
    #[regex("(?i)into")]
    Into,
    #[regex("(?i)values")]
    Values,
    #[regex("(?i)update")]
    Update,
    #[regex("(?i)set")]
    Set,
    #[regex("(?i)delete")]
    Delete,
    #[regex("(?i)count")]
    Count,
    #[regex("(?i)sum")]
    Sum,
    #[regex("(?i)avg")]
    Avg,
    #[regex("(?i)min")]
    Min,
    #[regex("(?i)max")]
    Max,

    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,
    #[token("!=")]
    #[token("<>")]
    Ne,
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("(")]
    LParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("*")]
    Star,
    #[token("-")]
    Minus,
    #[token(";")]
    Semi,

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_owned())]
    Ident(String),
    #[regex(r#""([^"]|"")*""#, |lex| unquote(lex.slice(), '"'))]
    QuotedIdent(String),
    #[regex(r"'([^']|'')*'", |lex| unquote(lex.slice(), '\''))]
    Str(String),
    #[regex(r"([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?", |lex| lex.slice().parse().ok())]
    Num(f64),
    /// Never produced by the lexer: appended by [`lex`] so every parser sees
    /// a token at the end of the statement, with the statement's end as its
    /// span.
    End,
}

/// Strip the surrounding quotes and collapse doubled quotes.
fn unquote(slice: &str, quote: char) -> String {
    let inner = &slice[1..slice.len() - 1];
    let doubled = format!("{quote}{quote}");
    inner.replace(&doubled, &quote.to_string())
}

/// A token with the byte range it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: Tok,
    pub span: Range<usize>,
}

/// Tokenize the whole input, failing on the first character no token matches.
pub fn lex(sql: &str) -> Result<Vec<Token>, ParseError> {
    let mut lexer = Tok::lexer(sql);
    let mut tokens = Vec::new();
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        match result {
            Ok(kind) => tokens.push(Token { kind, span }),
            Err(()) => {
                let found = &sql[span.clone()];
                let message = match found.chars().next() {
                    Some(quote @ ('"' | '\'')) => {
                        format!("unterminated quote starting at {quote}")
                    }
                    _ => format!("unexpected character {found:?}"),
                };
                return Err(ParseError { span, message });
            }
        }
    }
    tokens.push(Token {
        kind: Tok::End,
        span: sql.len()..sql.len(),
    });
    Ok(tokens)
}

impl Tok {
    /// How the token reads in an error message.
    /// The keyword's name in lower case, for a keyword used as a name after
    /// `AS`; `None` for every other token.
    pub fn keyword_name(&self) -> Option<String> {
        match self {
            Tok::Ident(_)
            | Tok::QuotedIdent(_)
            | Tok::Str(_)
            | Tok::Num(_)
            | Tok::Le
            | Tok::Ge
            | Tok::Ne
            | Tok::Eq
            | Tok::Lt
            | Tok::Gt
            | Tok::LParen
            | Tok::LBracket
            | Tok::RBracket
            | Tok::RParen
            | Tok::Comma
            | Tok::Dot
            | Tok::Star
            | Tok::Minus
            | Tok::Semi
            | Tok::End => None,
            keyword => Some(<&'static str>::from(keyword).to_lowercase()),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(name) => format!("\"{name}\""),
            Tok::QuotedIdent(name) => format!("\"{name}\""),
            Tok::Str(text) => format!("'{text}'"),
            Tok::Num(n) => n.to_string(),
            Tok::Le => "<=".into(),
            Tok::Ge => ">=".into(),
            Tok::Ne => "!=".into(),
            Tok::Eq => "=".into(),
            Tok::Lt => "<".into(),
            Tok::Gt => ">".into(),
            Tok::LParen => "(".into(),
            Tok::LBracket => "[".into(),
            Tok::RBracket => "]".into(),
            Tok::RParen => ")".into(),
            Tok::Comma => ",".into(),
            Tok::Dot => ".".into(),
            Tok::Star => "*".into(),
            Tok::Minus => "-".into(),
            Tok::Semi => ";".into(),
            Tok::End => "end of statement".into(),
            keyword => <&'static str>::from(keyword).into(),
        }
    }
}
