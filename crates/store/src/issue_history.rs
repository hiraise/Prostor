//! Change history of issues (HIST): append-only records written with each change. Shapes of
//! `removed` and `added`: `taskmaster_domain::issues::history`.

use serde_json::Value as Json;
use sqlx::{PgConnection, PgExecutor};
use taskmaster_domain::issues::history::{Origin, Record};
use taskmaster_domain::{CommentId, FieldId, IssueId, UserId};
use time::OffsetDateTime;

/// Who made a set of changes and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Author {
    pub user: UserId,
    pub origin: Origin,
    pub silent: bool,
}

/// A stored record.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: i64,
    pub issue_id: IssueId,
    pub at: OffsetDateTime,
    pub author_id: UserId,
    pub origin: String,
    pub silent: bool,
    pub kind: String,
    pub field_id: Option<FieldId>,
    pub comment_id: Option<CommentId>,
    pub removed: Option<Json>,
    pub added: Option<Json>,
}

/// Writes records of one issue, in order, with one statement.
pub async fn record(
    conn: &mut PgConnection,
    issue: IssueId,
    author: Author,
    records: &[Record],
) -> Result<(), sqlx::Error> {
    if records.is_empty() {
        return Ok(());
    }
    let issues: Vec<IssueId> = vec![issue; records.len()];
    record_many(conn, &issues, author, records).await
}

/// Writes records of several issues: `issues[i]` gets `records[i]`.
pub async fn record_many(
    conn: &mut PgConnection,
    issues: &[IssueId],
    author: Author,
    records: &[Record],
) -> Result<(), sqlx::Error> {
    if records.is_empty() {
        return Ok(());
    }
    let kinds: Vec<String> = records.iter().map(|r| r.kind.key().to_owned()).collect();
    let fields: Vec<Option<FieldId>> = records.iter().map(|r| r.field).collect();
    let comments: Vec<Option<CommentId>> = records.iter().map(|r| r.comment).collect();
    let removed: Vec<Option<Json>> = records.iter().map(|r| r.removed.clone()).collect();
    let added: Vec<Option<Json>> = records.iter().map(|r| r.added.clone()).collect();
    sqlx::query!(
        "insert into issue_history (issue_id, author_id, origin, silent, kind, field_id,
                                    comment_id, removed, added)
         select r.issue, $2, $3, $4, r.kind, r.field, r.comment, r.removed, r.added
         from unnest($1::bigint[], $5::text[], $6::bigint[], $7::bigint[], $8::jsonb[],
                     $9::jsonb[])
              with ordinality as r(issue, kind, field, comment, removed, added, n)
         order by r.n",
        issues,
        author.user,
        author.origin.key(),
        author.silent,
        &kinds,
        &fields as &[Option<FieldId>],
        &comments as &[Option<CommentId>],
        &removed as &[Option<Json>],
        &added as &[Option<Json>],
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Which way a page of history goes from its cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Older records, newest of them first.
    Older,
    /// Newer records, oldest of them first.
    Newer,
}

/// A page of an issue's records of these kinds after (or before) the record `from`, excluded
/// (HIST-010).
pub async fn page(
    db: impl PgExecutor<'_>,
    issue: IssueId,
    kinds: &[String],
    from: Option<i64>,
    direction: Direction,
    limit: i64,
) -> Result<Vec<Entry>, sqlx::Error> {
    match direction {
        Direction::Older => {
            sqlx::query_as!(
                Entry,
                "select id, issue_id, at, author_id, origin, silent, kind, field_id, comment_id,
                        removed, added
                 from issue_history where issue_id = $1 and kind = any($2) and id < $3
                 order by id desc limit $4",
                issue,
                kinds,
                from.unwrap_or(i64::MAX),
                limit
            )
            .fetch_all(db)
            .await
        }
        Direction::Newer => {
            sqlx::query_as!(
                Entry,
                "select id, issue_id, at, author_id, origin, silent, kind, field_id, comment_id,
                        removed, added
                 from issue_history where issue_id = $1 and kind = any($2) and id > $3
                 order by id limit $4",
                issue,
                kinds,
                from.unwrap_or(0),
                limit
            )
            .fetch_all(db)
            .await
        }
    }
}

/// The record of a comment's posting (CNT-007): where the stream around it starts.
pub async fn posting_of(
    db: impl PgExecutor<'_>,
    comment: CommentId,
) -> Result<Option<(IssueId, i64)>, sqlx::Error> {
    let row = sqlx::query!(
        "select issue_id, id from issue_history where comment_id = $1 and kind = 'comment'",
        comment
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.issue_id, r.id)))
}

/// Records of an issue, oldest first: for tests and small issues.
pub async fn all_of(db: impl PgExecutor<'_>, issue: IssueId) -> Result<Vec<Entry>, sqlx::Error> {
    sqlx::query_as!(
        Entry,
        "select id, issue_id, at, author_id, origin, silent, kind, field_id, comment_id, removed,
                added
         from issue_history where issue_id = $1 order by id",
        issue
    )
    .fetch_all(db)
    .await
}
