//! A resolved [`Filter`] → SQL conditions on issue row `i` (plan step 1.5 §4, D-024).

use sqlx::{Encode, Postgres, QueryBuilder, Type};
use trackflow_domain::fields::FieldKind;
use trackflow_domain::links::Side;
use trackflow_domain::search::{Bound, FieldRef, Filter, Range, Target, TextScope, TextTerm};

use crate::visibility::{Visibility, push_projects};

/// The value table of a field kind.
pub(crate) fn table(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::String => "issue_string_values",
        FieldKind::Text => "issue_text_values",
        FieldKind::Integer | FieldKind::Duration => "issue_int_values",
        FieldKind::Float => "issue_float_values",
        FieldKind::Date => "issue_date_values",
        FieldKind::DateTime => "issue_datetime_values",
        FieldKind::Enum
        | FieldKind::State
        | FieldKind::User
        | FieldKind::Group
        | FieldKind::Version
        | FieldKind::Build
        | FieldKind::Owned => "issue_ref_values",
    }
}

/// The base conditions of every query: published, not deleted, visible.
pub(crate) fn push_base(qb: &mut QueryBuilder<Postgres>, vis: &Visibility) {
    qb.push("i.number is not null and i.deleted_at is null");
    if let Some(project) = vis.within {
        qb.push(" and i.project_id = ");
        qb.push_bind(project);
    }
    vis.push_sql(qb, "i");
}

