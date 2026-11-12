//! Names and values of a parsed query → [`Filter`] (plan §3.3–§3.4).

use std::collections::BTreeMap;

use trackflow_query::words::{self, Kind};
use trackflow_query::{
    Atom, Condition, Expr, Item, ItemKind, Op, Problem, ProblemCode, Query, Severity, Span, fold,
    fold_value,
};

use super::catalog::{BoardEntry, FieldEntry, People, ProjectEntry, projects_named};
use super::dates::{self, Period};
use super::filter::{Bound, FieldRef, Filter, Range, Target};
use super::{Binding, Catalog, Ctx, Resolved, contains};
use crate::duration::{self, Schedule};
use crate::fields::{FieldKind, SetKind};
use crate::links::Side;
use crate::{FieldId, LinkTypeId, ProjectId, UserId};

/// Where names are looked up (plan §3.4): the context's projects; else those named by `project:`
/// conditions at the top level of the query; else every readable project that is not archived.
/// `projects` are the readable ones.
pub fn scope(q: &Query, projects: &[ProjectEntry], context: Option<&[ProjectId]>) -> Scope {
    if let Some(context) = context {
        return Scope {
            projects: context.to_vec(),
            narrowed: true,
        };
    }
    let top: Vec<&Expr> = match &q.expr {
        Some(Expr::And(list)) => list.iter().collect(),
        Some(expr) => vec![expr],
        None => vec![],
    };
    let mut named = Vec::new();
    for expr in top {
        let Expr::Cond(cond) = expr else { continue };
        if cond.op != Op::Is || attribute(&cond.name) != Some("project") {
            continue;
        }
        for item in cond.items.iter().filter(|i| !i.excluded) {
            if let ItemKind::Value(atom) = &item.kind {
                named.extend(projects_named(projects, &atom.text));
            }
        }
    }
    if !named.is_empty() {
        named.sort_unstable();
        named.dedup();
        return Scope {
            projects: named,
            narrowed: true,
        };
    }
    Scope {
        projects: projects
            .iter()
            .filter(|p| !p.archived)
            .map(|p| p.id)
            .collect(),
        narrowed: false,
    }
}

/// The projects a query's names are looked up in, and whether a context or `project:`
/// conditions chose them (then the query matches nothing outside them: the list's live topic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub projects: Vec<ProjectId>,
    pub narrowed: bool,
}

/// Resolves a parsed query. `Err` holds every problem (the parser's too, errors and notes) when
/// any is an error; the query then does not run.
pub fn resolve(q: &Query, cat: &Catalog, ctx: &Ctx) -> Result<Resolved, Vec<Problem>> {
    let mut r = R {
        cat,
        ctx,
        problems: q.problems.clone(),
        bindings: Vec::new(),
        concerns: false,
        negated: false,
        text_terms: 0,
        ranked: Vec::new(),
        bare_id: None,
    };
    let mut filter = q.expr.as_ref().map_or(Filter::True, |e| r.expr(e));
    let sort = r.sort(q.sort.as_ref().map_or(&[][..], |s| &s.keys[..]));
    let single_id = match (&q.expr, &filter) {
        (Some(Expr::Text(_)), _) => r.bare_id,
        (Some(Expr::Cond(_)), Filter::Issues(ids)) if ids.len() == 1 => ids.first().copied(),
        _ => None,
    };
    if ctx.hide_resolved && single_id.is_none() {
        if r.concerns {
            r.problems.push(Problem::new(
                ProblemCode::HideResolvedIgnored,
                Span::default(),
            ));
        } else {
            filter = Filter::and(vec![filter, Filter::Resolved(false)]);
        }
    }
    r.problems.sort_by_key(|p| (p.span.start, p.span.end));
    if r.problems.iter().any(|p| p.severity == Severity::Error) {
        return Err(r.problems);
    }
    Ok(Resolved {
        filter,
        sort,
        notes: r.problems,
        concerns_resolution: r.concerns,
        single_id,
        bindings: r.bindings,
    })
}

/// The attribute key an unquoted name means.
fn attribute(name: &Atom) -> Option<&'static str> {
    (!name.quoted)
        .then(|| words::keyword(&fold(&name.text), &[Kind::Attribute]))
        .flatten()
        .map(|w| w.key)
}

/// The value keyword an unquoted value means.
fn keyword(atom: &Atom) -> Option<&'static str> {
    (!atom.quoted)
        .then(|| words::keyword(&fold(&atom.text), words::VALUE))
        .flatten()
        .map(|w| w.key)
}

/// `me`, `none` and `any` are keywords in every value position; other keywords lose to data.
pub(super) fn strong_keyword(atom: &Atom) -> Option<&'static str> {
    keyword(atom).filter(|k| matches!(*k, "me" | "none" | "any"))
}

/// One atom of a range or a comparison, as an interval of its own.
type Ends<T> = (Bound<T>, Bound<T>);

/// The range an item under `op` covers, given the interval each atom names. `None` for a range
/// that holds nothing (below an open start).
fn span_of<T: Copy>(
    op: Op,
    item: &ItemKind,
    mut one: impl FnMut(&Atom) -> Option<Ends<T>>,
) -> Option<Option<Range<T>>> {
    let flip = |b: Bound<T>| match b {
        Bound::Included(v) => Some(Bound::Excluded(v)),
        Bound::Excluded(v) => Some(Bound::Included(v)),
        Bound::Open => None,
    };
    let range = match item {
        ItemKind::Range(from, to) => {
            let low = match from {
                Some(a) => one(a)?.0,
                None => Bound::Open,
            };
            let high = match to {
                Some(a) => one(a)?.1,
                None => Bound::Open,
            };
            Some(Range { low, high })
        }
        ItemKind::Value(atom) => {
            let (low, high) = one(atom)?;
            match op {
                Op::Is => Some(Range { low, high }),
                Op::Ge => Some(Range {
                    low,
                    high: Bound::Open,
                }),
                Op::Le => Some(Range {
                    low: Bound::Open,
                    high,
                }),
                Op::Gt => flip(high).map(|low| Range {
                    low,
                    high: Bound::Open,
                }),
                Op::Lt => flip(low).map(|high| Range {
                    low: Bound::Open,
                    high,
                }),
            }
        }
    };
    Some(range)
}

