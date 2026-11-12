//! Boards (step 1.8, D-044): the board row with its projects, columns, sprints and cards, and each
//! person's view of a board. Rules live in `domain::boards`; this module only reads and writes.

pub mod cards;
pub mod columns;
pub mod profiles;
pub mod sprints;
pub mod view;

use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;
use trackflow_domain::access::Projects;
use trackflow_domain::boards::access::BoardAccess;
use trackflow_domain::sharing::{Audience, Shared};
use trackflow_domain::{BoardId, FieldId, GroupId, ProjectId, SprintId, UserId};

pub use columns::Column;
pub use profiles::Profile;
pub use sprints::Sprint;

/// The saved search used as a backlog.
pub type SavedSearchId = i64;

/// A board row with its projects (sorted by ID).
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub id: BoardId,
    pub name: String,
    pub owner_id: UserId,
    /// `domain::boards::Preset` key.
    pub preset: String,
    pub sprints_on: bool,
    /// `domain::boards::Mode` key.
    pub mode: String,
    /// The stored form of the board query (mode `query`).
    pub query: Option<Value>,
    pub auto_sprint_id: Option<SprintId>,
    pub column_field_id: Option<FieldId>,
    pub card_fields: Vec<FieldId>,
    pub card_initials: Vec<FieldId>,
    /// `domain::boards::ColorMode` key.
    pub color_mode: String,
    pub color_field_id: Option<FieldId>,
    pub estimation_field_id: Option<FieldId>,
    pub original_estimation_field_id: Option<FieldId>,
    pub backlog_search_id: Option<SavedSearchId>,
    /// Lanes (BRD-049, 1.8c): `none`, `values`, `parents`.
    pub lane_mode: String,
    pub lane_field_id: Option<FieldId>,
    pub lane_attribute: Option<String>,
    pub lane_values: Vec<String>,
    pub other_lane: String,
    pub new_card_value_id: Option<i64>,
    pub view_users: Vec<UserId>,
    pub view_groups: Vec<GroupId>,
    pub edit_users: Vec<UserId>,
    pub edit_groups: Vec<GroupId>,
    pub view_readers: bool,
    pub edit_updaters: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub version: i64,
    pub projects: Vec<ProjectId>,
    /// Card colors by project (BRD-060), beside `projects`.
    pub project_colors: Vec<Option<i16>>,
}

impl Board {
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

    pub fn access(&self) -> BoardAccess {
        BoardAccess {
            shared: self.sharing(),
            view_readers: self.view_readers,
            edit_updaters: self.edit_updaters,
            projects: self.projects.clone(),
        }
    }
}

/// A board with everything its settings show.
#[derive(Debug, Clone, PartialEq)]
pub struct Full {
    pub board: Board,
    /// In rank order.
    pub columns: Vec<Column>,
    /// In creation order, archived and internal ones included.
    pub sprints: Vec<Sprint>,
}

/// The settings a board row holds; the service reads a board, changes these and writes them back.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub name: String,
    pub owner: UserId,
    pub preset: String,
    pub sprints_on: bool,
    pub mode: String,
    pub query: Option<Value>,
    pub auto_sprint: Option<SprintId>,
    pub column_field: Option<FieldId>,
    pub card_fields: Vec<FieldId>,
    pub card_initials: Vec<FieldId>,
    pub color_mode: String,
    pub color_field: Option<FieldId>,
    pub estimation_field: Option<FieldId>,
    pub original_estimation_field: Option<FieldId>,
    pub backlog_search: Option<SavedSearchId>,
    pub lanes: Lanes,
    pub view: Audience,
    pub edit: Audience,
    pub view_readers: bool,
    pub edit_updaters: bool,
}

/// The lane settings of a board (BRD-049…BRD-055).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lanes {
    pub mode: String,
    pub field: Option<FieldId>,
    pub attribute: Option<String>,
    pub values: Vec<String>,
    pub other: String,
    pub new_card_value: Option<i64>,
}

