//! The board's read path (BRD-068, BRD-057, BRD-028, BRD-070, BRD-081; D-044): the cards
//! of one sprint by column in one statement, each cell capped, with the counts over all cards.
//! A query board's cards are the matching issues, ranked by the rows of its hidden sprint.

use sqlx::{PgConnection, Postgres, QueryBuilder, Row};
use trackflow_domain::boards::lanes::Bounds;
use trackflow_domain::search::{Filter, SortSpec};
use trackflow_domain::{FieldId, IssueId, ProjectId, SprintId, ValueId};

use crate::search::compile::{push_base, push_filter};
use crate::search::order::push_order_exprs;
use crate::visibility::Visibility;

/// What the board shows: the sprint, the column field and values, the board projects, and what
/// narrows and orders the cards.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope<'a> {
    pub sprint: SprintId,
    pub column_field: FieldId,
    pub column_values: &'a [ValueId],
    pub projects: &'a [ProjectId],
    /// Mode `query`: every matching issue is a card; the sprint rows only rank them.
    pub query_mode: bool,
    /// The board query and the header filter, resolved.
    pub filter: Option<&'a Filter>,
    /// A sort clause orders each cell instead of the rank (BRD-070).
    pub sort: Option<&'a [SortSpec]>,
    /// The lanes, when the board has them (BRD-049).
    pub lanes: Option<LaneScope<'a>>,
    /// Only the cards of these lanes (a page of lanes, BRD-057, or one lane's cell).
    pub lane_page: Option<&'a [String]>,
    /// The estimation summed by cell (BRD-062).
    pub estimate: Option<Estimate>,
}

/// The field whose values are summed (BRD-062).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estimate {
    pub field: FieldId,
    /// A float field; integers and durations (minutes) are in the integer table.
    pub float: bool,
}

/// How a card's lane key is found (D-046).
#[derive(Debug, Clone, PartialEq)]
pub struct LaneScope<'a> {
    pub source: LaneSource<'a>,
    /// Cards matching no lane show in the lane `other`; else they are not on the board.
    pub other: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LaneSource<'a> {
    /// References of a field (set, user, group): the key is `prefix` and the first of `ids` the
    /// issue holds, in their order (a multi-value card shows in the topmost lane).
    Refs {
        field: FieldId,
        prefix: &'static str,
        ids: &'a [i64],
    },
    Projects {
        ids: &'a [ProjectId],
    },
    Tags {
        ids: &'a [i64],
    },
    /// A date bucketed by `bounds`; only the ranges of `keys` (`d:…`) are lanes.
    Dates {
        date: DateOf,
        bounds: Bounds,
        zone: &'a str,
        keys: &'a [String],
    },
    /// The parent through the subtask type, optionally holding one of `ids` in `field`.
    Parents {
        field: Option<(FieldId, &'a [i64])>,
    },
}

/// Which date a date lane reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOf {
    Created,
    Updated,
    Date(FieldId),
    DateTime(FieldId),
}

/// One shown card with the size of its cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    pub issue_id: IssueId,
    pub rank: Option<String>,
    pub column_value: ValueId,
    /// The lane key; `None` on a board without lanes.
    pub lane: Option<String>,
    /// Cards in the cell, the hidden ones included.
    pub total: i64,
    /// The estimation of the cell's cards, the hidden ones included (BRD-062).
    pub estimate: Option<f64>,
}

/// Where a cell page starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    /// After this card of the rank order.
    Card {
        rank: Option<String>,
        issue_id: IssueId,
    },
    /// After this many cards of a sorted cell (the keys of a sort are not a cursor here).
    Offset(i64),
}

