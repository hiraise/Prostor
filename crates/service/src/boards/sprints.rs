//! Sprints of a board (BRD-097…BRD-104, D-013; D-045): create, edit, start and finish by
//! hand, archive, restore, delete, and carry the unresolved cards to another sprint. Settings
//! editors of the board do all of it; a sprintless board has only its hidden sprint and none of
//! this. Nothing runs on a schedule: a sprint whose end passed is finished on read.

use prostor_domain::access::Access;
use prostor_domain::boards::sprints::{self, SprintDates};
use prostor_domain::boards::{self as domain, Mode, SPRINTS_MAX};
use prostor_domain::issues::history::{Origin, Record};
use prostor_domain::live::Change;
use prostor_domain::rank;
use prostor_domain::validation::{Problem, Problems};
use prostor_domain::{Actor, BoardId, SprintId};
use prostor_store::boards::cards as store_cards;
use prostor_store::boards::sprints::{self as store, SprintSettings};
use prostor_store::boards::{self as store_boards, Sprint};
use prostor_store::issue_history::Author;
use prostor_store::{PgPool, audit, issues, unique_violation};
use serde_json::json;
use sqlx::PgConnection;
use time::{Date, OffsetDateTime};

use super::cards::place;
use super::lock_edited;
use super::view::today;
use crate::live::Batch;
use crate::seed::internal;
use crate::{Error, Result};

/// Cards carried over in one statement (BRD-101).
pub const CARRY_BATCH: i64 = 500;

/// A new sprint (BRD-098).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewSprint {
    pub name: String,
    pub goal: String,
    pub starts_on: Option<Date>,
    pub ends_on: Option<Date>,
    /// Carry the unresolved cards of this sprint to the new one («Перенести нерешённые из …»).
    pub carry_from: Option<SprintId>,
    /// New issues join this sprint («Новые задачи попадают сюда»): the board's mode becomes `auto`.
    pub auto_target: bool,
}

/// A change of a sprint (BRD-102); `None` keeps a value, `Some(None)` clears a date.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SprintChange {
    pub name: Option<String>,
    pub goal: Option<String>,
    pub starts_on: Option<Option<Date>>,
    pub ends_on: Option<Option<Date>>,
    pub version: i64,
}

/// A sprint with what the answer shows of it.
#[derive(Debug, Clone)]
pub struct SprintRow {
    pub sprint: Sprint,
    /// The board's current sprint (BRD-099).
    pub current: Option<SprintId>,
    /// Today in the instance time zone: a sprint whose end passed is finished (BRD-100).
    pub today: Date,
}

/// The visible sprints of a board, archived ones included, in creation order.
#[derive(Debug, Clone)]
pub struct SprintList {
    pub sprints: Vec<Sprint>,
    pub current: Option<SprintId>,
    pub today: Date,
}

fn taken(error: sqlx::Error) -> Error {
    match unique_violation(&error) {
        Some(store::NAME_KEY) => Error::field(
            "name",
            Problem::Taken,
            "the board has a sprint with this name",
        ),
        _ => error.into(),
    }
}

fn no_sprint() -> Error {
    Error::NotFound("no such sprint")
}

fn target(board: BoardId) -> String {
    format!("board:{board}")
}

/// The visible sprint of the board (the hidden one of a sprintless board is not managed).
fn visible(all: &[Sprint], id: SprintId) -> Result<&Sprint> {
    all.iter()
        .find(|s| s.id == id && !s.internal)
        .ok_or_else(no_sprint)
}

fn current_of(all: &[Sprint], today: Date) -> Option<SprintId> {
    let dates: Vec<SprintDates> = all.iter().map(Sprint::dates).collect();
    sprints::current(&dates, today)
}

async fn answer(
    conn: &mut PgConnection,
    board: BoardId,
    id: SprintId,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let all = store::of_board(&mut *conn, board).await?;
    let today = today(conn, now).await?;
    let current = current_of(&all, today);
    let sprint = all.into_iter().find(|s| s.id == id).ok_or_else(no_sprint)?;
    Ok(SprintRow {
        sprint,
        current,
        today,
    })
}

