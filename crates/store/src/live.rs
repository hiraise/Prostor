//! The live event log (ISS-048, QRY-086; D-040, architecture.md §5): rows written in the
//! transaction of each change, read in ID order by the gateways of all nodes, purged after an hour;
//! and the facts guards are built from.
//!
//! IDs follow commit order: [`publish`] takes [`LIVE_LOCK`] before inserting, and the lock is held
//! until commit, so a transaction with a larger ID commits after every smaller one. A reader that
//! sees ID n knows every ID below it is final (rolled-back IDs are gaps). Publishing is the last
//! statement of a change, so the lock is held only for the commit and nothing is locked after it.

use std::time::Duration;

use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::access::visibility::Audience;
use trackflow_domain::issues::rights::Facts;
use trackflow_domain::live::{Change, EntityKind, Guard, NewEvent};
use trackflow_domain::{
    AttachmentId, BoardId, CommentId, GroupId, IssueId, LinkId, ProjectId, UserId, attachments,
    comments,
};

use crate::bus;

/// The advisory lock that orders publishing (`"live"` in ASCII).
pub const LIVE_LOCK: i64 = 0x6c69_7665;

/// An event as the log keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub id: i64,
    pub at: OffsetDateTime,
    pub event: NewEvent,
}

/// Writes the events of one transaction and tells every node the newest ID when it commits.
/// Answers that ID; nothing to write: `None`. Call it last before the commit.
pub async fn publish(
    conn: &mut PgConnection,
    events: &[NewEvent],
) -> Result<Option<i64>, sqlx::Error> {
    if events.is_empty() {
        return Ok(None);
    }
    let kinds: Vec<String> = events.iter().map(|e| e.kind.as_str().to_owned()).collect();
    let entities: Vec<i64> = events.iter().map(|e| e.entity).collect();
    let versions: Vec<i64> = events.iter().map(|e| e.version).collect();
    let changes: Vec<String> = events
        .iter()
        .map(|e| e.change.as_str().to_owned())
        .collect();
    // Arrays of arrays do not unnest row by row, so each list goes as an array literal.
    let issues: Vec<String> = events.iter().map(|e| array_literal(&e.issues)).collect();
    let projects: Vec<String> = events.iter().map(|e| array_literal(&e.projects)).collect();
    let boards: Vec<String> = events.iter().map(|e| array_literal(&e.boards)).collect();
    let patches = events
        .iter()
        .map(|e| e.patch.as_ref().map(serde_json::to_value).transpose())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let guards = events
        .iter()
        .map(|e| serde_json::to_value(&e.guard))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    sqlx::query!("select from pg_advisory_xact_lock($1)", LIVE_LOCK)
        .execute(&mut *conn)
        .await?;
    let newest = sqlx::query_scalar!(
        r#"with added as (
               insert into live_events (kind, entity_id, version, change, issues, projects,
                                        boards, patch, guard)
               select e.kind, e.entity, e.version, e.change, e.issues::bigint[],
                      e.projects::bigint[], e.boards::bigint[], e.patch, e.guard
               from unnest($1::text[], $2::bigint[], $3::bigint[], $4::text[], $5::text[],
                           $6::text[], $7::text[], $8::jsonb[], $9::jsonb[])
                    with ordinality as e(kind, entity, version, change, issues, projects, boards,
                                         patch, guard, n)
               order by e.n
               returning id)
           select max(id) as "newest!" from added"#,
        &kinds,
        &entities,
        &versions,
        &changes,
        &issues,
        &projects,
        &boards,
        &patches as &[Option<serde_json::Value>],
        &guards,
    )
    .fetch_one(&mut *conn)
    .await?;
    bus::notify(conn, bus::LIVE, &newest.to_string()).await?;
    Ok(Some(newest))
}

fn array_literal(ids: &[i64]) -> String {
    let inner: Vec<String> = ids.iter().map(i64::to_string).collect();
    format!("{{{}}}", inner.join(","))
}

struct Row {
    id: i64,
    at: OffsetDateTime,
    kind: String,
    entity_id: i64,
    version: i64,
    change: String,
    issues: Vec<IssueId>,
    projects: Vec<ProjectId>,
    boards: Vec<BoardId>,
    patch: Option<serde_json::Value>,
    guard: serde_json::Value,
}

