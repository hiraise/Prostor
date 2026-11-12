//! Rich text (D-031): rendering Markdown when it is saved, and resolving what rendered texts refer
//! to for each viewer when they are read.
//!
//! Saving: the logins after `@` are looked up in one query, then the crate `markup` renders the
//! text with the accounts found (large texts off the async runtime). Reading: every answer with
//! rendered text carries its references — the people mentioned (shown through the people
//! presenter by the server) and the issue IDs that name issues the viewer may see. One query per
//! relation for the whole answer.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use trackflow_domain::access::Access;
use trackflow_domain::issues::readable_id;
use trackflow_domain::{IssueId, UploadId, UserId};
use trackflow_markup::{Output, Profile, RENDER_VERSION};
use trackflow_store::PgPool;
use trackflow_store::attachments::{self, Attachment};
use trackflow_store::content::{self, Rendered};
use trackflow_store::issues::{self, Issue};
use trackflow_store::jobs::{self, Enqueue, JobError, JobKind};
use trackflow_store::projects::{self, Project};
use trackflow_store::uploads::{self, Upload};
use trackflow_store::users::{self, User};

use crate::{Error, Result};

/// Texts larger than this render on a blocking thread.
const INLINE_MAX: usize = 32 * 1024;

/// Renders a text to store: mentions resolved to accounts that exist now.
pub async fn prepare(conn: &mut PgConnection, text: &str, profile: Profile) -> Result<Rendered> {
    let mut known = HashMap::new();
    if profile == Profile::Full {
        let logins = trackflow_markup::mentions(text);
        if !logins.is_empty() {
            for user in users::by_usernames(&mut *conn, &logins).await? {
                known.insert(user.username.to_ascii_lowercase(), user.id);
            }
        }
    }
    let output = render(text.to_owned(), profile, known).await?;
    Ok(stored(output))
}

/// Renders several texts with one lookup of the logins they mention: the re-rendering job.
pub async fn prepare_many(
    conn: &mut PgConnection,
    texts: &[String],
    profile: Profile,
) -> Result<Vec<Rendered>> {
    let mut known = HashMap::new();
    if profile == Profile::Full {
        let mut logins: Vec<String> = texts
            .iter()
            .flat_map(|t| trackflow_markup::mentions(t))
            .collect();
        logins.sort_unstable();
        logins.dedup();
        if !logins.is_empty() {
            for user in users::by_usernames(&mut *conn, &logins).await? {
                known.insert(user.username.to_ascii_lowercase(), user.id);
            }
        }
    }
    let texts = texts.to_vec();
    let outputs = tokio::task::spawn_blocking(move || {
        texts
            .iter()
            .map(|t| trackflow_markup::render(t, profile, &known))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| Error::Internal(format!("rendering stopped: {e}")))?;
    Ok(outputs.into_iter().map(stored).collect())
}

async fn render(text: String, profile: Profile, known: HashMap<String, i64>) -> Result<Output> {
    if text.len() <= INLINE_MAX {
        return Ok(trackflow_markup::render(&text, profile, &known));
    }
    tokio::task::spawn_blocking(move || trackflow_markup::render(&text, profile, &known))
        .await
        .map_err(|e| Error::Internal(format!("rendering stopped: {e}")))
}

fn stored(output: Output) -> Rendered {
    Rendered {
        html: output.html,
        users: output.users,
        issue_tokens: output.issue_tokens,
        files: output.files,
        version: trackflow_markup::RENDER_VERSION,
    }
}

/// An issue ID in a text that names an issue the viewer may see (CNT-056).
#[derive(Debug, Clone)]
pub struct IssueMention {
    /// As written: `ABC-12`, possibly a former ID.
    pub token: String,
    pub issue: Issue,
    pub project: Project,
}

impl IssueMention {
    /// The issue's current readable ID.
    pub fn readable_id(&self) -> String {
        readable_id(&self.project.key, self.issue.number.unwrap_or_default())
    }
}

/// What rendered texts refer to, as one viewer may see it.
#[derive(Debug, Clone, Default)]
pub struct Refs {
    /// People mentioned; the server shows them through the people presenter.
    pub users: Vec<User>,
    pub issues: Vec<IssueMention>,
    /// Files of the issue the texts embed by name, that the viewer sees (CNT-044).
    pub files: Vec<Attachment>,
    /// The viewer's staged uploads named by a preview (CNT-034).
    pub uploads: Vec<Upload>,
    /// The viewer's link secret, for signing the files' links; empty without files.
    pub link_secret: Vec<u8>,
}

/// Where the file names of texts are looked up: the files of their issue, and for a preview
/// the viewer's staged uploads.
#[derive(Debug, Clone, Copy)]
pub struct FileScope<'a> {
    pub issue: Option<&'a Issue>,
    /// The names the texts embed.
    pub names: &'a [String],
    pub uploads: &'a [UploadId],
}

