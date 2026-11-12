//! The tree of a query, with byte spans into its text.

use serde::{Deserialize, Serialize};

use crate::print::{Role, significant};
use crate::problems::{Problem, Severity};

/// A byte range into the query text. Converted to UTF-16 offsets before it leaves Rust.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The smallest span covering both.
    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub expr: Option<Expr>,
    pub sort: Option<Sort>,
    /// Ordered by position.
    pub problems: Vec<Problem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    And(Vec<Expr>),
    Or(Vec<Expr>),
    /// The span is the operator's (`!` or `not`).
    Not(Box<Expr>, Span),
    Cond(Condition),
    Text(Atom),
    Error(Span),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub name: Atom,
    pub op: Op,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Is,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub excluded: bool,
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Value(Atom),
    /// Inclusive; a missing side is open.
    Range(Option<Atom>, Option<Atom>),
}

/// A name, value or text term. `text` is unescaped; `span` covers the quotes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Atom {
    pub text: String,
    pub quoted: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub keys: Vec<SortKey>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortKey {
    pub name: Atom,
    pub direction: Option<Direction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    Desc,
}

impl Op {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Is => ":",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

impl Direction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::And(list) | Self::Or(list) => match (list.first(), list.last()) {
                (Some(first), Some(last)) => first.span().to(last.span()),
                _ => Span::default(),
            },
            Self::Not(inner, op) => op.to(inner.span()),
            Self::Cond(cond) => cond.span,
            Self::Text(atom) => atom.span,
            Self::Error(span) => *span,
        }
    }
}

impl Query {
    /// Any problem of severity error: the query does not run.
    pub fn has_errors(&self) -> bool {
        self.problems.iter().any(|p| p.severity == Severity::Error)
    }

    /// A compact form of the tree without spans, for tests and debugging:
    /// `(or (is 'state' 'open' !'fixed') (not 'text')) (sort 'votes' desc)`. Atoms show as `'x'`,
    /// or `"x"` when their quoting changes the meaning (a quoted keyword, a literal operator word).
    pub fn outline(&self) -> String {
        let mut parts = Vec::new();
        if let Some(expr) = &self.expr {
            parts.push(outline_expr(expr));
        }
        if let Some(sort) = &self.sort {
            let mut out = String::from("(sort");
            for key in &sort.keys {
                out.push(' ');
                out.push_str(&show(&key.name, Role::SortKey));
                if let Some(direction) = key.direction {
                    out.push(' ');
                    out.push_str(direction.as_str());
                }
            }
            out.push(')');
            parts.push(out);
        }
        if parts.is_empty() {
            "()".to_owned()
        } else {
            parts.join(" ")
        }
    }
}

fn show(atom: &Atom, role: Role) -> String {
    if significant(atom, role) {
        format!("\"{}\"", atom.text)
    } else {
        format!("'{}'", atom.text)
    }
}

fn outline_expr(expr: &Expr) -> String {
    let list = |word: &str, items: &[Expr]| {
        let inner: Vec<String> = items.iter().map(outline_expr).collect();
        format!("({word} {})", inner.join(" "))
    };
    match expr {
        Expr::And(items) => list("and", items),
        Expr::Or(items) => list("or", items),
        Expr::Not(inner, _) => format!("(not {})", outline_expr(inner)),
        Expr::Error(_) => "(error)".to_owned(),
        Expr::Text(atom) => show(atom, Role::Text),
        Expr::Cond(cond) => {
            let op = match cond.op {
                Op::Is => "is",
                other => other.as_str(),
            };
            let mut out = format!("({op} {}", show(&cond.name, Role::Name));
            for item in &cond.items {
                out.push(' ');
                if item.excluded {
                    out.push('!');
                }
                match &item.kind {
                    ItemKind::Value(atom) => out.push_str(&show(atom, Role::Value)),
                    ItemKind::Range(from, to) => {
                        if let Some(from) = from {
                            out.push_str(&show(from, Role::Value));
                        }
                        out.push_str("..");
                        if let Some(to) = to {
                            out.push_str(&show(to, Role::Value));
                        }
                    }
                }
            }
            out.push(')');
            out
        }
    }
}