impl Default for Lanes {
    fn default() -> Self {
        Self {
            mode: "none".to_owned(),
            field: None,
            attribute: None,
            values: Vec::new(),
            other: "bottom".to_owned(),
            new_card_value: None,
        }
    }
}

impl Settings {
    pub fn of(board: &Board) -> Self {
        Self {
            name: board.name.clone(),
            owner: board.owner_id,
            preset: board.preset.clone(),
            sprints_on: board.sprints_on,
            mode: board.mode.clone(),
            query: board.query.clone(),
            auto_sprint: board.auto_sprint_id,
            column_field: board.column_field_id,
            card_fields: board.card_fields.clone(),
            card_initials: board.card_initials.clone(),
            color_mode: board.color_mode.clone(),
            color_field: board.color_field_id,
            estimation_field: board.estimation_field_id,
            original_estimation_field: board.original_estimation_field_id,
            backlog_search: board.backlog_search_id,
            lanes: Lanes {
                mode: board.lane_mode.clone(),
                field: board.lane_field_id,
                attribute: board.lane_attribute.clone(),
                values: board.lane_values.clone(),
                other: board.other_lane.clone(),
                new_card_value: board.new_card_value_id,
            },
            view: Audience {
                users: board.view_users.clone(),
                groups: board.view_groups.clone(),
            },
            edit: Audience {
                users: board.edit_users.clone(),
                groups: board.edit_groups.clone(),
            },
            view_readers: board.view_readers,
            edit_updaters: board.edit_updaters,
        }
    }
}

/// Inserts a board with its projects; columns and sprints follow with their own inserts.
pub async fn insert(
    conn: &mut PgConnection,
    settings: &Settings,
    projects: &[ProjectId],
) -> Result<BoardId, sqlx::Error> {
    let s = settings;
    let id = sqlx::query_scalar!(
        "insert into boards (name, owner_id, preset, sprints_on, mode, query, auto_sprint_id,
                column_field_id, card_fields, card_initials, color_mode, color_field_id,
                estimation_field_id, original_estimation_field_id, backlog_search_id,
                view_users, view_groups, edit_users, edit_groups, view_readers, edit_updaters,
                lane_mode, lane_field_id, lane_attribute, lane_values, other_lane,
                new_card_value_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, $19, $20, $21, $22, $23, $24, $25, $26, $27)
         returning id",
        s.name,
        s.owner,
        s.preset,
        s.sprints_on,
        s.mode,
        s.query,
        s.auto_sprint,
        s.column_field,
        &s.card_fields,
        &s.card_initials,
        s.color_mode,
        s.color_field,
        s.estimation_field,
        s.original_estimation_field,
        s.backlog_search,
        &s.view.users,
        &s.view.groups,
        &s.edit.users,
        &s.edit.groups,
        s.view_readers,
        s.edit_updaters,
        s.lanes.mode,
        s.lanes.field,
        s.lanes.attribute,
        &s.lanes.values,
        s.lanes.other,
        s.lanes.new_card_value
    )
    .fetch_one(&mut *conn)
    .await?;
    set_projects(conn, id, projects).await?;
    Ok(id)
}

