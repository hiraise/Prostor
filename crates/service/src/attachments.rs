//! Files of issues and comments (CNT-017…CNT-024, ISS-007, ACC-047…ACC-049): attaching staged
//! uploads, listing what the viewer may see, renaming, audiences and deletion. Design:
//! decisions.md D-033.
//!
//! Every change locks the issue first, then the file, and writes its history record through the
//! issue's change. A file the viewer may not see is "not found": the issue rule first, then the
//! comment rule for a comment's file, then the file's own audience, so audiences only narrow.
//! An issue's files are read whole (at most `ISSUE_FILES_MAX`) and filtered by these rules, so a
//! page never leaks or skips (architecture.md §4).

use prostor_domain::access::Access;
use prostor_domain::access::visibility::Audience;
use prostor_domain::attachments::{self as rules, Facts};
use prostor_domain::files::{self as file_rules, ISSUE_FILES_MAX, Kind};
use prostor_domain::issues::history::{Origin, Record};
use prostor_domain::live::Change;
use prostor_domain::validation::Problem;
use prostor_domain::{AttachmentId, CommentId, GroupId, IssueId, UploadId, UserId};
use prostor_files::FileStore;
use prostor_store::PgPool;
use prostor_store::attachments::{self as store, Attachment, NewAttachment};
use prostor_store::comments::{self as store_comments, Comment};
use prostor_store::groups::{self, Group};
use prostor_store::issues::{self as store_issues, Issue};
use prostor_store::jobs::{self, Enqueue, JobError, JobKind};
use prostor_store::projects::Project;
use prostor_store::uploads;
use prostor_store::users::{self, User};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::images;
use crate::issues::{self, AudienceInput};
use crate::live::Batch;
use crate::{Error, Result};

/// Most files on one page of an issue's list.
pub const PAGE_MAX: i64 = 100;
/// Larger pictures get no size or thumbnail: decoding them would cost too much.
const INSPECT_MAX_BYTES: i64 = 64 * 1024 * 1024;

/// What the viewer may do with a file; the UI hides the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Abilities {
    pub update: bool,
    pub delete: bool,
}

/// A file as one viewer sees it.
#[derive(Debug, Clone)]
pub struct AttachmentView {
    pub file: Attachment,
    pub can: Abilities,
}

/// Files with what their answers name.
#[derive(Debug, Clone, Default)]
pub struct Listed {
    pub files: Vec<AttachmentView>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    /// The viewer's link secret, for signing the files' links; empty without files.
    pub link_secret: Vec<u8>,
    /// Pass as `after` for the next page; `None` at the end.
    pub next: Option<AttachmentId>,
}

impl Listed {
    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
}

fn facts<'a>(issue: &Issue, file: &'a Attachment) -> Facts<'a> {
    Facts {
        project: issue.project_id,
        author: file.author_id,
        audience: Audience {
            users: &file.audience_users,
            groups: &file.audience_groups,
        },
    }
}

/// Whether the viewer sees a file of an issue they see, by the file's own rule (ACC-049). A
/// comment's file also needs its comment visible: [`visible`] checks both.
pub fn can_see(viewer: &Access, issue: &Issue, file: &Attachment) -> bool {
    file.issue_id == issue.id && rules::can_see(viewer, &facts(issue, file))
}

/// The files among `rows` of one issue the viewer sees (the caller checked the issue): a
/// comment's file only while its comment is visible and, unless `history`, not deleted.
pub(crate) async fn visible(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: &Issue,
    rows: Vec<Attachment>,
    history: bool,
) -> Result<Vec<Attachment>> {
    let mut comment_ids: Vec<CommentId> = rows.iter().filter_map(|f| f.comment_id).collect();
    comment_ids.sort_unstable();
    comment_ids.dedup();
    let found: Vec<Comment> = if comment_ids.is_empty() {
        Vec::new()
    } else {
        store_comments::get_many(&mut *conn, &comment_ids).await?
    };
    Ok(rows
        .into_iter()
        .filter(|file| can_see(viewer, issue, file))
        .filter(|file| match file.comment_id {
            None => true,
            Some(id) => found.iter().any(|c| {
                c.id == id
                    && (history || !c.is_deleted())
                    && crate::comments::can_see(viewer, issue, c)
            }),
        })
        .collect())
}

