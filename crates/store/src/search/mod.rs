//! Issue search (QRY-*, plan step 1.5 §4, D-024): a resolved filter and sort compiled to SQL, a
//! page in keyset order, counts, and the loaders of the names a query mentions ([`catalog`]).
//!
//! Statements are built per query (`QueryBuilder`) and unnamed (`persistent(false)`): each is
//! planned with its values, which matter for skewed columns.

pub mod catalog;
pub mod compile;
pub mod order;
mod positions;

use sqlx::{PgConnection, Postgres, QueryBuilder, Row};
use trackflow_domain::IssueId;
use trackflow_domain::search::{Filter, SortSpec};

pub use order::SortValue;

use crate::saved_searches::SavedSearchId;
use crate::visibility::Visibility;

/// One issue of a page with the values of its sort keys (the last one's make the cursor).
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: IssueId,
    pub keys: Vec<SortValue>,
}

/// Up to `limit` issues matching the filter, in the order of `sort` (which ends with the issue
/// ID), after the issue whose sort values are `after`; `unranked_of` leaves out the issues the
/// manual order of that saved search ranks (the second phase of [`page_ranked`]).
pub async fn page(
    conn: &mut PgConnection,
    filter: &Filter,
    sort: &[SortSpec],
    vis: &Visibility,
    after: Option<&[SortValue]>,
    unranked_of: Option<SavedSearchId>,
    limit: i64,
) -> Result<Vec<Hit>, sqlx::Error> {
    // The manual order's second phase adds its own condition; it keeps the scan below.
    if unranked_of.is_none()
        && let Some(hits) = positions::page(conn, filter, sort, vis, after, limit).await?
    {
        return Ok(hits);
    }
    let mut qb = QueryBuilder::<Postgres>::new("select s.id");
    for j in 0..sort.len() {
        qb.push(format!(", s.k{j}"));
    }
    qb.push(" from (select i.id");
    for (j, spec) in sort.iter().enumerate() {
        qb.push(", ");
        order::push_expr(&mut qb, &spec.by, vis);
        qb.push(format!(" as k{j}"));
    }
    qb.push(" from issues i where ");
    compile::push_base(&mut qb, vis);
    qb.push(" and ");
    compile::push_filter(&mut qb, filter, vis);
    if let Some(search) = unranked_of {
        qb.push(
            " and not exists (select from saved_search_ranks r where r.issue_id = i.id \
             and r.search_id = ",
        );
        qb.push_bind(search);
        qb.push(")");
    }
    qb.push(") s");
    if let Some(after) = after {
        qb.push(" where ");
        order::push_after(&mut qb, sort, after);
    }
    order::push_order(&mut qb, sort);
    qb.push(" limit ");
    qb.push_bind(limit);
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    rows.iter()
        .map(|row| {
            let keys = sort
                .iter()
                .enumerate()
                .map(|(j, spec)| order::read(row, j, &spec.by))
                .collect::<Result<_, _>>()?;
            Ok(Hit {
                id: row.try_get("id")?,
                keys,
            })
        })
        .collect()
}

/// Up to `limit` issues matching the filter that the manual order of `search` ranks (BRD-093),
/// by `(rank, id)` after `after`, with their ranks (`saved_search_ranks_rank_idx`).
pub async fn page_ranked(
    conn: &mut PgConnection,
    filter: &Filter,
    vis: &Visibility,
    search: SavedSearchId,
    after: Option<(&str, IssueId)>,
    limit: i64,
) -> Result<Vec<(IssueId, String)>, sqlx::Error> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "select i.id, r.rank from saved_search_ranks r join issues i on i.id = r.issue_id \
         where r.search_id = ",
    );
    qb.push_bind(search);
    if let Some((rank, id)) = after {
        qb.push(" and (r.rank, r.issue_id) > (");
        qb.push_bind(rank.to_owned());
        qb.push("::text collate \"C\", ");
        qb.push_bind(id);
        qb.push(")");
    }
    qb.push(" and ");
    compile::push_base(&mut qb, vis);
    qb.push(" and ");
    compile::push_filter(&mut qb, filter, vis);
    qb.push(" order by r.rank, r.issue_id limit ");
    qb.push_bind(limit);
    let rows = qb.build().persistent(false).fetch_all(&mut *conn).await?;
    rows.iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("rank")?)))
        .collect()
}

/// How many issues match: all and unresolved.
pub async fn count(
    conn: &mut PgConnection,
    filter: &Filter,
    vis: &Visibility,
) -> Result<(i64, i64), sqlx::Error> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "select count(*) as total, count(*) filter (where i.resolved_at is null) as open \
         from issues i where ",
    );
    compile::push_base(&mut qb, vis);
    qb.push(" and ");
    compile::push_filter(&mut qb, filter, vis);
    let row = qb.build().persistent(false).fetch_one(&mut *conn).await?;
    Ok((row.try_get("total")?, row.try_get("open")?))
}

/// Whether each condition holds for one issue, in one statement (the plan of a query board,
/// BRD-028).
pub async fn holds_each(
    conn: &mut PgConnection,
    issue: trackflow_domain::IssueId,
    conditions: &[&Filter],
    vis: &Visibility,
) -> Result<Vec<bool>, sqlx::Error> {
    if conditions.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = QueryBuilder::<Postgres>::new("select ");
    for (j, c) in conditions.iter().enumerate() {
        if j > 0 {
            qb.push(", ");
        }
        qb.push("coalesce(");
        compile::push_filter(&mut qb, c, vis);
        qb.push(format!(", false) as h{j}"));
    }
    qb.push(" from issues i where i.id = ");
    qb.push_bind(issue);
    let Some(row) = qb
        .build()
        .persistent(false)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(vec![false; conditions.len()]);
    };
    (0..conditions.len())
        .map(|j| row.try_get::<bool, _>(format!("h{j}").as_str()))
        .collect()
}
