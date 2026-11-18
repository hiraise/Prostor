//! The stored form of a saved query (plan step 1.5 §3.6, D-038): the parsed tree and what each
//! name and value resolved to. Printed back with the current names, so renaming a field, a value,
//! a project or a person never breaks a saved search; a target that is gone keeps its old words
//! and gives `deleted_reference`.
//!
//! Text terms and literals (numbers, dates, `mentions:` IDs as written) stay as they were.

use std::collections::HashMap;

use prostor_query::print::{Role, significant};
use prostor_query::{Atom, Expr, ItemKind, Problem, ProblemCode, Query, Span};
use serde::{Deserialize, Serialize};

use super::Binding;
use crate::links::Side;
use crate::{
    BoardId, FieldId, GroupId, IssueId, LinkTypeId, ProjectId, SprintId, TagId, UserId, ValueId,
};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    pub v: u32,
    /// Spans point into the text the query was saved from; problems are left out.
    pub query: Query,
    pub bindings: Vec<(Span, Binding)>,
}

/// The IDs a stored query names, by kind: what the caller loads names for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refs {
    pub fields: Vec<FieldId>,
    pub values: Vec<ValueId>,
    pub users: Vec<UserId>,
    pub groups: Vec<GroupId>,
    pub projects: Vec<ProjectId>,
    pub tags: Vec<TagId>,
    pub link_types: Vec<LinkTypeId>,
    pub issues: Vec<IssueId>,
    pub boards: Vec<BoardId>,
    pub sprints: Vec<SprintId>,
}

/// The current names of what a stored query names; a missing entry means the target is gone (or
/// hidden from the reader). Projects by key, users by login, issues by readable ID.
#[derive(Debug, Clone, Default)]
pub struct Names {
    pub fields: HashMap<FieldId, String>,
    pub values: HashMap<ValueId, String>,
    pub users: HashMap<UserId, String>,
    pub groups: HashMap<GroupId, String>,
    pub projects: HashMap<ProjectId, String>,
    pub tags: HashMap<TagId, String>,
    pub link_sides: HashMap<(LinkTypeId, Side), String>,
    pub issues: HashMap<IssueId, String>,
    /// Boards the reader can view.
    pub boards: HashMap<BoardId, String>,
    /// Sprints of those boards.
    pub sprints: HashMap<SprintId, String>,
}

impl Stored {
    /// The query as parsed and the bindings of its resolution (`Resolved::bindings`).
    pub fn new(query: &Query, bindings: Vec<(Span, Binding)>) -> Self {
        Self {
            v: VERSION,
            query: Query {
                problems: Vec::new(),
                ..query.clone()
            },
            bindings,
        }
    }

    pub fn refs(&self) -> Refs {
        let mut r = Refs::default();
        for (_, b) in &self.bindings {
            match b {
                Binding::Field(id) => r.fields.push(*id),
                Binding::Value(id) => r.values.push(*id),
                Binding::User(id) => r.users.push(*id),
                Binding::Group(id) => r.groups.push(*id),
                Binding::Project(id) => r.projects.push(*id),
                Binding::Tag(id) => r.tags.push(*id),
                Binding::LinkSide(id, _) => r.link_types.push(*id),
                Binding::Issue(id) => r.issues.push(*id),
                Binding::Board(id) | Binding::CurrentSprint(Some(id)) => r.boards.push(*id),
                Binding::Sprint(id) => r.sprints.push(*id),
                Binding::Attribute(_)
                | Binding::Keyword(_)
                | Binding::Literal(_)
                | Binding::CurrentSprint(None) => {}
            }
        }
        for list in [
            &mut r.fields,
            &mut r.values,
            &mut r.users,
            &mut r.groups,
            &mut r.projects,
            &mut r.tags,
            &mut r.link_types,
            &mut r.issues,
            &mut r.boards,
            &mut r.sprints,
        ] {
            list.sort_unstable();
            list.dedup();
        }
        r
    }

    /// The tree with the current names, and a `deleted_reference` per atom whose target is gone.
    /// Spans still point into the saved text: print the tree (`prostor_query::print`) and parse
    /// the result before resolving it.
    pub fn unbind(&self, names: &Names) -> (Query, Vec<Problem>) {
        let mut query = self.query.clone();
        let mut problems = Vec::new();
        let mut renamer = Renamer {
            bindings: &self.bindings,
            names,
            problems: &mut problems,
        };
        if let Some(expr) = &mut query.expr {
            renamer.expr(expr);
        }
        if let Some(sort) = &mut query.sort {
            for key in &mut sort.keys {
                renamer.atom(&mut key.name, Role::SortKey);
            }
        }
        (query, problems)
    }

