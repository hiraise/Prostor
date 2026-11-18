//! An issue's activity stream (HIST-001, HIST-006…HIST-011, CNT-005, CNT-007): change records and
//! comments in one order, in pages both ways from a record or around a comment, with what the
//! viewer may not see left out — changes of private fields without `issue.private.read`, other
//! people's personal tags, comments outside the viewer's reach.
//!
//! Comments are the records of their posting; edits, deletions and restorations of comments are
//! records too but never stream items (HIST-006). A deleted comment is a placeholder. A change
//! made by a comment's author within a minute after it is grouped under it (HIST-011).

use serde_json::Value as Json;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::{ChangeKind, LoggedLink, clone_of, read_ref};
use taskmaster_domain::issues::rights;
use taskmaster_domain::{
    AttachmentId, CommentId, FieldId, GroupId, IssueId, ProjectId, TagId, UserId,
};
use taskmaster_store::PgPool;
use taskmaster_store::attachments as store_attachments;
use taskmaster_store::comments::{self as store_comments, Comment};
use taskmaster_store::fields::{self as store_fields, Field};
use taskmaster_store::groups::Group;
use taskmaster_store::issue_history::{self, Direction, Entry};
use taskmaster_store::issues::{self, Issue};
use taskmaster_store::links::{self as store_links, LinkType, Linked};
use taskmaster_store::projects::{self, Project};
use taskmaster_store::tags::{self as store_tags, Tag};
use taskmaster_store::users::User;
use taskmaster_store::value_sets::{self, Value as SetValue};
use time::Duration;

use super::values::{self, ProjectFields};
use crate::attachments::AttachmentView;
use crate::comments::{self, CommentView};
use crate::text::Refs;
use crate::{Error, Result};

/// Most items on one page.
pub const PAGE_MAX: i64 = 100;
/// How many times a page is topped up when records were left out.
const ROUNDS: usize = 5;
/// Changes this soon after their author's comment are grouped with it (HIST-011).
const GROUPING: Duration = Duration::seconds(60);

/// What a page shows and where it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query {
    /// Comments (HIST-008).
    pub comments: bool,
    /// Change records (HIST-008).
    pub changes: bool,
    pub from: Option<i64>,
    pub direction: Direction,
    pub limit: i64,
    /// The page around this comment instead (CNT-007): its record, newer and older items.
    pub around_comment: Option<CommentId>,
}

/// One item of the stream.
#[derive(Debug, Clone)]
pub enum Item {
    /// A change record; `after_comment` is the comment it is grouped with (HIST-011).
    Change {
        entry: Entry,
        after_comment: Option<CommentId>,
    },
    /// A comment, at its posting record; a placeholder when deleted.
    Comment {
        entry: Entry,
        comment: Box<CommentView>,
    },
}

impl Item {
    pub fn entry(&self) -> &Entry {
        match self {
            Item::Change { entry, .. } | Item::Comment { entry, .. } => entry,
        }
    }
}

/// A page of the stream with what its items name.
#[derive(Debug, Clone, Default)]
pub struct ActivityPage {
    /// Newest first, except pages going `Newer`, which are oldest first.
    pub items: Vec<Item>,
    /// Pass as `from` with `Older` for the page before; `None` when nothing is older.
    pub older: Option<i64>,
    /// Pass as `from` with `Newer` for the page after; `None` when nothing is newer.
    pub newer: Option<i64>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    /// The viewer's tags named by the records.
    pub tags: Vec<Tag>,
    /// Set values, for their current colors; names come from the records.
    pub values: Vec<SetValue>,
    pub fields: Vec<Field>,
    pub projects: Vec<Project>,
    /// What the comments' texts refer to.
    pub refs: Refs,
    /// Files of the comments shown (CNT-018), signed with `refs.link_secret`.
    pub files: Vec<AttachmentView>,
    /// Link types named by link records, with their current names (LNK-020).
    pub link_types: Vec<LinkType>,
    /// Issues named by link records that the viewer may follow; their projects are in
    /// `projects`.
    pub linked: Vec<Linked>,
}

fn ids_of(json: Option<&Json>) -> Vec<i64> {
    match json {
        Some(Json::Array(items)) => items
            .iter()
            .filter_map(|i| read_ref(i).map(|r| r.0))
            .collect(),
        _ => Vec::new(),
    }
}

fn links_of(json: Option<&Json>) -> Vec<LoggedLink> {
    match json {
        Some(Json::Array(items)) => items.iter().filter_map(LoggedLink::read).collect(),
        _ => Vec::new(),
    }
}