pub(crate) fn abilities(
    viewer: &Access,
    issue: &Issue,
    project: &Project,
    file: &Attachment,
) -> Abilities {
    if project.is_archived() || file.is_deleted() {
        return Abilities::default();
    }
    let f = facts(issue, file);
    Abilities {
        update: rules::may_update(viewer, &f).is_ok(),
        delete: rules::may_delete(viewer, &f).is_ok(),
    }
}

/// Loads what answers show of files the viewer sees: one query per relation.
pub(crate) async fn describe(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: &Issue,
    project: &Project,
    rows: Vec<Attachment>,
) -> Result<Listed> {
    if rows.is_empty() {
        return Ok(Listed::default());
    }
    let mut user_ids: Vec<UserId> = Vec::new();
    let mut group_ids: Vec<GroupId> = Vec::new();
    for file in &rows {
        user_ids.push(file.author_id);
        user_ids.extend(&file.audience_users);
        group_ids.extend(&file.audience_groups);
    }
    user_ids.sort_unstable();
    user_ids.dedup();
    group_ids.sort_unstable();
    group_ids.dedup();
    let found_users = users::get_many(&mut *conn, &user_ids).await?;
    let found_groups = if group_ids.is_empty() {
        Vec::new()
    } else {
        groups::get_many(&mut *conn, &group_ids).await?
    };
    Ok(Listed {
        files: rows
            .into_iter()
            .map(|file| AttachmentView {
                can: abilities(viewer, issue, project, &file),
                file,
            })
            .collect(),
        users: found_users,
        groups: found_groups
            .into_iter()
            .filter(|g| crate::groups::team_visible(viewer, g))
            .collect(),
        link_secret: crate::files::viewer_secret(conn, viewer.user()).await?,
        next: None,
    })
}

/// The files of an issue the viewer sees, its comments' included, oldest first, in pages
/// (CNT-017, CNT-018).
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
    after: Option<AttachmentId>,
    limit: i64,
) -> Result<Listed> {
    let mut conn = pool.acquire().await?;
    let found = store_issues::get(&mut *conn, issue).await?;
    let (issue, project) = issues::checked(&mut conn, viewer, found).await?;
    list_of(&mut conn, viewer, &issue, &project, after, limit).await
}

/// The files of an issue already loaded and checked for the viewer, as [`list`] pages them.
pub(crate) async fn list_of(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: &Issue,
    project: &Project,
    after: Option<AttachmentId>,
    limit: i64,
) -> Result<Listed> {
    let limit = usize::try_from(limit.clamp(1, PAGE_MAX)).unwrap_or(1);
    let all = store::of_issue(&mut *conn, issue.id).await?;
    let mut shown: Vec<Attachment> = visible(conn, viewer, issue, all, false)
        .await?
        .into_iter()
        .filter(|f| after.is_none_or(|a| f.id > a))
        .collect();
    let next = if shown.len() > limit {
        shown.truncate(limit);
        shown.last().map(|f| f.id)
    } else {
        None
    };
    let mut listed = describe(conn, viewer, issue, project, shown).await?;
    listed.next = next;
    Ok(listed)
}

/// One file the viewer sees.
pub async fn get(pool: &PgPool, viewer: &Access, id: AttachmentId) -> Result<Listed> {
    let mut conn = pool.acquire().await?;
    let file = store::get(&mut *conn, id)
        .await?
        .filter(|f| !f.is_deleted())
        .ok_or(NO_FILE)?;
    let found = store_issues::get(&mut *conn, file.issue_id).await?;
    let (issue, project) = issues::checked(&mut conn, viewer, found)
        .await
        .map_err(|_| NO_FILE)?;
    let shown = visible(&mut conn, viewer, &issue, vec![file], false).await?;
    if shown.is_empty() {
        return Err(NO_FILE);
    }
    describe(&mut conn, viewer, &issue, &project, shown).await
}

const NO_FILE: Error = Error::NotFound("no such file");

