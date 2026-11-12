//! Files of issues and comments (CNT-017…CNT-024). Rules: `trackflow_domain::attachments`;
//! their records in the history: `issue_history`. The files are in the file store under
//! `storage_key` and `thumbnail_key`.

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::{AttachmentId, CommentId, GroupId, IssueId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub id: AttachmentId,
    pub issue_id: IssueId,
    pub comment_id: Option<CommentId>,
    pub author_id: UserId,
    pub name: String,
    pub size: i64,
    pub content_type: String,
    pub sha256: Vec<u8>,
    pub storage_key: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub thumbnail_key: Option<String>,
    pub secret: Vec<u8>,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub created_at: OffsetDateTime,
    pub updated_at: Option<OffsetDateTime>,
    pub updated_by: Option<UserId>,
    pub deleted_at: Option<OffsetDateTime>,
    pub deleted_by: Option<UserId>,
    pub version: i64,
}

impl Attachment {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A file to attach. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewAttachment<'a> {
    pub issue: IssueId,
    pub comment: Option<CommentId>,
    pub author: UserId,
    pub name: &'a str,
    pub size: i64,
    pub content_type: &'a str,
    pub sha256: &'a [u8],
    pub storage_key: &'a str,
    pub secret: &'a [u8],
}

pub async fn insert(
    conn: &mut PgConnection,
    new: &NewAttachment<'_>,
) -> Result<AttachmentId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into attachments (issue_id, comment_id, author_id, name, size, content_type,
                                  sha256, storage_key, secret)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning id",
        new.issue,
        new.comment,
        new.author,
        new.name,
        new.size,
        new.content_type,
        new.sha256,
        new.storage_key,
        new.secret
    )
    .fetch_one(conn)
    .await
}

/// Every file of an issue that is not deleted, its comments' included, oldest first. At most
/// `ISSUE_FILES_MAX` rows, so the whole list is one bounded read.
pub async fn of_issue(
    db: impl PgExecutor<'_>,
    issue: IssueId,
) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as!(
        Attachment,
        "select id, issue_id, comment_id, author_id, name, size, content_type, sha256,
                storage_key, width, height, thumbnail_key, secret, audience_users,
                audience_groups, created_at, updated_at, updated_by, deleted_at, deleted_by,
                version
         from attachments where issue_id = $1 and deleted_at is null order by id",
        issue
    )
    .fetch_all(db)
    .await
}

/// The files of these comments that are not deleted, oldest first.
pub async fn of_comments(
    db: impl PgExecutor<'_>,
    comments: &[CommentId],
) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as!(
        Attachment,
        "select id, issue_id, comment_id, author_id, name, size, content_type, sha256,
                storage_key, width, height, thumbnail_key, secret, audience_users,
                audience_groups, created_at, updated_at, updated_by, deleted_at, deleted_by,
                version
         from attachments where comment_id = any($1) and deleted_at is null order by id",
        comments
    )
    .fetch_all(db)
    .await
}

/// Files of an issue with these names that are not deleted: what texts embed (CNT-044).
pub async fn of_issue_named(
    db: impl PgExecutor<'_>,
    issue: IssueId,
    names: &[String],
) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as!(
        Attachment,
        "select id, issue_id, comment_id, author_id, name, size, content_type, sha256,
                storage_key, width, height, thumbnail_key, secret, audience_users,
                audience_groups, created_at, updated_at, updated_by, deleted_at, deleted_by,
                version
         from attachments
         where issue_id = $1 and name = any($2) and deleted_at is null order by id",
        issue,
        names
    )
    .fetch_all(db)
    .await
}