/// The sprints of a board the viewer views (BRD-103: archived ones in their own section).
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    board: BoardId,
    now: OffsetDateTime,
) -> Result<SprintList> {
    let mut conn = pool.acquire().await?;
    store_boards::get(&mut *conn, board)
        .await?
        .filter(|b| b.access().can_view(viewer))
        .ok_or_else(super::not_found)?;
    let all = store::of_board(&mut *conn, board).await?;
    let today = today(&mut conn, now).await?;
    let current = current_of(&all, today);
    Ok(SprintList {
        sprints: all.into_iter().filter(|s| !s.internal).collect(),
        current,
        today,
    })
}

/// Validated settings, from what is now and the change.
fn settings_of(
    name: Option<&str>,
    goal: Option<&str>,
    starts_on: Option<Date>,
    ends_on: Option<Date>,
    now: Option<&Sprint>,
) -> Result<SprintSettings> {
    let mut problems = Problems::new();
    let name = match name {
        Some(n) => problems.check("name", domain::sprint_name(n)),
        None => now.map(|s| s.name.clone()),
    };
    let goal = match goal {
        Some(g) => problems.check("goal", domain::goal(g)),
        None => Some(now.map(|s| s.goal.clone()).unwrap_or_default()),
    };
    problems.check("ends_on", sprints::schedule(starts_on, ends_on));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(goal)) = (name, goal) else {
        return Err(internal("validated values are missing"));
    };
    Ok(SprintSettings {
        name,
        goal,
        starts_on,
        ends_on,
    })
}

/// Creates a sprint on a sprint board (BRD-098): settings editors; at most [`SPRINTS_MAX`].
pub async fn create(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    input: NewSprint,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let settings = settings_of(
        Some(&input.name),
        Some(&input.goal),
        input.starts_on,
        input.ends_on,
        None,
    )?;
    let mut tx = pool.begin().await?;
    let board = lock_edited(&mut tx, actor, board_id).await?;
    if !board.sprints_on {
        return Err(Error::Refused(
            "this board has no sprints; switch them on first".into(),
        ));
    }
    let all = store::of_board(&mut *tx, board_id).await?;
    if all.len() >= SPRINTS_MAX {
        return Err(Error::Refused(
            "the board has as many sprints as it may".into(),
        ));
    }
    let from = match input.carry_from {
        Some(id) => Some(
            visible(&all, id)
                .map_err(|_| Error::field("carry_from", Problem::Invalid, "no such sprint"))?
                .clone(),
        ),
        None => None,
    };
    let id = store::insert_with(&mut tx, board_id, &settings)
        .await
        .map_err(taken)?;
    let mut batch = Batch::new();
    batch.sprint(board_id, id, Change::Created);
    if input.auto_target {
        set_auto(&mut tx, &board, Some(id), Mode::Auto).await?;
        batch.board(board_id, Change::Updated);
    }
    if let Some(from) = from {
        let to = Sprint {
            id,
            name: settings.name.clone(),
            ..from.clone()
        };
        carry_locked(&mut tx, &mut batch, actor, origin, &board, &from, &to).await?;
    }
    audit::record(
        &mut tx,
        Actor::User(actor.user()),
        "board.sprint_created",
        &target(board_id),
        json!({ "sprint": id, "name": settings.name }),
    )
    .await?;
    batch.publish(&mut tx).await?;
    let row = answer(&mut tx, board_id, id, now).await?;
    tx.commit().await?;
    Ok(row)
}

/// The board's auto-add target and mode (BRD-026).
async fn set_auto(
    conn: &mut PgConnection,
    board: &store_boards::Board,
    sprint: Option<SprintId>,
    mode: Mode,
) -> Result<()> {
    let mut settings = store_boards::Settings::of(board);
    settings.auto_sprint = sprint;
    settings.mode = mode.key().to_owned();
    store_boards::update(conn, board.id, &settings).await?;
    Ok(())
}

