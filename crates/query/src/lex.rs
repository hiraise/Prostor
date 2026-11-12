//! Tokens (plan §3.1).
//!
//! Whitespace separates tokens. `(` `)` `,` `:` `"` `<` `<=` `>` `>=` are symbols anywhere, `!`
//! only at the start of a token, and `..` splits a word (`2026-01..2026-03` is three tokens).
//! A word that starts with a digit keeps a `:` followed by a digit (`14:30`,
//! `2026-10-23T14:30`); anywhere else `:` ends the word.

use serde::{Deserialize, Serialize};

use crate::syntax::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenKind {
    Word,
    /// `"…"` with the escapes `\"` and `\\`; an unclosed quote runs to the end.
    Quoted {
        closed: bool,
    },
    LParen,
    RParen,
    Comma,
    Colon,
    Bang,
    Lt,
    Le,
    Gt,
    Ge,
    DotDot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let at = |i: usize| bytes.get(i).copied();
    let mut tokens = Vec::new();
    let mut i = 0;
    while let Some(c) = text.get(i..).and_then(|rest| rest.chars().next()) {
        let one = |kind| Token {
            kind,
            span: Span::new(i, i + 1),
        };
        let two = |kind| Token {
            kind,
            span: Span::new(i, i + 2),
        };
        let token = match c {
            c if c.is_whitespace() => {
                i += c.len_utf8();
                continue;
            }
            '(' => one(TokenKind::LParen),
            ')' => one(TokenKind::RParen),
            ',' => one(TokenKind::Comma),
            ':' => one(TokenKind::Colon),
            '!' => one(TokenKind::Bang),
            '<' if at(i + 1) == Some(b'=') => two(TokenKind::Le),
            '<' => one(TokenKind::Lt),
            '>' if at(i + 1) == Some(b'=') => two(TokenKind::Ge),
            '>' => one(TokenKind::Gt),
            '.' if at(i + 1) == Some(b'.') => two(TokenKind::DotDot),
            '"' => {
                // `"` and `\` are ASCII and never part of a multi-byte character.
                let mut j = i + 1;
                let mut closed = false;
                while let Some(b) = at(j) {
                    match b {
                        b'\\' if matches!(at(j + 1), Some(b'"' | b'\\')) => j += 2,
                        b'"' => {
                            closed = true;
                            j += 1;
                            break;
                        }
                        _ => j += 1,
                    }
                }
                Token {
                    kind: TokenKind::Quoted { closed },
                    span: Span::new(i, j),
                }
            }
            _ => {
                let digit_start = c.is_ascii_digit();
                let mut j = i;
                while let Some(ch) = text.get(j..).and_then(|rest| rest.chars().next()) {
                    let ends = match ch {
                        ch if ch.is_whitespace() => true,
                        '(' | ')' | ',' | '"' | '<' | '>' => true,
                        '.' => at(j + 1) == Some(b'.'),
                        ':' => !(digit_start && at(j + 1).is_some_and(|b| b.is_ascii_digit())),
                        _ => false,
                    };
                    if ends {
                        break;
                    }
                    j += ch.len_utf8();
                }
                Token {
                    kind: TokenKind::Word,
                    span: Span::new(i, j),
                }
            }
        };
        i = token.span.end;
        tokens.push(token);
    }
    tokens
}

/// The text inside quotes, without the escapes. Other backslashes stay as written.
pub fn unescape(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next @ ('"' | '\\')) = chars.peek()
        {
            out.push(next);
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

/// The text as a quoted string the lexer reads back to the same text.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(text).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn splits_symbols_and_ranges() {
        use TokenKind::*;
        assert_eq!(kinds("a:b"), [Word, Colon, Word]);
        assert_eq!(kinds("2026-01..2026-03"), [Word, DotDot, Word]);
        assert_eq!(
            kinds("x<=5 y>=6 z<1 w>2"),
            [
                Word, Le, Word, Word, Ge, Word, Word, Lt, Word, Word, Gt, Word
            ]
        );
        assert_eq!(kinds("!a!b"), [Bang, Word]);
        assert_eq!(
            kinds("14:30 2026-10-23T14:30 1:x"),
            [Word, Word, Word, Colon, Word]
        );
        assert_eq!(kinds("http://x"), [Word, Colon, Word]);
        assert_eq!(
            kinds(r#""a \" b" "open"#),
            [Quoted { closed: true }, Quoted { closed: false }]
        );
        assert_eq!(kinds("1.5 a..."), [Word, Word, DotDot, Word]);
    }

    #[test]
    fn quoting_round_trips() {
        for text in ["", "a b", r#"say "hi""#, r"back\slash\", r"\\"] {
            let quoted = quote(text);
            let tokens = lex(&quoted);
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, TokenKind::Quoted { closed: true });
            assert_eq!(unescape(&quoted[1..quoted.len() - 1]), text);
        }
        assert_eq!(unescape(r"a\nb"), r"a\nb");
    }
}