/// Sets the card colors of board projects (BRD-060); other projects are left as they are.
pub async fn set_project_colors(
    conn: &mut PgConnection,
    board: BoardId,
    colors: &[(ProjectId, Option<i16>)],
) -> Result<(), sqlx::Error> {
    let (projects, values): (Vec<ProjectId>, Vec<Option<i16>>) = colors.iter().copied().unzip();
    sqlx::query!(
        "update board_projects bp set color = c.color
         from unnest($2::bigint[], $3::smallint[]) as c(project_id, color)
         where bp.board_id = $1 and bp.project_id = c.project_id",
        board,
        &projects,
        &values as &[Option<i16>]
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// What a board uses a field for (BRD-022).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldUse {
    pub board: Board,
    pub column: bool,
    pub lane: bool,
    pub color: bool,
    pub estimation: bool,
    pub original_estimation: bool,
    pub cards: bool,
}

/// The boards using a field (BRD-022), by name: two statements.
pub async fn field_usage(
    conn: &mut PgConnection,
    field: FieldId,
) -> Result<Vec<FieldUse>, sqlx::Error> {
    let ids = sqlx::query_scalar!(
        "select id from boards
         where column_field_id = $1 or lane_field_id = $1 or color_field_id = $1
            or estimation_field_id = $1 or original_estimation_field_id = $1
            or $1 = any(card_fields)",
        field
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut boards = get_many(&mut *conn, &ids).await?;
    boards.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(boards
        .into_iter()
        .map(|b| FieldUse {
            column: b.column_field_id == Some(field),
            lane: b.lane_field_id == Some(field),
            color: b.color_field_id == Some(field),
            estimation: b.estimation_field_id == Some(field),
            original_estimation: b.original_estimation_field_id == Some(field),
            cards: b.card_fields.contains(&field),
            board: b,
        })
        .collect())
}

/// Replaces the board's projects; kept ones keep their color.
pub async fn set_projects(
    conn: &mut PgConnection,
    board: BoardId,
    projects: &[ProjectId],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from board_projects where board_id = $1 and project_id <> all($2)",
        board,
        projects
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "insert into board_projects (board_id, project_id)
         select $1, p from unnest($2::bigint[]) p on conflict do nothing",
        board,
        projects
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get(db: impl PgExecutor<'_>, id: BoardId) -> Result<Option<Board>, sqlx::Error> {
    sqlx::query_as!(
        Board,
        r#"select id, name, owner_id, preset, sprints_on, mode, query, auto_sprint_id,
                  column_field_id, card_fields, card_initials, color_mode, color_field_id,
                  estimation_field_id, original_estimation_field_id, backlog_search_id,
                  lane_mode, lane_field_id, lane_attribute, lane_values, other_lane,
                  new_card_value_id, view_users, view_groups, edit_users, edit_groups, view_readers, edit_updaters,
                  created_at, updated_at, version,
                  array(select project_id from board_projects where board_id = b.id
                        order by project_id) as "projects!",
                  array(select color from board_projects where board_id = b.id
                        order by project_id) as "project_colors!: Vec<Option<i16>>"
           from boards b where id = $1"#,
        id
    )
    .fetch_optional(db)
    .await
}

/// Several boards by ID, in no order.
pub async fn get_many(db: impl PgExecutor<'_>, ids: &[BoardId]) -> Result<Vec<Board>, sqlx::Error> {
    sqlx::query_as!(
        Board,
        r#"select id, name, owner_id, preset, sprints_on, mode, query, auto_sprint_id,
                  column_field_id, card_fields, card_initials, color_mode, color_field_id,
                  estimation_field_id, original_estimation_field_id, backlog_search_id,
                  lane_mode, lane_field_id, lane_attribute, lane_values, other_lane,
                  new_card_value_id, view_users, view_groups, edit_users, edit_groups, view_readers, edit_updaters,
                  created_at, updated_at, version,
                  array(select project_id from board_projects where board_id = b.id
                        order by project_id) as "projects!",
                  array(select color from board_projects where board_id = b.id
                        order by project_id) as "project_colors!: Vec<Option<i16>>"
           from boards b where id = any($1)"#,
        ids
    )
    .fetch_all(db)
    .await
}

/// The board row locked for a change.
pub async fn lock(conn: &mut PgConnection, id: BoardId) -> Result<Option<Board>, sqlx::Error> {
    sqlx::query_scalar!("select id from boards where id = $1 for update", id)
        .fetch_optional(&mut *conn)
        .await?;
    get(conn, id).await
}

/// A board with its columns and sprints: three statements.
pub async fn get_full(conn: &mut PgConnection, id: BoardId) -> Result<Option<Full>, sqlx::Error> {
    let Some(board) = get(&mut *conn, id).await? else {
        return Ok(None);
    };
    let columns = columns::of_board(&mut *conn, id).await?;
    let sprints = sprints::of_board(&mut *conn, id).await?;
    Ok(Some(Full {
        board,
        columns,
        sprints,
    }))
}

/// Writes the settings, bumping the version.
pub async fn update(
    conn: &mut PgConnection,
    id: BoardId,
    settings: &Settings,
) -> Result<(), sqlx::Error> {
    let s = settings;
    sqlx::query!(
        "update boards set name = $2, owner_id = $3, preset = $4, sprints_on = $5, mode = $6,
                query = $7, auto_sprint_id = $8, column_field_id = $9, card_fields = $10,
                card_initials = $11, color_mode = $12, color_field_id = $13,
                estimation_field_id = $14, original_estimation_field_id = $15,
                backlog_search_id = $16, view_users = $17, view_groups = $18, edit_users = $19,
                edit_groups = $20, view_readers = $21, edit_updaters = $22,
                lane_mode = $23, lane_field_id = $24, lane_attribute = $25, lane_values = $26,
                other_lane = $27, new_card_value_id = $28,
                version = version + 1, updated_at = now()
         where id = $1",
        id,
        s.name,
        s.owner,
        s.preset,
        s.sprints_on,
        s.mode,
        s.query,
        s.auto_sprint,
        s.column_field,
        &s.card_fields,
        &s.card_initials,
        s.color_mode,
        s.color_field,
        s.estimation_field,
        s.original_estimation_field,
        s.backlog_search,
        &s.view.users,
        &s.view.groups,
        &s.edit.users,
        &s.edit.groups,
        s.view_readers,
        s.edit_updaters,
        s.lanes.mode,
        s.lanes.field,
        s.lanes.attribute,
        &s.lanes.values,
        s.lanes.other,
        s.lanes.new_card_value
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets the backlog search of a board just made (BRD-089).
pub async fn set_backlog(
    conn: &mut PgConnection,
    id: BoardId,
    search: Option<SavedSearchId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update boards set backlog_search_id = $2 where id = $1",
        id,
        search
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Bumps the version after a change of columns, sprints or projects.
pub async fn touch(conn: &mut PgConnection, id: BoardId) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        "update boards set version = version + 1, updated_at = now() where id = $1
         returning version",
        id
    )
    .fetch_one(conn)
    .await
}

/// Sprints, columns, profiles and cards go with the board; issues keep their history.
pub async fn delete(conn: &mut PgConnection, id: BoardId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from boards where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// What deleting a board takes with it (BRD-016): its visible sprints and its distinct cards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Impact {
    pub sprints: i64,
    pub cards: i64,
}

pub async fn impact(db: impl PgExecutor<'_>, id: BoardId) -> Result<Impact, sqlx::Error> {
    let row = sqlx::query!(
        r#"select (select count(*) from sprints where board_id = $1 and not internal) as "sprints!",
                  (select count(distinct si.issue_id) from sprint_issues si
                   join sprints s on s.id = si.sprint_id where s.board_id = $1) as "cards!""#,
        id
    )
    .fetch_one(db)
    .await?;
    Ok(Impact {
        sprints: row.sprints,
        cards: row.cards,
    })
}

/// Who looks at the board list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer<'a> {
    pub me: UserId,
    pub groups: &'a [GroupId],
    /// Where the viewer holds `issue.read` (for `view_readers`).
    pub read: &'a Projects,
}

impl Viewer<'_> {
    /// The rule of `BoardAccess::can_view` (and of [`listed`]) on a loaded board.
    pub fn views(&self, b: &Board) -> bool {
        let reads = |p: &ProjectId| match self.read {
            Projects::All => true,
            Projects::Only(ids) => ids.contains(p),
        };
        b.owner_id == self.me
            || b.view_users.contains(&self.me)
            || b.edit_users.contains(&self.me)
            || b.view_groups
                .iter()
                .chain(&b.edit_groups)
                .any(|g| self.groups.contains(g))
            || (b.view_readers && !b.projects.is_empty() && b.projects.iter().all(reads))
    }
}

/// One row of the board list.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub id: BoardId,
    pub name: String,
    pub owner_id: UserId,
    pub favorite: bool,
    pub projects: Vec<ProjectId>,
}

/// Where a list page starts: after this row of the order (favorites first, then name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct After {
    pub favorite: bool,
    pub name: String,
    pub id: BoardId,
}