fn audience_ids(json: Option<&Json>, key: &str) -> Vec<i64> {
    json.and_then(|j| j.get(key))
        .and_then(Json::as_array)
        .map(|items| items.iter().filter_map(Json::as_i64).collect())
        .unwrap_or_default()
}

/// The record kinds a page reads.
fn kinds(query: &Query) -> Vec<String> {
    let mut kinds = Vec::new();
    if query.changes {
        kinds.extend(
            ChangeKind::ALL
                .into_iter()
                .filter(|k| !k.is_comment())
                .map(|k| k.key().to_owned()),
        );
    }
    if query.comments {
        kinds.push(ChangeKind::Comment.key().to_owned());
    }
    kinds
}

/// What one issue's stream needs to filter records.
struct Context<'a> {
    viewer: &'a Access,
    issue: Issue,
    fields: ProjectFields,
    private_read: bool,
    kinds: Vec<String>,
}

/// Records of one direction that the viewer may see, with their comments.
struct Run {
    kept: Vec<(Entry, Option<Comment>)>,
    next: Option<i64>,
}

async fn run(
    conn: &mut PgConnection,
    cx: &Context<'_>,
    from: Option<i64>,
    direction: Direction,
    limit: i64,
) -> Result<Run> {
    let mut kept: Vec<(Entry, Option<Comment>)> = Vec::new();
    let mut cursor = from;
    let mut exhausted = false;
    let mut stopped_early = false;
    if limit <= 0 {
        // Nothing asked for: whatever lies beyond the cursor is still there.
        return Ok(Run { kept, next: from });
    }
    for _ in 0..ROUNDS {
        let batch = issue_history::page(
            &mut *conn,
            cx.issue.id,
            &cx.kinds,
            cursor,
            direction,
            limit + 1,
        )
        .await?;
        exhausted = i64::try_from(batch.len()).unwrap_or(i64::MAX) <= limit;
        // Tags of the batch: only the viewer's stay in tag records.
        let tag_ids: Vec<TagId> = batch
            .iter()
            .filter(|e| e.kind == ChangeKind::Tags.key())
            .flat_map(|e| {
                ids_of(e.removed.as_ref())
                    .into_iter()
                    .chain(ids_of(e.added.as_ref()))
            })
            .collect();
        let mine: Vec<TagId> = if tag_ids.is_empty() {
            Vec::new()
        } else {
            store_tags::get_many(&mut *conn, &tag_ids)
                .await?
                .into_iter()
                // Tag records follow who sees the tag (HIST-007).
                .filter(|t| t.sharing().can_view(cx.viewer))
                .map(|t| t.id)
                .collect()
        };
        // Files of the batch: records keep only those the viewer sees (ACC-049).
        let file_ids: Vec<AttachmentId> = batch
            .iter()
            .filter(|e| e.kind == ChangeKind::Attachments.key())
            .flat_map(|e| {
                ids_of(e.removed.as_ref())
                    .into_iter()
                    .chain(ids_of(e.added.as_ref()))
            })
            .collect();
        let seen_files: Vec<AttachmentId> = if file_ids.is_empty() {
            Vec::new()
        } else {
            let rows = store_attachments::get_many(&mut *conn, &file_ids).await?;
            crate::attachments::visible(&mut *conn, cx.viewer, &cx.issue, rows, true)
                .await?
                .into_iter()
                .map(|f| f.id)
                .collect()
        };
        // Issues named by link records: kept only when the viewer may follow them (HIST-007).
        let mut linked_ids: Vec<IssueId> = batch
            .iter()
            .filter(|e| e.kind == ChangeKind::Links.key())
            .flat_map(|e| {
                links_of(e.removed.as_ref())
                    .into_iter()
                    .chain(links_of(e.added.as_ref()))
            })
            .map(|l| l.issue)
            .collect();
        linked_ids.sort_unstable();
        linked_ids.dedup();
        let followed: Vec<IssueId> = if linked_ids.is_empty() {
            Vec::new()
        } else {
            let found = store_links::linked_many(&mut *conn, &linked_ids).await?;
            crate::issue_links::followable(&mut *conn, cx.viewer, found)
                .await?
                .0
                .into_iter()
                .map(|l| l.id)
                .collect()
        };
        let comment_ids: Vec<CommentId> = batch.iter().filter_map(|e| e.comment_id).collect();
        let mut batch_comments = if comment_ids.is_empty() {
            Vec::new()
        } else {
            store_comments::get_many(&mut *conn, &comment_ids).await?
        };
        let batch_len = batch.len();
        for (index, mut entry) in batch.into_iter().enumerate() {
            if i64::try_from(kept.len()).unwrap_or(i64::MAX) >= limit {
                stopped_early = index < batch_len;
                break;
            }
            cursor = Some(entry.id);
            if entry.kind == ChangeKind::Comment.key() {
                let Some(at) = batch_comments
                    .iter()
                    .position(|c| Some(c.id) == entry.comment_id)
                else {
                    continue;
                };
                let comment = batch_comments.swap_remove(at);
                if !comments::can_see(cx.viewer, &cx.issue, &comment) {
                    continue;
                }
                kept.push((entry, Some(comment)));
                continue;
            }
            if entry.kind == ChangeKind::Field.key() {
                let Some(field) = entry.field_id else {
                    continue;
                };
                let visible = cx
                    .fields
                    .get(field)
                    .is_some_and(|f| !f.private() || cx.private_read);
                if !visible && !(cx.private_read && field_exists(conn, field).await?) {
                    continue;
                }
            }
            if entry.kind == ChangeKind::Tags.key() {
                let only_mine = |json: Option<&Json>| {
                    Json::from(
                        ids_of(json)
                            .into_iter()
                            .filter(|id| mine.contains(id))
                            .collect::<Vec<_>>(),
                    )
                };
                let removed = only_mine(entry.removed.as_ref());
                let added = only_mine(entry.added.as_ref());
                let empty = |j: &Json| j.as_array().is_none_or(Vec::is_empty);
                if empty(&removed) && empty(&added) {
                    continue;
                }
                entry.removed = Some(removed);
                entry.added = Some(added);
            }
            if entry.kind == ChangeKind::Attachments.key() {
                let only_seen = |json: Option<&Json>| match json {
                    Some(Json::Array(items)) => Json::Array(
                        items
                            .iter()
                            .filter(|i| read_ref(i).is_some_and(|r| seen_files.contains(&r.0)))
                            .cloned()
                            .collect(),
                    ),
                    _ => Json::Array(Vec::new()),
                };
                let removed = only_seen(entry.removed.as_ref());
                let added = only_seen(entry.added.as_ref());
                let empty = |j: &Json| j.as_array().is_none_or(Vec::is_empty);
                if empty(&removed) && empty(&added) {
                    continue;
                }
                entry.removed = Some(removed);
                entry.added = Some(added);
            }
            if entry.kind == ChangeKind::Links.key() {
                let only_followed = |json: Option<&Json>| match json {
                    Some(Json::Array(items)) => Json::Array(
                        items
                            .iter()
                            .filter(|i| {
                                LoggedLink::read(i).is_some_and(|l| followed.contains(&l.issue))
                            })
                            .cloned()
                            .collect(),
                    ),
                    _ => Json::Array(Vec::new()),
                };
                let removed = only_followed(entry.removed.as_ref());
                let added = only_followed(entry.added.as_ref());
                let empty = |j: &Json| j.as_array().is_none_or(Vec::is_empty);
                if empty(&removed) && empty(&added) {
                    continue;
                }
                entry.removed = Some(removed);
                entry.added = Some(added);
            }
            kept.push((entry, None));
        }
        if exhausted || i64::try_from(kept.len()).unwrap_or(i64::MAX) >= limit {
            break;
        }
    }
    let next = if exhausted && !stopped_early {
        None
    } else {
        cursor
    };
    Ok(Run { kept, next })
}