/// Pushes the filter as one parenthesized condition.
pub fn push_filter(qb: &mut QueryBuilder<Postgres>, filter: &Filter, vis: &Visibility) {
    match filter {
        Filter::True => {
            qb.push("true");
        }
        Filter::False => {
            qb.push("false");
        }
        Filter::And(parts) | Filter::Or(parts) => {
            let joint = if matches!(filter, Filter::And(_)) {
                " and "
            } else {
                " or "
            };
            qb.push("(");
            for (index, part) in parts.iter().enumerate() {
                if index > 0 {
                    qb.push(joint);
                }
                push_filter(qb, part, vis);
            }
            qb.push(")");
        }
        Filter::Not(inner) => {
            qb.push("not ");
            push_filter(qb, inner, vis);
        }
        Filter::RefValues {
            field,
            ids,
            implied,
        } => {
            qb.push("(");
            push_exists(qb, field, |qb| {
                qb.push(" and v.value = any(");
                qb.push_bind(ids.clone());
                qb.push(")");
            });
            match implied {
                Some(true) => qb.push(" and i.resolved_at is not null"),
                Some(false) => qb.push(" and i.resolved_at is null"),
                None => qb,
            };
            qb.push(")");
        }
        Filter::FieldEmpty { field, projects } => {
            qb.push("(i.project_id = any(");
            qb.push_bind(projects.clone());
            qb.push(") and not ");
            push_exists(
                qb,
                &FieldRef {
                    hidden: Vec::new(),
                    ..field.clone()
                },
                |_| {},
            );
            push_hidden(qb, field);
            qb.push(")");
        }
        Filter::FieldAny { field } => push_exists(qb, field, |_| {}),
        Filter::Int { field, range } => {
            push_exists(qb, field, |qb| push_range(qb, "v.value", range));
        }
        Filter::Float { field, range } => {
            push_exists(qb, field, |qb| push_range(qb, "v.value", range));
        }
        Filter::Date { field, range } => {
            push_exists(qb, field, |qb| push_range(qb, "v.value", range));
        }
        Filter::DateTime { field, range } => {
            push_exists(qb, field, |qb| push_range(qb, "v.value", range));
        }
        Filter::Str { field, value } => push_exists(qb, field, |qb| {
            qb.push(" and lower(v.value) = lower(");
            qb.push_bind(value.clone());
            qb.push(")");
        }),
        Filter::Resolved(true) => {
            qb.push("i.resolved_at is not null");
        }
        Filter::Resolved(false) => {
            qb.push("i.resolved_at is null");
        }
        Filter::Projects(ids) => push_any(qb, "i.project_id", ids),
        Filter::Issues(ids) => push_any(qb, "i.id", ids),
        Filter::Reporter(users) => push_any(qb, "i.reporter_id", users),
        Filter::Updater(users) => push_any(qb, "i.updated_by", users),
        Filter::Commenter(users) => push_comment(qb, vis, |qb| {
            qb.push(" and c.author_id = any(");
            qb.push_bind(users.clone());
            qb.push(")");
        }),
        Filter::Involves { users, fields } => {
            qb.push("(");
            push_any(qb, "i.reporter_id", users);
            for field in fields {
                qb.push(" or ");
                push_exists(qb, field, |qb| {
                    qb.push(" and v.value = any(");
                    qb.push_bind(users.clone());
                    qb.push(")");
                });
            }
            qb.push(" or ");
            push_comment(qb, vis, |qb| {
                qb.push(" and c.author_id = any(");
                qb.push_bind(users.clone());
                qb.push(")");
            });
            qb.push(")");
        }
        Filter::Created(range) => push_scalar(qb, "i.created_at", range),
        Filter::Updated(range) => push_scalar(qb, "i.updated_at", range),
        Filter::ResolvedAt(range) => push_scalar(qb, "i.resolved_at", range),
        Filter::Commented(range) => {
            push_comment(qb, vis, |qb| push_range(qb, "c.created_at", range));
        }
        Filter::Comments => push_comment(qb, vis, |_| {}),
        Filter::Attachments => push_attachment(qb, vis),
        Filter::DescriptionEmpty => {
            qb.push("i.description = ''");
        }
        Filter::Tags(ids) => {
            qb.push("exists (select from issue_tags t where t.issue_id = i.id and t.tag_id = any(");
            qb.push_bind(ids.clone());
            qb.push("))");
        }
        Filter::NoTags => {
            // Tags the viewer sees: own and shared for viewing (ISS-037).
            qb.push(
                "not exists (select from issue_tags t join tags g on g.id = t.tag_id \
                 where t.issue_id = i.id and (g.owner_id = ",
            );
            qb.push_bind(vis.me);
            qb.push(" or ");
            qb.push_bind(vis.me);
            qb.push(" = any(g.view_users) or g.view_groups && ");
            qb.push_bind(vis.groups.clone());
            qb.push("))");
        }
        Filter::OnBoards(ids) => {
            qb.push(
                "exists (select from sprint_issues si join sprints s on s.id = si.sprint_id \
                 where si.issue_id = i.id and s.board_id = any(",
            );
            qb.push_bind(ids.clone());
            qb.push("))");
        }
        Filter::Sprints(ids) => {
            qb.push(
                "exists (select from sprint_issues si where si.issue_id = i.id and si.sprint_id = any(",
            );
            qb.push_bind(ids.clone());
            qb.push("))");
        }
        Filter::Starred => {
            qb.push(
                "exists (select from issue_watchers w where w.issue_id = i.id and w.user_id = ",
            );
            qb.push_bind(vis.me);
            qb.push(")");
        }
        Filter::Linked { sides, target } => push_linked(qb, vis, sides.as_deref(), target),
        Filter::Votes(range) => push_scalar(qb, "i.votes", range),
        Filter::Text { scope, term } => push_text(qb, vis, scope, term),
        Filter::MentionsUsers(users) => {
            qb.push("((cardinality(i.mentioned_users) > 0 and i.mentioned_users && ");
            qb.push_bind(users.clone());
            qb.push(") or ");
            push_comment(qb, vis, |qb| {
                qb.push(" and cardinality(c.mentioned_users) > 0 and c.mentioned_users && ");
                qb.push_bind(users.clone());
            });
            qb.push(")");
        }
        Filter::MentionsIssue(id) => {
            qb.push("((cardinality(i.issue_tokens) > 0 and i.issue_tokens && ");
            push_tokens_of(qb, *id);
            qb.push(") or ");
            push_comment(qb, vis, |qb| {
                qb.push(" and cardinality(c.issue_tokens) > 0 and c.issue_tokens && ");
                push_tokens_of(qb, *id);
            });
            qb.push(")");
        }
        Filter::MentionedIn(id) => push_mentioned_in(qb, vis, *id),
    }
}

