//! Tags (ISS-034…ISS-037) and the tags on issues, with the audiences of sharing (D-038).

use sqlx::{PgConnection, PgExecutor};
use taskmaster_domain::sharing::{Audience, Shared};
use taskmaster_domain::{GroupId, IssueId, TagId, UserId};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub owner_id: UserId,
    pub color: Option<i16>,
    pub version: i64,
    pub created_at: OffsetDateTime,
    pub view_users: Vec<UserId>,
    pub view_groups: Vec<GroupId>,
    pub use_users: Vec<UserId>,
    pub use_groups: Vec<GroupId>,
    pub edit_users: Vec<UserId>,
    pub edit_groups: Vec<GroupId>,
}

impl Tag {
    pub fn sharing(&self) -> Shared {
        Shared {
            owner: self.owner_id,
            view: Audience {
                users: self.view_users.clone(),
                groups: self.view_groups.clone(),
            },
            use_: Audience {
                users: self.use_users.clone(),
                groups: self.use_groups.clone(),
            },
            edit: Audience {
                users: self.edit_users.clone(),
                groups: self.edit_groups.clone(),
            },
        }
    }
}

/// Which tags a listing shows (ISS-035, ISS-037).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// The viewer's own.
    Mine,
    /// Others' tags the viewer is in an audience of.
    Shared,
    /// Both.
    All,
}

/// Unique constraint on (owner, name); a violation names it.
pub const NAME_KEY: &str = "tags_name_key";

pub async fn insert(
    conn: &mut PgConnection,
    owner: UserId,
    name: &str,
    color: Option<i16>,
) -> Result<TagId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into tags (owner_id, name, color) values ($1, $2, $3) returning id",
        owner,
        name,
        color
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: TagId) -> Result<Option<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        "select id, name, owner_id, color, version, created_at,
                view_users, view_groups, use_users, use_groups, edit_users, edit_groups
         from tags where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(db: impl PgExecutor<'_>, ids: &[TagId]) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        "select id, name, owner_id, color, version, created_at,
                view_users, view_groups, use_users, use_groups, edit_users, edit_groups
         from tags where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(conn: &mut PgConnection, id: TagId) -> Result<Option<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        "select id, name, owner_id, color, version, created_at,
                view_users, view_groups, use_users, use_groups, edit_users, edit_groups
         from tags where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// Tags by name (ISS-035: the picker filters by name), after `after` in name order. Shared ones
/// are those where the viewer is in any audience, viewing or not (using or editing a tag needs
/// to find it).
#[allow(clippy::too_many_arguments)]
pub async fn listed(
    db: impl PgExecutor<'_>,
    viewer: UserId,
    groups: &[GroupId],
    listing: Listing,
    search: &str,
    after: Option<(String, TagId)>,
    limit: i64,
) -> Result<Vec<Tag>, sqlx::Error> {
    let (after_name, after_id) = after.unwrap_or_default();
    let (mine, shared) = match listing {
        Listing::Mine => (true, false),
        Listing::Shared => (false, true),
        Listing::All => (true, true),
    };
    sqlx::query_as!(
        Tag,
        "select id, name, owner_id, color, version, created_at,
                view_users, view_groups, use_users, use_groups, edit_users, edit_groups
         from tags
         where (($3 and owner_id = $1)
                or ($4 and owner_id <> $1
                    and ($1 = any(view_users) or $1 = any(use_users) or $1 = any(edit_users)
                         or view_groups && $2 or use_groups && $2 or edit_groups && $2)))
           and strpos(lower(name), lower($5)) > 0
           and (lower(name), id) > ($6, $7)
         order by lower(name), id limit $8",
        viewer,
        groups,
        mine,
        shared,
        search,
        after_name,
        after_id,
        limit
    )
    .fetch_all(db)
    .await
}

pub async fn count_of_owner(db: impl PgExecutor<'_>, owner: UserId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from tags where owner_id = $1"#,
        owner
    )
    .fetch_one(db)
    .await
}