/// A page of the issue's activity (HIST-001, HIST-008, HIST-010, CNT-007).
pub async fn page(
    pool: &PgPool,
    viewer: &Access,
    id: IssueId,
    query: Query,
) -> Result<ActivityPage> {
    if !query.comments && !query.changes {
        return Ok(ActivityPage::default());
    }
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, id).await?;
    let (issue, project) = super::checked(&mut conn, viewer, found).await?;
    page_of(&mut conn, viewer, issue, &project, query).await
}

/// A page of the activity of an issue already loaded and checked for the viewer.
pub(crate) async fn page_of(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: Issue,
    project: &Project,
    query: Query,
) -> Result<ActivityPage> {
    if !query.comments && !query.changes {
        return Ok(ActivityPage::default());
    }
    let cx = Context {
        viewer,
        fields: values::of_project(conn, issue.project_id).await?,
        private_read: rights::may_read_private(viewer, issue.project_id),
        kinds: kinds(&query),
        issue,
    };
    let limit = query.limit.clamp(1, PAGE_MAX);

    let (items, older, newer) = match query.around_comment {
        Some(comment) => {
            let (of_issue, record) = issue_history::posting_of(&mut *conn, comment)
                .await?
                .ok_or(Error::NotFound("no such comment"))?;
            let visible = store_comments::get(&mut *conn, comment)
                .await?
                .is_some_and(|c| comments::can_see(viewer, &cx.issue, &c));
            if of_issue != cx.issue.id || !visible || !query.comments {
                return Err(Error::NotFound("no such comment"));
            }
            // The comment and older items, then newer ones: newest first in the answer.
            let older_run = run(
                conn,
                &cx,
                Some(record + 1),
                Direction::Older,
                limit - limit / 2,
            )
            .await?;
            let newer_run = run(conn, &cx, Some(record), Direction::Newer, limit / 2).await?;
            let mut kept: Vec<(Entry, Option<Comment>)> =
                newer_run.kept.into_iter().rev().collect();
            kept.extend(older_run.kept);
            (kept, older_run.next, newer_run.next)
        }
        None => {
            let r = run(conn, &cx, query.from, query.direction, limit).await?;
            // The other way, there is more whenever the page did not start at an end.
            let edge =
                |kept: &[(Entry, Option<Comment>)]| kept.first().map(|(e, _)| e.id).or(query.from);
            match query.direction {
                Direction::Older => {
                    let newer = query.from.and_then(|_| edge(&r.kept));
                    (r.kept, r.next, newer)
                }
                Direction::Newer => {
                    let older = query.from.and_then(|_| edge(&r.kept));
                    (r.kept, older, r.next)
                }
            }
        }
    };
    let grouped = group_with_comments(conn, &cx, &items).await?;
    describe(conn, &cx, project, items, grouped, older, newer).await
}