/// The files and uploads that names of texts resolve to, for a viewer who sees the issue.
async fn file_refs(
    conn: &mut PgConnection,
    viewer: &Access,
    scope: FileScope<'_>,
    refs: &mut Refs,
) -> Result<()> {
    let mut names = scope.names.to_vec();
    names.sort_unstable();
    names.dedup();
    if let Some(issue) = scope.issue
        && !names.is_empty()
    {
        let rows = attachments::of_issue_named(&mut *conn, issue.id, &names).await?;
        refs.files = crate::attachments::visible(conn, viewer, issue, rows, false).await?;
    }
    if !scope.uploads.is_empty() {
        refs.uploads = uploads::of_owner(&mut *conn, viewer.user(), scope.uploads).await?;
    }
    if !refs.files.is_empty() {
        refs.link_secret = crate::files::viewer_secret(conn, viewer.user()).await?;
    }
    Ok(())
}

/// Resolves the references of rendered texts for a viewer: mentioned accounts, and issue IDs that
/// name issues the viewer may see. Unknown and invisible IDs are left out, so they stay text.
pub async fn refs(
    conn: &mut PgConnection,
    viewer: &Access,
    mentioned: &[UserId],
    tokens: &[String],
    files: Option<FileScope<'_>>,
) -> Result<Refs> {
    let mut found = issue_refs(conn, viewer, mentioned, tokens).await?;
    if let Some(scope) = files {
        file_refs(conn, viewer, scope, &mut found).await?;
    }
    Ok(found)
}

async fn issue_refs(
    conn: &mut PgConnection,
    viewer: &Access,
    mentioned: &[UserId],
    tokens: &[String],
) -> Result<Refs> {
    let mut user_ids = mentioned.to_vec();
    user_ids.sort_unstable();
    user_ids.dedup();
    let mut tokens = tokens.to_vec();
    tokens.sort_unstable();
    tokens.dedup();
    let users = if user_ids.is_empty() {
        Vec::new()
    } else {
        users::get_many(&mut *conn, &user_ids).await?
    };
    if tokens.is_empty() {
        return Ok(Refs {
            users,
            ..Refs::default()
        });
    }
    let resolved = issues::resolve_tokens(&mut *conn, &tokens).await?;
    let mut issue_ids: Vec<IssueId> = resolved.iter().map(|(_, id)| *id).collect();
    issue_ids.sort_unstable();
    issue_ids.dedup();
    let found = issues::get_many(&mut *conn, &issue_ids).await?;
    let mut project_ids: Vec<_> = found.iter().map(|i| i.project_id).collect();
    project_ids.sort_unstable();
    project_ids.dedup();
    let found_projects = projects::get_many(&mut *conn, &project_ids).await?;
    let mut mentions = Vec::new();
    for (token, id) in resolved {
        let Some(issue) = found.iter().find(|i| i.id == id) else {
            continue;
        };
        let Some(project) = found_projects.iter().find(|p| p.id == issue.project_id) else {
            continue;
        };
        if issue.is_draft() || !crate::issues::can_read(viewer, issue, project) {
            continue;
        }
        mentions.push(IssueMention {
            token,
            issue: issue.clone(),
            project: project.clone(),
        });
    }
    Ok(Refs {
        users,
        issues: mentions,
        ..Refs::default()
    })
}

/// Renders every stored text made with older rules again (`content.render`, D-031), in batches
/// of [`RENDER_BATCH`], each its own transaction; then the instance records the new version.
/// Idempotent: an interrupted run starts over where texts are still old.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderJob {
    /// The rules to bring texts to.
    pub version: i16,
}

impl JobKind for RenderJob {
    const KIND: &'static str = "content.render";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 10;
}

/// Rows one transaction renders.
pub const RENDER_BATCH: i64 = 500;

