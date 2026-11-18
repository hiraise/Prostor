//! Sort keys → SQL expressions, and the keyset continuation after the last issue of a page.
//! Empty values go last in both directions. A multi-valued field sorts by its smallest value.

use prostor_domain::fields::FieldKind;
use prostor_domain::search::{FieldRef, SortBy, SortSpec, TextScope, TextTerm};
use prostor_query::Direction;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};
use time::{Date, OffsetDateTime};

use super::compile::{
    push_comment, push_text_fields, push_tsquery, push_visible_comment, scope_fields, table,
};
use crate::visibility::Visibility;

/// The value of one sort key on an issue; what a cursor carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "t", content = "v")]
pub enum SortValue {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Time(#[serde(with = "time::serde::rfc3339")] OffsetDateTime),
    Date(Date),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Int,
    Float,
    Text,
    Time,
    Date,
}

/// How one key reads.
struct Key {
    shape: Shape,
    nullable: bool,
}

fn key(by: &SortBy) -> Key {
    let (shape, nullable) = match by {
        SortBy::Updated | SortBy::Created => (Shape::Time, false),
        SortBy::Resolved => (Shape::Time, true),
        SortBy::Id | SortBy::Votes | SortBy::Comments | SortBy::Starred => (Shape::Int, false),
        SortBy::Project | SortBy::Reporter | SortBy::Summary => (Shape::Text, false),
        SortBy::Positions { .. } => (Shape::Int, true),
        SortBy::Relevance(_) => (Shape::Float, false),
        SortBy::Users { .. } | SortBy::Groups { .. } => (Shape::Text, true),
        SortBy::Scalar { field } => (
            match field.kind {
                FieldKind::Float => Shape::Float,
                FieldKind::Date => Shape::Date,
                FieldKind::DateTime => Shape::Time,
                FieldKind::String | FieldKind::Text => Shape::Text,
                _ => Shape::Int,
            },
            true,
        ),
    };
    Key { shape, nullable }
}

/// `case when <hidden> then null else (<expr>) end` for a private field.
fn push_field_expr(
    qb: &mut QueryBuilder<Postgres>,
    field: &FieldRef,
    expr: impl FnOnce(&mut QueryBuilder<Postgres>),
) {
    let hidden = !field.hidden.is_empty();
    if hidden {
        qb.push("case when i.project_id = any(");
        qb.push_bind(field.hidden.clone());
        qb.push(") then null else ");
    }
    qb.push("(");
    expr(qb);
    qb.push(")");
    if hidden {
        qb.push(" end");
    }
}

/// The SQL expression of a key on issue row `i`.
pub(crate) fn push_expr(qb: &mut QueryBuilder<Postgres>, by: &SortBy, vis: &Visibility) {
    match by {
        SortBy::Updated => {
            qb.push("i.updated_at");
        }
        SortBy::Created => {
            qb.push("i.created_at");
        }
        SortBy::Resolved => {
            qb.push("i.resolved_at");
        }
        SortBy::Id => {
            qb.push("i.id");
        }
        SortBy::Votes => {
            qb.push("i.votes::bigint");
        }
        SortBy::Comments => {
            qb.push("(select count(*) from comments c where c.issue_id = i.id and ");
            push_visible_comment(qb, vis);
            qb.push(")");
        }
        SortBy::Project => {
            qb.push("(select p.key from projects p where p.id = i.project_id)");
        }
        SortBy::Reporter => {
            qb.push("(select lower(u.full_name) from users u where u.id = i.reporter_id)");
        }
        SortBy::Summary => {
            qb.push("lower(i.summary)");
        }
        SortBy::Starred => {
            qb.push(
                "(case when exists (select from issue_watchers w where w.issue_id = i.id \
                 and w.user_id = ",
            );
            qb.push_bind(vis.me);
            qb.push(") then 0 else 1 end)::bigint");
        }
        SortBy::Positions { field, positions } => {
            let (values, ranks): (Vec<i64>, Vec<i64>) = positions
                .iter()
                .map(|(value, position)| (*value, i64::from(*position)))
                .unzip();
            push_field_expr(qb, field, |qb| {
                // An array lookup per row: a join with `unnest` would build a hash per row.
                qb.push("select min((");
                qb.push_bind(ranks);
                qb.push("::bigint[])[array_position(");
                qb.push_bind(values);
                qb.push(
                    "::bigint[], s.value)]) from issue_ref_values s \
                     where s.issue_id = i.id and s.field_id = ",
                );
                qb.push_bind(field.id);
            });
        }
        SortBy::Users { field } => push_field_expr(qb, field, |qb| {
            qb.push(
                "select min(lower(u.full_name)) from issue_ref_values s join users u on u.id = s.value \
                 where s.issue_id = i.id and s.field_id = ",
            );
            qb.push_bind(field.id);
        }),
        SortBy::Groups { field } => push_field_expr(qb, field, |qb| {
            qb.push(
                "select min(lower(g.name)) from issue_ref_values s join groups g on g.id = s.value \
                 where s.issue_id = i.id and s.field_id = ",
            );
            qb.push_bind(field.id);
        }),
        SortBy::Relevance(terms) => push_relevance(qb, vis, terms),
        SortBy::Scalar { field } => push_field_expr(qb, field, |qb| {
            let value = if key(by).shape == Shape::Text {
                "lower(s.value)"
            } else {
                "s.value"
            };
            qb.push(format!(
                "select min({value}) from {} s where s.issue_id = i.id and s.field_id = ",
                table(field.kind)
            ));
            qb.push_bind(field.id);
        }),
    }
}

/// QRY-045: per term, twice the summary's rank plus the description's; then 0.3 for a matching
/// visible comment and 0.3 for a matching text field. `float8` of `real` sums: the same SQL
/// gives the same value, so the cursor compares exactly.
fn push_relevance(
    qb: &mut QueryBuilder<Postgres>,
    vis: &Visibility,
    terms: &[(TextScope, TextTerm)],
) {
    qb.push("((0::real");
    for (scope, term) in terms {
        if matches!(scope, TextScope::All { .. } | TextScope::Summary) {
            qb.push(" + 2 * ts_rank_cd(i.summary_doc, ");
            push_tsquery(qb, term);
            qb.push(")");
        }
        if matches!(scope, TextScope::All { .. } | TextScope::Description) {
            qb.push(" + ts_rank_cd(i.description_doc, ");
            push_tsquery(qb, term);
            qb.push(")");
        }
    }
    let in_comments: Vec<&TextTerm> = terms
        .iter()
        .filter(|(s, _)| matches!(s, TextScope::All { .. } | TextScope::Comments))
        .map(|(_, t)| t)
        .collect();
    if !in_comments.is_empty() {
        qb.push(" + case when ");
        push_comment(qb, vis, |qb| {
            qb.push(" and (false");
            for term in &in_comments {
                qb.push(" or c.doc @@ ");
                push_tsquery(qb, term);
            }
            qb.push(")");
        });
        qb.push(" then 0.3::real else 0::real end");
    }
    let in_fields: Vec<(&[FieldRef], &TextTerm)> = terms
        .iter()
        .map(|(s, t)| (scope_fields(s), t))
        .filter(|(fields, _)| !fields.is_empty())
        .collect();
    if !in_fields.is_empty() {
        qb.push(
            " + case when exists (select from issue_text_values t where t.issue_id = i.id and (false",
        );
        for (fields, term) in in_fields {
            qb.push(" or (");
            push_text_fields(qb, fields);
            qb.push(" and t.doc @@ ");
            push_tsquery(qb, term);
            qb.push(")");
        }
        qb.push(")) then 0.3::real else 0::real end");
    }
    qb.push(")::float8)");
}

fn op(direction: Direction) -> &'static str {
    match direction {
        Direction::Asc => ">",
        Direction::Desc => "<",
    }
}