/// The text configuration of the stored documents (migration 0010).
const CONFIG: &str = "'trackflow_text'";

/// The text query of a term: its words in any form, or the phrase.
pub(crate) fn push_tsquery(qb: &mut QueryBuilder<Postgres>, term: &TextTerm) {
    let function = match term {
        TextTerm::Phrase(_) => "phraseto_tsquery",
        TextTerm::Word(_) | TextTerm::Exact(_) => "plainto_tsquery",
    };
    qb.push(format!("{function}({CONFIG}, "));
    qb.push_bind(term.text().to_owned());
    qb.push(")");
}

/// `<doc> @@ q`; for an exact form also the same word, unstemmed, in `<text>` (QRY-037).
fn push_match(qb: &mut QueryBuilder<Postgres>, doc: &str, text: &str, term: &TextTerm) {
    qb.push(format!("({doc} @@ "));
    push_tsquery(qb, term);
    if let TextTerm::Exact(word) = term {
        // The `simple` configuration lowercases and keeps the form; it splits words as the
        // stored documents do, whatever the database's locale.
        qb.push(format!(
            " and to_tsvector('simple', {text}) @@ plainto_tsquery('simple', "
        ));
        qb.push_bind(word.clone());
        qb.push(")");
    }
    qb.push(")");
}

/// The text value `t` belongs to one of `fields` that the viewer reads in issue `i`'s project.
pub(crate) fn push_text_fields(qb: &mut QueryBuilder<Postgres>, fields: &[FieldRef]) {
    let (open, private): (Vec<&FieldRef>, Vec<&FieldRef>) =
        fields.iter().partition(|f| f.hidden.is_empty());
    qb.push("(false");
    if !open.is_empty() {
        qb.push(" or t.field_id = any(");
        qb.push_bind(open.iter().map(|f| f.id).collect::<Vec<i64>>());
        qb.push(")");
    }
    for field in private {
        qb.push(" or (t.field_id = ");
        qb.push_bind(field.id);
        push_hidden(qb, field);
        qb.push(")");
    }
    qb.push(")");
}

/// The text fields a scope searches.
pub(crate) fn scope_fields(scope: &TextScope) -> &[FieldRef] {
    match scope {
        TextScope::All { fields } => fields,
        TextScope::Field(field) => std::slice::from_ref(field),
        _ => &[],
    }
}

fn push_text(
    qb: &mut QueryBuilder<Postgres>,
    vis: &Visibility,
    scope: &TextScope,
    term: &TextTerm,
) {
    let summary = matches!(scope, TextScope::All { .. } | TextScope::Summary);
    let description = matches!(scope, TextScope::All { .. } | TextScope::Description);
    let comments = matches!(scope, TextScope::All { .. } | TextScope::Comments);
    let fields = scope_fields(scope);
    // Candidates from the GIN index of each part, uncorrelated, so the planner starts from them;
    // the exact condition below (audiences, private fields, exact forms) checks only those. Without
    // it the OR with the comment `exists` made every visible issue a candidate (D-037).
    let mut branches = Vec::new();
    if summary {
        branches.push(
            "select id from issues where number is not null and deleted_at is null and summary_doc @@ ",
        );
    }
    if description {
        branches.push(
            "select id from issues where number is not null and deleted_at is null and description_doc @@ ",
        );
    }
    if !fields.is_empty() {
        branches.push("select issue_id from issue_text_values where doc @@ ");
    }
    if comments {
        branches.push("select issue_id from comments where deleted_at is null and doc @@ ");
    }
    qb.push("(i.id in (");
    for (n, branch) in branches.into_iter().enumerate() {
        if n > 0 {
            qb.push(" union all ");
        }
        qb.push(branch);
        push_tsquery(qb, term);
    }
    // Every scope has at least one part.
    qb.push(") and ");
    qb.push("(false");
    if summary {
        qb.push(" or ");
        push_match(qb, "i.summary_doc", "i.summary", term);
    }
    if description {
        qb.push(" or ");
        push_match(qb, "i.description_doc", "i.description", term);
    }
    if !fields.is_empty() {
        qb.push(" or exists (select from issue_text_values t where t.issue_id = i.id and ");
        push_text_fields(qb, fields);
        qb.push(" and ");
        push_match(qb, "t.doc", "t.value", term);
        qb.push(")");
    }
    if comments {
        qb.push(" or ");
        push_comment(qb, vis, |qb| {
            qb.push(" and ");
            push_match(qb, "c.doc", "c.text", term);
        });
    }
    qb.push("))");
}