    /// The query's text with the current names, and its `deleted_reference` problems with spans
    /// into that text.
    pub fn text(&self, names: &Names) -> (String, Vec<Problem>) {
        let (tree, mut problems) = self.unbind(names);
        let text = prostor_query::print(&tree);
        let old = atom_spans(&tree);
        let new = atom_spans(&prostor_query::parse(&text));
        for problem in &mut problems {
            problem.span = old
                .iter()
                .position(|s| *s == problem.span)
                .filter(|_| old.len() == new.len())
                .and_then(|i| new.get(i).copied())
                .unwrap_or_default();
        }
        (text, problems)
    }
}

/// The spans of every name and value of a tree, in the order [`Renamer`] visits them.
fn atom_spans(query: &Query) -> Vec<Span> {
    fn expr(e: &Expr, out: &mut Vec<Span>) {
        match e {
            Expr::And(list) | Expr::Or(list) => list.iter().for_each(|e| expr(e, out)),
            Expr::Not(inner, _) => expr(inner, out),
            Expr::Cond(cond) => {
                out.push(cond.name.span);
                for item in &cond.items {
                    match &item.kind {
                        ItemKind::Value(atom) => out.push(atom.span),
                        ItemKind::Range(from, to) => {
                            out.extend([from, to].into_iter().flatten().map(|a| a.span));
                        }
                    }
                }
            }
            Expr::Text(_) | Expr::Error(_) => {}
        }
    }
    let mut out = Vec::new();
    if let Some(e) = &query.expr {
        expr(e, &mut out);
    }
    if let Some(sort) = &query.sort {
        out.extend(sort.keys.iter().map(|k| k.name.span));
    }
    out
}

struct Renamer<'a> {
    bindings: &'a [(Span, Binding)],
    names: &'a Names,
    problems: &'a mut Vec<Problem>,
}

