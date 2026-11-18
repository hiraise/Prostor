//! Cards: the issues of a sprint and their shared order (BRD-025, BRD-069). The order is
//! `(rank, issue_id)`, unranked last; two cards may share a rank after concurrent drops.

use prostor_domain::{BoardId, IssueId, SprintId};
use sqlx::{PgConnection, PgExecutor};

/// Where an issue stands on one board: a sprint and its rank there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub sprint: SprintId,
    pub rank: Option<String>,
}

/// The sprints of the board that hold the issue, in creation order.
pub async fn places(
    db: impl PgExecutor<'_>,
    board: BoardId,
    issue: IssueId,
) -> Result<Vec<Place>, sqlx::Error> {
    sqlx::query_as!(
        Place,
        "select si.sprint_id as sprint, si.rank
         from sprint_issues si join sprints s on s.id = si.sprint_id
         where s.board_id = $1 and si.issue_id = $2 order by s.created_at, s.id",
        board,
        issue
    )
    .fetch_all(db)
    .await
}

/// The ranks of these issues in the sprint; issues not in it are left out.
pub async fn ranks(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    issues: &[IssueId],
) -> Result<Vec<(IssueId, Option<String>)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select issue_id, rank from sprint_issues where sprint_id = $1 and issue_id = any($2)",
        sprint,
        issues
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.issue_id, r.rank)).collect())
}

/// The highest rank of the sprint.
pub async fn last_rank(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select max(rank) as "rank" from sprint_issues where sprint_id = $1"#,
        sprint
    )
    .fetch_one(db)
    .await
}

/// The lowest rank above `rank` in the sprint.
pub async fn rank_above(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select min(rank) as "rank" from sprint_issues where sprint_id = $1 and rank > $2"#,
        sprint,
        rank
    )
    .fetch_one(db)
    .await
}

/// The highest rank below `rank` in the sprint.
pub async fn rank_below(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select max(rank) as "rank" from sprint_issues where sprint_id = $1 and rank < $2"#,
        sprint,
        rank
    )
    .fetch_one(db)
    .await
}

/// The card before this one in the sprint's order.
pub async fn previous(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    issue: IssueId,
) -> Result<Option<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select p.issue_id from sprint_issues c join sprint_issues p on p.sprint_id = c.sprint_id
         where c.sprint_id = $1 and c.issue_id = $2 and p.rank is not null
           and (c.rank is null or (p.rank, p.issue_id) < (c.rank, c.issue_id))
         order by p.rank desc, p.issue_id desc limit 1",
        sprint,
        issue
    )
    .fetch_optional(db)
    .await
}

pub async fn insert(
    conn: &mut PgConnection,
    sprint: SprintId,
    issue: IssueId,
    rank: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into sprint_issues (sprint_id, issue_id, rank) values ($1, $2, $3)",
        sprint,
        issue,
        rank
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Gives the card a new rank and grows the sprint's order version (live). A card of a query
/// board without a row gets one.
pub async fn set_rank(
    conn: &mut PgConnection,
    sprint: SprintId,
    issue: IssueId,
    rank: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into sprint_issues (sprint_id, issue_id, rank) values ($1, $2, $3)
         on conflict (sprint_id, issue_id) do update set rank = excluded.rank",
        sprint,
        issue,
        rank
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update sprints set order_version = order_version + 1 where id = $1",
        sprint
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The highest lane rank of the sprint (BRD-053).
pub async fn last_lane_rank(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select max(lane_rank) as "rank" from sprint_issues where sprint_id = $1"#,
        sprint
    )
    .fetch_one(db)
    .await
}

/// The lane ranks of these issues in the sprint; issues without one are left out.
pub async fn lane_ranks(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    issues: &[IssueId],
) -> Result<Vec<(IssueId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select issue_id, lane_rank as "lane_rank!" from sprint_issues
           where sprint_id = $1 and issue_id = any($2) and lane_rank is not null"#,
        sprint,
        issues
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.issue_id, r.lane_rank))
        .collect())
}

/// The lowest lane rank above `rank` in the sprint.
pub async fn lane_rank_above(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select min(lane_rank) as "rank" from sprint_issues
           where sprint_id = $1 and lane_rank > $2"#,
        sprint,
        rank
    )
    .fetch_one(db)
    .await
}

/// The highest lane rank below `rank` in the sprint.
pub async fn lane_rank_below(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    rank: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select max(lane_rank) as "rank" from sprint_issues
           where sprint_id = $1 and lane_rank < $2"#,
        sprint,
        rank
    )
    .fetch_one(db)
    .await
}

/// Makes the issue a parent lane of the sprint at this lane rank (a card row stops being a card)
/// and grows the sprint's order version.
pub async fn set_lane_rank(
    conn: &mut PgConnection,
    sprint: SprintId,
    issue: IssueId,
    rank: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into sprint_issues (sprint_id, issue_id, lane_rank) values ($1, $2, $3)
         on conflict (sprint_id, issue_id) do update set lane_rank = excluded.lane_rank",
        sprint,
        issue,
        rank
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update sprints set order_version = order_version + 1 where id = $1",
        sprint
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The subtasks of `parent` that are rows of the sprint (a lane's cards, BRD-056).
pub async fn subtasks_in(
    db: impl PgExecutor<'_>,
    sprint: SprintId,
    parent: IssueId,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select si.issue_id from sprint_issues si
         join issue_links l on l.target_id = si.issue_id and l.source_id = $2
         join link_types t on t.id = l.type_id and t.builtin = 'subtask'
         where si.sprint_id = $1 and si.lane_rank is null
         order by si.issue_id",
        sprint,
        parent
    )
    .fetch_all(db)
    .await
}

/// The issues in the sprints of the board (all of them, or one), when a query board's query names
/// the board (`board: X`, `sprint: "X/…"`): what such a board must hear about before the rows go.
pub async fn named_rows(
    db: impl PgExecutor<'_>,
    board: BoardId,
    sprint: Option<SprintId>,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select distinct si.issue_id as "issue_id!"
           from sprint_issues si join sprints s on s.id = si.sprint_id
           where s.board_id = $1 and ($2::bigint is null or s.id = $2)
             and exists (
                 select 1 from boards q
                 where q.mode = 'query' and q.id <> $1
                   and (q.query -> 'bindings' @> jsonb_build_array(jsonb_build_array(
                            jsonb_build_object('t', 'board', 'v', $1)))
                        or q.query -> 'bindings' @> jsonb_build_array(jsonb_build_array(
                            jsonb_build_object('t', 'current_sprint', 'v', $1)))))"#,
        board,
        sprint
    )
    .fetch_all(db)
    .await
}

