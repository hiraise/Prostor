//! The union by position (D-024): sorting by the positions of a set field pages through one
//! branch per position instead of one sorted scan, when the filter is unselective enough that
//! the scan walks most of its matches (the «fixed bugs in all projects» shape). Each branch
//! walks the sort field's value index (`issue_ref_values_value_idx`) and stops at a full page;
//! the outer `limit` stops at the first branch that fills one, so the cost hardly depends on
//! how many issues match.

use std::collections::BTreeMap;

use prostor_domain::fields::FieldKind;
use prostor_domain::search::{FieldRef, Filter, SortBy, SortSpec};
use prostor_query::Direction;
use sqlx::{PgConnection, Postgres, QueryBuilder, Row};

use super::order::SortValue;
use super::{Hit, compile};
use crate::visibility::Visibility;

/// How many positions the union serves (D-024: the small ordered sets — enumerations, states,
/// versions, builds). Bigger sets keep the one sorted scan.
const POSITIONS_MAX: usize = 100;

/// One branch of the union: the issues whose smallest position is `rank` (`min` over the issue's
/// values, as [`super::order`] reads the key), or — `rank` `None` — the issues with no value of
/// the field, which sort last both ways.
struct Branch {
    rank: Option<u32>,
    /// The values at `rank`; at no rank, every value of the field (the branch is their negation).
    values: Vec<i64>,
    /// The values of every position that sorts before this one; a multi-valued issue sorts by
    /// its smallest position, so it must not appear in a later branch.
    earlier: Vec<i64>,
}

/// Where a branch stands relative to the cursor.
enum Place {
    Whole,
    /// Only the issues after this one in the tie-break order (`id`).
    Continue(i64),
    Skip,
}

/// What every branch of one page shares.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    field: &'a FieldRef,
    filter: &'a Filter,
    vis: &'a Visibility,
    /// The direction of the tie-break key (`id`).
    id_direction: Direction,
}

