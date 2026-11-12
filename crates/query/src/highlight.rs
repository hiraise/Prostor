//! Classes for highlighting, before names are resolved: a name is an attribute when an operator
//! follows it, a value is a keyword when the vocabulary knows it unquoted.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::lex::{TokenKind, lex};
use crate::problems::{ProblemCode, Severity};
use crate::syntax::{Atom, Expr, ItemKind, Query, Span};
use crate::words::{self, Kind, fold};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Attribute,
    Value,
    Keyword,
    Operator,
    Text,
    Punctuation,
    Error,
}

/// One class per token of `text` (the text `query` was parsed from), in order; an error node
/// covering several tokens gives one span. Tokens a problem points at are errors.
pub fn classes(query: &Query, text: &str) -> Vec<(Span, Class)> {
    let cut = query
        .problems
        .iter()
        .find(|p| p.code == ProblemCode::QueryTooLong)
        .map(|p| p.span);
    let body = cut.and_then(|s| text.get(..s.start)).unwrap_or(text);
    let mut claims = BTreeMap::new();
    let mut errors = Vec::new();
    if let Some(expr) = &query.expr {
        walk(expr, &mut claims, &mut errors);
    }
    if let Some(sort) = &query.sort {
        for key in &sort.keys {
            claims.insert(key.name.span.start, Class::Attribute);
        }
    }
    let flagged: Vec<Span> = query
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.span)
        .collect();

    let mut out = Vec::new();
    let mut last_error = None;
    for token in lex(body) {
        let span = token.span;
        if let Some(error) = errors
            .iter()
            .find(|e| e.start <= span.start && span.end <= e.end)
        {
            if last_error != Some(*error) {
                out.push((*error, Class::Error));
                last_error = Some(*error);
            }
            continue;
        }
        let class = if flagged.contains(&span) {
            Class::Error
        } else if let Some(class) = claims.get(&span.start) {
            *class
        } else {
            match token.kind {
                TokenKind::LParen
                | TokenKind::RParen
                | TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::DotDot => Class::Punctuation,
                TokenKind::Lt | TokenKind::Le | TokenKind::Gt | TokenKind::Ge | TokenKind::Bang => {
                    Class::Operator
                }
                TokenKind::Word => {
                    let folded = fold(body.get(span.start..span.end).unwrap_or(""));
                    if words::keyword(&folded, &[Kind::Operator]).is_some() {
                        Class::Operator
                    } else if words::keyword(&folded, &[Kind::Sort]).is_some() {
                        Class::Keyword
                    } else {
                        Class::Error
                    }
                }
                TokenKind::Quoted { .. } => Class::Error,
            }
        };
        out.push((span, class));
    }
    if let Some(cut) = cut {
        out.push((cut, Class::Error));
    }
    out
}

fn walk(expr: &Expr, claims: &mut BTreeMap<usize, Class>, errors: &mut Vec<Span>) {
    match expr {
        Expr::And(list) | Expr::Or(list) => {
            for child in list {
                walk(child, claims, errors);
            }
        }
        Expr::Not(inner, _) => walk(inner, claims, errors),
        Expr::Text(atom) => {
            claims.insert(atom.span.start, Class::Text);
        }
        Expr::Error(span) => errors.push(*span),
        Expr::Cond(cond) => {
            claims.insert(cond.name.span.start, Class::Attribute);
            for item in &cond.items {
                match &item.kind {
                    ItemKind::Value(atom) => value(atom, claims),
                    ItemKind::Range(from, to) => {
                        for atom in from.iter().chain(to) {
                            value(atom, claims);
                        }
                    }
                }
            }
        }
    }
}

fn value(atom: &Atom, claims: &mut BTreeMap<usize, Class>) {
    let keyword = !atom.quoted && words::keyword(&fold(&atom.text), words::VALUE).is_some();
    claims.insert(
        atom.span.start,
        if keyword {
            Class::Keyword
        } else {
            Class::Value
        },
    );
}
