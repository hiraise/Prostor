//! Comments on issues (CNT-001…CNT-005). Rules: `prostor_domain::comments`; their records in
//! the history: `issue_history`.

use prostor_domain::{CommentId, GroupId, IssueId, UserId};
use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;

use crate::content::Rendered;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub issue_id: IssueId,
    pub author_id: UserId,
    pub text: String,
    pub html: String,
    pub mentioned_users: Vec<UserId>,
    pub issue_tokens: Vec<String>,
    pub file_names: Vec<String>,
    pub render_version: i16,
    pub created_at: OffsetDateTime,
    /// `None` until the first edit.
    pub updated_at: Option<OffsetDateTime>,
    pub updated_by: Option<UserId>,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub deleted_at: Option<OffsetDateTime>,
    pub deleted_by: Option<UserId>,
    pub version: i64,
}

impl Comment {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A comment to create. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewComment<'a> {
    pub issue: IssueId,
    pub author: UserId,
    pub text: &'a str,
    pub rendered: &'a Rendered,
    pub audience_users: &'a [UserId],
    pub audience_groups: &'a [GroupId],
}

pub async fn insert(
    conn: &mut PgConnection,
    comment: &NewComment<'_>,
) -> Result<CommentId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into comments (issue_id, author_id, text, html, mentioned_users, issue_tokens,
                               file_names, render_version, audience_users, audience_groups)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning id",
        comment.issue,
        comment.author,
        comment.text,
        comment.rendered.html,
        &comment.rendered.users,
        &comment.rendered.issue_tokens,
        &comment.rendered.files,
        comment.rendered.version,
        comment.audience_users,
        comment.audience_groups,
    )
    .fetch_one(conn)
    .await
}

/// Whether the person has commented on the issue at all — deleted comments count, the first one
/// stays the first. The auto-watch of a first comment asks (ISS-030).
pub async fn authored(
    db: impl PgExecutor<'_>,
    issue: IssueId,
    author: UserId,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        "select exists(select 1 from comments where issue_id = $1 and author_id = $2)",
        issue,
        author
    )
    .fetch_one(db)
    .await?
    .unwrap_or(false))
}

pub async fn get(db: impl PgExecutor<'_>, id: CommentId) -> Result<Option<Comment>, sqlx::Error> {
    sqlx::query_as!(
        Comment,
        "select id, issue_id, author_id, text, html, mentioned_users, issue_tokens, file_names,
                render_version, created_at, updated_at, updated_by, audience_users,
                audience_groups, deleted_at, deleted_by, version
         from comments where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(
    db: impl PgExecutor<'_>,
    ids: &[CommentId],
) -> Result<Vec<Comment>, sqlx::Error> {
    sqlx::query_as!(
        Comment,
        "select id, issue_id, author_id, text, html, mentioned_users, issue_tokens, file_names,
                render_version, created_at, updated_at, updated_by, audience_users,
                audience_groups, deleted_at, deleted_by, version
         from comments where id = any($1) order by id",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: CommentId) -> Result<Option<Comment>, sqlx::Error> {
    sqlx::query_as!(
        Comment,
        "select id, issue_id, author_id, text, html, mentioned_users, issue_tokens, file_names,
                render_version, created_at, updated_at, updated_by, audience_users,
                audience_groups, deleted_at, deleted_by, version
         from comments where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// The issue a comment belongs to: locks go issue first, then comment.
pub async fn issue_of(
    db: impl PgExecutor<'_>,
    id: CommentId,
) -> Result<Option<IssueId>, sqlx::Error> {
    sqlx::query_scalar!("select issue_id from comments where id = $1", id)
        .fetch_optional(db)
        .await
}

/// Changes a comment's text and audience (CNT-004); its version grows.
pub async fn update(
    conn: &mut PgConnection,
    id: CommentId,
    text: &str,
    rendered: &Rendered,
    audience: (&[UserId], &[GroupId]),
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update comments set text = $2, html = $3, mentioned_users = $4, issue_tokens = $5,
                             file_names = $6, render_version = $7, audience_users = $8,
                             audience_groups = $9, updated_at = now(), updated_by = $10,
                             version = version + 1
         where id = $1",
        id,
        text,
        rendered.html,
        &rendered.users,
        &rendered.issue_tokens,
        &rendered.files,
        rendered.version,
        audience.0,
        audience.1,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes a comment for its readers: a placeholder stays (CNT-005).
pub async fn mark_deleted(
    conn: &mut PgConnection,
    id: CommentId,
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update comments set deleted_at = now(), deleted_by = $2, version = version + 1
         where id = $1",
        id,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn restore(conn: &mut PgConnection, id: CommentId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update comments set deleted_at = null, deleted_by = null, version = version + 1
         where id = $1",
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Comments of an issue by these authors posted in `[from, to]`, oldest first: what changes are
/// grouped with (HIST-011).
pub async fn posted_between(
    db: impl PgExecutor<'_>,
    issue: IssueId,
    authors: &[UserId],
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> Result<Vec<Comment>, sqlx::Error> {
    sqlx::query_as!(
        Comment,
        "select id, issue_id, author_id, text, html, mentioned_users, issue_tokens, file_names,
                render_version, created_at, updated_at, updated_by, audience_users,
                audience_groups, deleted_at, deleted_by, version
         from comments
         where issue_id = $1 and author_id = any($2) and created_at between $3 and $4
         order by id",
        issue,
        authors,
        from,
        to
    )
    .fetch_all(db)
    .await
}

// Deleted accounts and groups (architecture.md §6).

/// Puts the replacement into every comment audience that lists a deleted account.
pub async fn replace_audience_users(
    conn: &mut PgConnection,
    users: &[UserId],
    replacement: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update comments set audience_users = array(
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

/// Puts the replacement into every comment audience that lists a deleted group.
pub async fn replace_audience_groups(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update comments set audience_groups = array(
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