/// `from … where …` for the scope, the viewer's visibility and the given column values: the sprint
/// rows joined to their issues, or (mode `query`) the issues with their rows if any.
fn push_cards(
    qb: &mut QueryBuilder<Postgres>,
    scope: &Scope<'_>,
    values: &[ValueId],
    vis: &Visibility,
) {
    if scope.query_mode {
        qb.push(
            " from issues i left join sprint_issues si on si.issue_id = i.id and si.sprint_id = ",
        );
        qb.push_bind(scope.sprint);
    } else {
        qb.push(" from sprint_issues si join issues i on i.id = si.issue_id");
    }
    qb.push(" join issue_ref_values cv on cv.issue_id = i.id and cv.field_id = ");
    qb.push_bind(scope.column_field);
    if let Some(lanes) = &scope.lanes {
        push_lane_join(qb, lanes, vis);
    }
    if let Some(e) = scope.estimate {
        qb.push(if e.float {
            " left join issue_float_values est"
        } else {
            " left join issue_int_values est"
        });
        qb.push(" on est.issue_id = i.id and est.field_id = ");
        qb.push_bind(e.field);
    }
    if scope.query_mode {
        qb.push(" where ");
    } else {
        qb.push(" where si.sprint_id = ");
        qb.push_bind(scope.sprint);
        qb.push(" and ");
    }
    push_base(qb, vis);
    qb.push(" and i.project_id = any(");
    qb.push_bind(scope.projects.to_vec());
    qb.push(") and cv.value = any(");
    qb.push_bind(values.to_vec());
    qb.push(")");
    if let Some(filter) = scope.filter {
        qb.push(" and ");
        push_filter(qb, filter, vis);
    }
    if let Some(lanes) = &scope.lanes {
        if matches!(lanes.source, LaneSource::Parents { .. }) {
            // A parent lane's own row is not a card.
            qb.push(" and si.lane_rank is null");
        }
        if !lanes.other {
            qb.push(" and ln.key is not null");
        }
        if let Some(page) = scope.lane_page {
            qb.push(" and coalesce(ln.key, 'other') = any(");
            qb.push_bind(page.to_vec());
            qb.push(")");
        }
    }
}

/// `left join lateral (…) ln`: the card's lane key, null when it matches no lane.
fn push_lane_join(qb: &mut QueryBuilder<Postgres>, lanes: &LaneScope<'_>, vis: &Visibility) {
    qb.push(" left join lateral (");
    match &lanes.source {
        LaneSource::Refs { field, prefix, ids } => {
            qb.push("select ");
            qb.push_bind(*prefix);
            qb.push(
                " || lv.value::text as key from issue_ref_values lv \
                 where lv.issue_id = i.id and lv.field_id = ",
            );
            qb.push_bind(*field);
            qb.push(" and lv.value = any(");
            qb.push_bind(ids.to_vec());
            qb.push(") order by array_position(");
            qb.push_bind(ids.to_vec());
            qb.push(", lv.value) limit 1");
        }
        LaneSource::Projects { ids } => {
            qb.push("select 'p:' || i.project_id::text as key where i.project_id = any(");
            qb.push_bind(ids.to_vec());
            qb.push(")");
        }
        LaneSource::Tags { ids } => {
            qb.push(
                "select 't:' || lt.tag_id::text as key from issue_tags lt \
                 where lt.issue_id = i.id and lt.tag_id = any(",
            );
            qb.push_bind(ids.to_vec());
            qb.push(") order by array_position(");
            qb.push_bind(ids.to_vec());
            qb.push(", lt.tag_id) limit 1");
        }
        LaneSource::Dates {
            date,
            bounds,
            zone,
            keys,
        } => {
            qb.push(
                "select k.key from (select 'd:' || case when x.d is null then 'none' when x.d > ",
            );
            qb.push_bind(bounds.today);
            qb.push(" then 'later' when x.d = ");
            qb.push_bind(bounds.today);
            qb.push(" then 'today' when x.d = ");
            qb.push_bind(bounds.yesterday);
            qb.push(" then 'yesterday' when x.d >= ");
            qb.push_bind(bounds.week);
            qb.push(" then 'this_week' when x.d >= ");
            qb.push_bind(bounds.last_week);
            qb.push(" then 'last_week' when x.d >= ");
            qb.push_bind(bounds.month);
            qb.push(" then 'this_month' else 'older' end as key from (select ");
            match date {
                DateOf::Created | DateOf::Updated => {
                    qb.push(if *date == DateOf::Created {
                        "(i.created_at at time zone "
                    } else {
                        "(i.updated_at at time zone "
                    });
                    qb.push_bind(zone.to_string());
                    qb.push(")::date");
                }
                DateOf::Date(field) => {
                    qb.push(
                        "(select dv.value from issue_date_values dv \
                         where dv.issue_id = i.id and dv.field_id = ",
                    );
                    qb.push_bind(*field);
                    qb.push(")");
                }
                DateOf::DateTime(field) => {
                    qb.push("(select (dv.value at time zone ");
                    qb.push_bind(zone.to_string());
                    qb.push(
                        ")::date from issue_datetime_values dv \
                         where dv.issue_id = i.id and dv.field_id = ",
                    );
                    qb.push_bind(*field);
                    qb.push(")");
                }
            }
            qb.push(" as d) x) k where k.key = any(");
            qb.push_bind(keys.to_vec());
            qb.push(")");
        }
        LaneSource::Parents { field } => {
            qb.push(
                "select 'i:' || l.source_id::text as key from issue_links l \
                 join link_types lt on lt.id = l.type_id and lt.builtin = 'subtask' \
                 join issues p on p.id = l.source_id \
                 where l.target_id = i.id and p.number is not null and p.deleted_at is null",
            );
            vis.push_sql(qb, "p");
            if let Some((field, ids)) = field {
                push_parent_holds(qb, *field, ids);
            }
            qb.push(" limit 1");
        }
    }
    qb.push(") ln on true");
}