impl TryFrom<Row> for Stored {
    type Error = sqlx::Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let malformed = |what: String| sqlx::Error::Decode(what.into());
        let kind = EntityKind::parse(&row.kind)
            .ok_or_else(|| malformed(format!("unknown live event kind {:?}", row.kind)))?;
        let change = Change::parse(&row.change)
            .ok_or_else(|| malformed(format!("unknown live event change {:?}", row.change)))?;
        let guard: Guard =
            serde_json::from_value(row.guard).map_err(|error| sqlx::Error::Decode(error.into()))?;
        let patch = row
            .patch
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| sqlx::Error::Decode(error.into()))?;
        Ok(Stored {
            id: row.id,
            at: row.at,
            event: NewEvent {
                kind,
                entity: row.entity_id,
                version: row.version,
                change,
                issues: row.issues,
                projects: row.projects,
                boards: row.boards,
                patch,
                guard,
            },
        })
    }
}

fn stored(rows: Vec<Row>) -> Result<Vec<Stored>, sqlx::Error> {
    rows.into_iter().map(Stored::try_from).collect()
}

/// Up to `limit` events after `after_id`, oldest first.
pub async fn after(
    db: impl PgExecutor<'_>,
    after_id: i64,
    limit: i64,
) -> Result<Vec<Stored>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "select id, at, kind, entity_id, version, change, issues, projects, boards, patch, guard
         from live_events where id > $1 order by id limit $2",
        after_id,
        limit
    )
    .fetch_all(db)
    .await?;
    stored(rows)
}

/// Events written since `since`, oldest first: what a node preloads into its replay buffer.
pub async fn since_time(
    db: impl PgExecutor<'_>,
    since: OffsetDateTime,
) -> Result<Vec<Stored>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "select id, at, kind, entity_id, version, change, issues, projects, boards, patch, guard
         from live_events where at >= $1 order by id",
        since
    )
    .fetch_all(db)
    .await?;
    stored(rows)
}

/// The newest event ID; 0 before the first event.
pub async fn head(db: impl PgExecutor<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(r#"select coalesce(max(id), 0) as "head!" from live_events"#)
        .fetch_one(db)
        .await
}

/// Removes events older than `older_than`, never the newest one (it keeps the head known).
/// Answers how many went.
pub async fn purge(db: impl PgExecutor<'_>, older_than: Duration) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "delete from live_events
         where at < now() - make_interval(secs => $1)
           and id < (select max(id) from live_events)",
        older_than.as_secs_f64()
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

/// What a guard needs of an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueFacts {
    pub id: IssueId,
    pub project: ProjectId,
    pub reporter: UserId,
    /// Not published yet.
    pub draft: bool,
    /// In the trash.
    pub deleted: bool,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub version: i64,
}

impl IssueFacts {
    pub fn rule(&self) -> Facts<'_> {
        Facts {
            project: self.project,
            reporter: self.reporter,
            draft: self.draft,
            audience: Audience {
                users: &self.audience_users,
                groups: &self.audience_groups,
            },
        }
    }
}

/// What a guard needs of a comment and its issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentFacts {
    pub id: CommentId,
    pub issue: IssueFacts,
    pub author: UserId,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub version: i64,
}

impl CommentFacts {
    pub fn rule(&self) -> comments::Facts<'_> {
        comments::Facts {
            project: self.issue.project,
            author: self.author,
            audience: Audience {
                users: &self.audience_users,
                groups: &self.audience_groups,
            },
        }
    }
}

/// The author and audience of the comment a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerComment {
    pub author: UserId,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
}

/// What a guard needs of a file, its issue and its comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentFacts {
    pub id: AttachmentId,
    pub issue: IssueFacts,
    pub comment: Option<OwnerComment>,
    pub author: UserId,
    pub audience_users: Vec<UserId>,
    pub audience_groups: Vec<GroupId>,
    pub version: i64,
}

impl AttachmentFacts {
    pub fn rule(&self) -> attachments::Facts<'_> {
        attachments::Facts {
            project: self.issue.project,
            author: self.author,
            audience: Audience {
                users: &self.audience_users,
                groups: &self.audience_groups,
            },
        }
    }

    pub fn comment_rule(&self) -> Option<comments::Facts<'_>> {
        self.comment.as_ref().map(|comment| comments::Facts {
            project: self.issue.project,
            author: comment.author,
            audience: Audience {
                users: &comment.audience_users,
                groups: &comment.audience_groups,
            },
        })
    }
}

/// What a guard needs of a link: both ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkFacts {
    pub id: LinkId,
    pub source: IssueFacts,
    pub target: IssueFacts,
}

