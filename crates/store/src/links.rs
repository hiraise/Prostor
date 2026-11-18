//! Link types and links between issues (LNK). Rules: `taskmaster_domain::links`; design:
//! decisions.md D-034.

use sqlx::{PgConnection, PgExecutor};
use taskmaster_domain::{GroupId, IssueId, LinkId, LinkTypeId, ProjectId, UserId};
use time::OffsetDateTime;

/// Name space of the advisory locks taken here (`'lnk'`).
const LOCK_SPACE: i32 = 0x006c_6e6b;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkType {
    pub id: LinkTypeId,
    pub name: String,
    pub source_name: String,
    /// `None` for undirected types.
    pub target_name: Option<String>,
    /// `undirected`, `directed` or `hierarchy`.
    pub direction: String,
    pub builtin: Option<String>,
    pub locked: bool,
    pub deleted_at: Option<OffsetDateTime>,
    pub deleted_by: Option<UserId>,
    pub version: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl LinkType {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// Unique index on the internal names of types that are not deleted; a violation names it.
pub const NAME_KEY: &str = "link_types_name_key";

/// Every type, deleted ones included, oldest first. Types are few.
pub async fn all_types(db: impl PgExecutor<'_>) -> Result<Vec<LinkType>, sqlx::Error> {
    sqlx::query_as!(
        LinkType,
        "select id, name, source_name, target_name, direction, builtin, locked, deleted_at,
                deleted_by, version, created_at, updated_at
         from link_types order by id"
    )
    .fetch_all(db)
    .await
}

pub async fn get_type(
    db: impl PgExecutor<'_>,
    id: LinkTypeId,
) -> Result<Option<LinkType>, sqlx::Error> {
    sqlx::query_as!(
        LinkType,
        "select id, name, source_name, target_name, direction, builtin, locked, deleted_at,
                deleted_by, version, created_at, updated_at
         from link_types where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn type_by_builtin(
    db: impl PgExecutor<'_>,
    key: &str,
) -> Result<Option<LinkType>, sqlx::Error> {
    sqlx::query_as!(
        LinkType,
        "select id, name, source_name, target_name, direction, builtin, locked, deleted_at,
                deleted_by, version, created_at, updated_at
         from link_types where builtin = $1",
        key
    )
    .fetch_optional(db)
    .await
}

/// Serializes changes of type names, so two administrators never take the same name; locks the
/// type too when one is given.
pub async fn lock_types(
    conn: &mut PgConnection,
    id: Option<LinkTypeId>,
) -> Result<Option<LinkType>, sqlx::Error> {
    sqlx::query!("select pg_advisory_xact_lock($1, 0)", LOCK_SPACE)
        .execute(&mut *conn)
        .await?;
    let Some(id) = id else { return Ok(None) };
    sqlx::query_as!(
        LinkType,
        "select id, name, source_name, target_name, direction, builtin, locked, deleted_at,
                deleted_by, version, created_at, updated_at
         from link_types where id = $1 for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// Serializes changes of one hierarchy type's links, so concurrent links never give a target two
/// sources or close a loop.
pub async fn lock_hierarchy(conn: &mut PgConnection, id: LinkTypeId) -> Result<(), sqlx::Error> {
    let key = i32::try_from(id).unwrap_or(i32::MAX);
    sqlx::query!("select pg_advisory_xact_lock($1, $2)", LOCK_SPACE, key)
        .execute(conn)
        .await?;
    Ok(())
}

/// A type to create. Names are already validated.
#[derive(Debug, Clone)]
pub struct NewLinkType<'a> {
    pub name: &'a str,
    pub source_name: &'a str,
    pub target_name: Option<&'a str>,
    pub direction: &'a str,
    pub builtin: Option<&'a str>,
    pub locked: bool,
}

pub async fn insert_type(
    conn: &mut PgConnection,
    new: &NewLinkType<'_>,
) -> Result<LinkTypeId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into link_types (name, source_name, target_name, direction, builtin, locked)
         values ($1, $2, $3, $4, $5, $6) returning id",
        new.name,
        new.source_name,
        new.target_name,
        new.direction,
        new.builtin,
        new.locked,
    )
    .fetch_one(conn)
    .await
}

/// A type's names and direction as they become.
#[derive(Debug, Clone)]
pub struct TypeChange<'a> {
    pub name: &'a str,
    pub source_name: &'a str,
    pub target_name: Option<&'a str>,
    pub direction: &'a str,
}

pub async fn update_type(
    conn: &mut PgConnection,
    id: LinkTypeId,
    change: &TypeChange<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update link_types set name = $2, source_name = $3, target_name = $4, direction = $5,
                version = version + 1, updated_at = now()
         where id = $1",
        id,
        change.name,
        change.source_name,
        change.target_name,
        change.direction,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Puts a type in the trash (LNK-021, D-013); its links stay and are hidden.
pub async fn mark_type_deleted(
    conn: &mut PgConnection,
    id: LinkTypeId,
    by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update link_types set deleted_at = now(), deleted_by = $2, version = version + 1,
                updated_at = now()
         where id = $1",
        id,
        by
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn restore_type(conn: &mut PgConnection, id: LinkTypeId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update link_types set deleted_at = null, deleted_by = null, version = version + 1,
                updated_at = now()
         where id = $1",
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// How many links each type has (LNK-018).
pub async fn usage_counts(db: impl PgExecutor<'_>) -> Result<Vec<(LinkTypeId, i64)>, sqlx::Error> {
    let rows =
        sqlx::query!(r#"select type_id, count(*) as "count!" from issue_links group by type_id"#)
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(|r| (r.type_id, r.count)).collect())
}

pub async fn usage_of(db: impl PgExecutor<'_>, id: LinkTypeId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "count!" from issue_links where type_id = $1"#,
        id
    )
    .fetch_one(db)
    .await
}

/// Every link of a type as `(source, target)`: whether it can become a hierarchy.
pub async fn pairs_of_type(
    db: impl PgExecutor<'_>,
    id: LinkTypeId,
) -> Result<Vec<(IssueId, IssueId)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select source_id, target_id from issue_links where type_id = $1",
        id
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.source_id, r.target_id))
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub id: LinkId,
    pub type_id: LinkTypeId,
    pub source_id: IssueId,
    pub target_id: IssueId,
    pub created_at: OffsetDateTime,
    pub created_by: UserId,
}

pub async fn get(db: impl PgExecutor<'_>, id: LinkId) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as!(
        Link,
        "select id, type_id, source_id, target_id, created_at, created_by
         from issue_links where id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

/// The link from the issue's parent through the built-in subtask type (BRD-051).
pub async fn parent_link(
    db: impl PgExecutor<'_>,
    child: IssueId,
) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as!(
        Link,
        "select l.id, l.type_id, l.source_id, l.target_id, l.created_at, l.created_by
         from issue_links l join link_types t on t.id = l.type_id and t.builtin = 'subtask'
         where l.target_id = $1 limit 1",
        child
    )
    .fetch_optional(db)
    .await
}

/// The ID of the built-in subtask type.
pub async fn subtask_type(db: impl PgExecutor<'_>) -> Result<Option<LinkTypeId>, sqlx::Error> {
    sqlx::query_scalar!("select id from link_types where builtin = 'subtask'")
        .fetch_optional(db)
        .await
}

/// Adds links of one type, `(source, target)` each, skipping pairs that have one already: answers
/// the links made.
pub async fn insert_many(
    conn: &mut PgConnection,
    type_id: LinkTypeId,
    pairs: &[(IssueId, IssueId)],
    by: UserId,
) -> Result<Vec<Link>, sqlx::Error> {
    let sources: Vec<IssueId> = pairs.iter().map(|p| p.0).collect();
    let targets: Vec<IssueId> = pairs.iter().map(|p| p.1).collect();
    sqlx::query_as!(
        Link,
        "insert into issue_links (type_id, source_id, target_id, created_by)
         select $1, p.source, p.target, $4
         from unnest($2::bigint[], $3::bigint[]) with ordinality as p(source, target, n)
         order by p.n
         on conflict do nothing
         returning id, type_id, source_id, target_id, created_at, created_by",
        type_id,
        &sources,
        &targets,
        by
    )
    .fetch_all(conn)
    .await
}

/// Locks a type against changes while links of it are made, so a type never turns into a
/// hierarchy under links that break its rules.
pub async fn share_type(
    conn: &mut PgConnection,
    id: LinkTypeId,
) -> Result<Option<LinkType>, sqlx::Error> {
    sqlx::query_as!(
        LinkType,
        "select id, name, source_name, target_name, direction, builtin, locked, deleted_at,
                deleted_by, version, created_at, updated_at
         from link_types where id = $1 for share",
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn delete(conn: &mut PgConnection, id: LinkId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from issue_links where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// The link of a type between two issues, when they have one; the duplicates rule rewrites the
/// links it collapses (§3.8).
pub async fn between(
    db: impl PgExecutor<'_>,
    type_id: LinkTypeId,
    source: IssueId,
    target: IssueId,
) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as!(
        Link,
        "select id, type_id, source_id, target_id, created_at, created_by
         from issue_links where type_id = $1 and source_id = $2 and target_id = $3",
        type_id,
        source,
        target
    )
    .fetch_optional(db)
    .await
}

/// How many links each of some issues has, of any type; issues without links are left out.
pub async fn counts_of(
    db: impl PgExecutor<'_>,
    issues: &[IssueId],
) -> Result<Vec<(IssueId, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select e.issue as "issue!", count(*) as "count!"
           from (select source_id as issue from issue_links where source_id = any($1)
                 union all
                 select target_id from issue_links where target_id = any($1)) e
           group by e.issue"#,
        issues
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.issue, r.count)).collect())
}