/// The sprint locked through its board for a change by a settings editor.
async fn locked(
    conn: &mut PgConnection,
    actor: &Access,
    board_id: BoardId,
    id: SprintId,
) -> Result<(store_boards::Board, Vec<Sprint>, Sprint)> {
    let board = lock_edited(conn, actor, board_id).await?;
    let all = store::of_board(&mut *conn, board_id).await?;
    let sprint = visible(&all, id)?.clone();
    Ok((board, all, sprint))
}

/// Edits the name, goal and dates at any time (BRD-102); a stale `version` is a conflict.
pub async fn update(
    pool: &PgPool,
    actor: &Access,
    board_id: BoardId,
    id: SprintId,
    change: SprintChange,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let mut tx = pool.begin().await?;
    let (_, _, sprint) = locked(&mut tx, actor, board_id, id).await?;
    if sprint.version != change.version {
        return Err(Error::Conflict("the sprint was changed by someone else"));
    }
    let settings = settings_of(
        change.name.as_deref(),
        change.goal.as_deref(),
        change.starts_on.unwrap_or(sprint.starts_on),
        change.ends_on.unwrap_or(sprint.ends_on),
        Some(&sprint),
    )?;
    store::update(&mut tx, id, &settings).await.map_err(taken)?;
    let mut batch = Batch::new();
    batch.sprint(board_id, id, Change::Updated);
    batch.publish(&mut tx).await?;
    let row = answer(&mut tx, board_id, id, now).await?;
    tx.commit().await?;
    Ok(row)
}

/// Starts a sprint by hand (D-013): it starts today unless it started already; a finish by hand
/// and an end already passed go.
pub async fn start(
    pool: &PgPool,
    actor: &Access,
    board_id: BoardId,
    id: SprintId,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let mut tx = pool.begin().await?;
    let (_, _, sprint) = locked(&mut tx, actor, board_id, id).await?;
    let day = today(&mut tx, now).await?;
    let (starts_on, ends_on) = sprints::started(sprint.starts_on, sprint.ends_on, day);
    let settings = SprintSettings {
        name: sprint.name.clone(),
        goal: sprint.goal.clone(),
        starts_on: Some(starts_on),
        ends_on,
    };
    store::update(&mut tx, id, &settings).await?;
    if sprint.finished_at.is_some() {
        store::set_finished(&mut tx, id, None).await?;
    }
    let mut batch = Batch::new();
    batch.sprint(board_id, id, Change::Updated);
    batch.publish(&mut tx).await?;
    let row = answer(&mut tx, board_id, id, now).await?;
    tx.commit().await?;
    Ok(row)
}

/// Finishes a sprint by hand (D-013); the client then offers to carry its unresolved cards
/// (BRD-101). Finishing a finished sprint changes nothing.
pub async fn finish(
    pool: &PgPool,
    actor: &Access,
    board_id: BoardId,
    id: SprintId,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let mut tx = pool.begin().await?;
    let (_, _, sprint) = locked(&mut tx, actor, board_id, id).await?;
    if sprint.finished_at.is_none() {
        store::set_finished(&mut tx, id, Some(now)).await?;
        let mut batch = Batch::new();
        batch.sprint(board_id, id, Change::Updated);
        batch.publish(&mut tx).await?;
    }
    let row = answer(&mut tx, board_id, id, now).await?;
    tx.commit().await?;
    Ok(row)
}

/// Archives (`true`) or restores a sprint (BRD-103): archived sprints leave detection and the
/// suggestions and stay usable in queries. An archived sprint stops being the auto-add target.
pub async fn archive(
    pool: &PgPool,
    actor: &Access,
    board_id: BoardId,
    id: SprintId,
    archived: bool,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let mut tx = pool.begin().await?;
    let (board, _, sprint) = locked(&mut tx, actor, board_id, id).await?;
    if sprint.archived_at.is_some() != archived {
        store::set_archived(&mut tx, id, archived.then_some(now)).await?;
        let mut batch = Batch::new();
        batch.sprint(board_id, id, Change::Updated);
        if archived && board.auto_sprint_id == Some(id) {
            let mode = Mode::parse(&board.mode).unwrap_or(Mode::Manual);
            set_auto(&mut tx, &board, None, mode).await?;
            batch.board(board_id, Change::Updated);
        }
        batch.publish(&mut tx).await?;
    }
    let row = answer(&mut tx, board_id, id, now).await?;
    tx.commit().await?;
    Ok(row)
}

