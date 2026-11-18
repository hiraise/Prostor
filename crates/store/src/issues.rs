//! Issues and drafts (ISS), their former IDs, watchers and votes. Field values:
//! `issue_values`; history: `issue_history`; tags: `tags`. Rules: `prostor_domain::issues`.

use prostor_domain::{GroupId, IssueId, ProjectId, UserId};
use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;

use crate::content::Rendered;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub id: IssueId,
    pub project_id: ProjectId,
    /// `None` for a draft.
    pub number: Option<i64>,
    pub summary: String,
    pub description: String,
    pub reporter_id: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub updated_by: Option<UserId>,
    pub resolved_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
    pub deleted_by: Option<UserId>,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub votes: i32,
    pub version: i64,
    pub text_version: i64,
    /// The description rendered (D-031) and what it refers to.
    pub description_html: String,
    pub mentioned_users: Vec<UserId>,
    pub issue_tokens: Vec<String>,
    pub file_names: Vec<String>,
}

impl Issue {
    pub fn is_draft(&self) -> bool {
        self.number.is_none()
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A draft to create. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewDraft<'a> {
    pub project: ProjectId,
    pub reporter: UserId,
    pub summary: &'a str,
    pub description: &'a str,
    pub rendered: &'a Rendered,
    pub audience_users: &'a [UserId],
    pub audience_groups: &'a [GroupId],
}

pub async fn insert_draft(
    conn: &mut PgConnection,
    draft: &NewDraft<'_>,
) -> Result<IssueId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into issues (project_id, reporter_id, summary, description, audience_users,
                             audience_groups, updated_by, description_html, mentioned_users,
                             issue_tokens, file_names, render_version)
         values ($1, $2, $3, $4, $5, $6, $2, $7, $8, $9, $10, $11) returning id",
        draft.project,
        draft.reporter,
        draft.summary,
        draft.description,
        draft.audience_users,
        draft.audience_groups,
        draft.rendered.html,
        &draft.rendered.users,
        &draft.rendered.issue_tokens,
        &draft.rendered.files,
        draft.rendered.version,
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: IssueId) -> Result<Option<Issue>, sqlx::Error> {
    sqlx::query_as!(
        Issue,
        "select id, project_id, number, summary, description, reporter_id, created_at, updated_at,
                updated_by, resolved_at, deleted_at, deleted_by, audience_users, audience_groups,
                votes, version, text_version, description_html, mentioned_users, issue_tokens,
                file_names
         from issues where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(db: impl PgExecutor<'_>, ids: &[IssueId]) -> Result<Vec<Issue>, sqlx::Error> {
    sqlx::query_as!(
        Issue,
        "select id, project_id, number, summary, description, reporter_id, created_at, updated_at,
                updated_by, resolved_at, deleted_at, deleted_by, audience_users, audience_groups,
                votes, version, text_version, description_html, mentioned_users, issue_tokens,
                file_names
         from issues where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: IssueId) -> Result<Option<Issue>, sqlx::Error> {
    sqlx::query_as!(
        Issue,
        "select id, project_id, number, summary, description, reporter_id, created_at, updated_at,
                updated_by, resolved_at, deleted_at, deleted_by, audience_users, audience_groups,
                votes, version, text_version, description_html, mentioned_users, issue_tokens,
                file_names
         from issues where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// Locks issues in ID order, so concurrent changes of several never deadlock.
pub async fn lock_many(
    conn: &mut PgConnection,
    ids: &[IssueId],
) -> Result<Vec<Issue>, sqlx::Error> {
    sqlx::query_as!(
        Issue,
        "select id, project_id, number, summary, description, reporter_id, created_at, updated_at,
                updated_by, resolved_at, deleted_at, deleted_by, audience_users, audience_groups,
                votes, version, text_version, description_html, mentioned_users, issue_tokens,
                file_names
         from issues where id = any($1) order by id for update",
        ids
    )
    .fetch_all(conn)
    .await
}

/// The issue that has or had a number in a project (ISS-003): the current holder, else the issue
/// that moved away from it.
pub async fn by_number(
    db: &mut PgConnection,
    project: ProjectId,
    number: i64,
) -> Result<Option<IssueId>, sqlx::Error> {
    let current = sqlx::query_scalar!(
        "select id from issues where project_id = $1 and number = $2",
        project,
        number
    )
    .fetch_optional(&mut *db)
    .await?;
    if current.is_some() {
        return Ok(current);
    }
    sqlx::query_scalar!(
        "select issue_id from issue_aliases where project_id = $1 and number = $2",
        project,
        number
    )
    .fetch_optional(db)
    .await
}

/// The issues that readable ID tokens such as `ABC-12` name (CNT-056), through current and former
/// keys and numbers, in one query: `(token, issue)` for those that resolve.
pub async fn resolve_tokens(
    db: impl PgExecutor<'_>,
    tokens: &[String],
) -> Result<Vec<(String, IssueId)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"with wanted as (
             select t.token, k.project_id, split_part(t.token, '-', 2)::bigint as number
             from unnest($1::text[]) t(token)
             join project_keys k on k.key = split_part(t.token, '-', 1)
             where split_part(t.token, '-', 2) ~ '^[1-9][0-9]{0,17}$'
           )
           select w.token as "token!",
                  coalesce(i.id, a.issue_id) as "issue!"
           from wanted w
           left join issues i on i.project_id = w.project_id and i.number = w.number
           left join issue_aliases a on a.project_id = w.project_id and a.number = w.number
           where i.id is not null or a.issue_id is not null"#,
        tokens
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.token, r.issue)).collect())
}