/// What the rules need to know about the issue at the other end of a link, and what a row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    pub id: IssueId,
    pub project_id: ProjectId,
    /// `None` for a draft.
    pub number: Option<i64>,
    pub summary: String,
    pub reporter_id: UserId,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub resolved_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
}

/// A link seen from one issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct End {
    pub link: LinkId,
    pub type_id: LinkTypeId,
    /// This issue is the link's source.
    pub is_source: bool,
    pub created_at: OffsetDateTime,
    pub other: Linked,
}

/// Every link of an issue with the issue at the other end (LNK-003, LNK-010): one query.
pub async fn of_issue(db: impl PgExecutor<'_>, issue: IssueId) -> Result<Vec<End>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"with ends as (
             select id, type_id, true as is_source, target_id as other_id, created_at
             from issue_links where source_id = $1
             union all
             select id, type_id, false, source_id, created_at
             from issue_links where target_id = $1
           )
           select e.id as "link!", e.type_id as "type_id!", e.is_source as "is_source!",
                  e.created_at as "created_at!", o.id, o.project_id, o.number, o.summary,
                  o.reporter_id, o.audience_users, o.audience_groups, o.resolved_at, o.deleted_at
           from ends e join issues o on o.id = e.other_id"#,
        issue
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| End {
            link: r.link,
            type_id: r.type_id,
            is_source: r.is_source,
            created_at: r.created_at,
            other: Linked {
                id: r.id,
                project_id: r.project_id,
                number: r.number,
                summary: r.summary,
                reporter_id: r.reporter_id,
                audience_users: r.audience_users,
                audience_groups: r.audience_groups,
                resolved_at: r.resolved_at,
                deleted_at: r.deleted_at,
            },
        })
        .collect())
}

