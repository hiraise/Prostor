//! Rendered rich text (D-031): what is stored next to each Markdown text, and the queries of the
//! job that renders texts of older rules again. Texts: issue descriptions, text field values,
//! comments and project descriptions.

use sqlx::{PgConnection, PgExecutor};
use trackflow_domain::{CommentId, FieldId, IssueId, ProjectId, UserId};

/// A text rendered by the crate `markup`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    /// Accounts mentioned.
    pub users: Vec<UserId>,
    /// Issue ID tokens such as `ABC-12`.
    pub issue_tokens: Vec<String>,
    /// File names the text embeds.
    pub files: Vec<String>,
    /// `markup::RENDER_VERSION` it was made with.
    pub version: i16,
}

/// The rendering rules every stored text has been brought to.
pub async fn instance_version(db: impl PgExecutor<'_>) -> Result<i16, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("select render_version from instance where id")
            .fetch_optional(db)
            .await?
            .unwrap_or(0),
    )
}

pub async fn set_instance_version(
    conn: &mut PgConnection,
    version: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query!("update instance set render_version = $1 where id", version)
        .execute(conn)
        .await?;
    Ok(())
}

/// Issues whose description was rendered with older rules: `(id, description)` after `after`.
pub async fn stale_issues(
    db: impl PgExecutor<'_>,
    version: i16,
    after: IssueId,
    limit: i64,
) -> Result<Vec<(IssueId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select id, description from issues where render_version < $1 and id > $2
         order by id limit $3",
        version,
        after,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.description)).collect())
}

/// Stores an issue description's rendering; the issue itself does not count as changed.
pub async fn set_issue(
    conn: &mut PgConnection,
    id: IssueId,
    rendered: &Rendered,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issues set description_html = $2, mentioned_users = $3, issue_tokens = $4,
                           file_names = $5, render_version = $6
         where id = $1",
        id,
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

/// Text field values rendered with older rules: `(issue, field, text)` after `after`.
pub async fn stale_text_values(
    db: impl PgExecutor<'_>,
    version: i16,
    after: (IssueId, FieldId),
    limit: i64,
) -> Result<Vec<(IssueId, FieldId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select issue_id, field_id, value from issue_text_values
         where render_version < $1 and (issue_id, field_id) > ($2, $3)
         order by issue_id, field_id limit $4",
        version,
        after.0,
        after.1,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.issue_id, r.field_id, r.value))
        .collect())
}

pub async fn set_text_value(
    conn: &mut PgConnection,
    issue: IssueId,
    field: FieldId,
    rendered: &Rendered,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update issue_text_values set html = $3, mentioned_users = $4, issue_tokens = $5,
                                      file_names = $6, render_version = $7
         where issue_id = $1 and field_id = $2",
        issue,
        field,
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

/// Comments rendered with older rules: `(id, text)` after `after`.
pub async fn stale_comments(
    db: impl PgExecutor<'_>,
    version: i16,
    after: CommentId,
    limit: i64,
) -> Result<Vec<(CommentId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select id, text from comments where render_version < $1 and id > $2
         order by id limit $3",
        version,
        after,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.text)).collect())
}

/// Stores a comment's rendering; the comment itself does not count as edited.
pub async fn set_comment(
    conn: &mut PgConnection,
    id: CommentId,
    rendered: &Rendered,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update comments set html = $2, mentioned_users = $3, issue_tokens = $4,
                             file_names = $5, render_version = $6
         where id = $1",
        id,
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

/// Projects whose description was rendered with older rules: `(id, description)`.
pub async fn stale_projects(
    db: impl PgExecutor<'_>,
    version: i16,
    after: ProjectId,
    limit: i64,
) -> Result<Vec<(ProjectId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select id, description from projects where render_version < $1 and id > $2
         order by id limit $3",
        version,
        after,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.description)).collect())
}

/// Stores a project description's rendering (basic profile: HTML only).
pub async fn set_project(
    conn: &mut PgConnection,
    id: ProjectId,
    html: &str,
    version: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update projects set description_html = $2, render_version = $3 where id = $1",
        id,
        html,
        version
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A text field value's rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextValueHtml {
    pub issue: IssueId,
    pub field: FieldId,
    pub html: String,
    pub users: Vec<UserId>,
    pub issue_tokens: Vec<String>,
    pub files: Vec<String>,
}

/// The renderings of the issues' text field values.
pub async fn text_values_of(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
) -> Result<Vec<TextValueHtml>, sqlx::Error> {
    sqlx::query_as!(
        TextValueHtml,
        r#"select issue_id as issue, field_id as field, html, mentioned_users as users,
                  issue_tokens, file_names as files
           from issue_text_values where issue_id = any($1)"#,
        issues
    )
    .fetch_all(db)
    .await
}
