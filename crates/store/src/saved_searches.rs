//! Saved searches (QRY-063…QRY-068, PRJ-019) with the audiences of sharing (D-038). Hidden rows
//! are deleted predefined searches: no read here returns them.

use prostor_domain::links::Side;
use prostor_domain::search::stored::{Names, Refs};
use prostor_domain::sharing::{Audience, Shared};
use prostor_domain::{GroupId, ProjectId, UserId};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;

pub type SavedSearchId = i64;

#[derive(Debug, Clone, PartialEq)]
pub struct SavedSearch {
    pub id: SavedSearchId,
    pub owner_id: UserId,
    pub name: String,
    /// The stored form (`domain::search::stored::Stored`).
    pub query: Value,
    /// Key of a predefined search (QRY-068).
    pub builtin: Option<String>,
    /// The project of an "unassigned" search (PRJ-019).
    pub project_id: Option<ProjectId>,
    pub view_users: Vec<UserId>,
    pub view_groups: Vec<GroupId>,
    pub edit_users: Vec<UserId>,
    pub edit_groups: Vec<GroupId>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub version: i64,
}

impl SavedSearch {
    /// Viewing a saved search is using it: `use_` stays empty.
    pub fn sharing(&self) -> Shared {
        Shared {
            owner: self.owner_id,
            view: Audience {
                users: self.view_users.clone(),
                groups: self.view_groups.clone(),
            },
            use_: Audience::default(),
            edit: Audience {
                users: self.edit_users.clone(),
                groups: self.edit_groups.clone(),
            },
        }
    }
}

/// Which saved searches a listing shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    Mine,
    /// Others' searches shared with the viewer for viewing.
    Shared,
    All,
}

/// Unique constraint on (owner, lower(name)) of visible searches.
pub const NAME_KEY: &str = "saved_searches_name_key";

/// A new row; `builtin` and `project` only for the searches the system creates.
pub struct NewSavedSearch<'a> {
    pub owner: UserId,
    pub name: &'a str,
    pub query: &'a Value,
    pub builtin: Option<&'a str>,
    pub project: Option<ProjectId>,
}

pub async fn insert(
    conn: &mut PgConnection,
    new: &NewSavedSearch<'_>,
) -> Result<SavedSearchId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into saved_searches (owner_id, name, query, builtin, project_id)
         values ($1, $2, $3, $4, $5) returning id",
        new.owner,
        new.name,
        new.query,
        new.builtin,
        new.project
    )
    .fetch_one(conn)
    .await
}

/// Predefined searches the owner has had, hidden ones too (QRY-068: a hidden one is not created
/// again).
pub async fn builtin_keys(
    db: impl PgExecutor<'_>,
    owner: UserId,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select builtin as "builtin!" from saved_searches
           where owner_id = $1 and builtin is not null"#,
        owner
    )
    .fetch_all(db)
    .await
}