/// Facts of these issues, in no particular order; missing ones are left out.
pub async fn issue_facts(
    db: impl PgExecutor<'_>,
    ids: &[IssueId],
) -> Result<Vec<IssueFacts>, sqlx::Error> {
    sqlx::query_as!(
        IssueFacts,
        r#"select id, project_id as project, reporter_id as reporter, number is null as "draft!",
                  deleted_at is not null as "deleted!", audience_users, audience_groups, version
           from issues where id = any($1)"#,
        ids
    )
    .fetch_all(db)
    .await
}

/// Facts of these comments with their issues; missing ones are left out.
pub async fn comment_facts(
    db: impl PgExecutor<'_>,
    ids: &[CommentId],
) -> Result<Vec<CommentFacts>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select c.id, c.author_id, c.audience_users, c.audience_groups, c.version,
                  i.id as issue_id, i.project_id, i.reporter_id, i.number is null as "draft!",
                  i.deleted_at is not null as "deleted!", i.audience_users as issue_users,
                  i.audience_groups as issue_groups, i.version as issue_version
           from comments c join issues i on i.id = c.issue_id
           where c.id = any($1)"#,
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CommentFacts {
            id: r.id,
            issue: IssueFacts {
                id: r.issue_id,
                project: r.project_id,
                reporter: r.reporter_id,
                draft: r.draft,
                deleted: r.deleted,
                audience_users: r.issue_users,
                audience_groups: r.issue_groups,
                version: r.issue_version,
            },
            author: r.author_id,
            audience_users: r.audience_users,
            audience_groups: r.audience_groups,
            version: r.version,
        })
        .collect())
}

/// Facts of these files with their issues and comments; missing ones are left out.
pub async fn attachment_facts(
    db: impl PgExecutor<'_>,
    ids: &[AttachmentId],
) -> Result<Vec<AttachmentFacts>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select a.id as "id!", a.author_id as "author_id!",
                  a.audience_users as "audience_users!", a.audience_groups as "audience_groups!",
                  a.version as "version!",
                  k.author_id as "comment_author?", k.audience_users as "comment_users?",
                  k.audience_groups as "comment_groups?",
                  i.id as "issue_id!", i.project_id as "project_id!",
                  i.reporter_id as "reporter_id!", i.number is null as "draft!",
                  i.deleted_at is not null as "deleted!", i.audience_users as "issue_users!",
                  i.audience_groups as "issue_groups!", i.version as "issue_version!"
           from attachments a
                join issues i on i.id = a.issue_id
                left join comments k on k.id = a.comment_id
           where a.id = any($1)"#,
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AttachmentFacts {
            id: r.id,
            issue: IssueFacts {
                id: r.issue_id,
                project: r.project_id,
                reporter: r.reporter_id,
                draft: r.draft,
                deleted: r.deleted,
                audience_users: r.issue_users,
                audience_groups: r.issue_groups,
                version: r.issue_version,
            },
            comment: r.comment_author.map(|author| OwnerComment {
                author,
                audience_users: r.comment_users.unwrap_or_default(),
                audience_groups: r.comment_groups.unwrap_or_default(),
            }),
            author: r.author_id,
            audience_users: r.audience_users,
            audience_groups: r.audience_groups,
            version: r.version,
        })
        .collect())
}

/// Facts of these links with both ends; missing ones are left out.
pub async fn link_facts(
    db: impl PgExecutor<'_>,
    ids: &[LinkId],
) -> Result<Vec<LinkFacts>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select l.id,
                  s.id as s_id, s.project_id as s_project, s.reporter_id as s_reporter,
                  s.number is null as "s_draft!", s.deleted_at is not null as "s_deleted!",
                  s.audience_users as s_users, s.audience_groups as s_groups,
                  s.version as s_version,
                  t.id as t_id, t.project_id as t_project, t.reporter_id as t_reporter,
                  t.number is null as "t_draft!", t.deleted_at is not null as "t_deleted!",
                  t.audience_users as t_users, t.audience_groups as t_groups,
                  t.version as t_version
           from issue_links l
                join issues s on s.id = l.source_id
                join issues t on t.id = l.target_id
           where l.id = any($1)"#,
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| LinkFacts {
            id: r.id,
            source: IssueFacts {
                id: r.s_id,
                project: r.s_project,
                reporter: r.s_reporter,
                draft: r.s_draft,
                deleted: r.s_deleted,
                audience_users: r.s_users,
                audience_groups: r.s_groups,
                version: r.s_version,
            },
            target: IssueFacts {
                id: r.t_id,
                project: r.t_project,
                reporter: r.t_reporter,
                draft: r.t_draft,
                deleted: r.t_deleted,
                audience_users: r.t_users,
                audience_groups: r.t_groups,
                version: r.t_version,
            },
        })
        .collect())
}
