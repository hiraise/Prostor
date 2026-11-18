//! Comments on issues (CNT-001…CNT-005, CNT-054): posting, reading, editing text and audience,
//! deleting to a placeholder and restoring. Design: decisions.md D-031.
//!
//! Every change locks the issue first, then the comment, and writes its history record through
//! the issue's change (which also marks the issue as updated). A comment the viewer may not see is
//! "not found": the issue rule first, then the comment rule, so an audience only narrows (CNT-003).

use prostor_domain::access::Access;
use prostor_domain::access::visibility::Audience;
use prostor_domain::attachments as attachment_rules;
use prostor_domain::comments::{self as rules, Facts, Refusal};
use prostor_domain::issues::history::{CommentEvent, Origin, Record};
use prostor_domain::live::Change;
use prostor_domain::{CommentId, GroupId, IssueId, UploadId, UserId};
use prostor_markup::Profile;
use prostor_store::attachments::{self, Attachment};
use prostor_store::comments::{self as store, Comment, NewComment as StoredNew};
use prostor_store::groups::{self, Group};
use prostor_store::issues::Issue;
use prostor_store::projects::Project;
use prostor_store::users::{self, User};
use prostor_store::{PgPool, issues as store_issues};
use sqlx::PgConnection;

use crate::attachments::AttachmentView;
use crate::issues::{self, AudienceInput};
use crate::live::Batch;
use crate::text::{self, FileScope, Refs};
use crate::{Error, Result};

/// What the viewer may do with a comment; the UI hides the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Abilities {
    pub update: bool,
    pub delete: bool,
    pub restore: bool,
}

/// A comment as one viewer sees it.
#[derive(Debug, Clone)]
pub struct CommentView {
    pub comment: Comment,
    pub can: Abilities,
}

impl CommentView {
    pub fn is_deleted(&self) -> bool {
        self.comment.is_deleted()
    }
}

/// Comments with what their answers name.
#[derive(Debug, Clone, Default)]
pub struct Described {
    pub comments: Vec<CommentView>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    pub refs: Refs,
    /// The files of the comments the viewer sees (CNT-018); their links are signed with
    /// `refs.link_secret`.
    pub files: Vec<AttachmentView>,
}

impl Described {
    /// The files posted with a comment.
    pub fn files_of(&self, comment: CommentId) -> impl Iterator<Item = &AttachmentView> {
        self.files
            .iter()
            .filter(move |f| f.file.comment_id == Some(comment))
    }
}

impl Described {
    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
}

fn facts<'a>(issue: &Issue, comment: &'a Comment) -> Facts<'a> {
    Facts {
        project: issue.project_id,
        author: comment.author_id,
        audience: Audience {
            users: &comment.audience_users,
            groups: &comment.audience_groups,
        },
    }
}

/// Whether the viewer sees a comment of an issue they see (CNT-002, CNT-003). A deleted one shows
/// as a placeholder to the same people.
pub fn can_see(viewer: &Access, issue: &Issue, comment: &Comment) -> bool {
    comment.issue_id == issue.id && rules::can_see(viewer, &facts(issue, comment))
}

fn abilities(viewer: &Access, issue: &Issue, project: &Project, comment: &Comment) -> Abilities {
    if project.is_archived() {
        return Abilities::default();
    }
    let f = facts(issue, comment);
    let deleted = comment.is_deleted();
    Abilities {
        update: !deleted && rules::may_update(viewer, &f).is_ok(),
        delete: !deleted && rules::may_delete(viewer, &f).is_ok(),
        restore: deleted && rules::may_restore(viewer, &f).is_ok(),
    }
}