/// Boards the viewer can view (the rule of `domain::boards::access::BoardAccess::can_view` in
/// SQL), favorites first, then by name. `q` filters by a part of the name; `project` keeps boards
/// that include it.
pub async fn listed(
    db: impl PgExecutor<'_>,
    viewer: &Viewer<'_>,
    q: Option<&str>,
    project: Option<ProjectId>,
    after: Option<&After>,
    limit: i64,
) -> Result<Vec<Listed>, sqlx::Error> {
    let (read_all, read) = match viewer.read {
        Projects::All => (true, Vec::new()),
        Projects::Only(ids) => (false, ids.clone()),
    };
    let pattern = q.map(str::trim).filter(|q| !q.is_empty()).map(|q| {
        format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    // `not favorite` sorts favorites (false) first.
    let (after_rest, after_name, after_id) = after
        .map(|a| (!a.favorite, a.name.to_lowercase(), a.id))
        .unwrap_or((false, String::new(), 0));
    let start = after.is_some();
    sqlx::query_as!(
        Listed,
        r#"select b.id, b.name, b.owner_id, coalesce(p.favorite, false) as "favorite!",
                  array(select project_id from board_projects where board_id = b.id
                        order by project_id) as "projects!"
           from boards b
           left join board_profiles p on p.board_id = b.id and p.user_id = $1
           where (b.owner_id = $1 or $1 = any(b.view_users) or b.view_groups && $2
                  or $1 = any(b.edit_users) or b.edit_groups && $2
                  or (b.view_readers
                      and exists (select from board_projects bp where bp.board_id = b.id)
                      and ($3 or not exists (select from board_projects bp
                                             where bp.board_id = b.id
                                               and bp.project_id <> all($4)))))
             and ($5::text is null or b.name ilike $5)
             and ($6::bigint is null or exists (select from board_projects bp
                                                where bp.board_id = b.id and bp.project_id = $6))
             and (not $7 or (not coalesce(p.favorite, false), lower(b.name), b.id) > ($8, $9, $10))
           order by not coalesce(p.favorite, false), lower(b.name), b.id
           limit $11"#,
        viewer.me,
        viewer.groups,
        read_all,
        &read,
        pattern,
        project,
        start,
        after_rest,
        after_name,
        after_id,
        limit
    )
    .fetch_all(db)
    .await
}

/// Deleted accounts (§6 of the architecture): their boards pass to the heir, their audience
/// entries too (never listing a board's own owner), their profiles go. Without an heir the boards
/// stay with the deleted account and the entries go.
pub async fn pass_from_deleted(
    conn: &mut PgConnection,
    owners: &[UserId],
    heir: Option<UserId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from board_profiles where user_id = any($1)", owners)
        .execute(&mut *conn)
        .await?;
    let Some(heir) = heir else {
        sqlx::query!(
            "update boards set
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
        "update boards set owner_id = $2, version = version + 1 where owner_id = any($1)",
        owners,
        heir
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update boards set
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
        "update boards set
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

/// A deleted field leaves the cards' field lists; the other references are `set null` by the
/// foreign keys.
pub async fn remove_field(conn: &mut PgConnection, field: FieldId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update boards set card_fields = array_remove(card_fields, $1),
                card_initials = array_remove(card_initials, $1), version = version + 1
         where $1 = any(card_fields) or $1 = any(card_initials)",
        field
    )
    .execute(conn)
    .await?;
    Ok(())
}
