//! What the resolver needs from the database, collected from a parsed query in one pass, so the
//! service loads the [`super::Catalog`] with one query per kind of name.

use std::collections::BTreeSet;

use taskmaster_query::words::{self, Kind};
use taskmaster_query::{Atom, Expr, ItemKind, Query, fold, fold_value};

use crate::issues::ReadableId;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs {
    /// Value texts as [`fold_value`] writes them: set values, logins, groups, tags.
    pub words: BTreeSet<String>,
    /// Value texts as [`fold`] writes them: full names compare this way.
    pub people: BTreeSet<String>,
    /// Folded names of conditions and sort keys: fields, aliases and link sides.
    pub names: BTreeSet<String>,
    /// The subset of `names` whose value sets are needed whole: used in a range, a comparison,
    /// a sort, or with `resolved` / `unresolved`.
    pub whole_sets: BTreeSet<String>,
    /// Issue IDs as [`issue_key`] writes them.
    pub issue_ids: BTreeSet<String>,
    /// Text terms as written (trimmed): the service finds those that are only common words
    /// ([`super::Catalog::common_words`]).
    pub texts: BTreeSet<String>,
    /// Board names of `board:` values, as [`fold_value`] writes them (QRY-049).
    pub boards: BTreeSet<String>,
    /// Every board the viewer can view, with its sprints: `board: any` / `none` and `sprint:`
    /// (a sprint name may be on any board).
    pub all_boards: bool,
}

pub fn needs(q: &Query) -> Needs {
    let mut out = Needs::default();
    if let Some(expr) = &q.expr {
        walk(expr, &mut out);
    }
    if let Some(sort) = &q.sort {
        for key in &sort.keys {
            let name = fold(&key.name.text);
            out.whole_sets.insert(name.clone());
            out.names.insert(name);
        }
    }
    out
}

fn walk(expr: &Expr, out: &mut Needs) {
    match expr {
        Expr::And(list) | Expr::Or(list) => list.iter().for_each(|e| walk(e, out)),
        Expr::Not(inner, _) => walk(inner, out),
        Expr::Error(_) => {}
        Expr::Text(atom) => {
            out.texts.insert(atom.text.trim().to_owned());
            if let Some(key) = issue_key(&atom.text).filter(|_| looks_like_id(&atom.text)) {
                out.issue_ids.insert(key);
            }
        }
        Expr::Cond(cond) => {
            boards(cond, out);
            let name = fold(&cond.name.text);
            let mut whole = cond.op != taskmaster_query::Op::Is;
            let text = may_hold_text(&cond.name);
            for item in &cond.items {
                match &item.kind {
                    ItemKind::Value(atom) => {
                        whole |= is_resolution_word(atom);
                        if text {
                            out.texts.insert(atom.text.trim().to_owned());
                        }
                        value(atom, out);
                    }
                    ItemKind::Range(from, to) => {
                        whole = true;
                        from.iter().chain(to.iter()).for_each(|a| value(a, out));
                    }
                }
            }
            if whole {
                out.whole_sets.insert(name.clone());
            }
            out.names.insert(name);
        }
    }
}

/// What `board:` and `sprint:` conditions need.
fn boards(cond: &taskmaster_query::Condition, out: &mut Needs) {
    if cond.name.quoted {
        return;
    }
    match words::keyword(&fold(&cond.name.text), &[Kind::Attribute]).map(|w| w.key) {
        Some("sprint") => out.all_boards = true,
        Some("board") => {
            for item in &cond.items {
                let ItemKind::Value(atom) = &item.kind else {
                    continue;
                };
                let any_or_none = !atom.quoted
                    && words::keyword(&fold(&atom.text), words::VALUE)
                        .is_some_and(|w| w.key == "any" || w.key == "none");
                if any_or_none {
                    out.all_boards = true;
                } else {
                    out.boards.insert(fold_value(&atom.text));
                }
            }
        }
        _ => {}
    }
}