/// Pages by position when the shape fits, else `None` (the caller falls back to the scan).
pub(super) async fn page(
    conn: &mut PgConnection,
    filter: &Filter,
    sort: &[SortSpec],
    vis: &Visibility,
    after: Option<&[SortValue]>,
    limit: i64,
) -> Result<Option<Vec<Hit>>, sqlx::Error> {
    if sort.len() != 2
        || !matches!(sort[0].by, SortBy::Positions { .. })
        || sort[1].by != SortBy::Id
        || vis.within.is_some()
        || !unselective(filter)
    {
        return Ok(None);
    }
    let SortBy::Positions { field, positions } = &sort[0].by else {
        return Ok(None);
    };
    // One branch per position, in the order the sort walks them; `positions` arrives by value
    // id, not by rank, and a rank's values may interleave with another's.
    let mut by_rank: BTreeMap<u32, Vec<i64>> = BTreeMap::new();
    for &(value, position) in positions {
        by_rank.entry(position).or_default().push(value);
    }
    if by_rank.len() > POSITIONS_MAX {
        return Ok(None);
    }
    let mut ranks: Vec<(u32, Vec<i64>)> = by_rank.into_iter().collect();
    if sort[0].direction == Direction::Desc {
        ranks.reverse();
    }
    let all: Vec<i64> = ranks.iter().flat_map(|(_, v)| v.iter().copied()).collect();
    let mut branches: Vec<Branch> = Vec::with_capacity(ranks.len() + 1);
    let mut seen: Vec<i64> = Vec::new();
    for (rank, values) in &ranks {
        branches.push(Branch {
            rank: Some(*rank),
            values: values.clone(),
            earlier: std::mem::take(&mut seen),
        });
        seen.extend(values.iter().copied());
    }
    branches.push(Branch {
        rank: None,
        values: all,
        earlier: Vec::new(),
    });

    // A positive top-level condition on the sort field itself selects branches (D-024): a
    // single-valued issue at a position whose values the condition does not name cannot match,
    // so its branch is dropped — and so does the no-value branch. At a multi-valued field the
    // smallest position may differ from the one the condition names, so every branch stays.
    if let Some(ids) = on_sort_field(filter, field).filter(|_| !field.multi) {
        branches.retain(|branch| {
            branch
                .rank
                .is_some_and(|_| branch.values.iter().any(|value| ids.contains(value)))
        });
        if branches.is_empty() {
            return Ok(None);
        }
    }

    // The cursor holds the last hit's keys: the rank (or no value) and the issue. Branches
    // before its own are served; its own continues after the issue; the rest stay whole.
    let cursor = match after {
        Some([SortValue::Int(rank), SortValue::Int(id)]) => match u32::try_from(*rank) {
            Ok(rank) => Some((Some(rank), *id)),
            Err(_) => return Ok(None),
        },
        Some([SortValue::Null, SortValue::Int(id)]) => Some((None, *id)),
        _ => None,
    };
    let at = cursor.map(|(rank, id)| {
        (
            branches
                .iter()
                .position(|b| b.rank == rank)
                .unwrap_or(branches.len() - 1),
            id,
        )
    });

    let ctx = Ctx {
        field,
        filter,
        vis,
        id_direction: sort[1].direction,
    };
    let mut qb = QueryBuilder::<Postgres>::new("select s.id, s.k0, s.k1 from (");
    let mut pushed = false;
    for (index, branch) in branches.iter().enumerate() {
        let place = match at {
            Some((at, _)) if index < at => Place::Skip,
            Some((at, id)) if index == at => Place::Continue(id),
            _ => Place::Whole,
        };
        if matches!(place, Place::Skip) {
            continue;
        }
        if pushed {
            qb.push(" union all ");
        }
        pushed = true;
        push_branch(&mut qb, &ctx, limit, branch, place);
    }
    qb.push(") s limit ");
    qb.push_bind(limit);

    // The append must run its branches in order: the outer limit stops at the first full page
    // only while no worker pulls a later branch first (D-024). Set for this connection only,
    // and back afterwards — the pool shares it.
    sqlx::query("set enable_parallel_append = off")
        .execute(&mut *conn)
        .await?;
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await;
    sqlx::query("set enable_parallel_append = on")
        .execute(&mut *conn)
        .await?;
    Ok(Some(
        rows?
            .iter()
            .map(|row| {
                Ok(Hit {
                    id: row.try_get("id")?,
                    keys: vec![
                        row.try_get::<Option<i64>, _>("k0")?
                            .map_or(SortValue::Null, SortValue::Int),
                        SortValue::Int(row.try_get::<i64, _>("k1")?),
                    ],
                })
            })
            .collect::<Result<_, sqlx::Error>>()?,
    ))
}

/// Whether the filter leaves the union a page that fills fast (D-024's "unselective filters"):
/// every top-level conjunct is a condition the value indexes check per candidate — set values,
/// emptiness, resolution, the exclusion of archived projects. Anything narrower (a person, a
/// project, dates, text) keeps the one sorted scan, which such filters already serve in budget.
fn unselective(filter: &Filter) -> bool {
    match filter {
        Filter::And(parts) => parts.iter().all(unselective),
        Filter::True | Filter::Resolved(_) => true,
        Filter::RefValues { field, .. } | Filter::FieldAny { field } => is_set(field),
        Filter::FieldEmpty { field, .. } => is_set(field),
        Filter::Not(inner) => match &**inner {
            Filter::Projects(_) | Filter::Resolved(_) => true,
            Filter::RefValues { field, .. } | Filter::FieldAny { field } => is_set(field),
            Filter::FieldEmpty { field, .. } => is_set(field),
            _ => false,
        },
        _ => false,
    }
}

fn is_set(field: &FieldRef) -> bool {
    matches!(
        field.kind,
        FieldKind::Enum
            | FieldKind::State
            | FieldKind::Version
            | FieldKind::Build
            | FieldKind::Owned
    )
}

