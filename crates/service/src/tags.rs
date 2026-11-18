//! Tags (ISS-034…ISS-037): owned by one person, shared for viewing, using and editing by the
//! rules of `domain::sharing` (D-038). Putting a tag on an issue or taking it off needs use
//! rights and the right to update the issue. Deleting a tag takes it off every issue.

use prostor_domain::access::Access;
use prostor_domain::issues::history::{Origin, Record};
use prostor_domain::issues::rights;
use prostor_domain::issues::{self as domain, TAGS_PER_ISSUE_MAX, TAGS_PER_OWNER_MAX};
use prostor_domain::sharing::{Audience, Shared};
use prostor_domain::validation::{Problem, Problems};
use prostor_domain::{Actor, IssueId, TagId, UserId};
use prostor_store::issues::Issue;
use prostor_store::tags::{self, Listing, Tag};
use prostor_store::{PgPool, audit, groups, unique_violation, users};
use serde_json::json;
use sqlx::PgConnection;

use crate::issues::{self, One};
use crate::live::Batch;
use crate::{Error, Result};

const SEARCH_MAX: usize = 100;

fn taken(error: sqlx::Error) -> Error {
    match unique_violation(&error) {
        Some(tags::NAME_KEY) => {
            Error::field("name", Problem::Taken, "you have a tag with this name")
        }
        _ => error.into(),
    }
}

/// Tags by name, filtered by `text` (ISS-035): the viewer's own, others' shared with them, or both.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    listing: Listing,
    text: &str,
    after: Option<(String, TagId)>,
    limit: i64,
) -> Result<Vec<Tag>> {
    let text: String = text.trim().chars().take(SEARCH_MAX).collect();
    Ok(tags::listed(
        pool,
        viewer.user(),
        viewer.groups(),
        listing,
        &text,
        after,
        limit,
    )
    .await?)
}

/// A tag the viewer knows of: owns it, is in an audience, or administers users. Others get "not
/// found".
async fn known(conn: &mut PgConnection, viewer: &Access, id: TagId) -> Result<(Tag, Shared)> {
    let tag = tags::lock(conn, id)
        .await?
        .ok_or(Error::NotFound("no such tag"))?;
    let shared = tag.sharing();
    let knows = shared.can_view(viewer)
        || shared.can_use(viewer)
        || shared.can_edit(viewer)
        || shared.may_delete(viewer);
    if !knows {
        return Err(Error::NotFound("no such tag"));
    }
    Ok((tag, shared))
}

/// One tag the viewer knows (owns, or it is shared with them in any way): the list's context.
pub async fn get(pool: &PgPool, viewer: &Access, id: TagId) -> Result<Tag> {
    let mut tx = pool.begin().await?;
    let (tag, _) = known(&mut tx, viewer, id).await?;
    tx.commit().await?;
    Ok(tag)
}

/// Creates a tag for the viewer (ISS-034).
pub async fn create(pool: &PgPool, viewer: &Access, name: &str, color: Option<i16>) -> Result<Tag> {
    let mut problems = Problems::new();
    let name = problems.check("name", domain::tag_name(name));
    let color = problems.check("color", domain::tag_color(color));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(color)) = (name, color) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    let mut tx = pool.begin().await?;
    if tags::count_of_owner(&mut *tx, viewer.user()).await?
        >= i64::try_from(TAGS_PER_OWNER_MAX).unwrap_or(i64::MAX)
    {
        return Err(Error::Conflict("you have too many tags"));
    }
    let id = tags::insert(&mut tx, viewer.user(), &name, color)
        .await
        .map_err(taken)?;
    tx.commit().await?;
    tags::get(pool, id)
        .await?
        .ok_or(Error::NotFound("no such tag"))
}

/// A change of a tag; `None` keeps a value, `color: Some(None)` clears the color.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagChange {
    pub name: Option<String>,
    pub color: Option<Option<i16>>,
    /// New audiences (ISS-037): the owner or a user administrator, with share-views (ACC-066).
    pub sharing: Option<Sharing>,
    /// A new owner: user administrators with share-views.
    pub owner: Option<UserId>,
    pub version: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sharing {
    pub view: Audience,
    pub use_: Audience,
    pub edit: Audience,
}