fn value(atom: &Atom, out: &mut Needs) {
    out.words.insert(fold_value(&atom.text));
    out.people.insert(fold(&atom.text));
    if let Some(key) = issue_key(&atom.text) {
        out.issue_ids.insert(key);
    }
}

/// The values of a condition with this name may be text terms: a text attribute, or a name
/// that may be a text field.
fn may_hold_text(name: &Atom) -> bool {
    if name.quoted {
        return true;
    }
    match words::keyword(&fold(&name.text), &[Kind::Attribute]) {
        Some(word) => matches!(word.key, "summary" | "description" | "comments" | "text"),
        None => true,
    }
}

fn is_resolution_word(atom: &Atom) -> bool {
    !atom.quoted
        && words::keyword(&fold(&atom.text), &[Kind::Value])
            .is_some_and(|w| w.key == "resolved" || w.key == "unresolved")
}

/// The lookup form of an issue ID: a readable ID in upper case (`web-42` → `WEB-42`), or an
/// internal ID in digits. `None` for anything else.
pub fn issue_key(text: &str) -> Option<String> {
    let text = text.trim();
    if !text.is_empty() && text.len() <= 18 && text.bytes().all(|b| b.is_ascii_digit()) {
        let id: i64 = text.parse().ok()?;
        return (id > 0).then(|| id.to_string());
    }
    let id = ReadableId::parse(text)?;
    Some(crate::issues::readable_id(&id.key, id.number))
}

/// A bare word shaped like a readable ID (QRY-016): `[A-Z][A-Z0-9_]{1,9}-[1-9][0-9]*`, any case.
pub fn looks_like_id(text: &str) -> bool {
    let Some((key, number)) = text.split_once('-') else {
        return false;
    };
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && (2..=10).contains(&key.len())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && number.starts_with(|c: char| ('1'..='9').contains(&c))
        && number.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_names_values_and_ids() {
        let q = taskmaster_query::parse(
            "Due-Date: 2026-01..2026-03 state: Ёлка, resolved web-7 assignee: \"Anna  Petrova\" sort: Priority",
        );
        let needs = needs(&q);
        assert!(needs.names.contains("due date"));
        assert!(needs.names.contains("priority"));
        assert!(needs.whole_sets.contains("due date"));
        assert!(needs.whole_sets.contains("state"));
        assert!(needs.whole_sets.contains("priority"));
        assert!(!needs.whole_sets.contains("assignee"));
        assert!(needs.words.contains("елка"));
        assert!(needs.people.contains("anna petrova"));
        assert_eq!(needs.issue_ids.iter().collect::<Vec<_>>(), vec!["WEB-7"]);
        assert!(needs.boards.is_empty() && !needs.all_boards);
    }

    #[test]
    fn qry_049_boards_and_sprints() {
        let needs = needs(&taskmaster_query::parse("доска: Команда, !Ops"));
        assert_eq!(
            needs.boards.iter().collect::<Vec<_>>(),
            vec!["ops", "команда"]
        );
        assert!(!needs.all_boards);
        assert!(super::needs(&taskmaster_query::parse("board: none")).all_boards);
        assert!(super::needs(&taskmaster_query::parse("sprint: \"A/B\"")).all_boards);
        assert!(!super::needs(&taskmaster_query::parse("\"board\": none")).all_boards);
    }

    #[test]
    fn qry_016_id_shapes() {
        assert!(looks_like_id("WEB-42"));
        assert!(looks_like_id("web_2-1"));
        assert!(!looks_like_id("W-1"));
        assert!(!looks_like_id("WEB-0"));
        assert!(!looks_like_id("2WEB-1"));
        assert!(!looks_like_id("ABCDEFGHIJK-1"));
        assert_eq!(issue_key("web-42").as_deref(), Some("WEB-42"));
        assert_eq!(issue_key("1042").as_deref(), Some("1042"));
        assert_eq!(issue_key("open"), None);
    }
}