/// One file, deleted or not.
pub async fn get(
    db: impl PgExecutor<'_>,
    id: AttachmentId,
) -> Result<Option<Attachment>, sqlx::Error> {
    sqlx::query_as!(
        Attachment,
        "select id, issue_id, comment_id, author_id, name, size, content_type, sha256,
                storage_key, width, height, thumbnail_key, secret, audience_users,
                audience_groups, created_at, updated_at, updated_by, deleted_at, deleted_by,
                version
         from attachments where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

/// Who sees a file attached to an issue itself (not to a comment): what cards count (ISS-024).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAudience {
    pub issue_id: IssueId,
    pub author_id: UserId,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
}

/// The files of these issues that are not deleted and not posted with a comment.
pub async fn audiences_of_issues(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
) -> Result<Vec<FileAudience>, sqlx::Error> {
    sqlx::query_as!(
        FileAudience,
        "select issue_id, author_id, audience_users, audience_groups from attachments
         where issue_id = any($1) and comment_id is null and deleted_at is null",
        issues
    )
    .fetch_all(db)
    .await
}

/// Files by ID, deleted or not, in any order.
pub async fn get_many(
    db: impl PgExecutor<'_>,
    ids: &[AttachmentId],
) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as!(
        Attachment,
        "select id, issue_id, comment_id, author_id, name, size, content_type, sha256,
                storage_key, width, height, thumbnail_key, secret, audience_users,
                audience_groups, created_at, updated_at, updated_by, deleted_at, deleted_by,
                version
         from attachments where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(
    conn: &mut PgConnection,
    id: AttachmentId,
) -> Result<Option<Attachment>, sqlx::Error> {
    sqlx::query_as!(
        Attachment,
        "select id, issue_id, comment_id, author_id, name, size, content_type, sha256,
                storage_key, width, height, thumbnail_key, secret, audience_users,
                audience_groups, created_at, updated_at, updated_by, deleted_at, deleted_by,
                version
         from attachments where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// The issue a file belongs to: locks go issue first, then file.
pub async fn issue_of(
    db: impl PgExecutor<'_>,
    id: AttachmentId,
) -> Result<Option<IssueId>, sqlx::Error> {
    sqlx::query_scalar!("select issue_id from attachments where id = $1", id)
        .fetch_optional(db)
        .await
}

/// Names of the files of an issue that are not deleted: new names must differ from them.
pub async fn names_of_issue(
    db: impl PgExecutor<'_>,
    issue: IssueId,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "select name from attachments where issue_id = $1 and deleted_at is null",
        issue
    )
    .fetch_all(db)
    .await
}

/// Renames a file or changes its audience (CNT-020, CNT-023); its version grows.
pub async fn update(
    conn: &mut PgConnection,
    id: AttachmentId,
    name: &str,
    audience: (&[UserId], &[GroupId]),
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update attachments set name = $2, audience_users = $3, audience_groups = $4,
                                updated_at = now(), updated_by = $5, version = version + 1
         where id = $1",
        id,
        name,
        audience.0,
        audience.1,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes a file for its readers (CNT-024); the row and the file wait for the trash purge.
pub async fn mark_deleted(
    conn: &mut PgConnection,
    id: AttachmentId,
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update attachments set deleted_at = now(), deleted_by = $2, version = version + 1
         where id = $1",
        id,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Records what a job learned of a picture (CNT-022).
pub async fn set_image_info(
    conn: &mut PgConnection,
    id: AttachmentId,
    width: i32,
    height: i32,
    thumbnail_key: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update attachments set width = $2, height = $3, thumbnail_key = $4 where id = $1",
        id,
        width,
        height,
        thumbnail_key
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Every file key of these issues' attachments, deleted ones and thumbnails included: what to
/// remove from the file store when the issues go.
pub async fn keys_of_issues(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select k as "key!" from attachments a,
                  unnest(array[a.storage_key, a.thumbnail_key]) k
           where a.issue_id = any($1) and k is not null"#,
        issues
    )
    .fetch_all(db)
    .await
}

// Deleted accounts and groups (architecture.md §6).

/// Puts the replacement into every file audience that lists a deleted account.
pub async fn replace_audience_users(
    conn: &mut PgConnection,
    users: &[UserId],
    replacement: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update attachments set audience_users = array(
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

/// Puts the replacement into every file audience that lists a deleted group.
pub async fn replace_audience_groups(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update attachments set audience_groups = array(
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