/// Publishes a draft (ISS-002): the project's next number, taken under the project's row lock so
/// two publications never get the same one, and the publication time.
pub async fn publish(conn: &mut PgConnection, id: IssueId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"with taken as (
             update projects set next_number = next_number + 1
             where id = (select project_id from issues where id = $1)
             returning next_number - 1 as number
           )
           update issues set number = taken.number, created_at = now(), updated_at = now(),
                             version = version + 1
           from taken where issues.id = $1
           returning issues.number as "number!""#,
        id
    )
    .fetch_one(conn)
    .await
}

/// Records that someone changed the issue: update time, updater, version.
pub async fn touch(conn: &mut PgConnection, id: IssueId, by: UserId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set updated_at = now(), updated_by = $2, version = version + 1
         where id = $1",
        id,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Grows the version only, for changes of counters in the issue's shape (votes, stars): the
/// update time and updater stay.
pub async fn bump(conn: &mut PgConnection, id: IssueId) -> Result<(), sqlx::Error> {
    sqlx::query!("update issues set version = version + 1 where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Records that someone changed several issues at once, such as both ends of a link.
pub async fn touch_many(
    conn: &mut PgConnection,
    ids: &[IssueId],
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set updated_at = now(), updated_by = $2, version = version + 1
         where id = any($1)",
        ids,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets the summary and description with its rendering; their version grows.
pub async fn set_texts(
    conn: &mut PgConnection,
    id: IssueId,
    summary: &str,
    description: &str,
    rendered: &Rendered,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set summary = $2, description = $3, text_version = text_version + 1,
                           description_html = $4, mentioned_users = $5, issue_tokens = $6,
                           file_names = $7, render_version = $8
         where id = $1",
        id,
        summary,
        description,
        rendered.html,
        &rendered.users,
        &rendered.issue_tokens,
        &rendered.files,
        rendered.version
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_audience(
    conn: &mut PgConnection,
    id: IssueId,
    users: &[UserId],
    groups: &[GroupId],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set audience_users = $2, audience_groups = $3 where id = $1",
        id,
        users,
        groups
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_resolved_at(
    conn: &mut PgConnection,
    id: IssueId,
    at: Option<OffsetDateTime>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("update issues set resolved_at = $2 where id = $1", id, at)
        .execute(conn)
        .await?;
    Ok(())
}

/// [`set_resolved_at`] for the parents of one walk (D-049): they cross in the same direction,
/// so one `at` fits every one of them.
pub async fn set_resolved_at_many(
    conn: &mut PgConnection,
    ids: &[IssueId],
    at: Option<OffsetDateTime>,
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "update issues set resolved_at = $2 where id = any($1)",
        ids,
        at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Moves a draft to another project; it has no number to keep.
pub async fn set_draft_project(
    conn: &mut PgConnection,
    id: IssueId,
    project: ProjectId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set project_id = $2 where id = $1 and number is null",
        id,
        project
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Moves an issue to another project (ISS-025): the target's next number; the old place becomes
/// an alias (ISS-003). Returns the new number.
pub async fn move_to(
    conn: &mut PgConnection,
    id: IssueId,
    from: ProjectId,
    old_number: i64,
    to: ProjectId,
) -> Result<i64, sqlx::Error> {
    sqlx::query!(
        "insert into issue_aliases (project_id, number, issue_id) values ($1, $2, $3)
         on conflict (project_id, number) do update set issue_id = excluded.issue_id",
        from,
        old_number,
        id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query_scalar!(
        r#"with taken as (
             update projects set next_number = next_number + 1 where id = $2
             returning next_number - 1 as number
           )
           update issues set project_id = $2, number = taken.number
           from taken where issues.id = $1
           returning issues.number as "number!""#,
        id,
        to
    )
    .fetch_one(conn)
    .await
}

/// Deletes an issue for its users (ISS-028): it stays whole for the trash.
pub async fn mark_deleted(
    conn: &mut PgConnection,
    id: IssueId,
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set deleted_at = now(), deleted_by = $2, version = version + 1
         where id = $1",
        id,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes a draft for good.
pub async fn delete_draft(conn: &mut PgConnection, id: IssueId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from issues where id = $1 and number is null", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// A person's drafts, newest first.
pub async fn drafts_of(
    db: impl PgExecutor<'_>,
    reporter: UserId,
    limit: i64,
) -> Result<Vec<Issue>, sqlx::Error> {
    sqlx::query_as!(
        Issue,
        "select id, project_id, number, summary, description, reporter_id, created_at, updated_at,
                updated_by, resolved_at, deleted_at, deleted_by, audience_users, audience_groups,
                votes, version, text_version, description_html, mentioned_users, issue_tokens,
                file_names
         from issues where reporter_id = $1 and number is null order by id desc limit $2",
        reporter,
        limit
    )
    .fetch_all(db)
    .await
}

/// Published issues of a project, deleted ones included: what deleting a project removes.
pub async fn count_of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "count!" from issues where project_id = $1"#,
        project
    )
    .fetch_one(db)
    .await
}

/// Issue IDs of a project in pages of `limit` after `after`: for jobs that walk a project.
pub async fn ids_of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
    after: IssueId,
    limit: i64,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select id from issues where project_id = $1 and id > $2 order by id limit $3",
        project,
        after,
        limit
    )
    .fetch_all(db)
    .await
}

/// The projects issues are in.
pub async fn projects_of(
    db: impl PgExecutor<'_>,
    ids: &[IssueId],
) -> Result<Vec<(IssueId, ProjectId)>, sqlx::Error> {
    let rows = sqlx::query!("select id, project_id from issues where id = any($1)", ids)
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.project_id)).collect())
}

// Watchers (ISS-030) and votes (ISS-032).

/// Adds watchers; answers how many were new.
pub async fn watch(
    conn: &mut PgConnection,
    issue: IssueId,
    users: &[UserId],
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "insert into issue_watchers (issue_id, user_id) select $1, unnest($2::bigint[])
         on conflict do nothing",
        issue,
        users
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Removes a watcher; `false` when the person did not watch.
pub async fn unwatch(
    conn: &mut PgConnection,
    issue: IssueId,
    user: UserId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "delete from issue_watchers where issue_id = $1 and user_id = $2",
        issue,
        user
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Which of the issues the person watches.
pub async fn watched(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
    user: UserId,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select issue_id from issue_watchers where issue_id = any($1) and user_id = $2",
        issues,
        user
    )
    .fetch_all(db)
    .await
}

/// How many people watch each issue.
pub async fn watcher_counts(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
) -> Result<Vec<(IssueId, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select issue_id, count(*) as "count!" from issue_watchers where issue_id = any($1)
           group by issue_id"#,
        issues
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.issue_id, r.count)).collect())
}

/// Adds a vote; `false` when the person had voted already.
pub async fn vote(
    conn: &mut PgConnection,
    issue: IssueId,
    user: UserId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into issue_votes (issue_id, user_id) values ($1, $2) on conflict do nothing",
        issue,
        user
    )
    .execute(&mut *conn)
    .await?;
    if done.rows_affected() == 0 {
        return Ok(false);
    }
    sqlx::query!("update issues set votes = votes + 1 where id = $1", issue)
        .execute(conn)
        .await?;
    Ok(true)
}