/// The values of the one positive top-level condition on `field`, when exactly one names it. A
/// repeated attribute (an AND of two value lists) leaves the choice to the scan.
fn on_sort_field<'a>(filter: &'a Filter, field: &FieldRef) -> Option<&'a [i64]> {
    let parts = match filter {
        Filter::And(parts) => parts.as_slice(),
        other => std::slice::from_ref(other),
    };
    let mut found: Option<&[i64]> = None;
    for part in parts {
        if let Filter::RefValues { field: f, ids, .. } = part
            && f.id == field.id
        {
            if found.is_some() {
                return None;
            }
            found = Some(ids.as_slice());
        }
    }
    found
}

/// One parenthesized branch of the union, in the tie-break order of the id key. A rank branch
/// walks the value index (`field_id, value, issue_id`), so its `order by issue_id` needs no sort
/// and stops at the page; the filter rides along as conditions on the joined issue row.
fn push_branch(
    qb: &mut QueryBuilder<Postgres>,
    ctx: &Ctx<'_>,
    limit: i64,
    branch: &Branch,
    place: Place,
) {
    let Ctx {
        field,
        filter,
        vis,
        id_direction,
    } = *ctx;
    let order = match id_direction {
        Direction::Asc => "asc",
        Direction::Desc => "desc",
    };
    let after = match id_direction {
        Direction::Asc => "> ",
        Direction::Desc => "< ",
    };
    let issue = match branch.rank {
        Some(rank) => {
            qb.push("(select s.issue_id as id, ");
            qb.push(i64::from(rank).to_string());
            qb.push(
                "::bigint as k0, s.issue_id as k1 \
                 from issue_ref_values s join issues i on i.id = s.issue_id \
                 where s.field_id = ",
            );
            qb.push_bind(field.id);
            qb.push(" and s.value = any(");
            qb.push_bind(branch.values.clone());
            qb.push(") and ");
            // In the projects where the field is hidden the key reads as empty (FLD-062), so no
            // rank branch may hold their issues; they fall to the no-value branch below.
            if !field.hidden.is_empty() {
                qb.push("i.project_id <> all(");
                qb.push_bind(field.hidden.clone());
                qb.push(") and ");
            }
            "s.issue_id"
        }
        None => {
            qb.push("(select i.id as id, null::bigint as k0, i.id as k1 from issues i where ");
            "i.id"
        }
    };
    compile::push_base(qb, vis);
    qb.push(" and ");
    compile::push_filter(qb, filter, vis);
    if branch.rank.is_none() {
        qb.push(" and not ");
        push_has(qb, field, &branch.values);
    }
    // A multi-valued issue sorts by its smallest position: it must not appear in a later branch.
    if !branch.earlier.is_empty() {
        qb.push(" and not exists (select from issue_ref_values e where e.issue_id = ");
        qb.push(issue);
        qb.push(" and e.field_id = ");
        qb.push_bind(field.id);
        qb.push(" and e.value = any(");
        qb.push_bind(branch.earlier.clone());
        qb.push("))");
    }
    if let Place::Continue(id) = place {
        qb.push(" and ");
        qb.push(issue);
        qb.push(" ");
        qb.push(after);
        qb.push_bind(id);
    }
    qb.push(" order by ");
    qb.push(issue);
    qb.push(" ");
    qb.push(order);
    qb.push(" limit ");
    qb.push_bind(limit);
    qb.push(")");
}

/// `(`the field is readable here and` the issue holds one of `values`)` on issue row `i`.
fn push_has(qb: &mut QueryBuilder<Postgres>, field: &FieldRef, values: &[i64]) {
    qb.push("(");
    if !field.hidden.is_empty() {
        qb.push("i.project_id <> all(");
        qb.push_bind(field.hidden.clone());
        qb.push(") and ");
    }
    qb.push("exists (select from issue_ref_values v where v.issue_id = i.id and v.field_id = ");
    qb.push_bind(field.id);
    qb.push(" and v.value = any(");
    qb.push_bind(values.to_vec());
    qb.push(")))");
}