/// Every ID issue `id` is written with: its number under each key of its project, current and
/// former, and its old places (D-029), as the renderer stores tokens (`KEY-12`).
fn push_tokens_of(qb: &mut QueryBuilder<Postgres>, id: i64) {
    qb.push(
        "(select coalesce(array_agg(k.key || '-' || x.number), '{}') from          (select project_id, number from issues where id = ",
    );
    qb.push_bind(id);
    qb.push(" and number is not null union all select project_id, number from issue_aliases where issue_id = ");
    qb.push_bind(id);
    qb.push(") x join project_keys k on k.project_id = x.project_id)");
}

/// The issues that issue `id`'s description and its comments visible to the viewer mention;
/// the outer visibility predicate keeps only readable ones.
fn push_mentioned_in(qb: &mut QueryBuilder<Postgres>, vis: &Visibility, id: i64) {
    qb.push(
        "i.id in (select coalesce(x.id, a.issue_id) from          (select k.project_id, case when split_part(w.token, '-', 2) ~ '^[1-9][0-9]{0,17}$'          then split_part(w.token, '-', 2)::bigint end as number from          (select unnest(m.issue_tokens) as token from issues m where m.id = ",
    );
    qb.push_bind(id);
    qb.push(
        " union select unnest(c.issue_tokens) from issues m join comments c on c.issue_id = m.id          where m.id = ",
    );
    qb.push_bind(id);
    qb.push(" and ");
    push_visible_comment_of(qb, vis, "m");
    qb.push(
        ") w join project_keys k on k.key = split_part(w.token, '-', 1)) r          left join issues x on x.project_id = r.project_id and x.number = r.number          left join issue_aliases a on a.project_id = r.project_id and a.number = r.number          where r.number is not null and (x.id is not null or a.issue_id is not null))",
    );
}

fn push_any(qb: &mut QueryBuilder<Postgres>, column: &str, ids: &[i64]) {
    qb.push(format!("{column} = any("));
    qb.push_bind(ids.to_vec());
    qb.push(")");
}

/// `exists (select from <table> v where v.issue_id = i.id and v.field_id = …<more>)`, false in
/// the projects where the field is hidden from the viewer.
fn push_exists(
    qb: &mut QueryBuilder<Postgres>,
    field: &FieldRef,
    more: impl FnOnce(&mut QueryBuilder<Postgres>),
) {
    let hidden = !field.hidden.is_empty();
    if hidden {
        qb.push("(");
    }
    qb.push(format!(
        "exists (select from {} v where v.issue_id = i.id and v.field_id = ",
        table(field.kind)
    ));
    qb.push_bind(field.id);
    more(qb);
    qb.push(")");
    if hidden {
        push_hidden(qb, field);
        qb.push(")");
    }
}

fn push_hidden(qb: &mut QueryBuilder<Postgres>, field: &FieldRef) {
    if !field.hidden.is_empty() {
        qb.push(" and i.project_id <> all(");
        qb.push_bind(field.hidden.clone());
        qb.push(")");
    }
}

/// ` and <column> >= a and <column> < b` for the bounds that are not open.
fn push_range<T>(qb: &mut QueryBuilder<Postgres>, column: &str, range: &Range<T>)
where
    T: Copy + for<'t> Encode<'t, Postgres> + Type<Postgres>,
{
    for (bound, inclusive, exclusive) in [(range.low, ">=", ">"), (range.high, "<=", "<")] {
        match bound {
            Bound::Open => {}
            Bound::Included(value) => {
                qb.push(format!(" and {column} {inclusive} "));
                qb.push_bind(value);
            }
            Bound::Excluded(value) => {
                qb.push(format!(" and {column} {exclusive} "));
                qb.push_bind(value);
            }
        }
    }
}