/// Changes a tag (ISS-036, ISS-037). Editors change the name and color only.
pub async fn update(pool: &PgPool, viewer: &Access, id: TagId, change: TagChange) -> Result<Tag> {
    let mut problems = Problems::new();
    let name = change
        .name
        .as_deref()
        .and_then(|n| problems.check("name", domain::tag_name(n)));
    let color = match change.color {
        Some(c) => problems.check("color", domain::tag_color(c)),
        None => None,
    };
    problems.finish().map_err(Error::Invalid)?;
    let mut tx = pool.begin().await?;
    let (tag, shared) = known(&mut tx, viewer, id).await?;
    if tag.version != change.version {
        return Err(Error::Conflict("the tag was changed by someone else"));
    }
    if (change.name.is_some() || change.color.is_some()) && !shared.can_edit(viewer) {
        return Err(Error::Refused("you may not edit this tag".into()));
    }
    if change.name.is_some() || change.color.is_some() {
        tags::update(
            &mut tx,
            id,
            name.as_deref().unwrap_or(&tag.name),
            color.unwrap_or(tag.color),
        )
        .await
        .map_err(taken)?;
    }
    if change.sharing.is_some() || change.owner.is_some() {
        let next = sharing_change(
            &mut tx,
            viewer,
            &shared,
            change.sharing.as_ref(),
            change.owner,
        )
        .await?;
        if next != shared {
            tags::set_sharing(&mut tx, id, &next).await.map_err(taken)?;
            audit::record(
                &mut tx,
                Actor::User(viewer.user()),
                "tag.shared",
                &format!("tag:{id}"),
                json!({
                    "owner": next.owner,
                    "view": { "users": next.view.users, "groups": next.view.groups },
                    "use": { "users": next.use_.users, "groups": next.use_.groups },
                    "edit": { "users": next.edit.users, "groups": next.edit.groups },
                }),
            )
            .await?;
        }
    }
    tx.commit().await?;
    tags::get(pool, id)
        .await?
        .ok_or(Error::NotFound("no such tag"))
}

/// The owner and audiences after a change, checked by the rules of D-038.
pub(crate) async fn sharing_change(
    conn: &mut PgConnection,
    viewer: &Access,
    shared: &Shared,
    sharing: Option<&Sharing>,
    owner: Option<UserId>,
) -> Result<Shared> {
    let mut next = shared.clone();
    if let Some(owner) = owner.filter(|o| *o != shared.owner) {
        shared.may_transfer(viewer)?;
        let found = users::get_many(&mut *conn, &[owner]).await?;
        if !found
            .iter()
            .any(|u| u.id == owner && !u.is_deleted() && !u.is_guest())
        {
            return Err(Error::field("owner", Problem::Invalid, "no such account"));
        }
        next.owner = owner;
    }
    let changed = sharing.is_some();
    let sharing = match sharing {
        Some(sharing) => {
            shared.may_share(viewer)?;
            sharing.clone()
        }
        None => Sharing {
            view: shared.view.clone(),
            use_: shared.use_.clone(),
            edit: shared.edit.clone(),
        },
    };
    let mut problems = Problems::new();
    let view = problems.check("sharing.view", sharing.view.validated(next.owner));
    let use_ = problems.check("sharing.use", sharing.use_.validated(next.owner));
    let edit = problems.check("sharing.edit", sharing.edit.validated(next.owner));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(view), Some(use_), Some(edit)) = (view, use_, edit) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };
    if changed {
        check_audiences(conn, viewer, &[&view, &use_, &edit]).await?;
    }
    next.view = view;
    next.use_ = use_;
    next.edit = edit;
    Ok(next)
}

/// Every account is active and every group visible to the one sharing (no loop: one load each).
pub(crate) async fn check_audiences(
    conn: &mut PgConnection,
    viewer: &Access,
    lists: &[&Audience],
) -> Result<()> {
    let mut user_ids: Vec<UserId> = lists.iter().flat_map(|a| a.users.iter().copied()).collect();
    user_ids.sort_unstable();
    user_ids.dedup();
    let mut group_ids: Vec<i64> = lists
        .iter()
        .flat_map(|a| a.groups.iter().copied())
        .collect();
    group_ids.sort_unstable();
    group_ids.dedup();
    if !user_ids.is_empty() {
        let found = users::get_many(&mut *conn, &user_ids).await?;
        if user_ids.iter().any(|id| {
            !found
                .iter()
                .any(|u| u.id == *id && !u.is_deleted() && !u.is_guest())
        }) {
            return Err(Error::field("sharing", Problem::Invalid, "no such account"));
        }
    }
    if !group_ids.is_empty() {
        let found = groups::get_many(&mut *conn, &group_ids).await?;
        if group_ids.iter().any(|id| {
            !found
                .iter()
                .any(|g| g.id == *id && crate::groups::team_visible(viewer, g))
        }) {
            return Err(Error::field("sharing", Problem::Invalid, "no such group"));
        }
    }
    Ok(())
}