pub(super) struct R<'a> {
    pub(super) cat: &'a Catalog,
    pub(super) ctx: &'a Ctx,
    pub(super) problems: Vec<Problem>,
    pub(super) bindings: Vec<(Span, Binding)>,
    concerns: bool,
    /// Under an odd number of `not`s: text terms there do not rank.
    pub(super) negated: bool,
    pub(super) text_terms: usize,
    /// The positive text terms, for relevance (QRY-045).
    pub(super) ranked: Vec<(super::TextScope, super::TextTerm)>,
    /// The issue a bare ID names.
    pub(super) bare_id: Option<crate::IssueId>,
}

/// What a value of a people attribute means.
pub(super) enum Who {
    Users(Vec<UserId>),
    None,
    Any,
}

impl R<'_> {
    pub(super) fn problem(&mut self, code: ProblemCode, span: Span) {
        self.problems.push(Problem::new(code, span));
    }

    pub(super) fn bind(&mut self, span: Span, binding: Binding) {
        self.bindings.push((span, binding));
    }

    fn errors(&self) -> usize {
        self.problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .count()
    }

    fn expr(&mut self, expr: &Expr) -> Filter {
        self.part(expr).unwrap_or(Filter::True)
    }

    /// `None`: the part holds only common words and is left out (QRY-041), so it neither
    /// narrows an AND nor widens an OR.
    fn part(&mut self, expr: &Expr) -> Option<Filter> {
        match expr {
            Expr::And(list) => {
                self.same_field_twice(list);
                let parts: Vec<Filter> = list.iter().filter_map(|e| self.part(e)).collect();
                (!parts.is_empty()).then(|| Filter::and(parts))
            }
            Expr::Or(list) => {
                let parts: Vec<Filter> = list.iter().filter_map(|e| self.part(e)).collect();
                (!parts.is_empty()).then(|| Filter::or(parts))
            }
            Expr::Not(inner, _) => {
                self.negated = !self.negated;
                let inner = self.part(inner);
                self.negated = !self.negated;
                inner.map(Filter::not)
            }
            Expr::Error(_) => Some(Filter::True),
            Expr::Text(atom) => self.text_atom(atom),
            Expr::Cond(cond) => self.cond(cond),
        }
    }

    /// Notes two positive `:` conditions on one single-valued field in an AND group.
    fn same_field_twice(&mut self, list: &[Expr]) {
        let mut seen: BTreeMap<FieldId, usize> = BTreeMap::new();
        for expr in list {
            let Expr::Cond(cond) = expr else { continue };
            if cond.op != Op::Is
                || cond.items.iter().all(|i| i.excluded)
                || attribute(&cond.name).is_some()
            {
                continue;
            }
            let fields = self
                .cat
                .fields_named(&fold(&cond.name.text), &self.ctx.scope);
            let mut noted = false;
            for field in fields
                .iter()
                .filter(|f| !f.multi && f.kind != FieldKind::Text)
            {
                let count = seen.entry(field.id).or_default();
                *count += 1;
                if *count > 1 && !noted {
                    self.problem(ProblemCode::SameFieldTwice, cond.span);
                    noted = true;
                }
            }
        }
    }

    fn cond(&mut self, c: &Condition) -> Option<Filter> {
        let folded = fold(&c.name.text);
        if let Some(key) = attribute(&c.name) {
            if !self.cat.fields_named(&folded, &self.ctx.scope).is_empty() {
                self.problem(ProblemCode::FieldShadowed, c.name.span);
            }
            self.bind(c.name.span, Binding::Attribute(key.to_owned()));
            if let Some(scope) = self.text_scope(key, c) {
                return self.text_cond(&scope, c);
            }
            return Some(self.attribute(key, c));
        }
        Some(self.named(&folded, c))
    }

    /// A field or a link side.
    fn named(&mut self, folded: &str, c: &Condition) -> Filter {
        let fields = self.cat.fields_named(folded, &self.ctx.scope);
        if !fields.is_empty() {
            for field in &fields {
                self.bind(c.name.span, Binding::Field(field.id));
            }
            return self.fields(&fields, c);
        }
        let sides = self.cat.link_sides_named(folded);
        if !sides.is_empty() {
            for (link_type, side) in &sides {
                self.bind(c.name.span, Binding::LinkSide(*link_type, *side));
            }
            return self.linked(Some(sides), c);
        }
        self.problem(ProblemCode::UnknownAttribute, c.name.span);
        Filter::True
    }

    /// Any-of the positive items, none of the excluded ones (plan §3.2).
    fn items(&mut self, c: &Condition, mut each: impl FnMut(&mut Self, &Item) -> Filter) -> Filter {
        let mut positive = Vec::new();
        let mut negative = Vec::new();
        for item in &c.items {
            let filter = each(self, item);
            if item.excluded {
                negative.push(Filter::not(filter));
            } else {
                positive.push(filter);
            }
        }
        if !positive.is_empty() {
            negative.insert(0, merge(Filter::or(positive)));
        }
        Filter::and(negative)
    }

    /// Items that are single values under `:`; ranges and comparisons are `wrong_operator`.
    pub(super) fn values(
        &mut self,
        c: &Condition,
        mut each: impl FnMut(&mut Self, &Atom) -> Filter,
    ) -> Filter {
        if c.op != Op::Is {
            self.problem(ProblemCode::WrongOperator, c.span);
            return Filter::True;
        }
        self.items(c, |r, item| match &item.kind {
            ItemKind::Value(atom) => each(r, atom),
            ItemKind::Range(..) => {
                r.problem(ProblemCode::WrongOperator, item.span);
                Filter::True
            }
        })
    }

    fn attribute(&mut self, key: &str, c: &Condition) -> Filter {
        match key {
            "project" => self.values(c, |r, atom| {
                if strong_keyword(atom).is_some() {
                    r.problem(ProblemCode::UnknownValue, atom.span);
                    return Filter::True;
                }
                let ids = r.cat.projects_named(&atom.text);
                if ids.is_empty() {
                    r.problem(ProblemCode::UnknownValue, atom.span);
                    return Filter::True;
                }
                for id in &ids {
                    r.bind(atom.span, Binding::Project(*id));
                }
                Filter::Projects(ids)
            }),
            "id" => self.values(c, |r, atom| match r.cat.issue(&atom.text) {
                Some(id) => {
                    r.bind(atom.span, Binding::Issue(id));
                    Filter::Issues(vec![id])
                }
                None => {
                    r.problem(ProblemCode::UnknownValue, atom.span);
                    Filter::True
                }
            }),
            "reporter" | "updater" | "commenter" | "involves" => self.values(c, |r, atom| {
                let Some(who) = r.who(atom) else {
                    return Filter::True;
                };
                match (key, who) {
                    ("reporter", Who::Users(ids)) => Filter::Reporter(ids),
                    ("updater", Who::Users(ids)) => Filter::Updater(ids),
                    ("commenter", Who::Users(ids)) => Filter::Commenter(ids),
                    ("involves", Who::Users(ids)) => r.involves(ids),
                    ("reporter" | "updater", Who::Any) => Filter::True,
                    ("reporter" | "updater", Who::None) => Filter::False,
                    ("commenter", Who::Any) => Filter::Comments,
                    ("commenter", Who::None) => Filter::not(Filter::Comments),
                    _ => {
                        r.problem(ProblemCode::UnknownValue, atom.span);
                        Filter::True
                    }
                }
            }),
            "created" | "updated" | "resolved_date" | "commented" => {
                if key == "resolved_date" {
                    self.concerns = true;
                }
                self.moments(c, key)
            }
            "tag" => self.values(c, |r, atom| match strong_keyword(atom) {
                Some("none") => r.keyword(atom, "none", Filter::NoTags),
                Some("any") => r.keyword(atom, "any", Filter::not(Filter::NoTags)),
                _ => match r.cat.tags_named(&atom.text) {
                    ids if !ids.is_empty() => {
                        for id in &ids {
                            r.bind(atom.span, Binding::Tag(*id));
                        }
                        Filter::Tags(ids)
                    }
                    _ => {
                        r.problem(ProblemCode::UnknownValue, atom.span);
                        Filter::True
                    }
                },
            }),
            "linked" => self.linked(None, c),
            "links" | "comments" | "attachments" | "description" => {
                let has = match key {
                    "links" => Filter::Linked {
                        sides: None,
                        target: Target::Any,
                    },
                    "comments" => Filter::Comments,
                    "attachments" => Filter::Attachments,
                    _ => Filter::not(Filter::DescriptionEmpty),
                };
                self.values(c, |r, atom| match strong_keyword(atom) {
                    Some("any") => r.keyword(atom, "any", has.clone()),
                    Some("none") => r.keyword(atom, "none", Filter::not(has.clone())),
                    _ => {
                        r.problem(ProblemCode::UnknownValue, atom.span);
                        Filter::True
                    }
                })
            }
            "votes" => self.numbers(
                c,
                |r, atom| match strong_keyword(atom) {
                    Some("none") => {
                        Some(r.keyword(atom, "none", Filter::Votes(Range::closed(0, 0))))
                    }
                    Some("any") => Some(r.keyword(
                        atom,
                        "any",
                        Filter::Votes(Range {
                            low: Bound::Excluded(0),
                            high: Bound::Open,
                        }),
                    )),
                    _ => None,
                },
                Filter::Votes,
            ),
            "is" => self.values(c, |r, atom| r.standalone(atom)),
            "board" => self.values(c, |r, atom| match strong_keyword(atom) {
                Some(key @ ("any" | "none")) => r.any_board(atom, key),
                _ => match r.cat.boards_named(&atom.text) {
                    ids if !ids.is_empty() => {
                        // A board of cards by its cards, a query board by its query.
                        let mut on = Vec::new();
                        let mut parts = Vec::new();
                        for id in &ids {
                            r.bind(atom.span, Binding::Board(*id));
                            match r.cat.boards.iter().find(|b| b.id == *id) {
                                Some(BoardEntry { query: Some(q), .. }) => parts.push(q.clone()),
                                _ => on.push(*id),
                            }
                        }
                        if !on.is_empty() {
                            parts.insert(0, Filter::OnBoards(on));
                        }
                        Filter::or(parts)
                    }
                    _ => {
                        r.problem(ProblemCode::UnknownValue, atom.span);
                        Filter::True
                    }
                },
            }),
            "sprint" => self.values(c, |r, atom| r.sprint(atom)),
            "mentions" => self.mentions(c),
            "mentioned_in" => self.mentioned_in(c),
            // summary and text go to `text_cond`.
            _ => {
                self.problem(ProblemCode::UnknownAttribute, c.name.span);
                Filter::True
            }
        }
    }

    /// `board: any` / `none`, and the same under `sprint:`: on a board the viewer can view.
    fn any_board(&mut self, atom: &Atom, key: &str) -> Filter {
        let all: Vec<_> = self
            .cat
            .boards
            .iter()
            .filter(|b| b.query.is_none())
            .map(|b| b.id)
            .collect();
        let on = if all.is_empty() {
            Filter::False
        } else {
            Filter::OnBoards(all)
        };
        let filter = if key == "any" { on } else { Filter::not(on) };
        self.keyword(atom, key, filter)
    }

    /// A value of `sprint:` (QRY-049): `current`, a sprint name on any board (the union), or
    /// `board/sprint` with `current` as the sprint. A whole name wins over a split one.
    fn sprint(&mut self, atom: &Atom) -> Filter {
        let today = dates::to_time_date(self.ctx.clock.today());
        match (strong_keyword(atom), keyword(atom)) {
            (Some(key @ ("any" | "none")), _) => return self.any_board(atom, key),
            (_, Some("current")) => {
                let ids: Vec<_> = self
                    .cat
                    .boards
                    .iter()
                    .filter_map(|b| today.and_then(|t| self.cat.current_sprint(b.id, t)))
                    .collect();
                return self.keyword_sprints(atom, Binding::CurrentSprint(None), ids);
            }
            _ => {}
        }
        let whole = self.cat.sprints_named(&atom.text, None);
        if !whole.is_empty() {
            for id in &whole {
                self.bind(atom.span, Binding::Sprint(*id));
            }
            return Filter::Sprints(whole);
        }
        let mut ids = Vec::new();
        let mut bindings = Vec::new();
        for (at, _) in atom.text.match_indices('/') {
            let (board_name, rest) = (&atom.text[..at], &atom.text[at + 1..]);
            let current =
                words::keyword(&fold(rest), words::VALUE).is_some_and(|w| w.key == "current");
            for board in self.cat.card_boards_named(board_name) {
                if current {
                    bindings.push(Binding::CurrentSprint(Some(board)));
                    ids.extend(today.and_then(|t| self.cat.current_sprint(board, t)));
                    continue;
                }
                let found = self.cat.sprints_named(rest, Some(board));
                if !found.is_empty() {
                    bindings.push(Binding::Board(board));
                    bindings.extend(found.iter().map(|id| Binding::Sprint(*id)));
                    ids.extend(found);
                }
            }
        }
        if bindings.is_empty() {
            self.problem(ProblemCode::UnknownValue, atom.span);
            return Filter::True;
        }
        for binding in bindings {
            self.bind(atom.span, binding);
        }
        if ids.is_empty() {
            Filter::False
        } else {
            Filter::Sprints(ids)
        }
    }

    /// Sprints a keyword resolved to; none (no current sprint) matches nothing.
    fn keyword_sprints(
        &mut self,
        atom: &Atom,
        binding: Binding,
        ids: Vec<crate::SprintId>,
    ) -> Filter {
        self.bind(atom.span, binding);
        if ids.is_empty() {
            Filter::False
        } else {
            Filter::Sprints(ids)
        }
    }

    pub(super) fn keyword(&mut self, atom: &Atom, key: &str, filter: Filter) -> Filter {
        self.bind(atom.span, Binding::Keyword(key.to_owned()));
        filter
    }

    /// Users a value of a people attribute names: `me`, a login or full name, a group's members.
    pub(super) fn who(&mut self, atom: &Atom) -> Option<Who> {
        match strong_keyword(atom) {
            Some("me") => {
                self.bind(atom.span, Binding::Keyword("me".to_owned()));
                return Some(Who::Users(vec![self.ctx.me]));
            }
            Some(key @ ("none" | "any")) => {
                self.bind(atom.span, Binding::Keyword(key.to_owned()));
                return Some(if key == "none" { Who::None } else { Who::Any });
            }
            _ => {}
        }
        match self.cat.user(&atom.text) {
            People::One(id) => {
                self.bind(atom.span, Binding::User(id));
                return Some(Who::Users(vec![id]));
            }
            People::Ambiguous => {
                self.problem(ProblemCode::AmbiguousUser, atom.span);
                return None;
            }
            People::None => {}
        }
        if let Some(group) = self.cat.group(&atom.text) {
            self.bind(atom.span, Binding::Group(group.id));
            return Some(Who::Users(group.members.clone()));
        }
        self.problem(ProblemCode::UnknownValue, atom.span);
        None
    }

    /// Reporter, any user field in scope, or commenter (QRY-013).
    fn involves(&self, users: Vec<UserId>) -> Filter {
        let fields = self
            .cat
            .fields
            .iter()
            .filter(|f| f.kind == FieldKind::User)
            .filter(|f| !self.cat.attachments(f.id, &self.ctx.scope).is_empty())
            .map(|f| self.field_ref(f))
            .collect();
        Filter::Involves { users, fields }
    }

    pub(super) fn field_ref(&self, field: &FieldEntry) -> FieldRef {
        let mut hidden: Vec<ProjectId> = self
            .cat
            .attached
            .iter()
            .filter(|a| {
                a.field == field.id && a.private && !contains(&self.ctx.private_readable, a.project)
            })
            .map(|a| a.project)
            .collect();
        hidden.sort_unstable();
        hidden.dedup();
        FieldRef {
            id: field.id,
            kind: field.kind,
            multi: field.multi,
            hidden,
        }
    }

    /// Projects in scope that have the field and show it to the viewer.
    fn field_projects(&self, field: &FieldRef) -> Vec<ProjectId> {
        let mut out: Vec<ProjectId> = self
            .cat
            .attachments(field.id, &self.ctx.scope)
            .iter()
            .map(|a| a.project)
            .filter(|p| !field.hidden.contains(p))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// `created`, `updated`, `resolved`, `commented`: dates with ranges and comparisons.
    fn moments(&mut self, c: &Condition, key: &str) -> Filter {
        let make = |range: Range<time::OffsetDateTime>| match key {
            "created" => Filter::Created(range),
            "updated" => Filter::Updated(range),
            "resolved_date" => Filter::ResolvedAt(range),
            _ => Filter::Commented(range),
        };
        let (none, any) = match key {
            "resolved_date" => (Filter::Resolved(false), Filter::Resolved(true)),
            "commented" => (Filter::not(Filter::Comments), Filter::Comments),
            _ => (Filter::False, Filter::True),
        };
        let op = c.op;
        self.items(c, |r, item| {
            if let ItemKind::Value(atom) = &item.kind
                && let Some(key @ ("none" | "any")) = strong_keyword(atom)
            {
                if op != Op::Is {
                    r.problem(ProblemCode::WrongOperator, item.span);
                    return Filter::True;
                }
                let filter = if key == "none" {
                    none.clone()
                } else {
                    any.clone()
                };
                return r.keyword(atom, key, filter);
            }
            match span_of(op, &item.kind, |atom| r.period(atom)) {
                None => Filter::True,
                Some(None) => Filter::False,
                Some(Some(range)) => match r.instants(range) {
                    Some(range) => make(range),
                    None => {
                        r.problem(ProblemCode::BadDate, item.span);
                        Filter::True
                    }
                },
            }
        })
    }

    /// The period a date atom names, as an interval; reports `bad_date`.
    fn period(&mut self, atom: &Atom) -> Option<Ends<jiff::civil::DateTime>> {
        let clock = &self.ctx.clock;
        let word = (!atom.quoted)
            .then(|| words::keyword(&fold(&atom.text), &[Kind::Date]))
            .flatten();
        let period = match word {
            Some(word) => {
                self.bind(atom.span, Binding::Keyword(word.key.to_owned()));
                clock.word(word.key)
            }
            None => {
                let period = clock.literal(atom.text.trim());
                if period.is_some() {
                    self.bind(atom.span, Binding::Literal(atom.text.clone()));
                }
                period
            }
        };
        let Some(Period { start, end }) = period else {
            self.problem(ProblemCode::BadDate, atom.span);
            return None;
        };
        Some((
            start.map_or(Bound::Open, Bound::Included),
            end.map_or(Bound::Open, Bound::Excluded),
        ))
    }

    fn instants(&self, range: Range<jiff::civil::DateTime>) -> Option<Range<time::OffsetDateTime>> {
        let clock = &self.ctx.clock;
        let at = |b: Bound<jiff::civil::DateTime>| -> Option<Bound<time::OffsetDateTime>> {
            Some(match b {
                Bound::Open => Bound::Open,
                Bound::Included(t) => Bound::Included(dates::to_offset(clock.instant(t)?)?),
                Bound::Excluded(t) => Bound::Excluded(dates::to_offset(clock.instant(t)?)?),
            })
        };
        Some(Range {
            low: at(range.low)?,
            high: at(range.high)?,
        })
    }

    /// Integers and durations: `special` handles keywords first.
    fn numbers(
        &mut self,
        c: &Condition,
        mut special: impl FnMut(&mut Self, &Atom) -> Option<Filter>,
        make: impl Fn(Range<i64>) -> Filter,
    ) -> Filter {
        let op = c.op;
        self.items(c, |r, item| {
            if let ItemKind::Value(atom) = &item.kind
                && strong_keyword(atom).is_some()
            {
                if op != Op::Is {
                    r.problem(ProblemCode::WrongOperator, item.span);
                    return Filter::True;
                }
                if let Some(filter) = special(r, atom) {
                    return filter;
                }
                r.problem(ProblemCode::UnknownValue, atom.span);
                return Filter::True;
            }
            let parsed = span_of(op, &item.kind, |atom| {
                let n = r.integer(atom, FieldKind::Integer)?;
                Some((Bound::Included(n), Bound::Included(n)))
            });
            match parsed {
                None => Filter::True,
                Some(None) => Filter::False,
                Some(Some(range)) => make(range),
            }
        })
    }

    fn integer(&mut self, atom: &Atom, kind: FieldKind) -> Option<i64> {
        let text = atom.text.trim();
        let parsed = if kind == FieldKind::Duration {
            duration::parse(text, Schedule::default()).ok()
        } else {
            text.parse::<i64>().ok()
        };
        match parsed {
            Some(n) => {
                self.bind(atom.span, Binding::Literal(atom.text.clone()));
                Some(n)
            }
            None => {
                self.problem(ProblemCode::BadNumber, atom.span);
                None
            }
        }
    }

    fn linked(&mut self, sides: Option<Vec<(LinkTypeId, Side)>>, c: &Condition) -> Filter {
        self.values(c, |r, atom| {
            let target = match strong_keyword(atom) {
                Some(key @ ("any" | "none")) => {
                    r.bind(atom.span, Binding::Keyword(key.to_owned()));
                    let filter = Filter::Linked {
                        sides: sides.clone(),
                        target: Target::Any,
                    };
                    return if key == "any" {
                        filter
                    } else {
                        Filter::not(filter)
                    };
                }
                _ => match r.cat.issue(&atom.text) {
                    Some(id) => {
                        r.bind(atom.span, Binding::Issue(id));
                        Target::Issues(vec![id])
                    }
                    None => {
                        r.problem(ProblemCode::UnknownValue, atom.span);
                        return Filter::True;
                    }
                },
            };
            Filter::Linked {
                sides: sides.clone(),
                target,
            }
        })
    }

    /// Two fields with one name are both meant (FLD-016): each reads the items its own way; a
    /// field that cannot read them drops out, and the problems of the first are reported when
    /// none can.
    fn fields(&mut self, fields: &[&FieldEntry], c: &Condition) -> Filter {
        if fields.iter().any(|f| f.kind == FieldKind::State) {
            self.concerns = true;
        }
        if let [field] = fields {
            return self.field(field, c);
        }
        let mut out = Vec::new();
        let mut first_problems = None;
        for field in fields {
            let (start, errors) = (self.problems.len(), self.errors());
            let filter = self.field(field, c);
            if self.errors() == errors {
                out.push(filter);
            } else if first_problems.is_none() {
                first_problems = Some(self.problems.split_off(start));
            } else {
                self.problems.truncate(start);
            }
        }
        if out.is_empty() {
            self.problems.extend(first_problems.unwrap_or_default());
            return Filter::True;
        }
        Filter::or(out)
    }

    fn field(&mut self, field: &FieldEntry, c: &Condition) -> Filter {
        let fref = self.field_ref(field);
        let op = c.op;
        match field.kind {
            FieldKind::Enum
            | FieldKind::State
            | FieldKind::Version
            | FieldKind::Build
            | FieldKind::Owned => self.items(c, |r, item| r.set_item(&fref, op, item)),
            FieldKind::User | FieldKind::Group => {
                self.values(c, |r, atom| r.people_value(&fref, atom))
            }
            FieldKind::String => self.values(c, |r, atom| {
                r.empty_any(&fref, atom).unwrap_or_else(|| {
                    r.bind(atom.span, Binding::Literal(atom.text.clone()));
                    Filter::Str {
                        field: fref.clone(),
                        value: atom.text.trim().to_owned(),
                    }
                })
            }),
            FieldKind::Text => self.text_field(&fref, c),
            FieldKind::Integer | FieldKind::Duration => self.scalar(
                &fref,
                c,
                |r, atom| {
                    let n = r.integer(atom, field.kind)?;
                    Some((Bound::Included(n), Bound::Included(n)))
                },
                |_, field, range| Filter::Int { field, range },
            ),
            FieldKind::Float => self.scalar(
                &fref,
                c,
                |r, atom| match atom
                    .text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                {
                    Some(n) => {
                        r.bind(atom.span, Binding::Literal(atom.text.clone()));
                        Some((Bound::Included(n), Bound::Included(n)))
                    }
                    None => {
                        r.problem(ProblemCode::BadNumber, atom.span);
                        None
                    }
                },
                |_, field, range| Filter::Float { field, range },
            ),
            FieldKind::Date => {
                self.scalar(&fref, c, Self::period, |_, field, range| {
                    match calendar(range) {
                        Some(range) => Filter::Date { field, range },
                        None => Filter::False,
                    }
                })
            }
            FieldKind::DateTime => self.scalar(&fref, c, Self::period, |r, field, range| {
                match r.instants(range) {
                    Some(range) => Filter::DateTime { field, range },
                    None => {
                        r.problem(ProblemCode::BadDate, c.span);
                        Filter::True
                    }
                }
            }),
        }
    }

    /// `none` / `any` on a field.
    pub(super) fn empty_any(&mut self, field: &FieldRef, atom: &Atom) -> Option<Filter> {
        match strong_keyword(atom)? {
            "none" => {
                let projects = self.field_projects(field);
                Some(self.keyword(
                    atom,
                    "none",
                    Filter::FieldEmpty {
                        field: field.clone(),
                        projects,
                    },
                ))
            }
            "any" => Some(self.keyword(
                atom,
                "any",
                Filter::FieldAny {
                    field: field.clone(),
                },
            )),
            _ => None,
        }
    }

    /// The projects in scope whose empty label for the field is the value text (FLD-056).
    fn empty_label(&mut self, field: &FieldRef, atom: &Atom) -> Option<Filter> {
        let text = fold_value(&atom.text);
        let mut projects: Vec<ProjectId> = self
            .cat
            .attachments(field.id, &self.ctx.scope)
            .iter()
            .filter(|a| !a.empty_label.trim().is_empty() && fold_value(&a.empty_label) == text)
            .map(|a| a.project)
            .filter(|p| !field.hidden.contains(p))
            .collect();
        if projects.is_empty() {
            return None;
        }
        projects.sort_unstable();
        projects.dedup();
        self.bind(atom.span, Binding::Literal(atom.text.clone()));
        Some(Filter::FieldEmpty {
            field: field.clone(),
            projects,
        })
    }

    /// Numbers and dates of a field: `none`/`any` first, then ranges and comparisons.
    fn scalar<T: Copy>(
        &mut self,
        field: &FieldRef,
        c: &Condition,
        mut one: impl FnMut(&mut Self, &Atom) -> Option<Ends<T>>,
        mut make: impl FnMut(&mut Self, FieldRef, Range<T>) -> Filter,
    ) -> Filter {
        let op = c.op;
        self.items(c, |r, item| {
            if let ItemKind::Value(atom) = &item.kind
                && strong_keyword(atom).is_some()
            {
                if op != Op::Is {
                    r.problem(ProblemCode::WrongOperator, item.span);
                    return Filter::True;
                }
                return r.empty_any(field, atom).unwrap_or_else(|| {
                    r.problem(ProblemCode::UnknownValue, atom.span);
                    Filter::True
                });
            }
            match span_of(op, &item.kind, |atom| one(r, atom)) {
                None => Filter::True,
                Some(None) => Filter::False,
                Some(Some(range)) => make(r, field.clone(), range),
            }
        })
    }

    /// The sets of a field in scope.
    fn sets(&self, field: FieldId) -> Vec<crate::SetId> {
        let mut sets: Vec<_> = self
            .cat
            .attachments(field, &self.ctx.scope)
            .iter()
            .filter_map(|a| a.set)
            .collect();
        sets.sort_unstable();
        sets.dedup();
        sets
    }

    /// One item of a set field (enum, state, version, build, owned).
    fn set_item(&mut self, field: &FieldRef, op: Op, item: &Item) -> Filter {
        let sets = self.sets(field.id);
        if let (Op::Is, ItemKind::Value(atom)) = (op, &item.kind) {
            if let Some(filter) = self.empty_any(field, atom) {
                return filter;
            }
            if strong_keyword(atom).is_some() {
                self.problem(ProblemCode::UnknownValue, atom.span);
                return Filter::True;
            }
            let mut ids = Vec::new();
            for set in &sets {
                ids.extend(self.cat.values_named(*set, &atom.text).iter().map(|v| v.id));
            }
            let label = self.empty_label(field, atom);
            if !ids.is_empty() {
                for id in &ids {
                    self.bind(atom.span, Binding::Value(*id));
                }
                let values = self.ref_values(field, ids);
                return Filter::or(vec![values, label.unwrap_or(Filter::False)]);
            }
            if let Some(label) = label {
                return label;
            }
            if field.kind == FieldKind::State
                && let Some(key @ ("resolved" | "unresolved")) = keyword(atom)
            {
                let want = key == "resolved";
                let ids = sets
                    .iter()
                    .flat_map(|set| self.cat.set_values(*set))
                    .filter(|v| v.resolved == want)
                    .map(|v| v.id)
                    .collect();
                self.bind(atom.span, Binding::Keyword(key.to_owned()));
                return self.ref_values(field, ids);
            }
            self.problem(ProblemCode::UnknownValue, atom.span);
            return Filter::True;
        }
        if !field.kind.set_kind().is_some_and(SetKind::ordered) {
            self.problem(ProblemCode::WrongOperator, item.span);
            return Filter::True;
        }
        // Ranges and comparisons go by position, set by set (QRY-008).
        let atoms: Vec<&Atom> = match &item.kind {
            ItemKind::Value(atom) => vec![atom],
            ItemKind::Range(from, to) => from.iter().chain(to.iter()).collect(),
        };
        for atom in &atoms {
            let known = sets
                .iter()
                .any(|set| !self.cat.values_named(*set, &atom.text).is_empty());
            if !known {
                self.problem(ProblemCode::UnknownValue, atom.span);
                return Filter::True;
            }
        }
        let mut ids = Vec::new();
        let mut any_set = false;
        for set in &sets {
            let position = |atom: &Atom| -> Option<Ends<u32>> {
                let p = self.cat.values_named(*set, &atom.text).first()?.position;
                Some((Bound::Included(p), Bound::Included(p)))
            };
            let Some(range) = span_of(op, &item.kind, position) else {
                continue;
            };
            any_set = true;
            let Some(range) = range else { continue };
            ids.extend(
                self.cat
                    .set_values(*set)
                    .filter(|v| within(v.position, &range))
                    .map(|v| v.id),
            );
        }
        for atom in atoms {
            for set in &sets {
                for v in self.cat.values_named(*set, &atom.text) {
                    self.bindings.push((atom.span, Binding::Value(v.id)));
                }
            }
        }
        if !any_set {
            return Filter::False;
        }
        self.ref_values(field, ids)
    }

    /// The values filter, with the implied resolution condition for state fields (D-024): all
    /// unresolved values mean an unresolved issue; all resolved ones a resolved issue when the
    /// field is the only state field of every project in scope that has it.
    fn ref_values(&self, field: &FieldRef, mut ids: Vec<i64>) -> Filter {
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Filter::False;
        }
        let mut implied = None;
        if field.kind == FieldKind::State {
            let flags: Vec<bool> = self
                .cat
                .values
                .iter()
                .filter(|v| ids.contains(&v.id))
                .map(|v| v.resolved)
                .collect();
            if flags.iter().all(|f| !f) {
                implied = Some(false);
            } else if flags.iter().all(|f| *f) && self.only_state_field(field.id) {
                implied = Some(true);
            }
        }
        Filter::RefValues {
            field: field.clone(),
            ids,
            implied,
        }
    }

    fn only_state_field(&self, field: FieldId) -> bool {
        self.cat
            .attachments(field, &self.ctx.scope)
            .iter()
            .all(|a| {
                self.cat
                    .attached
                    .iter()
                    .filter(|b| b.project == a.project)
                    .all(|b| {
                        b.field == field
                            || self
                                .cat
                                .fields
                                .iter()
                                .find(|f| f.id == b.field)
                                .is_none_or(|f| f.kind != FieldKind::State)
                    })
            })
    }

    /// One value of a user or group field.
    fn people_value(&mut self, field: &FieldRef, atom: &Atom) -> Filter {
        if let Some(filter) = self.empty_any(field, atom) {
            return filter;
        }
        if field.kind == FieldKind::Group {
            if let Some(group) = self.cat.group(&atom.text) {
                self.bind(atom.span, Binding::Group(group.id));
                return self.ref_values(field, vec![group.id]);
            }
            if let Some(label) = self.empty_label(field, atom) {
                return label;
            }
            self.problem(ProblemCode::UnknownValue, atom.span);
            return Filter::True;
        }
        if strong_keyword(atom).is_none()
            && self.cat.user(&atom.text) == People::None
            && self.cat.group(&atom.text).is_none()
            && let Some(label) = self.empty_label(field, atom)
        {
            return label;
        }
        match self.who(atom) {
            Some(Who::Users(ids)) if ids.is_empty() => Filter::False,
            Some(Who::Users(ids)) => self.ref_values(field, ids),
            _ => Filter::True,
        }
    }

    /// `is: value` (plan §3.4): the union of every reading of the value.
    fn standalone(&mut self, atom: &Atom) -> Filter {
        match strong_keyword(atom) {
            Some("me") => {
                self.bind(atom.span, Binding::Keyword("me".to_owned()));
                return self.involves(vec![self.ctx.me]);
            }
            Some(_) => {
                self.problem(ProblemCode::UnknownValue, atom.span);
                return Filter::True;
            }
            None => {}
        }
        let mut out = Vec::new();
        let fields: Vec<FieldEntry> = self
            .cat
            .fields
            .iter()
            .filter(|f| !self.cat.attachments(f.id, &self.ctx.scope).is_empty())
            .cloned()
            .collect();
        for field in &fields {
            let fref = self.field_ref(field);
            if let Some(label) = self.empty_label(&fref, atom) {
                out.push(label);
            }
            if field.kind.set_kind().is_some_and(SetKind::ordered) {
                let ids: Vec<i64> = self
                    .sets(field.id)
                    .iter()
                    .flat_map(|set| self.cat.values_named(*set, &atom.text))
                    .map(|v| v.id)
                    .collect();
                if !ids.is_empty() {
                    for id in &ids {
                        self.bind(atom.span, Binding::Value(*id));
                    }
                    if field.kind == FieldKind::State {
                        self.concerns = true;
                    }
                    out.push(self.ref_values(&fref, ids));
                }
            }
        }
        let projects = self.cat.projects_named(&atom.text);
        if !projects.is_empty() {
            for id in &projects {
                self.bind(atom.span, Binding::Project(*id));
            }
            out.push(Filter::Projects(projects));
        }
        let tags = self.cat.tags_named(&atom.text);
        if !tags.is_empty() {
            for id in &tags {
                self.bind(atom.span, Binding::Tag(*id));
            }
            out.push(Filter::Tags(tags));
        }
        let lower = fold_value(&atom.text);
        if let Some(user) = self
            .cat
            .users
            .iter()
            .find(|u| fold_value(&u.login) == lower)
        {
            self.bind(atom.span, Binding::User(user.id));
            out.push(self.involves(vec![user.id]));
        }
        if !out.is_empty() {
            return merge(Filter::or(out));
        }
        match keyword(atom) {
            Some(key @ ("resolved" | "unresolved")) => {
                self.concerns = true;
                self.keyword(atom, key, Filter::Resolved(key == "resolved"))
            }
            Some("starred") => self.keyword(atom, "starred", Filter::Starred),
            _ => {
                self.problem(ProblemCode::UnknownValue, atom.span);
                Filter::True
            }
        }
    }
}