/// A range on a column of the issue row.
fn push_scalar<T>(qb: &mut QueryBuilder<Postgres>, column: &str, range: &Range<T>)
where
    T: Copy + for<'t> Encode<'t, Postgres> + Type<Postgres>,
{
    qb.push(format!("({column} is not null"));
    push_range(qb, column, range);
    qb.push(")");
}

/// The comment `c` of issue `i` is visible to the viewer: the Rust rule is
/// `domain::access::visibility::can_see(Kind::Comment, …)`.
pub(crate) fn push_visible_comment(qb: &mut QueryBuilder<Postgres>, vis: &Visibility) {
    push_visible_comment_of(qb, vis, "i");
}

/// The comment `c` of issue row `issue` is visible to the viewer.
fn push_visible_comment_of(qb: &mut QueryBuilder<Postgres>, vis: &Visibility, issue: &str) {
    qb.push("c.deleted_at is null and (c.author_id = ");
    qb.push_bind(vis.me);
    qb.push(" or ");
    push_projects(qb, &format!("{issue}.project_id"), &vis.comment_read);
    qb.push(")");
    push_audience(qb, vis, "c", issue);
}

/// ` and (<row> has no audience, or is the viewer's, or includes them, or they see all)`.
fn push_audience(qb: &mut QueryBuilder<Postgres>, vis: &Visibility, row: &str, issue: &str) {
    qb.push(format!(
        " and (({row}.audience_users = '{{}}' and {row}.audience_groups = '{{}}') or {row}.author_id = "
    ));
    qb.push_bind(vis.me);
    qb.push(" or ");
    qb.push_bind(vis.me);
    qb.push(format!(
        " = any({row}.audience_users) or {row}.audience_groups && "
    ));
    qb.push_bind(vis.groups.clone());
    qb.push(" or ");
    push_projects(qb, &format!("{issue}.project_id"), &vis.bypass);
    qb.push(")");
}

pub(crate) fn push_comment(
    qb: &mut QueryBuilder<Postgres>,
    vis: &Visibility,
    more: impl FnOnce(&mut QueryBuilder<Postgres>),
) {
    qb.push("exists (select from comments c where c.issue_id = i.id and ");
    push_visible_comment(qb, vis);
    more(qb);
    qb.push(")");
}

/// A visible file: the issue's readers see it within its audience; a comment's file needs the
/// comment visible too.
fn push_attachment(qb: &mut QueryBuilder<Postgres>, vis: &Visibility) {
    qb.push(
        "exists (select from attachments a where a.issue_id = i.id and a.deleted_at is null \
         and (a.author_id = ",
    );
    qb.push_bind(vis.me);
    qb.push(" or ");
    push_projects(qb, "i.project_id", &vis.read);
    qb.push(")");
    push_audience(qb, vis, "a", "i");
    qb.push(" and (a.comment_id is null or exists (select from comments c where c.id = a.comment_id and ");
    push_visible_comment(qb, vis);
    qb.push(")))");
}

/// Linked to the target: as the source by the types of source sides (and by undirected types,
/// which have one name for both ends), as the target by the types of target sides.
fn push_linked(
    qb: &mut QueryBuilder<Postgres>,
    vis: &Visibility,
    sides: Option<&[(i64, Side)]>,
    target: &Target,
) {
    qb.push(
        "exists (select from issue_links l join link_types lt on lt.id = l.type_id \
         and lt.deleted_at is null",
    );
    if *target == Target::Any {
        qb.push(
            " join issues o on o.id = case when l.source_id = i.id then l.target_id else l.source_id end \
             and o.number is not null and o.deleted_at is null",
        );
        let other = Visibility {
            within: None,
            ..vis.clone()
        };
        other.push_sql(qb, "o");
    }
    let end = |qb: &mut QueryBuilder<Postgres>, other: &str| {
        if let Target::Issues(ids) = target {
            qb.push(format!(" and {other} = any("));
            qb.push_bind(ids.clone());
            qb.push(")");
        }
    };
    qb.push(" where ((l.source_id = i.id");
    end(qb, "l.target_id");
    if let Some(sides) = sides {
        let (source, target_types) = split(sides);
        qb.push(" and l.type_id = any(");
        qb.push_bind(source);
        qb.push("))");
        qb.push(" or (l.target_id = i.id");
        end(qb, "l.source_id");
        qb.push(" and (l.type_id = any(");
        qb.push_bind(target_types);
        qb.push(") or (lt.direction = 'undirected' and l.type_id = any(");
        qb.push_bind(split(sides).0);
        qb.push("))))))");
    } else {
        qb.push(") or (l.target_id = i.id");
        end(qb, "l.source_id");
        qb.push(")))");
    }
}