/// Loads what answers show of comments the viewer sees: one query per relation, none for an
/// empty one. `more_users` and `more_groups` are loaded with them (the activity stream's records).
/// Deleted comments name only their author and who deleted them.
pub(crate) async fn describe(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: &Issue,
    project: &Project,
    list: Vec<Comment>,
    more_users: &[UserId],
    more_groups: &[GroupId],
) -> Result<Described> {
    let mut user_ids: Vec<UserId> = more_users.to_vec();
    let mut group_ids: Vec<GroupId> = more_groups.to_vec();
    let mut mentioned: Vec<UserId> = Vec::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut file_names: Vec<String> = Vec::new();
    let shown_ids: Vec<CommentId> = list
        .iter()
        .filter(|c| !c.is_deleted())
        .map(|c| c.id)
        .collect();
    let files: Vec<Attachment> = if shown_ids.is_empty() {
        Vec::new()
    } else {
        attachments::of_comments(&mut *conn, &shown_ids)
            .await?
            .into_iter()
            .filter(|f| crate::attachments::can_see(viewer, issue, f))
            .collect()
    };
    for file in &files {
        user_ids.push(file.author_id);
        user_ids.extend(&file.audience_users);
        group_ids.extend(&file.audience_groups);
    }
    let mut views = Vec::with_capacity(list.len());
    for comment in list {
        user_ids.push(comment.author_id);
        user_ids.extend(comment.deleted_by);
        if !comment.is_deleted() {
            user_ids.extend(comment.updated_by);
            user_ids.extend(&comment.audience_users);
            group_ids.extend(&comment.audience_groups);
            mentioned.extend(&comment.mentioned_users);
            tokens.extend(comment.issue_tokens.iter().cloned());
            file_names.extend(comment.file_names.iter().cloned());
        }
        views.push(CommentView {
            can: abilities(viewer, issue, project, &comment),
            comment,
        });
    }
    user_ids.sort_unstable();
    user_ids.dedup();
    group_ids.sort_unstable();
    group_ids.dedup();
    let found_users = if user_ids.is_empty() {
        Vec::new()
    } else {
        users::get_many(&mut *conn, &user_ids).await?
    };
    let found_groups = if group_ids.is_empty() {
        Vec::new()
    } else {
        groups::get_many(&mut *conn, &group_ids).await?
    };
    let scope = FileScope {
        issue: Some(issue),
        names: &file_names,
        uploads: &[],
    };
    let mut refs = text::refs(&mut *conn, viewer, &mentioned, &tokens, Some(scope)).await?;
    if !files.is_empty() && refs.link_secret.is_empty() {
        refs.link_secret = crate::files::viewer_secret(conn, viewer.user()).await?;
    }
    let files = files
        .into_iter()
        .map(|file| AttachmentView {
            can: crate::attachments::abilities(viewer, issue, project, &file),
            file,
        })
        .collect();
    Ok(Described {
        comments: views,
        users: found_users,
        groups: found_groups
            .into_iter()
            .filter(|g| crate::groups::team_visible(viewer, g))
            .collect(),
        refs,
        files,
    })
}

/// One comment with its lookups.
pub type One = (CommentView, Described);

fn single(described: Described) -> Result<One> {
    let mut rest = described;
    let view = rest
        .comments
        .pop()
        .ok_or(Error::NotFound("no such comment"))?;
    Ok((view, rest))
}

/// A comment by ID, a placeholder when deleted (CNT-005).
pub async fn get(pool: &PgPool, viewer: &Access, id: CommentId) -> Result<One> {
    let mut conn = pool.acquire().await?;
    let comment = store::get(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such comment"))?;
    let found = store_issues::get(&mut *conn, comment.issue_id).await?;
    let (issue, project) = issues::checked(&mut conn, viewer, found)
        .await
        .map_err(|_| Error::NotFound("no such comment"))?;
    if !can_see(viewer, &issue, &comment) {
        return Err(Error::NotFound("no such comment"));
    }
    single(describe(&mut conn, viewer, &issue, &project, vec![comment], &[], &[]).await?)
}

/// A new comment (CNT-001, CNT-003).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewComment {
    /// Markdown.
    pub text: String,
    /// `None` or empty: the same people as the issue.
    pub audience: Option<AudienceInput>,
    /// Without notifications (needs `issue.silent`).
    pub silent: bool,
    /// Staged uploads posted with it (CNT-018); then the text may be empty.
    pub uploads: Vec<UploadId>,
}

/// What posting a comment planted for the caller's change: its records and mentions join the
/// change the caller writes with [`issues::finish_change`], and the behaviors' records go to
/// [`crate::rules::finish`] (plan §3.5).
pub(crate) struct Planted {
    pub id: CommentId,
    pub records: Vec<Record>,
    pub mentions: Vec<UserId>,
    pub rule_records: Vec<Record>,
}