/// ` and` parent `p` holds one of the values in the field.
fn push_parent_holds(qb: &mut QueryBuilder<Postgres>, field: FieldId, ids: &[i64]) {
    qb.push(
        " and exists (select 1 from issue_ref_values pv where pv.issue_id = p.id \
         and pv.field_id = ",
    );
    qb.push_bind(field);
    qb.push(" and pv.value = any(");
    qb.push_bind(ids.to_vec());
    qb.push("))");
}

/// The lane column of a select.
/// The sum of the estimation over `window` (empty for a group): null without an estimation.
fn estimate_sum(scope: &Scope<'_>, window: &str) -> String {
    if scope.estimate.is_some() {
        format!("coalesce(sum(est.value::float8){window}, 0)")
    } else {
        "null::float8".to_owned()
    }
}

fn push_lane_col(qb: &mut QueryBuilder<Postgres>, scope: &Scope<'_>) {
    if scope.lanes.is_some() {
        qb.push(", coalesce(ln.key, 'other') as lane");
    } else {
        qb.push(", null::text as lane");
    }
}

/// The order of a cell: the sort's keys, else the rank (unranked last); the issue ID breaks ties.
fn push_cell_order(qb: &mut QueryBuilder<Postgres>, scope: &Scope<'_>, vis: &Visibility) {
    if let Some(sort) = scope.sort {
        push_order_exprs(qb, sort, vis);
        qb.push(", i.id");
    } else {
        qb.push("si.rank nulls last, i.id");
    }
}

fn read(rows: &[sqlx::postgres::PgRow]) -> Result<Vec<CardRow>, sqlx::Error> {
    rows.iter()
        .map(|row| {
            Ok(CardRow {
                issue_id: row.try_get("issue_id")?,
                rank: row.try_get("rank")?,
                column_value: row.try_get("column_value")?,
                lane: row.try_get("lane")?,
                total: row.try_get("total")?,
                estimate: row.try_get("estimate")?,
            })
        })
        .collect()
}