pub async fn update(
    conn: &mut PgConnection,
    id: TagId,
    name: &str,
    color: Option<i16>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update tags set name = $2, color = $3, version = version + 1, updated_at = now()
         where id = $1",
        id,
        name,
        color
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Replaces the owner and the audiences (validated by `domain::sharing`).
pub async fn set_sharing(
    conn: &mut PgConnection,
    id: TagId,
    shared: &Shared,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update tags set owner_id = $2, view_users = $3, view_groups = $4, use_users = $5,
                use_groups = $6, edit_users = $7, edit_groups = $8,
                version = version + 1, updated_at = now()
         where id = $1",
        id,
        shared.owner,
        &shared.view.users,
        &shared.view.groups,
        &shared.use_.users,
        &shared.use_.groups,
        &shared.edit.users,
        &shared.edit.groups
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes a tag; it leaves every issue (ISS-036).
pub async fn delete(conn: &mut PgConnection, id: TagId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from tags where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// How many issues carry the tag.
pub async fn issue_count(db: impl PgExecutor<'_>, id: TagId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from issue_tags where tag_id = $1"#,
        id
    )
    .fetch_one(db)
    .await
}

/// Tags a viewer sees on issues: their own and those shared for viewing (ISS-037).
pub async fn on_issues(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
    viewer: UserId,
    groups: &[GroupId],
) -> Result<Vec<(IssueId, Tag)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select it.issue_id, t.id, t.name, t.owner_id, t.color, t.version, t.created_at,
                t.view_users, t.view_groups, t.use_users, t.use_groups, t.edit_users,
                t.edit_groups
         from issue_tags it join tags t on t.id = it.tag_id
         where it.issue_id = any($1)
           and (t.owner_id = $2 or $2 = any(t.view_users) or t.view_groups && $3)
         order by it.issue_id, lower(t.name), t.id",
        issues,
        viewer,
        groups
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.issue_id,
                Tag {
                    id: r.id,
                    name: r.name,
                    owner_id: r.owner_id,
                    color: r.color,
                    version: r.version,
                    created_at: r.created_at,
                    view_users: r.view_users,
                    view_groups: r.view_groups,
                    use_users: r.use_users,
                    use_groups: r.use_groups,
                    edit_users: r.edit_users,
                    edit_groups: r.edit_groups,
                },
            )
        })
        .collect())
}

/// Every tag on an issue, whoever owns it: for the per-issue limit.
pub async fn count_on_issue(db: impl PgExecutor<'_>, issue: IssueId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from issue_tags where issue_id = $1"#,
        issue
    )
    .fetch_one(db)
    .await
}

/// Puts a tag on an issue; `false` when it was there.
pub async fn add(conn: &mut PgConnection, issue: IssueId, tag: TagId) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into issue_tags (issue_id, tag_id) values ($1, $2) on conflict do nothing",
        issue,
        tag
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Takes a tag off an issue; `false` when it was not there.
pub async fn remove(
    conn: &mut PgConnection,
    issue: IssueId,
    tag: TagId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "delete from issue_tags where issue_id = $1 and tag_id = $2",
        issue,
        tag
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Deleted accounts (ACC-006, D-038): their shared tags pass to the heir (renamed with the ID when
/// the heir has a tag of that name), unshared ones go; audience entries pass to the heir, never
/// listing a tag's own owner. Without an heir the shared tags stay with the deleted account.
pub async fn pass_from_deleted(
    conn: &mut PgConnection,
    owners: &[UserId],
    heir: Option<UserId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from tags where owner_id = any($1)
           and cardinality(view_users) + cardinality(view_groups) + cardinality(use_users)
             + cardinality(use_groups) + cardinality(edit_users) + cardinality(edit_groups) = 0",
        owners
    )
    .execute(&mut *conn)
    .await?;
    let Some(heir) = heir else {
        sqlx::query!(
            "update tags set
               view_users = array(select x from unnest(view_users) x where x <> all($1)),
               use_users = array(select x from unnest(use_users) x where x <> all($1)),
               edit_users = array(select x from unnest(edit_users) x where x <> all($1))
             where view_users && $1 or use_users && $1 or edit_users && $1",
            owners
        )
        .execute(&mut *conn)
        .await?;
        return Ok(());
    };
    sqlx::query!(
        "update tags t set owner_id = $2,
           name = case when exists (select from tags h
                                    where lower(h.name) = lower(t.name) and h.id <> t.id
                                      and (h.owner_id = $2
                                           or (h.owner_id = any($1) and h.id < t.id)))
                       then t.name || ' #' || t.id else t.name end
         where t.owner_id = any($1)",
        owners,
        heir
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update tags set
           view_users = array(select distinct x from unnest(
             array(select case when x = any($1) then $2 else x end from unnest(view_users) x)) x
             where x <> owner_id order by x),
           use_users = array(select distinct x from unnest(
             array(select case when x = any($1) then $2 else x end from unnest(use_users) x)) x
             where x <> owner_id order by x),
           edit_users = array(select distinct x from unnest(
             array(select case when x = any($1) then $2 else x end from unnest(edit_users) x)) x
             where x <> owner_id order by x),
           version = version + 1
         where view_users && $1 or use_users && $1 or edit_users && $1
            or (owner_id = $2
                and ($2 = any(view_users) or $2 = any(use_users) or $2 = any(edit_users)))",
        owners,
        heir
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Deleted groups: audience entries pass to the replacement group.
pub async fn replace_audience_groups(
    conn: &mut PgConnection,
    groups: &[GroupId],
    replacement: GroupId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update tags set
           view_groups = array(select distinct case when x = any($1) then $2 else x end
                               from unnest(view_groups) x order by 1),
           use_groups = array(select distinct case when x = any($1) then $2 else x end
                              from unnest(use_groups) x order by 1),
           edit_groups = array(select distinct case when x = any($1) then $2 else x end
                               from unnest(edit_groups) x order by 1),
           version = version + 1
         where view_groups && $1 or use_groups && $1 or edit_groups && $1",
        groups,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}