/// The comment itself on the caller's connection: the checks, the rendering, the insert and the
/// project's behaviors, without the transaction and the history write — commands apply (step
/// 2.1, plan §4.3) reuses this inside its per-issue change. The issue is locked.
pub(crate) async fn create_conn(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut crate::rules::Ctx<'_>,
    actor: &Access,
    issue: &Issue,
    input: NewComment,
) -> Result<Planted> {
    let text_value = rules::text(&input.text, !input.uploads.is_empty())
        .map_err(|r| Error::rejected("text", r))?;
    match rules::may_comment(actor, issue.project_id, issue.is_draft()) {
        Err(Refusal::Draft) => return Err(Error::Conflict("a draft takes no comments")),
        Err(Refusal::Denied(denied)) => return Err(denied.into()),
        Ok(()) => {}
    }
    if !input.uploads.is_empty() {
        attachment_rules::may_attach_to_comment(actor, issue.project_id)?;
    }
    let (audience_users, audience_groups) = match &input.audience {
        Some(audience) => issues::check_audience(&mut *conn, actor, audience).await?,
        None => (Vec::new(), Vec::new()),
    };
    let rendered = text::prepare(&mut *conn, &text_value, Profile::Full).await?;
    // Auto-watch (ISS-030): the first comment on the issue makes its author a watcher by their
    // own switch's word; later comments never re-add a star they took off.
    if !store::authored(&mut *conn, issue.id, actor.user()).await? {
        crate::notifications::auto_watch(
            &mut *conn,
            actor.user(),
            issue.id,
            crate::notifications::AutoWatch::Commented,
        )
        .await?;
    }
    let id = store::insert(
        &mut *conn,
        &StoredNew {
            issue: issue.id,
            author: actor.user(),
            text: &text_value,
            rendered: &rendered,
            audience_users: &audience_users,
            audience_groups: &audience_groups,
        },
    )
    .await?;
    batch.comment(id, Change::Created);
    let added = crate::attachments::attach_locked(
        &mut *conn,
        batch,
        actor,
        issue,
        Some(id),
        &input.uploads,
    )
    .await?;
    let mut records = vec![Record::comment(CommentEvent::Posted, id)];
    records.extend(crate::attachments::added_record(&added));
    // The behaviors hear the comment before its records (§3.5); a `+1` becomes a vote (AUT-079).
    let rule_records = crate::rules::on(
        &mut *conn,
        batch,
        ctx,
        crate::rules::Event::CommentAdded {
            issue,
            text: &text_value,
        },
    )
    .await?;
    Ok(Planted {
        id,
        records,
        mentions: rendered.users,
        rule_records,
    })
}

/// Posts a comment on a published issue the actor sees (CNT-001): `comment.create`, an active
/// project, a text that is not empty, an audience of accounts and groups the actor may name.
pub async fn create(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    issue: IssueId,
    input: NewComment,
) -> Result<crate::rules::Answer<One>> {
    let mut tx = pool.begin().await?;
    let (issue, _) = issues::lock_for_change(&mut tx, actor, issue).await?;
    let silent = input.silent;
    let mut batch = Batch::new();
    let mut ctx = crate::rules::Ctx::new(actor);
    let planted = create_conn(&mut tx, &mut batch, &mut ctx, actor, &issue, input).await?;
    let who = issues::author(actor, origin, silent, issue.project_id)?;
    // The comment's mentions ride its event (NTF-035).
    issues::finish_change(
        &mut tx,
        &mut batch,
        &issue,
        who,
        &planted.records,
        &planted.mentions,
    )
    .await?;
    crate::rules::finish(&mut tx, &mut batch, &ctx, &issue, &planted.rule_records).await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = get(pool, actor, planted.id).await?;
    Ok(crate::rules::Answer {
        done,
        messages: ctx.messages,
    })
}

/// A comment's issue and the comment, both locked, for a change by someone who sees them.
async fn lock(
    conn: &mut PgConnection,
    actor: &Access,
    id: CommentId,
) -> Result<(Issue, Project, Comment)> {
    let issue_id = store::issue_of(&mut *conn, id)
        .await?
        .ok_or(Error::NotFound("no such comment"))?;
    let (issue, project) = issues::lock_for_change(conn, actor, issue_id)
        .await
        .map_err(|error| match error {
            Error::NotFound(_) => Error::NotFound("no such comment"),
            other => other,
        })?;
    let comment = store::lock(&mut *conn, id)
        .await?
        .filter(|c| can_see(actor, &issue, c))
        .ok_or(Error::NotFound("no such comment"))?;
    Ok((issue, project, comment))
}

/// Changes of a comment (CNT-004); `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentChange {
    pub text: Option<String>,
    /// Empty lists: the same people as the issue.
    pub audience: Option<AudienceInput>,
    /// The version the edit started from: a stale one is refused.
    pub version: i64,
    pub silent: bool,
    /// Staged uploads to add to the comment (CNT-018).
    pub add_uploads: Vec<UploadId>,
}