/// How many issues carry a tag: shown before deleting it.
pub async fn usage(pool: &PgPool, viewer: &Access, id: TagId) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let (_, shared) = known(&mut tx, viewer, id).await?;
    if !shared.may_delete(viewer) {
        return Err(Error::Refused(
            "only the owner or a user administrator deletes a tag".into(),
        ));
    }
    Ok(tags::issue_count(&mut *tx, id).await?)
}

/// Deletes a tag; it leaves every issue (ISS-036). Its owner or a user administrator.
pub async fn delete(pool: &PgPool, viewer: &Access, id: TagId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (tag, shared) = known(&mut tx, viewer, id).await?;
    if !shared.may_delete(viewer) {
        return Err(Error::Refused(
            "only the owner or a user administrator deletes a tag".into(),
        ));
    }
    tags::delete(&mut tx, id).await?;
    if tag.owner_id != viewer.user() {
        audit::record(
            &mut tx,
            Actor::User(viewer.user()),
            "tag.deleted",
            &format!("tag:{id}"),
            json!({ "name": tag.name, "owner": tag.owner_id }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The tag write itself on the caller's connection (plan §4.3): the same checks as
/// [`set_on_issue`], the add or remove, and the history record for the caller's change —
/// `None` when the tag was already as asked. The issue is locked.
pub(crate) async fn set_on_issue_conn(
    conn: &mut PgConnection,
    viewer: &Access,
    issue: &Issue,
    tag_id: TagId,
    on: bool,
) -> Result<Option<Record>> {
    rights::may_update(viewer, &issues::facts(issue))?;
    let tag = tags::get(&mut *conn, tag_id)
        .await?
        .filter(|t| t.sharing().can_use(viewer))
        .ok_or(Error::NotFound("no such tag"))?;
    let changed = if on {
        if tags::count_on_issue(&mut *conn, issue.id).await?
            >= i64::try_from(TAGS_PER_ISSUE_MAX).unwrap_or(i64::MAX)
        {
            return Err(Error::Conflict("the issue has too many tags"));
        }
        tags::add(&mut *conn, issue.id, tag.id).await?
    } else {
        tags::remove(&mut *conn, issue.id, tag.id).await?
    };
    Ok(if !changed {
        None
    } else if on {
        Record::tags(&[], &[tag.id])
    } else {
        Record::tags(&[tag.id], &[])
    })
}

/// Puts a tag on an issue or takes it off (ISS-035, ISS-037: own or shared for use), with a
/// history record.
pub async fn set_on_issue(
    pool: &PgPool,
    viewer: &Access,
    origin: Origin,
    issue_id: IssueId,
    tag_id: TagId,
    on: bool,
) -> Result<One> {
    let mut tx = pool.begin().await?;
    let (issue, _) = issues::lock_for_change(&mut tx, viewer, issue_id).await?;
    if let Some(record) = set_on_issue_conn(&mut tx, viewer, &issue, tag_id, on).await? {
        let author = issues::author(viewer, origin, false, issue.project_id)?;
        let mut batch = Batch::new();
        issues::finish_change(&mut tx, &mut batch, &issue, author, &[record], &[]).await?;
        batch.publish(&mut tx).await?;
    }
    tx.commit().await?;
    issues::get(pool, viewer, issue_id).await
}

/// The people and groups named by the sharing of several items, loaded at once: owners and every
/// audience, for answers that show them. Deleted accounts come back as they are.
#[derive(Debug, Clone, Default)]
pub struct Parties {
    pub users: Vec<users::User>,
    pub groups: Vec<groups::Group>,
}

pub async fn parties(pool: &PgPool, shared: &[Shared]) -> Result<Parties> {
    let mut user_ids: Vec<UserId> = Vec::new();
    let mut group_ids = Vec::new();
    for s in shared {
        user_ids.push(s.owner);
        for a in [&s.view, &s.use_, &s.edit] {
            user_ids.extend(&a.users);
            group_ids.extend(&a.groups);
        }
    }
    user_ids.sort_unstable();
    user_ids.dedup();
    group_ids.sort_unstable();
    group_ids.dedup();
    let mut conn = pool.acquire().await?;
    let users = users::get_many(&mut *conn, &user_ids).await?;
    let groups = if group_ids.is_empty() {
        Vec::new()
    } else {
        groups::get_many(&mut *conn, &group_ids).await?
    };
    Ok(Parties { users, groups })
}