/// Turns the actor's staged uploads into files of a locked issue (or of a comment on it): names
/// made unique on the issue, at most [`ISSUE_FILES_MAX`] files, pictures queued for inspection.
/// Rights are the caller's to check. Answers the new files' IDs and names.
pub(crate) async fn attach_locked(
    conn: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    issue: &Issue,
    comment: Option<CommentId>,
    upload_ids: &[UploadId],
) -> Result<Vec<(AttachmentId, String)>> {
    if upload_ids.is_empty() {
        return Ok(Vec::new());
    }
    // In the order given, so names that collide number in that order.
    let mut ordered: Vec<UploadId> = Vec::with_capacity(upload_ids.len());
    for id in upload_ids {
        if !ordered.contains(id) {
            ordered.push(*id);
        }
    }
    let staged = uploads::lock_of_owner(&mut *conn, actor.user(), &ordered).await?;
    if staged.len() != ordered.len() {
        return Err(Error::field("uploads", Problem::Invalid, "no such upload"));
    }
    let mut names = store::names_of_issue(&mut *conn, issue.id).await?;
    if names.len() + staged.len() > ISSUE_FILES_MAX {
        return Err(Error::Conflict("the issue has too many files"));
    }
    let mut added = Vec::with_capacity(staged.len());
    for upload in &staged {
        let name = file_rules::unique_name(&names, &upload.name);
        let secret = crate::files::new_secret()?;
        let id = store::insert(
            &mut *conn,
            &NewAttachment {
                issue: issue.id,
                comment,
                author: actor.user(),
                name: &name,
                size: upload.size,
                content_type: &upload.content_type,
                sha256: &upload.sha256,
                storage_key: &upload.storage_key,
                secret: &secret,
            },
        )
        .await?;
        if Kind::of_content_type(&upload.content_type).is_image()
            && upload.size <= INSPECT_MAX_BYTES
        {
            jobs::enqueue(
                &mut *conn,
                &ImageInspection { attachment: id },
                Enqueue::default(),
            )
            .await?;
        }
        names.push(name.clone());
        batch.attachment(id, Change::Created);
        added.push((id, name));
    }
    uploads::forget(&mut *conn, &ordered).await?;
    Ok(added)
}

/// The history record of files added.
pub(crate) fn added_record(added: &[(AttachmentId, String)]) -> Option<Record> {
    let pairs: Vec<(i64, &str)> = added.iter().map(|(id, n)| (*id, n.as_str())).collect();
    Record::attachments(&[], &pairs)
}

/// Attaches staged uploads to an issue or draft (CNT-017, ISS-007): changing the issue (or being
/// its reporter) and `attachment.create`, an active project.
pub async fn attach(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    issue: IssueId,
    upload_ids: &[UploadId],
    silent: bool,
) -> Result<Listed> {
    if upload_ids.is_empty() {
        return Err(Error::field("uploads", Problem::Required, "no files given"));
    }
    let mut tx = pool.begin().await?;
    let (issue, project) = issues::lock_for_change(&mut tx, actor, issue).await?;
    rules::may_attach(actor, &issues::facts(&issue))?;
    let who = issues::author(actor, origin, silent, issue.project_id)?;
    let mut batch = Batch::new();
    let added = attach_locked(&mut tx, &mut batch, actor, &issue, None, upload_ids).await?;
    let records: Vec<Record> = added_record(&added).into_iter().collect();
    issues::finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let ids: Vec<AttachmentId> = added.iter().map(|(id, _)| *id).collect();
    let mut conn = pool.acquire().await?;
    let mut rows = store::get_many(&mut *conn, &ids).await?;
    rows.sort_by_key(|f| f.id);
    describe(&mut conn, actor, &issue, &project, rows).await
}

/// A file's issue and the file, both locked, for a change by someone who sees them.
async fn lock(
    conn: &mut PgConnection,
    actor: &Access,
    id: AttachmentId,
) -> Result<(Issue, Project, Attachment)> {
    let issue_id = store::issue_of(&mut *conn, id).await?.ok_or(NO_FILE)?;
    let (issue, project) = issues::lock_for_change(conn, actor, issue_id)
        .await
        .map_err(|error| match error {
            Error::NotFound(_) => NO_FILE,
            other => other,
        })?;
    let file = store::lock(&mut *conn, id).await?.ok_or(NO_FILE)?;
    let mut shown = visible(conn, actor, &issue, vec![file], false).await?;
    let file = shown.pop().ok_or(NO_FILE)?;
    Ok((issue, project, file))
}

/// Changes of a file; `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttachmentChange {
    pub name: Option<String>,
    /// Empty lists: the same people as the issue (or the comment).
    pub audience: Option<AudienceInput>,
    /// The version the edit started from: a stale one is refused.
    pub version: i64,
}