fn split(sides: &[(i64, Side)]) -> (Vec<i64>, Vec<i64>) {
    let pick = |side: Side| -> Vec<i64> {
        sides
            .iter()
            .filter(|(_, s)| *s == side)
            .map(|(t, _)| *t)
            .collect()
    };
    (pick(Side::Source), pick(Side::Target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trackflow_domain::access::Projects;

    fn vis() -> Visibility {
        Visibility {
            me: 5,
            read: Projects::Only(vec![1, 2]),
            create_only: vec![3],
            bypass: Projects::Only(vec![]),
            comment_read: Projects::Only(vec![1]),
            groups: vec![7],
            within: None,
        }
    }

    fn sql(filter: &Filter) -> String {
        let mut qb = QueryBuilder::<Postgres>::new("");
        push_filter(&mut qb, filter, &vis());
        qb.sql().as_str().to_owned()
    }

    fn field(hidden: Vec<i64>) -> FieldRef {
        FieldRef {
            id: 9,
            kind: FieldKind::Enum,
            multi: false,
            hidden,
        }
    }

    #[test]
    fn ref_values_with_the_implied_resolution() {
        let text = sql(&Filter::RefValues {
            field: field(vec![]),
            ids: vec![1, 2],
            implied: Some(false),
        });
        assert_eq!(
            text,
            "(exists (select from issue_ref_values v where v.issue_id = i.id and v.field_id = $1 \
             and v.value = any($2)) and i.resolved_at is null)"
        );
    }

    #[test]
    fn private_fields_are_false_where_hidden() {
        let text = sql(&Filter::Not(Box::new(Filter::FieldAny {
            field: field(vec![4]),
        })));
        assert!(
            text.starts_with("not (exists (select from issue_ref_values")
                && text.ends_with(") and i.project_id <> all($2))"),
            "{text}"
        );
    }

    #[test]
    fn ranges_and_combinations() {
        let text = sql(&Filter::and(vec![
            Filter::Votes(Range::half_open(Some(2), None)),
            Filter::or(vec![Filter::Starred, Filter::Resolved(true)]),
        ]));
        assert_eq!(
            text,
            "((i.votes is not null and i.votes >= $1) and (exists (select from issue_watchers w \
             where w.issue_id = i.id and w.user_id = $2) or i.resolved_at is not null))"
        );
    }

    #[test]
    fn comments_are_checked_for_visibility() {
        let text = sql(&Filter::Commenter(vec![3]));
        assert!(
            text.contains("c.deleted_at is null and (c.author_id = $1 or i.project_id = any($2))"),
            "{text}"
        );
        assert!(text.contains("c.audience_groups && $5 or false)"), "{text}");
        assert!(text.ends_with("and c.author_id = any($6))"), "{text}");
    }

    #[test]
    fn linked_by_side() {
        let text = sql(&Filter::Linked {
            sides: Some(vec![(1, Side::Source), (2, Side::Target)]),
            target: Target::Issues(vec![40]),
        });
        assert!(
            text.contains("(l.source_id = i.id and l.target_id = any($1) and l.type_id = any($2))"),
            "{text}"
        );
        assert!(
            text.contains(
                "or (l.target_id = i.id and l.source_id = any($3) and (l.type_id = any($4)"
            ),
            "{text}"
        );
    }
}