/// Creates a predefined search unless the owner has it or has a search of that name; `false`
/// when nothing was created (two first requests at once, or the name is taken).
pub async fn ensure_builtin(
    conn: &mut PgConnection,
    owner: UserId,
    key: &str,
    name: &str,
    query: &Value,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into saved_searches (owner_id, name, query, builtin)
         values ($1, $2, $3, $4) on conflict do nothing",
        owner,
        name,
        query,
        key
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn get(
    db: impl PgExecutor<'_>,
    id: SavedSearchId,
) -> Result<Option<SavedSearch>, sqlx::Error> {
    sqlx::query_as!(
        SavedSearch,
        "select id, owner_id, name, query, builtin, project_id, view_users, view_groups,
                edit_users, edit_groups, created_at, updated_at, version
         from saved_searches where id = $1 and hidden_at is null",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn get_many(
    db: impl PgExecutor<'_>,
    ids: &[SavedSearchId],
) -> Result<Vec<SavedSearch>, sqlx::Error> {
    sqlx::query_as!(
        SavedSearch,
        "select id, owner_id, name, query, builtin, project_id, view_users, view_groups,
                edit_users, edit_groups, created_at, updated_at, version
         from saved_searches where id = any($1) and hidden_at is null
         order by id",
        ids
    )
    .fetch_all(db)
    .await
}

pub async fn lock(
    conn: &mut PgConnection,
    id: SavedSearchId,
) -> Result<Option<SavedSearch>, sqlx::Error> {
    sqlx::query_as!(
        SavedSearch,
        "select id, owner_id, name, query, builtin, project_id, view_users, view_groups,
                edit_users, edit_groups, created_at, updated_at, version
         from saved_searches where id = $1 and hidden_at is null for update",
        id
    )
    .fetch_optional(conn)
    .await
}

/// The viewer's searches and those shared with them for viewing, by name after `after`.
pub async fn listed(
    db: impl PgExecutor<'_>,
    viewer: UserId,
    groups: &[GroupId],
    listing: Listing,
    after: Option<(String, SavedSearchId)>,
    limit: i64,
) -> Result<Vec<SavedSearch>, sqlx::Error> {
    let (after_name, after_id) = after.unwrap_or_default();
    let (mine, shared) = match listing {
        Listing::Mine => (true, false),
        Listing::Shared => (false, true),
        Listing::All => (true, true),
    };
    sqlx::query_as!(
        SavedSearch,
        "select id, owner_id, name, query, builtin, project_id, view_users, view_groups,
                edit_users, edit_groups, created_at, updated_at, version
         from saved_searches
         where hidden_at is null
           and (($3 and owner_id = $1)
                or ($4 and owner_id <> $1 and ($1 = any(view_users) or view_groups && $2)))
           and (lower(name), id) > ($5, $6)
         order by lower(name), id limit $7",
        viewer,
        groups,
        mine,
        shared,
        after_name,
        after_id,
        limit
    )
    .fetch_all(db)
    .await
}

/// Visible searches of an owner, for the per-owner limit.
pub async fn count_of_owner(db: impl PgExecutor<'_>, owner: UserId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from saved_searches where owner_id = $1 and hidden_at is null"#,
        owner
    )
    .fetch_one(db)
    .await
}

pub async fn update(
    conn: &mut PgConnection,
    id: SavedSearchId,
    name: &str,
    query: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update saved_searches set name = $2, query = $3, version = version + 1,
                updated_at = now()
         where id = $1",
        id,
        name,
        query
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Replaces the owner and the audiences (validated by `domain::sharing`).
pub async fn set_sharing(
    conn: &mut PgConnection,
    id: SavedSearchId,
    shared: &Shared,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update saved_searches set owner_id = $2, view_users = $3, view_groups = $4,
                edit_users = $5, edit_groups = $6, version = version + 1, updated_at = now()
         where id = $1",
        id,
        shared.owner,
        &shared.view.users,
        &shared.view.groups,
        &shared.edit.users,
        &shared.edit.groups
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A deleted predefined search stays as a hidden row (QRY-068).
pub async fn hide(conn: &mut PgConnection, id: SavedSearchId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update saved_searches set hidden_at = now(), version = version + 1, updated_at = now()
         where id = $1",
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: SavedSearchId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from saved_searches where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Deleted accounts (D-038): their own searches pass to the heir (renamed with the ID on a name
/// clash), their predefined ones go (the heir has their own); audience entries pass to the heir,
/// never listing a search's own owner. Without an heir the searches stay with the deleted account
/// and the audience entries go.
pub async fn pass_from_deleted(
    conn: &mut PgConnection,
    owners: &[UserId],
    heir: Option<UserId>,
) -> Result<(), sqlx::Error> {
    let Some(heir) = heir else {
        sqlx::query!(
            "update saved_searches set
               view_users = array(select x from unnest(view_users) x where x <> all($1)),
               edit_users = array(select x from unnest(edit_users) x where x <> all($1)),
               version = version + 1
             where view_users && $1 or edit_users && $1",
            owners
        )
        .execute(&mut *conn)
        .await?;
        return Ok(());
    };
    sqlx::query!(
        "delete from saved_searches where owner_id = any($1) and builtin is not null",
        owners
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update saved_searches s set owner_id = $2,
           name = case when exists (select from saved_searches h
                                    where lower(h.name) = lower(s.name) and h.id <> s.id
                                      and h.hidden_at is null
                                      and (h.owner_id = $2
                                           or (h.owner_id = any($1) and h.id < s.id)))
                       then s.name || ' #' || s.id else s.name end,
           version = version + 1
         where s.owner_id = any($1)",
        owners,
        heir
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update saved_searches set
           view_users = array(select distinct x from unnest(
             array(select case when x = any($1) then $2 else x end from unnest(view_users) x)) x
             where x <> owner_id order by x),
           edit_users = array(select distinct x from unnest(
             array(select case when x = any($1) then $2 else x end from unnest(edit_users) x)) x
             where x <> owner_id order by x),
           version = version + 1
         where view_users && $1 or edit_users && $1
            or (owner_id = $2 and ($2 = any(view_users) or $2 = any(edit_users)))",
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
        "update saved_searches set
           view_groups = array(select distinct case when x = any($1) then $2 else x end
                               from unnest(view_groups) x order by 1),
           edit_groups = array(select distinct case when x = any($1) then $2 else x end
                               from unnest(edit_groups) x order by 1),
           version = version + 1
         where view_groups && $1 or edit_groups && $1",
        groups,
        replacement
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The ID of the field of the default set `assignee` attached to a project (PRJ-019), with its
/// name.
pub async fn assignee_field(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Option<(i64, String)>, sqlx::Error> {
    let row = sqlx::query!(
        "select f.id, f.name from project_fields pf join fields f on f.id = pf.field_id
         where pf.project_id = $1 and f.builtin = 'assignee'",
        project
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.id, r.name)))
}

/// The name of the field of the default set `assignee`, wherever it is attached.
pub async fn assignee_field_name(db: impl PgExecutor<'_>) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("select name from fields where builtin = 'assignee'")
        .fetch_optional(db)
        .await
}

/// The current names of what stored queries name, as a reader sees them: tags and boards (with
/// their sprints) only when the reader sees them, deleted accounts and link types are gone. One
/// query per kind named.
pub async fn names(
    conn: &mut PgConnection,
    refs: &Refs,
    viewer: &crate::boards::Viewer<'_>,
) -> Result<Names, sqlx::Error> {
    let (me, groups) = (viewer.me, viewer.groups);
    let mut names = Names::default();
    if !refs.fields.is_empty() {
        for r in sqlx::query!(
            "select id, name from fields where id = any($1)",
            &refs.fields
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.fields.insert(r.id, r.name);
        }
    }
    if !refs.values.is_empty() {
        for r in sqlx::query!(
            "select id, name from field_values where id = any($1)",
            &refs.values
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.values.insert(r.id, r.name);
        }
    }
    if !refs.users.is_empty() {
        for r in sqlx::query!(
            "select id, username from users where id = any($1) and deleted_at is null",
            &refs.users
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.users.insert(r.id, r.username);
        }
    }
    if !refs.groups.is_empty() {
        for r in sqlx::query!(
            "select id, name from groups where id = any($1)",
            &refs.groups
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.groups.insert(r.id, r.name);
        }
    }
    if !refs.projects.is_empty() {
        for r in sqlx::query!(
            "select id, key from projects where id = any($1) and deleting_at is null",
            &refs.projects
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.projects.insert(r.id, r.key);
        }
    }
    if !refs.tags.is_empty() {
        for r in sqlx::query!(
            "select id, name from tags
             where id = any($1) and (owner_id = $2 or $2 = any(view_users) or view_groups && $3)",
            &refs.tags,
            me,
            groups
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.tags.insert(r.id, r.name);
        }
    }
    if !refs.link_types.is_empty() {
        for r in sqlx::query!(
            "select id, source_name, target_name from link_types
             where id = any($1) and deleted_at is null",
            &refs.link_types
        )
        .fetch_all(&mut *conn)
        .await?
        {
            if let Some(target) = r.target_name {
                names.link_sides.insert((r.id, Side::Target), target);
            }
            names.link_sides.insert((r.id, Side::Source), r.source_name);
        }
    }
    if !refs.boards.is_empty() || !refs.sprints.is_empty() {
        let mut boards = refs.boards.clone();
        boards.extend(
            sqlx::query_scalar!(
                "select distinct board_id from sprints where id = any($1)",
                &refs.sprints
            )
            .fetch_all(&mut *conn)
            .await?,
        );
        boards.sort_unstable();
        boards.dedup();
        let visible = crate::boards::get_many(&mut *conn, &boards).await?;
        let visible: Vec<_> = visible.into_iter().filter(|b| viewer.views(b)).collect();
        for b in &visible {
            names.boards.insert(b.id, b.name.clone());
        }
        let ids: Vec<_> = visible.iter().map(|b| b.id).collect();
        for r in sqlx::query!(
            "select id, name from sprints where id = any($1) and board_id = any($2)",
            &refs.sprints,
            &ids
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.sprints.insert(r.id, r.name);
        }
    }
    if !refs.issues.is_empty() {
        for r in sqlx::query!(
            r#"select i.id, p.key || '-' || i.number as "readable!"
               from issues i join projects p on p.id = i.project_id
               where i.id = any($1) and i.number is not null"#,
            &refs.issues
        )
        .fetch_all(&mut *conn)
        .await?
        {
            names.issues.insert(r.id, r.readable);
        }
    }
    Ok(names)
}

/// The manual order of a saved search (BRD-093, the backlog's priority): ranked issues first by
/// `(rank, issue_id)`, the rest after them in the query's sort.
pub mod ranks {
    use prostor_domain::{BoardId, IssueId};
    use sqlx::{PgConnection, PgExecutor};

    use super::SavedSearchId;

    /// The rank of the issue in the order, if it has one.
    pub async fn of(
        db: impl PgExecutor<'_>,
        search: SavedSearchId,
        issue: IssueId,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar!(
            "select rank from saved_search_ranks where search_id = $1 and issue_id = $2",
            search,
            issue
        )
        .fetch_optional(db)
        .await
    }

    /// The highest rank, if any issue is ranked.
    pub async fn last(
        db: impl PgExecutor<'_>,
        search: SavedSearchId,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"select max(rank) as "rank" from saved_search_ranks where search_id = $1"#,
            search
        )
        .fetch_one(db)
        .await
    }

    /// The next ranked issue after `(rank, issue)` but `skip`, as `(issue, rank)`.
    pub async fn next(
        db: impl PgExecutor<'_>,
        search: SavedSearchId,
        rank: &str,
        issue: IssueId,
        skip: IssueId,
    ) -> Result<Option<(IssueId, String)>, sqlx::Error> {
        let row = sqlx::query!(
            r#"select issue_id, rank from saved_search_ranks
               where search_id = $1 and (rank, issue_id) > ($2::text collate "C", $3)
                 and issue_id <> $4
               order by rank, issue_id limit 1"#,
            search,
            rank,
            issue,
            skip
        )
        .fetch_optional(db)
        .await?;
        Ok(row.map(|r| (r.issue_id, r.rank)))
    }

    /// The ranked issue before `(rank, issue)` but `skip`, as `(issue, rank)`.
    pub async fn previous(
        db: impl PgExecutor<'_>,
        search: SavedSearchId,
        rank: &str,
        issue: IssueId,
        skip: IssueId,
    ) -> Result<Option<(IssueId, String)>, sqlx::Error> {
        let row = sqlx::query!(
            r#"select issue_id, rank from saved_search_ranks
               where search_id = $1 and (rank, issue_id) < ($2::text collate "C", $3)
                 and issue_id <> $4
               order by rank desc, issue_id desc limit 1"#,
            search,
            rank,
            issue,
            skip
        )
        .fetch_optional(db)
        .await?;
        Ok(row.map(|r| (r.issue_id, r.rank)))
    }

    /// Gives the issue a rank (a new row or a new value).
    pub async fn set(
        conn: &mut PgConnection,
        search: SavedSearchId,
        issue: IssueId,
        rank: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "insert into saved_search_ranks (search_id, issue_id, rank) values ($1, $2, $3)
             on conflict (search_id, issue_id) do update set rank = excluded.rank",
            search,
            issue,
            rank
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Forgets the order; answers how many issues were ranked.
    pub async fn discard(
        conn: &mut PgConnection,
        search: SavedSearchId,
    ) -> Result<u64, sqlx::Error> {
        let done = sqlx::query!(
            "delete from saved_search_ranks where search_id = $1",
            search
        )
        .execute(conn)
        .await?;
        Ok(done.rows_affected())
    }

    /// The boards whose backlog is this search (`boards_backlog_idx`).
    pub async fn boards_of(
        db: impl PgExecutor<'_>,
        search: SavedSearchId,
    ) -> Result<Vec<BoardId>, sqlx::Error> {
        sqlx::query_scalar!(
            "select id from boards where backlog_search_id = $1 order by id",
            search
        )
        .fetch_all(db)
        .await
    }

    /// Whether the owner has a saved search of this name (case folded).
    pub async fn name_taken(
        db: impl PgExecutor<'_>,
        owner: prostor_domain::UserId,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar!(
            r#"select exists (select from saved_searches
                              where owner_id = $1 and lower(name) = lower($2)) as "taken!""#,
            owner,
            name
        )
        .fetch_one(db)
        .await
    }
}