/// For each change record, the newest comment its author posted within a minute before it that
/// the viewer sees (HIST-011). One query for the page.
async fn group_with_comments(
    conn: &mut PgConnection,
    cx: &Context<'_>,
    items: &[(Entry, Option<Comment>)],
) -> Result<Vec<Option<CommentId>>> {
    let changes: Vec<&Entry> = items
        .iter()
        .filter(|(_, c)| c.is_none())
        .map(|(e, _)| e)
        .collect();
    let (Some(first), Some(last)) = (
        changes.iter().map(|e| e.at).min(),
        changes.iter().map(|e| e.at).max(),
    ) else {
        return Ok(vec![None; items.len()]);
    };
    let mut authors: Vec<UserId> = changes.iter().map(|e| e.author_id).collect();
    authors.sort_unstable();
    authors.dedup();
    let posted: Vec<Comment> =
        store_comments::posted_between(&mut *conn, cx.issue.id, &authors, first - GROUPING, last)
            .await?
            .into_iter()
            .filter(|c| !c.is_deleted() && comments::can_see(cx.viewer, &cx.issue, c))
            .collect();
    Ok(items
        .iter()
        .map(|(entry, comment)| {
            if comment.is_some() {
                return None;
            }
            posted
                .iter()
                .filter(|c| {
                    c.author_id == entry.author_id
                        && c.created_at <= entry.at
                        && c.created_at >= entry.at - GROUPING
                })
                .max_by_key(|c| c.id)
                .map(|c| c.id)
        })
        .collect())
}