fn push_value(qb: &mut QueryBuilder<Postgres>, value: &SortValue) {
    match value {
        SortValue::Null => qb.push("null"),
        SortValue::Int(v) => qb.push_bind(*v),
        SortValue::Float(v) => qb.push_bind(*v),
        SortValue::Text(v) => qb.push_bind(v.clone()),
        SortValue::Time(v) => qb.push_bind(*v),
        SortValue::Date(v) => qb.push_bind(*v),
    };
}

/// The condition that row `s` (columns `k0…`) comes after `after` in the order of `sort`.
pub(crate) fn push_after(qb: &mut QueryBuilder<Postgres>, sort: &[SortSpec], after: &[SortValue]) {
    let keys: Vec<Key> = sort.iter().map(|s| key(&s.by)).collect();
    let plain = keys.iter().all(|k| !k.nullable)
        && sort.iter().all(|s| s.direction == sort[0].direction)
        && after.iter().all(|v| *v != SortValue::Null);
    if plain {
        // A row comparison the planner reads with an index.
        let columns: Vec<String> = (0..sort.len()).map(|j| format!("s.k{j}")).collect();
        qb.push(format!(
            "({}) {} (",
            columns.join(", "),
            op(sort[0].direction)
        ));
        for (j, value) in after.iter().enumerate() {
            if j > 0 {
                qb.push(", ");
            }
            push_value(qb, value);
        }
        qb.push(")");
        return;
    }
    qb.push("(false");
    for j in 0..sort.len() {
        let Some(value) = after.get(j) else { break };
        qb.push(" or (true");
        for (e, earlier) in after.iter().enumerate().take(j) {
            if *earlier == SortValue::Null {
                qb.push(format!(" and s.k{e} is null"));
            } else {
                qb.push(format!(" and s.k{e} = "));
                push_value(qb, earlier);
            }
        }
        if *value == SortValue::Null {
            // Empty values are last: nothing comes after them in this key.
            qb.push(" and false");
        } else {
            qb.push(format!(" and (s.k{j} {} ", op(sort[j].direction)));
            push_value(qb, value);
            if keys[j].nullable {
                qb.push(format!(" or s.k{j} is null"));
            }
            qb.push(")");
        }
        qb.push(")");
    }
    qb.push(")");
}