/// Deletes a sprint with its cards (BRD-104); the issues keep their history. The last visible
/// sprint of a sprint board stays.
pub async fn delete(pool: &PgPool, actor: &Access, board_id: BoardId, id: SprintId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (board, all, sprint) = locked(&mut tx, actor, board_id, id).await?;
    if all.iter().filter(|s| !s.internal).count() <= 1 {
        return Err(Error::Refused(
            "a sprint board keeps at least one sprint".into(),
        ));
    }
    let mut batch = Batch::new();
    if board.auto_sprint_id == Some(id) {
        // The foreign key clears the target; the board's version still grows for its readers.
        store_boards::touch(&mut tx, board_id).await?;
        batch.board(board_id, Change::Updated);
    }
    // Query boards naming this board hear of the cards that leave (BRD-114).
    for issue in store_boards::cards::named_rows(&mut *tx, board_id, Some(id)).await? {
        batch.issue_on_board(issue, Change::Updated, board_id);
    }
    store::delete(&mut tx, id).await?;
    batch.sprint(board_id, id, Change::Deleted);
    audit::record(
        &mut tx,
        Actor::User(actor.user()),
        "board.sprint_deleted",
        &target(board_id),
        json!({ "sprint": id, "name": sprint.name }),
    )
    .await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Carries the unresolved cards of a sprint to another (BRD-101): they leave `from` and join the
/// end of `to`, with a record on each issue.
pub async fn carry(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    from: SprintId,
    to: SprintId,
    now: OffsetDateTime,
) -> Result<SprintRow> {
    let mut tx = pool.begin().await?;
    let (board, all, source) = locked(&mut tx, actor, board_id, from).await?;
    let dest = visible(&all, to)
        .map_err(|_| Error::field("to", Problem::Invalid, "no such sprint"))?
        .clone();
    if dest.id == source.id {
        return Err(Error::field("to", Problem::Invalid, "the same sprint"));
    }
    let mut batch = Batch::new();
    carry_locked(&mut tx, &mut batch, actor, origin, &board, &source, &dest).await?;
    batch.publish(&mut tx).await?;
    let row = answer(&mut tx, board_id, to, now).await?;
    tx.commit().await?;
    Ok(row)
}

/// Moves the unresolved cards in batches of [`CARRY_BATCH`], all in the caller's transaction.
/// The board's editor carries them: like deleting a sprint, it needs no right on each issue.
async fn carry_locked(
    conn: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    board: &store_boards::Board,
    from: &Sprint,
    to: &Sprint,
) -> Result<()> {
    let author = Author {
        user: actor.user(),
        origin,
        silent: false,
    };
    let left = place(board, from);
    let joined = place(board, to);
    let Some(record) = Record::boards(Some(&left), Some(&joined)) else {
        return Ok(());
    };
    let mut last = store_cards::last_rank(&mut *conn, to.id).await?;
    loop {
        let moving = store_cards::unresolved(&mut *conn, from.id, CARRY_BATCH).await?;
        if moving.is_empty() {
            break;
        }
        let mut ranks = Vec::with_capacity(moving.len());
        for _ in &moving {
            let next = rank::between(last.as_deref(), None).map_err(internal)?;
            last = Some(next.clone());
            ranks.push(next);
        }
        store_cards::carry(conn, from.id, to.id, &moving, &ranks).await?;
        let records = vec![record.clone(); moving.len()];
        prostor_store::issue_history::record_many(conn, &moving, author, &records).await?;
        issues::touch_many(conn, &moving, actor.user()).await?;
        for issue in &moving {
            batch.issue_on_board(*issue, Change::Updated, board.id);
        }
        if (moving.len() as i64) < CARRY_BATCH {
            break;
        }
    }
    Ok(())
}
