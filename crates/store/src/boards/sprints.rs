//! Sprints of a board (BRD-097…BRD-104). Every board has at least one: a sprintless board holds
//! its cards in an internal one (BRD-024). Sprint management is step 1.8b; 1.8a creates the first.

use sqlx::{PgConnection, PgExecutor};
use taskmaster_domain::boards::sprints::SprintDates;
use taskmaster_domain::{BoardId, SprintId};
use time::{Date, OffsetDateTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprint {
    pub id: SprintId,
    pub board_id: BoardId,
    pub name: String,
    pub goal: String,
    pub starts_on: Option<Date>,
    pub ends_on: Option<Date>,
    pub finished_at: Option<OffsetDateTime>,
    pub archived_at: Option<OffsetDateTime>,
    pub internal: bool,
    pub order_version: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub version: i64,
}

impl Sprint {
    pub fn dates(&self) -> SprintDates {
        SprintDates {
            id: self.id,
            starts_on: self.starts_on,
            ends_on: self.ends_on,
            finished_at: self.finished_at,
            archived: self.archived_at.is_some(),
            internal: self.internal,
            created_at: self.created_at,
        }
    }
}

/// Unique index on (board, lower(name)) of visible sprints.
pub const NAME_KEY: &str = "sprints_name_key";

/// In creation order.
pub async fn of_board(db: impl PgExecutor<'_>, board: BoardId) -> Result<Vec<Sprint>, sqlx::Error> {
    sqlx::query_as!(
        Sprint,
        "select id, board_id, name, goal, starts_on, ends_on, finished_at, archived_at, internal,
                order_version, created_at, updated_at, version
         from sprints where board_id = $1 order by created_at, id",
        board
    )
    .fetch_all(db)
    .await
}

/// A new unscheduled sprint; `internal` for the hidden sprint of a sprintless board.
pub async fn insert(
    conn: &mut PgConnection,
    board: BoardId,
    name: &str,
    internal: bool,
) -> Result<SprintId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into sprints (board_id, name, internal) values ($1, $2, $3) returning id",
        board,
        name,
        internal
    )
    .fetch_one(conn)
    .await
}

/// The sprints of several boards, in creation order.
pub async fn of_boards(
    db: impl PgExecutor<'_>,
    boards: &[BoardId],
) -> Result<Vec<Sprint>, sqlx::Error> {
    if boards.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as!(
        Sprint,
        "select id, board_id, name, goal, starts_on, ends_on, finished_at, archived_at, internal,
                order_version, created_at, updated_at, version
         from sprints where board_id = any($1) order by created_at, id",
        boards
    )
    .fetch_all(db)
    .await
}

/// What a person sets on a sprint (BRD-098, BRD-102).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprintSettings {
    pub name: String,
    pub goal: String,
    pub starts_on: Option<Date>,
    pub ends_on: Option<Date>,
}

/// A new visible sprint with its settings.
pub async fn insert_with(
    conn: &mut PgConnection,
    board: BoardId,
    s: &SprintSettings,
) -> Result<SprintId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into sprints (board_id, name, goal, starts_on, ends_on) values ($1, $2, $3, $4, $5)
         returning id",
        board,
        s.name,
        s.goal,
        s.starts_on,
        s.ends_on
    )
    .fetch_one(conn)
    .await
}

/// Writes the settings, bumping the version.
pub async fn update(
    conn: &mut PgConnection,
    id: SprintId,
    s: &SprintSettings,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update sprints set name = $2, goal = $3, starts_on = $4, ends_on = $5,
                version = version + 1, updated_at = now()
         where id = $1",
        id,
        s.name,
        s.goal,
        s.starts_on,
        s.ends_on
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets when the sprint was finished by hand (`None`: not finished), bumping the version.
pub async fn set_finished(
    conn: &mut PgConnection,
    id: SprintId,
    at: Option<OffsetDateTime>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update sprints set finished_at = $2, version = version + 1, updated_at = now()
         where id = $1",
        id,
        at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Archives (`Some`) or restores (`None`) the sprint, bumping the version.
pub async fn set_archived(
    conn: &mut PgConnection,
    id: SprintId,
    at: Option<OffsetDateTime>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update sprints set archived_at = $2, version = version + 1, updated_at = now()
         where id = $1",
        id,
        at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Archives every visible, not archived sprint of the board but `except`; answers their IDs.
pub async fn archive_others(
    conn: &mut PgConnection,
    board: BoardId,
    except: SprintId,
) -> Result<Vec<SprintId>, sqlx::Error> {
    sqlx::query_scalar!(
        "update sprints set archived_at = now(), version = version + 1, updated_at = now()
         where board_id = $1 and id <> $2 and not internal and archived_at is null
         returning id",
        board,
        except
    )
    .fetch_all(conn)
    .await
}

/// Makes the sprint the hidden one of a sprintless board (`internal`), or a visible one with
/// `name` (BRD-024). An internal sprint is never finished or archived.
pub async fn set_internal(
    conn: &mut PgConnection,
    id: SprintId,
    internal: bool,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update sprints set internal = $2, name = $3, archived_at = null,
                finished_at = case when $2 then null else finished_at end,
                version = version + 1, updated_at = now()
         where id = $1",
        id,
        internal,
        name
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: SprintId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from sprints where id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}