fn within(position: u32, range: &Range<u32>) -> bool {
    let low = match range.low {
        Bound::Open => true,
        Bound::Included(v) => position >= v,
        Bound::Excluded(v) => position > v,
    };
    let high = match range.high {
        Bound::Open => true,
        Bound::Included(v) => position <= v,
        Bound::Excluded(v) => position < v,
    };
    low && high
}

/// A wall-clock range as calendar dates (date fields): a start inside a day keeps that day, an
/// end inside a day keeps it too. `None` when a date is out of range.
fn calendar(range: Range<jiff::civil::DateTime>) -> Option<Range<time::Date>> {
    let low = match range.low {
        Bound::Open => Bound::Open,
        Bound::Included(t) => Bound::Included(dates::to_time_date(t.date())?),
        Bound::Excluded(t) => Bound::Excluded(dates::to_time_date(t.date())?),
    };
    let high = match range.high {
        Bound::Open => Bound::Open,
        Bound::Excluded(t) => Bound::Excluded(dates::to_time_date(dates::date_ceil(t)?)?),
        Bound::Included(t) => Bound::Included(dates::to_time_date(t.date())?),
    };
    Some(Range { low, high })
}

/// Merges `RefValues` of one field and `Tags`/`Projects`/`Issues` lists inside an OR, so
/// `state: open, fixed` is one atom.
fn merge(filter: Filter) -> Filter {
    let Filter::Or(parts) = filter else {
        return filter;
    };
    let mut out: Vec<Filter> = Vec::with_capacity(parts.len());
    for part in parts {
        let joined = out.iter_mut().any(|seen| match (seen, &part) {
            (
                Filter::RefValues {
                    field,
                    ids,
                    implied,
                },
                Filter::RefValues {
                    field: f2,
                    ids: ids2,
                    implied: i2,
                },
            ) if *field == *f2 => {
                ids.extend(ids2);
                ids.sort_unstable();
                ids.dedup();
                if implied != i2 {
                    *implied = None;
                }
                true
            }
            (Filter::Tags(a), Filter::Tags(b))
            | (Filter::Projects(a), Filter::Projects(b))
            | (Filter::Issues(a), Filter::Issues(b))
            | (Filter::Reporter(a), Filter::Reporter(b))
            | (Filter::Updater(a), Filter::Updater(b))
            | (Filter::Commenter(a), Filter::Commenter(b))
            | (Filter::OnBoards(a), Filter::OnBoards(b))
            | (Filter::Sprints(a), Filter::Sprints(b)) => {
                a.extend(b);
                a.sort_unstable();
                a.dedup();
                true
            }
            _ => false,
        });
        if !joined {
            out.push(part);
        }
    }
    Filter::or(out)
}