/// Renames a file or changes its audience (CNT-020, CNT-023): one's own with
/// `attachment.create`, others' with `attachment.update`. Names stay unique on the issue.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    id: AttachmentId,
    change: AttachmentChange,
) -> Result<Listed> {
    let new_name = change
        .name
        .as_deref()
        .map(file_rules::name)
        .transpose()
        .map_err(|r| Error::rejected("name", r))?;
    let mut tx = pool.begin().await?;
    let (issue, project, file) = lock(&mut tx, actor, id).await?;
    if file.is_deleted() {
        return Err(Error::Conflict("the file is deleted"));
    }
    rules::may_update(actor, &facts(&issue, &file))?;
    if file.version != change.version {
        return Err(Error::Conflict("the file was changed by someone else"));
    }
    let name = new_name.unwrap_or_else(|| file.name.clone());
    if name.to_lowercase() != file.name.to_lowercase() {
        let names = store::names_of_issue(&mut *tx, issue.id).await?;
        if names
            .iter()
            .any(|n| n.to_lowercase() == name.to_lowercase())
        {
            return Err(Error::field(
                "name",
                Problem::Taken,
                "the issue has a file of this name",
            ));
        }
    }
    let (audience_users, audience_groups) = match &change.audience {
        Some(audience) => issues::check_audience(&mut tx, actor, audience).await?,
        None => (file.audience_users.clone(), file.audience_groups.clone()),
    };
    store::update(
        &mut tx,
        id,
        &name,
        (&audience_users, &audience_groups),
        actor.user(),
    )
    .await?;
    let mut batch = Batch::new();
    batch.attachment(id, Change::Updated);
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let mut conn = pool.acquire().await?;
    let changed = store::get(&mut *conn, id).await?.ok_or(NO_FILE)?;
    let shown = visible(&mut conn, actor, &issue, vec![changed], false).await?;
    describe(&mut conn, actor, &issue, &project, shown).await
}

/// Deletes a file (CNT-024): one's own always, others' with `attachment.delete`. It disappears
/// and its links stop working at once; the file stays for the trash (D-013). Deleting a deleted
/// file changes nothing.
pub async fn delete(pool: &PgPool, actor: &Access, origin: Origin, id: AttachmentId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (issue, _, file) = lock(&mut tx, actor, id).await?;
    rules::may_delete(actor, &facts(&issue, &file))?;
    if !file.is_deleted() {
        store::mark_deleted(&mut tx, id, actor.user()).await?;
        let who = issues::author(actor, origin, false, issue.project_id)?;
        let records: Vec<Record> = Record::attachments(&[(file.id, &file.name)], &[])
            .into_iter()
            .collect();
        let mut batch = Batch::new();
        batch.attachment(id, Change::Updated);
        issues::finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
        batch.publish(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Learns a new picture's size and makes its thumbnail (`attachments.inspect`, CNT-022). Files
/// that are not pictures after all are left as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInspection {
    pub attachment: AttachmentId,
}

impl JobKind for ImageInspection {
    const KIND: &'static str = "attachments.inspect";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 5;
}

pub async fn run_inspection(
    pool: &PgPool,
    files: &FileStore,
    job: ImageInspection,
) -> Result<(), JobError> {
    let Some(file) = store::get(pool, job.attachment).await? else {
        return Ok(());
    };
    let kind = Kind::of_content_type(&file.content_type);
    if !kind.is_image() || file.width.is_some() {
        return Ok(());
    }
    let bytes = match files.read(&file.storage_key).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(JobError::Retry(format!("cannot read the file: {error}"))),
    };
    let inspected = images::off_thread(move || images::inspect(&bytes, kind))
        .await
        .map_err(|e| JobError::Retry(e.to_string()))?;
    let Ok(found) = inspected else {
        return Ok(());
    };
    let thumbnail_key = match &found.thumbnail {
        Some(thumbnail) => Some(
            files
                .put(&thumbnail.bytes, thumbnail.kind.extension())
                .await
                .map_err(|e| JobError::Retry(format!("cannot store the thumbnail: {e}")))?,
        ),
        None => None,
    };
    let mut conn = pool.acquire().await?;
    let width = i32::try_from(found.width).unwrap_or(i32::MAX);
    let height = i32::try_from(found.height).unwrap_or(i32::MAX);
    store::set_image_info(&mut conn, file.id, width, height, thumbnail_key.as_deref()).await?;
    Ok(())
}