/// Withdraws a vote; `false` when there was none.
pub async fn unvote(
    conn: &mut PgConnection,
    issue: IssueId,
    user: UserId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "delete from issue_votes where issue_id = $1 and user_id = $2",
        issue,
        user
    )
    .execute(&mut *conn)
    .await?;
    if done.rows_affected() == 0 {
        return Ok(false);
    }
    sqlx::query!("update issues set votes = votes - 1 where id = $1", issue)
        .execute(conn)
        .await?;
    Ok(true)
}

/// Which of the issues the person voted for.
pub async fn voted(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
    user: UserId,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select issue_id from issue_votes where issue_id = any($1) and user_id = $2",
        issues,
        user
    )
    .fetch_all(db)
    .await
}

// Deleted accounts and groups (architecture.md §6, ACC-006, ACC-032).

/// Puts the replacement into every audience that lists a deleted account.
pub async fn replace_audience_users(
    conn: &mut PgConnection,
    users: &[UserId],
    replacement: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set audience_users = array(
           select distinct u from unnest(
             array(select x from unnest(audience_users) x where x <> all($1)) || $2::bigint) u)
         where audience_users && $1",
        users,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Puts the replacement into every audience that lists a deleted group.
pub async fn replace_audience_groups(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set audience_groups = array(
           select distinct g from unnest(
             array(select x from unnest(audience_groups) x where x <> all($1)) || $2::bigint) g)
         where audience_groups && $1",
        groups,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Drops deleted accounts' stars and votes; vote counts follow.
pub async fn forget_users(conn: &mut PgConnection, users: &[UserId]) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from issue_watchers where user_id = any($1)", users)
        .execute(&mut *conn)
        .await?;
    sqlx::query!(
        "with gone as (delete from issue_votes where user_id = any($1) returning issue_id)
         update issues i set votes = i.votes - g.n
         from (select issue_id, count(*)::int as n from gone group by issue_id) g
         where i.id = g.issue_id",
        users
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Drafts of deleted accounts go with them.
pub async fn delete_drafts_of(
    conn: &mut PgConnection,
    users: &[UserId],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from issues where reporter_id = any($1) and number is null",
        users
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Drafts of these reporters: what goes with deleted accounts.
pub async fn draft_ids_of(
    db: impl PgExecutor<'_>,
    users: &[UserId],
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select id from issues where reporter_id = any($1) and number is null",
        users
    )
    .fetch_all(db)
    .await
}

/// Deletes issues by ID with everything that belongs to them.
pub async fn delete_ids(conn: &mut PgConnection, ids: &[IssueId]) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!("delete from issues where id = any($1)", ids)
        .execute(conn)
        .await?;
    Ok(done.rows_affected())
}

/// Deletes up to `limit` empty drafts untouched for `idle` (ISS-010): no summary, description,
/// files, links or tags. Answers how many went.
pub async fn sweep_empty_drafts(
    conn: &mut PgConnection,
    idle: std::time::Duration,
    limit: i64,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "delete from issues where id in (
           select i.id from issues i
           where i.number is null and i.updated_at < now() - make_interval(secs => $1)
             and i.summary = '' and i.description = ''
             and not exists (select 1 from attachments a where a.issue_id = i.id)
             and not exists (select 1 from issue_links l where l.source_id = i.id)
             and not exists (select 1 from issue_links l where l.target_id = i.id)
             and not exists (select 1 from issue_tags t where t.issue_id = i.id)
           order by i.updated_at
           limit $2
           for update skip locked
         )",
        idle.as_secs_f64(),
        limit
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}