/// `order by s.k0 …`: empty values last both ways.
pub(crate) fn push_order(qb: &mut QueryBuilder<Postgres>, sort: &[SortSpec]) {
    qb.push(" order by ");
    for (j, spec) in sort.iter().enumerate() {
        if j > 0 {
            qb.push(", ");
        }
        let nulls = match (spec.direction, key(&spec.by).nullable) {
            (Direction::Desc, true) => " nulls last",
            _ => "",
        };
        let direction = match spec.direction {
            Direction::Asc => "asc",
            Direction::Desc => "desc",
        };
        qb.push(format!("s.k{j} {direction}{nulls}"));
    }
}

/// Reads column `k{j}` of a page row.
pub(crate) fn read(row: &PgRow, j: usize, by: &SortBy) -> Result<SortValue, sqlx::Error> {
    let column = format!("k{j}");
    let column = column.as_str();
    Ok(match key(by).shape {
        Shape::Int => row
            .try_get::<Option<i64>, _>(column)?
            .map_or(SortValue::Null, SortValue::Int),
        Shape::Float => row
            .try_get::<Option<f64>, _>(column)?
            .map_or(SortValue::Null, SortValue::Float),
        Shape::Text => row
            .try_get::<Option<String>, _>(column)?
            .map_or(SortValue::Null, SortValue::Text),
        Shape::Time => row
            .try_get::<Option<OffsetDateTime>, _>(column)?
            .map_or(SortValue::Null, SortValue::Time),
        Shape::Date => row
            .try_get::<Option<Date>, _>(column)?
            .map_or(SortValue::Null, SortValue::Date),
    })
}

/// The sort's keys as an `order by` list over issue row `i` (a window's order, BRD-070): empty
/// values last both ways, as [`push_order`] does.
pub(crate) fn push_order_exprs(
    qb: &mut QueryBuilder<Postgres>,
    sort: &[SortSpec],
    vis: &Visibility,
) {
    for (j, spec) in sort.iter().enumerate() {
        if j > 0 {
            qb.push(", ");
        }
        push_expr(qb, &spec.by, vis);
        let nulls = match (spec.direction, key(&spec.by).nullable) {
            (Direction::Desc, true) => " nulls last",
            _ => "",
        };
        let direction = match spec.direction {
            Direction::Asc => " asc",
            Direction::Desc => " desc",
        };
        qb.push(direction);
        qb.push(nulls);
    }
}