/// Takes the card off the sprint; answers whether it was there.
pub async fn remove(
    conn: &mut PgConnection,
    sprint: SprintId,
    issue: IssueId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "delete from sprint_issues where sprint_id = $1 and issue_id = $2",
        sprint,
        issue
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Up to `limit` unresolved cards of the sprint `from` in its order, to carry over (BRD-101).
pub async fn unresolved(
    db: impl PgExecutor<'_>,
    from: SprintId,
    limit: i64,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select si.issue_id from sprint_issues si join issues i on i.id = si.issue_id
         where si.sprint_id = $1 and i.resolved_at is null
         order by si.rank nulls last, si.issue_id limit $2",
        from,
        limit
    )
    .fetch_all(db)
    .await
}

/// Moves the cards from one sprint to another: they leave `from` and join `to` after its last
/// card with these ranks, in order; a card already in `to` keeps its place there.
pub async fn carry(
    conn: &mut PgConnection,
    from: SprintId,
    to: SprintId,
    issues: &[IssueId],
    ranks: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from sprint_issues where sprint_id = $1 and issue_id = any($2)",
        from,
        issues
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into sprint_issues (sprint_id, issue_id, rank)
         select $1, t.issue, t.rank from unnest($2::bigint[], $3::text[]) as t(issue, rank)
         on conflict (sprint_id, issue_id) do nothing",
        to,
        issues,
        ranks
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Where a just published issue joins on its own (BRD-026, BRD-027): a board in mode `auto` with
/// its project and a column for its column value, that does not hold it yet; the target is the
/// internal sprint of a sprintless board and the auto-add sprint of a sprint board (none: no
/// target). `skipped`: the draft was kept off it (ISS-007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoTarget {
    pub board_id: BoardId,
    pub board_name: String,
    pub sprint_id: SprintId,
    pub sprint_name: String,
    pub internal: bool,
    pub skipped: bool,
}

/// One statement over the boards of the issue's project (`board_projects_project_idx`).
pub async fn auto_targets(
    db: impl PgExecutor<'_>,
    issue: IssueId,
) -> Result<Vec<AutoTarget>, sqlx::Error> {
    sqlx::query_as!(
        AutoTarget,
        r#"select b.id as board_id, b.name as board_name, s.id as sprint_id,
                  s.name as sprint_name, s.internal, b.id = any(i.board_skips) as "skipped!"
           from issues i
           join board_projects bp on bp.project_id = i.project_id
           join boards b on b.id = bp.board_id and b.mode = 'auto'
           join issue_ref_values cv on cv.issue_id = i.id and cv.field_id = b.column_field_id
           join board_columns c on c.board_id = b.id and c.value_id = cv.value
           join sprints s on s.board_id = b.id and s.archived_at is null
                and (case when b.sprints_on then s.id = b.auto_sprint_id else s.internal end)
           where i.id = $1
             and not exists (select from sprint_issues x join sprints y on y.id = x.sprint_id
                             where y.board_id = b.id and x.issue_id = i.id)
           order by b.id"#,
        issue
    )
    .fetch_all(db)
    .await
}

/// Keeps a draft off a board it would join on publishing (ISS-007), or lets it join again.
pub async fn set_skip(
    conn: &mut PgConnection,
    issue: IssueId,
    board: BoardId,
    skip: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set board_skips = case
             when $3 then array_append(array_remove(board_skips, $2), $2)
             else array_remove(board_skips, $2) end
         where id = $1",
        issue,
        board,
        skip
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A sprint of a board holding the issue (BRD-033, ISS-020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub board_id: BoardId,
    pub sprint_id: SprintId,
    pub sprint_name: String,
    pub internal: bool,
}

/// The sprints holding the issue on boards not in query mode (there a row is only a rank), in
/// the order the issue joined them (`sprint_issues_issue_idx`).
pub async fn of_issue(
    db: impl PgExecutor<'_>,
    issue: IssueId,
) -> Result<Vec<Membership>, sqlx::Error> {
    sqlx::query_as!(
        Membership,
        "select s.board_id, s.id as sprint_id, s.name as sprint_name, s.internal
         from sprint_issues si
         join sprints s on s.id = si.sprint_id
         join boards b on b.id = s.board_id and b.mode <> 'query'
         where si.issue_id = $1
         order by si.added_at, s.id",
        issue
    )
    .fetch_all(db)
    .await
}

/// Forgets the boards a draft was kept off (ISS-007): they count only at publishing.
pub async fn clear_skips(conn: &mut PgConnection, issue: IssueId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set board_skips = '{}' where id = $1 and cardinality(board_skips) > 0",
        issue
    )
    .execute(conn)
    .await?;
    Ok(())
}
