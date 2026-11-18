//! The canonical text of a query: single spaces, `, ` in lists, English operators, the sort
//! last, and quotes only where needed. `parse(print(q))` gives the same tree as `q` (by
//! [`Query::outline`]), which the stored form of saved searches relies on (1.5d).

use crate::lex::{TokenKind, lex, quote};
use crate::syntax::{Atom, Expr, ItemKind, Op, Query};
use crate::words::{self, Kind, fold, is_operator};

/// Where an atom stands, which decides when its quoting carries meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Name,
    SortKey,
    Value,
    Text,
}

/// Whether the atom's quoting changes the meaning: a quoted keyword is data (`"text": …` is the
/// field, `"me"` the literal word), quoted text is a phrase, and a text term that is an operator
/// word is that literal word however it was written.
pub fn significant(atom: &Atom, role: Role) -> bool {
    let folded = || fold(&atom.text);
    match role {
        Role::Name | Role::SortKey => {
            atom.quoted && words::keyword(&folded(), words::NAME).is_some()
        }
        Role::Value => atom.quoted && words::keyword(&folded(), words::VALUE).is_some(),
        Role::Text => atom.quoted || is_operator(&atom.text),
    }
}

pub fn print(query: &Query) -> String {
    let mut out = String::new();
    if let Some(expr) = &query.expr {
        print_expr(expr, &mut out);
    }
    if let Some(sort) = &query.sort {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("sort: ");
        for (i, key) in sort.keys.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            push_atom(&key.name, Role::SortKey, false, &mut out);
            if let Some(direction) = key.direction {
                out.push(' ');
                out.push_str(direction.as_str());
            }
        }
    }
    out
}

fn print_expr(expr: &Expr, out: &mut String) {
    match expr {
        Expr::And(list) | Expr::Or(list) => {
            let separator = if matches!(expr, Expr::Or(_)) {
                " or "
            } else {
                " "
            };
            let mut first = true;
            for child in list.iter().filter(|c| !matches!(c, Expr::Error(_))) {
                if !first {
                    out.push_str(separator);
                }
                first = false;
                let wrap = matches!(expr, Expr::And(_)) && matches!(child, Expr::Or(_));
                print_wrapped(child, wrap, out);
            }
        }
        Expr::Not(inner, _) => {
            out.push('!');
            print_wrapped(inner, matches!(**inner, Expr::And(_) | Expr::Or(_)), out);
        }
        Expr::Text(atom) => push_atom(atom, Role::Text, false, out),
        Expr::Error(_) => {}
        Expr::Cond(cond) => {
            let name_is_sort = cond.op == Op::Is
                && words::keyword(&fold(&cond.name.text), &[Kind::Sort])
                    .is_some_and(|w| w.key == "sort");
            if name_is_sort {
                out.push_str(&quote(&cond.name.text));
            } else {
                push_atom(&cond.name, Role::Name, false, out);
            }
            match cond.op {
                Op::Is => out.push_str(": "),
                op => {
                    out.push(' ');
                    out.push_str(op.as_str());
                    out.push(' ');
                }
            }
            for (i, item) in cond.items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if item.excluded {
                    out.push('!');
                }
                match &item.kind {
                    ItemKind::Value(atom) => push_atom(atom, Role::Value, false, out),
                    ItemKind::Range(from, to) => {
                        if let Some(from) = from {
                            push_atom(from, Role::Value, true, out);
                        }
                        out.push_str("..");
                        if let Some(to) = to {
                            push_atom(to, Role::Value, false, out);
                        }
                    }
                }
            }
        }
    }
}

fn print_wrapped(expr: &Expr, wrap: bool, out: &mut String) {
    if wrap {
        out.push('(');
        print_expr(expr, out);
        out.push(')');
    } else {
        print_expr(expr, out);
    }
}

/// `before_dots`: the atom starts a range, so a final `.` would merge with the `..`.
fn push_atom(atom: &Atom, role: Role, before_dots: bool, out: &mut String) {
    let bare = is_bare_word(&atom.text) && !(before_dots && atom.text.ends_with('.'));
    if bare && !significant(atom, role) {
        out.push_str(&atom.text);
    } else {
        out.push_str(&quote(&atom.text));
    }
}

/// The text reads back as exactly one word.
pub(crate) fn is_bare_word(text: &str) -> bool {
    let tokens = lex(text);
    matches!(tokens.as_slice(), [t] if t.kind == TokenKind::Word && t.span.start == 0 && t.span.end == text.len())
}