impl Renamer<'_> {
    fn expr(&mut self, expr: &mut Expr) {
        match expr {
            Expr::And(list) | Expr::Or(list) => list.iter_mut().for_each(|e| self.expr(e)),
            Expr::Not(inner, _) => self.expr(inner),
            Expr::Cond(cond) => {
                self.atom(&mut cond.name, Role::Name);
                for item in &mut cond.items {
                    match &mut item.kind {
                        ItemKind::Value(atom) => self.atom(atom, Role::Value),
                        ItemKind::Range(from, to) => {
                            for atom in [from, to].into_iter().flatten() {
                                self.atom(atom, Role::Value);
                            }
                        }
                    }
                }
            }
            Expr::Text(_) | Expr::Error(_) => {}
        }
    }

    /// `None`: kept as written; `Some(None)`: gone.
    fn name_of(&self, binding: &Binding) -> Option<Option<&String>> {
        let n = self.names;
        Some(match binding {
            Binding::Attribute(_)
            | Binding::Keyword(_)
            | Binding::Literal(_)
            | Binding::CurrentSprint(None) => return None,
            Binding::Field(id) => n.fields.get(id),
            Binding::Value(id) => n.values.get(id),
            Binding::User(id) => n.users.get(id),
            Binding::Group(id) => n.groups.get(id),
            Binding::Project(id) => n.projects.get(id),
            Binding::Tag(id) => n.tags.get(id),
            Binding::LinkSide(id, side) => n.link_sides.get(&(*id, *side)),
            Binding::Issue(id) => n.issues.get(id),
            Binding::Board(id) | Binding::CurrentSprint(Some(id)) => n.boards.get(id),
            Binding::Sprint(id) => n.sprints.get(id),
        })
    }

    /// The text of a `board/sprint` value with today's names: `None` when the atom is not one.
    /// `Some(None)`: keep the words (a target is gone, or several disagree on a name).
    fn qualified(&mut self, atom: &Atom) -> Option<Option<String>> {
        let mut boards: Vec<Option<&String>> = Vec::new();
        let mut sprints: Vec<Option<&String>> = Vec::new();
        let mut current = false;
        for (_, binding) in self.bindings.iter().filter(|(s, _)| *s == atom.span) {
            match binding {
                Binding::Board(id) => boards.push(self.names.boards.get(id)),
                Binding::CurrentSprint(Some(id)) => {
                    boards.push(self.names.boards.get(id));
                    current = true;
                }
                Binding::Sprint(id) => sprints.push(self.names.sprints.get(id)),
                _ => {}
            }
        }
        if boards.is_empty() || (sprints.is_empty() && !current) {
            return None;
        }
        if boards.iter().chain(&sprints).any(Option::is_none) {
            self.problems
                .push(Problem::new(ProblemCode::DeletedReference, atom.span));
            return Some(None);
        }
        let agreed = |names: &[Option<&String>]| -> Option<String> {
            let first = names.first().copied().flatten()?;
            names
                .iter()
                .all(|n| *n == Some(first))
                .then(|| first.clone())
        };
        let sprint = if current {
            // The keyword as written: the last part, since keywords hold no `/`.
            atom.text.rsplit('/').next().map(str::to_owned)
        } else {
            agreed(&sprints)
        };
        Some(agreed(&boards).zip(sprint).map(|(b, s)| format!("{b}/{s}")))
    }

    /// One atom: a name meaning several fields (or a value of several) takes the new name only
    /// when every target that is left agrees on it, else keeps its words.
    fn atom(&mut self, atom: &mut Atom, role: Role) {
        if let Some(text) = self.qualified(atom) {
            if let Some(text) = text.filter(|t| *t != atom.text) {
                atom.text = text;
                atom.quoted = significant(
                    &Atom {
                        quoted: true,
                        ..atom.clone()
                    },
                    role,
                );
            }
            return;
        }
        let mut present: Vec<&String> = Vec::new();
        let mut gone = false;
        for (_, binding) in self.bindings.iter().filter(|(s, _)| *s == atom.span) {
            match self.name_of(binding) {
                None => return,
                Some(Some(name)) => present.push(name),
                Some(None) => gone = true,
            }
        }
        let Some(first) = present.first() else {
            if gone {
                self.problems
                    .push(Problem::new(ProblemCode::DeletedReference, atom.span));
            }
            return;
        };
        // Unchanged names keep their words and quotes as written.
        if present.iter().any(|n| n != first) || atom.text == **first {
            return;
        }
        atom.text = (*first).clone();
        // A data name equal to a keyword must stay quoted (`"me"` the person, not the word).
        atom.quoted = significant(
            &Atom {
                quoted: true,
                ..atom.clone()
            },
            role,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prostor_query::{parse, print};

    fn span_of(text: &str, word: &str) -> Span {
        let start = text.find(word).unwrap();
        Span::new(start, start + word.len())
    }

    #[test]
    fn qry_063_renamed_names_print_new_words() {
        let text = "Priority: High, !Low Assignee: john fix crash sort: Priority desc";
        let q = parse(text);
        let sort_name = text.rfind("Priority").unwrap();
        let bindings = vec![
            (span_of(text, "Priority"), Binding::Field(1)),
            (span_of(text, "High"), Binding::Value(11)),
            (span_of(text, "Low"), Binding::Value(12)),
            (span_of(text, "Assignee"), Binding::Field(2)),
            (span_of(text, "john"), Binding::User(7)),
            (
                Span::new(sort_name, sort_name + "Priority".len()),
                Binding::Field(1),
            ),
        ];
        let stored = Stored::new(&q, bindings);
        let json = serde_json::to_string(&stored).unwrap();
        let back: Stored = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stored);
        assert_eq!(back.refs().fields, vec![1, 2]);

        let mut names = Names::default();
        names.fields.insert(1, "Urgency".to_owned());
        names.fields.insert(2, "Owner".to_owned());
        names.values.insert(11, "Top level".to_owned());
        names.values.insert(12, "Low".to_owned());
        names.users.insert(7, "me".to_owned());
        let (tree, problems) = back.unbind(&names);
        assert!(problems.is_empty());
        assert_eq!(
            print(&tree),
            r#"Urgency: "Top level", !Low Owner: "me" fix crash sort: Urgency desc"#
        );
        assert!(!parse(&print(&tree)).has_errors());
    }

    #[test]
    fn qry_063_gone_target_keeps_words_and_reports() {
        let text = "Priority: High";
        let q = parse(text);
        let stored = Stored::new(
            &q,
            vec![
                (span_of(text, "Priority"), Binding::Field(1)),
                (span_of(text, "High"), Binding::Value(11)),
            ],
        );
        let mut names = Names::default();
        names.fields.insert(1, "Priority".to_owned());
        let (tree, problems) = stored.unbind(&names);
        assert_eq!(print(&tree), "Priority: High");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code, ProblemCode::DeletedReference);
        assert_eq!(problems[0].span, span_of(text, "High"));
        names.fields.insert(1, "Importance level".to_owned());
        let (printed, problems) = stored.text(&names);
        assert_eq!(printed, r#""Importance level": High"#);
        assert_eq!(problems[0].span, span_of(&printed, "High"));
    }

    #[test]
    fn qry_063_one_name_of_two_fields() {
        let text = "Stage: Open";
        let q = parse(text);
        let s = span_of(text, "Stage");
        let stored = Stored::new(&q, vec![(s, Binding::Field(1)), (s, Binding::Field(2))]);
        let mut names = Names::default();
        names.fields.insert(1, "Phase".to_owned());
        names.fields.insert(2, "Stage".to_owned());
        assert_eq!(print(&stored.unbind(&names).0), "Stage: Open");
        names.fields.insert(2, "Phase".to_owned());
        assert_eq!(print(&stored.unbind(&names).0), "Phase: Open");
        names.fields.remove(&1);
        let (tree, problems) = stored.unbind(&names);
        assert_eq!(print(&tree), "Phase: Open");
        assert!(problems.is_empty());
    }
}