/// Queues the job when stored texts are older than the renderer (called at startup). `true`
/// when it was queued now.
pub async fn ensure_rendered(pool: &PgPool) -> Result<bool> {
    let mut tx = pool.begin().await?;
    if content::instance_version(&mut *tx).await? >= RENDER_VERSION {
        return Ok(false);
    }
    let queued = jobs::enqueue(
        &mut tx,
        &RenderJob {
            version: RENDER_VERSION,
        },
        Enqueue {
            unique_key: Some(RenderJob::KIND.to_owned()),
            ..Enqueue::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(queued.created)
}

fn job_error(error: Error) -> JobError {
    match error {
        Error::Database(e) => JobError::Retry(e.to_string()),
        other => JobError::Fail(other.to_string()),
    }
}

/// Runs the job to the end.
pub async fn run_render(pool: &PgPool, job: RenderJob) -> Result<(), JobError> {
    let version = job.version;
    let mut after = 0;
    loop {
        let batch = content::stale_issues(pool, version, after, RENDER_BATCH).await?;
        let Some(last) = batch.last().map(|(id, _)| *id) else {
            break;
        };
        let mut tx = pool.begin().await?;
        let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
        let rendered = prepare_many(&mut tx, &texts, Profile::Full)
            .await
            .map_err(job_error)?;
        for ((id, _), r) in batch.iter().zip(&rendered) {
            content::set_issue(&mut tx, *id, &with_version(r, version)).await?;
        }
        tx.commit().await?;
        after = last;
    }
    let mut after = (0, 0);
    loop {
        let batch = content::stale_text_values(pool, version, after, RENDER_BATCH).await?;
        let Some(last) = batch.last().map(|(issue, field, _)| (*issue, *field)) else {
            break;
        };
        let mut tx = pool.begin().await?;
        let texts: Vec<String> = batch.iter().map(|(_, _, t)| t.clone()).collect();
        let rendered = prepare_many(&mut tx, &texts, Profile::Full)
            .await
            .map_err(job_error)?;
        for ((issue, field, _), r) in batch.iter().zip(&rendered) {
            content::set_text_value(&mut tx, *issue, *field, &with_version(r, version)).await?;
        }
        tx.commit().await?;
        after = last;
    }
    let mut after = 0;
    loop {
        let batch = content::stale_comments(pool, version, after, RENDER_BATCH).await?;
        let Some(last) = batch.last().map(|(id, _)| *id) else {
            break;
        };
        let mut tx = pool.begin().await?;
        let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
        let rendered = prepare_many(&mut tx, &texts, Profile::Full)
            .await
            .map_err(job_error)?;
        for ((id, _), r) in batch.iter().zip(&rendered) {
            content::set_comment(&mut tx, *id, &with_version(r, version)).await?;
        }
        tx.commit().await?;
        after = last;
    }
    let mut after = 0;
    loop {
        let batch = content::stale_projects(pool, version, after, RENDER_BATCH).await?;
        let Some(last) = batch.last().map(|(id, _)| *id) else {
            break;
        };
        let mut tx = pool.begin().await?;
        let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
        let rendered = prepare_many(&mut tx, &texts, Profile::Basic)
            .await
            .map_err(job_error)?;
        for ((id, _), r) in batch.iter().zip(&rendered) {
            content::set_project(&mut tx, *id, &r.html, version).await?;
        }
        tx.commit().await?;
        after = last;
    }
    let mut conn = pool.acquire().await?;
    content::set_instance_version(&mut conn, version).await?;
    Ok(())
}

/// The job brings texts to the version it was queued for.
fn with_version(rendered: &Rendered, version: i16) -> Rendered {
    Rendered {
        version,
        ..rendered.clone()
    }
}

/// Largest text a preview renders (the largest description).
pub const PREVIEW_MAX: usize = trackflow_domain::issues::DESCRIPTION_MAX;

/// Renders a text for the editor's preview (CNT-029, CNT-038), exactly as it would be stored.
/// `issue`, when given, must be visible to the viewer; its files and the viewer's `uploads` show
/// where the text names them (CNT-034).
pub async fn preview(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    profile: Profile,
    issue: Option<IssueId>,
    uploads: &[UploadId],
) -> Result<(Rendered, Refs)> {
    if text.len() > PREVIEW_MAX {
        return Err(Error::field(
            "text",
            trackflow_domain::validation::Problem::TooLong,
            format!("the text is larger than {PREVIEW_MAX} bytes"),
        ));
    }
    let mut conn = pool.acquire().await?;
    let issue = match issue {
        Some(id) => {
            let found = issues::get(&mut *conn, id).await?;
            Some(crate::issues::checked(&mut conn, viewer, found).await?.0)
        }
        None => None,
    };
    let rendered = prepare(&mut conn, text, profile).await?;
    let scope = FileScope {
        issue: issue.as_ref(),
        names: &rendered.files,
        uploads,
    };
    let found = refs(
        &mut conn,
        viewer,
        &rendered.users,
        &rendered.issue_tokens,
        Some(scope),
    )
    .await?;
    Ok((rendered, found))
}