/// The issues at the other end of links: what history records name (HIST-005).
pub async fn linked_many(
    db: impl PgExecutor<'_>,
    ids: &[IssueId],
) -> Result<Vec<Linked>, sqlx::Error> {
    sqlx::query_as!(
        Linked,
        "select id, project_id, number, summary, reporter_id, audience_users, audience_groups,
                resolved_at, deleted_at
         from issues where id = any($1)",
        ids
    )
    .fetch_all(db)
    .await
}

/// The sources of targets in a type, as `(target, source)`.
pub async fn sources_of(
    db: impl PgExecutor<'_>,
    type_id: LinkTypeId,
    targets: &[IssueId],
) -> Result<Vec<(IssueId, IssueId)>, sqlx::Error> {
    let rows = sqlx::query!(
        "select target_id, source_id from issue_links where type_id = $1 and target_id = any($2)",
        type_id,
        targets
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.target_id, r.source_id))
        .collect())
}

/// The sources above an issue in a hierarchy type, nearest first, at most `depth` levels.
pub async fn ancestors(
    db: impl PgExecutor<'_>,
    type_id: LinkTypeId,
    issue: IssueId,
    depth: i32,
) -> Result<Vec<IssueId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"with recursive up(id, depth) as (
             select source_id, 1 from issue_links where type_id = $1 and target_id = $2
             union all
             select l.source_id, up.depth + 1
             from issue_links l join up on l.target_id = up.id
             where l.type_id = $1 and up.depth < $3
           )
           select id as "id!" from up order by depth"#,
        type_id,
        issue,
        depth
    )
    .fetch_all(db)
    .await
}