/// Edits a comment's text or audience (CNT-004): own ones with `comment.update.own`, others' with
/// `comment.update.any`. The record keeps both versions of what changed.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: CommentId,
    change: CommentChange,
) -> Result<One> {
    // Emptiness is checked once the comment's files are known.
    let new_text = change
        .text
        .as_deref()
        .map(|raw| rules::text(raw, true))
        .transpose()
        .map_err(|r| Error::rejected("text", r))?;
    let mut tx = pool.begin().await?;
    let (issue, _, comment) = lock(&mut tx, actor, id).await?;
    if comment.is_deleted() {
        return Err(Error::Conflict("the comment is deleted"));
    }
    rules::may_update(actor, &facts(&issue, &comment))?;
    if comment.version != change.version {
        return Err(Error::Conflict("the comment was changed by someone else"));
    }
    if !change.add_uploads.is_empty() {
        attachment_rules::may_attach_to_comment(actor, issue.project_id)?;
    }
    if new_text.as_deref().is_some_and(str::is_empty)
        && change.add_uploads.is_empty()
        && attachments::of_comments(&mut *tx, &[id]).await?.is_empty()
    {
        return Err(Error::field(
            "text",
            prostor_domain::validation::Problem::Required,
            "the comment is empty",
        ));
    }
    let who = issues::author(actor, origin, change.silent, issue.project_id)?;
    let audience = match &change.audience {
        Some(audience) => Some(issues::check_audience(&mut tx, actor, audience).await?),
        None => None,
    };
    let text_value = new_text.unwrap_or_else(|| comment.text.clone());
    let (users_list, groups_list) = audience.unwrap_or_else(|| {
        (
            comment.audience_users.clone(),
            comment.audience_groups.clone(),
        )
    });
    let record = Record::comment_edited(
        id,
        Some((&comment.text, &text_value)),
        Some((
            (&comment.audience_users, &comment.audience_groups),
            (&users_list, &groups_list),
        )),
    );
    let mut batch = Batch::new();
    let added = crate::attachments::attach_locked(
        &mut tx,
        &mut batch,
        actor,
        &issue,
        Some(id),
        &change.add_uploads,
    )
    .await?;
    let files_record = crate::attachments::added_record(&added);
    if record.is_none()
        && let Some(files_record) = files_record.clone()
    {
        issues::finish_change(&mut tx, &mut batch, &issue, who, &[files_record], &[]).await?;
    }
    if let Some(record) = record {
        let rendered = if text_value == comment.text {
            prostor_store::content::Rendered {
                html: comment.html.clone(),
                users: comment.mentioned_users.clone(),
                issue_tokens: comment.issue_tokens.clone(),
                files: comment.file_names.clone(),
                version: comment.render_version,
            }
        } else {
            text::prepare(&mut tx, &text_value, Profile::Full).await?
        };
        store::update(
            &mut tx,
            id,
            &text_value,
            &rendered,
            (&users_list, &groups_list),
            actor.user(),
        )
        .await?;
        batch.comment(id, Change::Updated);
        let mut records = vec![record];
        records.extend(files_record);
        issues::finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    get(pool, actor, id).await
}

/// Deletes a comment to a placeholder (CNT-005): own ones with `comment.delete.own`, others' with
/// `comment.delete.any`. Deleting a deleted comment changes nothing.
pub async fn delete(pool: &PgPool, actor: &Access, origin: Origin, id: CommentId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (issue, _, comment) = lock(&mut tx, actor, id).await?;
    rules::may_delete(actor, &facts(&issue, &comment))?;
    if !comment.is_deleted() {
        store::mark_deleted(&mut tx, id, actor.user()).await?;
        let who = issues::author(actor, origin, false, issue.project_id)?;
        let mut batch = Batch::new();
        batch.comment(id, Change::Updated);
        issues::finish_change(
            &mut tx,
            &mut batch,
            &issue,
            who,
            &[Record::comment(CommentEvent::Deleted, id)],
            &[],
        )
        .await?;
        batch.publish(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Restores a deleted comment with its text and audience (CNT-005): whoever could delete it.
pub async fn restore(pool: &PgPool, actor: &Access, origin: Origin, id: CommentId) -> Result<One> {
    let mut tx = pool.begin().await?;
    let (issue, _, comment) = lock(&mut tx, actor, id).await?;
    rules::may_restore(actor, &facts(&issue, &comment))?;
    if comment.is_deleted() {
        store::restore(&mut tx, id).await?;
        let who = issues::author(actor, origin, false, issue.project_id)?;
        let mut batch = Batch::new();
        batch.comment(id, Change::Updated);
        issues::finish_change(
            &mut tx,
            &mut batch,
            &issue,
            who,
            &[Record::comment(CommentEvent::Restored, id)],
            &[],
        )
        .await?;
        batch.publish(&mut tx).await?;
    }
    tx.commit().await?;
    get(pool, actor, id).await
}