/// The first `cap` cards of every cell in the shared order (BRD-069: rank, unranked last, then
/// issue ID), with each cell's total.
pub async fn cards(
    conn: &mut PgConnection,
    scope: &Scope<'_>,
    vis: &Visibility,
    cap: i64,
) -> Result<Vec<CardRow>, sqlx::Error> {
    if scope.column_values.is_empty() || scope.projects.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = QueryBuilder::<Postgres>::new(
        "select issue_id, rank, column_value, lane, total, estimate from (\
         select i.id as issue_id, si.rank, cv.value as column_value",
    );
    push_lane_col(&mut qb, scope);
    let cell = if scope.lanes.is_some() {
        "cv.value, coalesce(ln.key, 'other')"
    } else {
        "cv.value"
    };
    qb.push(format!(
        ", count(*) over (partition by {cell}) as total, {} as estimate, \
         row_number() over (partition by {cell} order by ",
        estimate_sum(scope, &format!(" over (partition by {cell})"))
    ));
    push_cell_order(&mut qb, scope, vis);
    qb.push(") as n");
    push_cards(&mut qb, scope, scope.column_values, vis);
    qb.push(") c where n <= ");
    qb.push_bind(cap);
    qb.push(" order by column_value, lane, n");
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    read(&rows)
}

/// A page of one cell after a card (a truncated cell's rest).
pub async fn cell(
    conn: &mut PgConnection,
    scope: &Scope<'_>,
    value: ValueId,
    vis: &Visibility,
    after: Option<&After>,
    limit: i64,
) -> Result<Vec<CardRow>, sqlx::Error> {
    if scope.projects.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb =
        QueryBuilder::<Postgres>::new("select i.id as issue_id, si.rank, cv.value as column_value");
    push_lane_col(&mut qb, scope);
    qb.push(", 0::bigint as total, null::float8 as estimate");
    push_cards(&mut qb, scope, &[value], vis);
    if let Some(After::Card { rank, issue_id }) = after {
        // The order is (rank is null, rank, issue ID): unranked cards come last.
        match rank {
            Some(rank) => {
                qb.push(" and (si.rank is null or si.rank > ");
                qb.push_bind(rank.clone());
                qb.push(" or (si.rank = ");
                qb.push_bind(rank.clone());
                qb.push(" and i.id > ");
                qb.push_bind(*issue_id);
                qb.push("))");
            }
            None => {
                qb.push(" and si.rank is null and i.id > ");
                qb.push_bind(*issue_id);
            }
        }
    }
    qb.push(" order by ");
    push_cell_order(&mut qb, scope, vis);
    qb.push(" limit ");
    qb.push_bind(limit);
    if let Some(After::Offset(n)) = after {
        qb.push(" offset ");
        qb.push_bind(*n);
    }
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    read(&rows)
}

/// Where these issues stand in the scope now (live, BRD-114): the shown cards among them; an
/// issue left out is not on the board for this viewer.
pub async fn of_issues(
    conn: &mut PgConnection,
    scope: &Scope<'_>,
    vis: &Visibility,
    issues: &[IssueId],
) -> Result<Vec<CardRow>, sqlx::Error> {
    if scope.column_values.is_empty() || scope.projects.is_empty() || issues.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb =
        QueryBuilder::<Postgres>::new("select i.id as issue_id, si.rank, cv.value as column_value");
    push_lane_col(&mut qb, scope);
    qb.push(", 0::bigint as total, null::float8 as estimate");
    push_cards(&mut qb, scope, scope.column_values, vis);
    qb.push(" and i.id = any(");
    qb.push_bind(issues.to_vec());
    qb.push(")");
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    read(&rows)
}

/// The cards of one cell of a board with lanes, over every lane (BRD-053, BRD-057).
#[derive(Debug, Clone, PartialEq)]
pub struct CellCount {
    pub column_value: ValueId,
    pub lane: String,
    pub count: i64,
    /// The lowest rank in the cell: parent lanes without a lane rank come in this order.
    pub first_rank: Option<String>,
    /// The estimation of the cell's cards (BRD-062).
    pub estimate: Option<f64>,
}