/// Subtasks of issues (LNK-013, LNK-014): `(parent, child)` through the built-in subtask type,
/// one query for a page of parents.
pub async fn children_of(
    db: impl PgExecutor<'_>,
    parents: &[IssueId],
) -> Result<Vec<(IssueId, Linked)>, sqlx::Error> {
    children_of_builtin(db, parents, "subtask").await
}

/// The duplicates of issues (§3.8): `(original, duplicate)` through the built-in duplicate
/// type — the source is the original, the target the duplicate (D-034).
pub async fn duplicates_of(
    db: impl PgExecutor<'_>,
    originals: &[IssueId],
) -> Result<Vec<(IssueId, Linked)>, sqlx::Error> {
    children_of_builtin(db, originals, "duplicate").await
}

/// The issues below parents through one built-in hierarchy type, one query for a page of parents.
async fn children_of_builtin(
    db: impl PgExecutor<'_>,
    parents: &[IssueId],
    builtin: &str,
) -> Result<Vec<(IssueId, Linked)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select l.source_id as parent, o.id, o.project_id, o.number, o.summary, o.reporter_id,
                  o.audience_users, o.audience_groups, o.resolved_at, o.deleted_at
           from issue_links l
           join link_types t on t.id = l.type_id and t.builtin = $2
           join issues o on o.id = l.target_id
           where l.source_id = any($1)"#,
        parents,
        builtin
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.parent,
                Linked {
                    id: r.id,
                    project_id: r.project_id,
                    number: r.number,
                    summary: r.summary,
                    reporter_id: r.reporter_id,
                    audience_users: r.audience_users,
                    audience_groups: r.audience_groups,
                    resolved_at: r.resolved_at,
                    deleted_at: r.deleted_at,
                },
            )
        })
        .collect())
}

/// The parent of an issue through the built-in subtask type (LNK-013).
pub async fn parent_of(
    db: impl PgExecutor<'_>,
    child: IssueId,
) -> Result<Option<Linked>, sqlx::Error> {
    sqlx::query_as!(
        Linked,
        "select o.id, o.project_id, o.number, o.summary, o.reporter_id, o.audience_users,
                o.audience_groups, o.resolved_at, o.deleted_at
         from issue_links l
         join link_types t on t.id = l.type_id and t.builtin = 'subtask'
         join issues o on o.id = l.source_id
         where l.target_id = $1
         limit 1",
        child
    )
    .fetch_optional(db)
    .await
}