/// What the records and comments name: one query per relation.
async fn describe(
    conn: &mut PgConnection,
    cx: &Context<'_>,
    project: &Project,
    kept: Vec<(Entry, Option<Comment>)>,
    grouped: Vec<Option<CommentId>>,
    older: Option<i64>,
    newer: Option<i64>,
) -> Result<ActivityPage> {
    let mut user_ids: Vec<UserId> = kept.iter().map(|(e, _)| e.author_id).collect();
    let mut group_ids: Vec<GroupId> = Vec::new();
    let mut value_ids: Vec<i64> = Vec::new();
    let mut tag_ids: Vec<TagId> = Vec::new();
    let mut field_ids: Vec<FieldId> = Vec::new();
    let mut project_ids: Vec<ProjectId> = Vec::new();
    let mut linked_ids: Vec<IssueId> = Vec::new();
    let mut type_ids: Vec<i64> = Vec::new();
    for (e, _) in &kept {
        let both = [e.removed.as_ref(), e.added.as_ref()];
        match ChangeKind::parse(&e.kind) {
            Some(ChangeKind::Field) => {
                let Some(field) = e.field_id else { continue };
                field_ids.push(field);
                let kind = cx.fields.get(field).map(|f| f.kind.kind);
                for json in both {
                    let ids = ids_of(json);
                    match kind {
                        Some(FieldKind::User) => user_ids.extend(ids),
                        Some(FieldKind::Group) => group_ids.extend(ids),
                        Some(k) if k.set_kind().is_some() => value_ids.extend(ids),
                        _ => {}
                    }
                }
            }
            Some(ChangeKind::Tags) => {
                for json in both {
                    tag_ids.extend(ids_of(json));
                }
            }
            Some(ChangeKind::Visibility) => {
                for json in both {
                    user_ids.extend(audience_ids(json, "users"));
                    group_ids.extend(audience_ids(json, "groups"));
                }
            }
            Some(ChangeKind::Links) => {
                for json in both {
                    for l in links_of(json) {
                        linked_ids.push(l.issue);
                        type_ids.push(l.link_type);
                    }
                }
            }
            // The original a clone's creation record names (AUT-066): loaded with the linked
            // issues, kept when the viewer may follow it.
            Some(ChangeKind::Created) => {
                if let Some(id) = e.added.as_ref().and_then(clone_of) {
                    linked_ids.push(id);
                }
            }
            Some(ChangeKind::Project) => {
                for json in both {
                    if let Some(p) = json.and_then(|j| j.get("project")).and_then(Json::as_i64) {
                        project_ids.push(p);
                    }
                }
            }
            _ => {}
        }
    }
    for list in [
        &mut user_ids,
        &mut group_ids,
        &mut value_ids,
        &mut tag_ids,
        &mut field_ids,
        &mut project_ids,
        &mut linked_ids,
        &mut type_ids,
    ] {
        list.sort_unstable();
        list.dedup();
    }
    // People and groups of the records come with the comments' in one query each; relations
    // with nothing to load cost no query.
    let comment_rows: Vec<Comment> = kept.iter().filter_map(|(_, c)| c.clone()).collect();
    let described = comments::describe(
        &mut *conn,
        cx.viewer,
        &cx.issue,
        project,
        comment_rows,
        &user_ids,
        &group_ids,
    )
    .await?;
    let mut found_projects: Vec<Project> = if project_ids.is_empty() {
        Vec::new()
    } else {
        projects::get_many(&mut *conn, &project_ids)
            .await?
            .into_iter()
            .filter(|p| crate::projects::can_see(cx.viewer, p))
            .collect()
    };
    let (linked, link_types) = if linked_ids.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let found = store_links::linked_many(&mut *conn, &linked_ids).await?;
        let (kept_linked, linked_projects) =
            crate::issue_links::followable(&mut *conn, cx.viewer, found).await?;
        for p in linked_projects {
            if !found_projects.iter().any(|f| f.id == p.id) {
                found_projects.push(p);
            }
        }
        let types: Vec<LinkType> = store_links::all_types(&mut *conn)
            .await?
            .into_iter()
            .filter(|t| type_ids.contains(&t.id))
            .collect();
        (kept_linked, types)
    };
    let found_tags = if tag_ids.is_empty() {
        Vec::new()
    } else {
        store_tags::get_many(&mut *conn, &tag_ids)
            .await?
            .into_iter()
            .filter(|t| t.sharing().can_view(cx.viewer))
            .collect()
    };
    let found_values = if value_ids.is_empty() {
        Vec::new()
    } else {
        value_sets::get_values(&mut *conn, &value_ids).await?
    };
    let found_fields = if field_ids.is_empty() {
        Vec::new()
    } else {
        store_fields::get_many(&mut *conn, &field_ids).await?
    };
    let mut views = described.comments.into_iter();
    let mut items = Vec::with_capacity(kept.len());
    for ((entry, comment), after_comment) in kept.into_iter().zip(grouped) {
        match comment {
            Some(_) => {
                let Some(view) = views.next() else { continue };
                items.push(Item::Comment {
                    entry,
                    comment: Box::new(view),
                });
            }
            None => items.push(Item::Change {
                entry,
                after_comment,
            }),
        }
    }
    Ok(ActivityPage {
        items,
        older,
        newer,
        users: described.users,
        groups: described.groups,
        tags: found_tags,
        values: found_values,
        fields: found_fields,
        projects: found_projects,
        refs: described.refs,
        files: described.files,
        link_types,
        linked,
    })
}

/// Whether a field still exists (records of fields the project no longer has stay readable to
/// those who may read private fields).
async fn field_exists(conn: &mut PgConnection, field: FieldId) -> Result<bool> {
    Ok(store_fields::get(&mut *conn, field).await?.is_some())
}