/// The size of every cell of the scope by lane, in one statement (`lane_page` is ignored).
pub async fn cell_counts(
    conn: &mut PgConnection,
    scope: &Scope<'_>,
    vis: &Visibility,
) -> Result<Vec<CellCount>, sqlx::Error> {
    if scope.column_values.is_empty() || scope.projects.is_empty() || scope.lanes.is_none() {
        return Ok(Vec::new());
    }
    let all = Scope {
        lane_page: None,
        ..scope.clone()
    };
    let mut qb = QueryBuilder::<Postgres>::new(
        "select cv.value as column_value, coalesce(ln.key, 'other') as lane, \
         count(*) as count, min(si.rank) as first_rank",
    );
    qb.push(format!(", {} as estimate", estimate_sum(&all, "")));
    push_cards(&mut qb, &all, all.column_values, vis);
    qb.push(" group by 1, 2");
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    rows.iter()
        .map(|row| {
            Ok(CellCount {
                column_value: row.try_get("column_value")?,
                lane: row.try_get("lane")?,
                count: row.try_get("count")?,
                first_rank: row.try_get("first_rank")?,
                estimate: row.try_get("estimate")?,
            })
        })
        .collect()
}

/// The issues added to the sprint as parent lanes (BRD-051, BRD-096) that the viewer sees, by
/// lane rank.
pub async fn lane_rows(
    conn: &mut PgConnection,
    sprint: SprintId,
    projects: &[ProjectId],
    vis: &Visibility,
) -> Result<Vec<(IssueId, String)>, sqlx::Error> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "select i.id as issue_id, si.lane_rank from sprint_issues si \
         join issues i on i.id = si.issue_id where si.sprint_id = ",
    );
    qb.push_bind(sprint);
    qb.push(" and si.lane_rank is not null and ");
    push_base(&mut qb, vis);
    qb.push(" and i.project_id = any(");
    qb.push_bind(projects.to_vec());
    qb.push(") order by si.lane_rank, i.id");
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    rows.iter()
        .map(|row| Ok((row.try_get("issue_id")?, row.try_get("lane_rank")?)))
        .collect()
}

/// Whether an issue is a parent lane's issue (BRD-096): it holds one of the values of the field,
/// or, without a field, has subtasks.
pub async fn is_parent_lane(
    conn: &mut PgConnection,
    issue: IssueId,
    field: Option<(FieldId, &[i64])>,
) -> Result<bool, sqlx::Error> {
    let (field, ids) = match field {
        Some((f, ids)) => (Some(f), ids.to_vec()),
        None => (None, Vec::new()),
    };
    sqlx::query_scalar!(
        r#"select case when $2::bigint is null then exists (
                    select 1 from issue_links l
                    join link_types t on t.id = l.type_id and t.builtin = 'subtask'
                    where l.source_id = $1)
                else exists (
                    select 1 from issue_ref_values
                    where issue_id = $1 and field_id = $2 and value = any($3))
                end as "lane!""#,
        issue,
        field,
        &ids
    )
    .fetch_one(conn)
    .await
}

/// What [`pin`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pinned {
    /// This many cards got a row.
    Done(u64),
    /// More cards than allowed: nothing changed.
    TooMany(i64),
}

/// Gives every card of a query board a row of its sprint (query → manual, BRD-028): the scope's
/// cards as `vis` sees them, at most `max`. Cards with a row keep their rank; new rows come
/// unranked, so the order stays as it was shown.
pub async fn pin(
    conn: &mut PgConnection,
    scope: &Scope<'_>,
    vis: &Visibility,
    max: i64,
) -> Result<Pinned, sqlx::Error> {
    if scope.column_values.is_empty() || scope.projects.is_empty() {
        return Ok(Pinned::Done(0));
    }
    let mut qb = QueryBuilder::<Postgres>::new("select count(*) as n");
    push_cards(&mut qb, scope, scope.column_values, vis);
    qb.push(" and si.issue_id is null");
    let n: i64 = qb
        .build()
        .persistent(false)
        .fetch_one(&mut *conn)
        .await?
        .try_get("n")?;
    if n > max {
        return Ok(Pinned::TooMany(n));
    }
    let mut qb =
        QueryBuilder::<Postgres>::new("insert into sprint_issues (sprint_id, issue_id) select ");
    qb.push_bind(scope.sprint);
    qb.push(", i.id");
    push_cards(&mut qb, scope, scope.column_values, vis);
    qb.push(" and si.issue_id is null on conflict do nothing");
    let done = qb.build().persistent(false).execute(&mut *conn).await?;
    Ok(Pinned::Done(done.rows_affected()))
}
