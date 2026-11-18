//! How issues join and leave boards (BRD-026, BRD-027, BRD-034; D-045): new issues join
//! boards in mode `auto` when they are published, and the operations commands, search and
//! automation use name a sprint by role — the current one or the auto-add one.

use prostor_domain::access::Access;
use prostor_domain::boards::Mode;
use prostor_domain::boards::sprints::{self, SprintDates};
use prostor_domain::issues::history::{BoardPlace, Origin, Record};
use prostor_domain::issues::rights;
use prostor_domain::live::Change;
use prostor_domain::rank;
use prostor_domain::{BoardId, IssueId, SprintId};
use prostor_store::PgPool;
use prostor_store::boards::Sprint;
use prostor_store::boards::cards as store_cards;
use prostor_store::issues::Issue;
use sqlx::PgConnection;
use time::OffsetDateTime;

use super::cards::{self, NewCard, Placed, place};
use super::load;
use super::view::today;
use crate::issues::{author, facts, finish_change, lock_for_change};
use crate::live::Batch;
use crate::seed::internal;
use crate::{Error, Result};

/// Which sprint of a board an operation means (BRD-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A named sprint.
    Sprint(SprintId),
    /// The current sprint (BRD-099).
    Current,
    /// The auto-add sprint (BRD-026).
    Auto,
}

/// The sprint a target means on this board. A sprintless board has one place for cards: every
/// target means its hidden sprint.
fn resolve(
    sprints_on: bool,
    auto_sprint: Option<SprintId>,
    all: &[Sprint],
    target: Target,
    now: time::Date,
) -> Result<SprintId> {
    if !sprints_on {
        return all
            .iter()
            .find(|s| s.internal)
            .map(|s| s.id)
            .ok_or_else(|| internal("a sprintless board without its sprint"));
    }
    match target {
        Target::Sprint(id) => Ok(id),
        Target::Current => {
            let dates: Vec<SprintDates> = all.iter().map(Sprint::dates).collect();
            sprints::current(&dates, now)
                .ok_or(Error::Refused("the board has no current sprint".into()))
        }
        Target::Auto => auto_sprint
            .filter(|id| all.iter().any(|s| s.id == *id && s.archived_at.is_none()))
            .ok_or(Error::Refused(
                "the board has no sprint new issues join".into(),
            )),
    }
}

/// Adds an issue to the target sprint of a board (BRD-034), as [`cards::add`] does.
pub async fn add(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    target: Target,
    card: NewCard,
    now: OffsetDateTime,
) -> Result<crate::rules::Answer<Placed>> {
    let sprint = {
        let mut conn = pool.acquire().await?;
        let board = load(&mut conn, actor, board_id, true).await?;
        let day = today(&mut conn, now).await?;
        let b = &board.full.board;
        resolve(
            b.sprints_on,
            b.auto_sprint_id,
            &board.full.sprints,
            target,
            day,
        )?
    };
    cards::add(pool, actor, origin, board_id, sprint, card).await
}

/// Takes an issue off every sprint of a board (BRD-034); refused on a query board.
pub async fn remove_from_board(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    issue_id: IssueId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    if Mode::parse(&board.full.board.mode) == Some(Mode::Query) {
        return Err(Error::Refused(
            "issues leave this board through its query; change their fields instead".into(),
        ));
    }
    let (issue, _) = lock_for_change(&mut tx, actor, issue_id).await?;
    rights::may_update(actor, &facts(&issue))?;
    let places = store_cards::places(&mut *tx, board_id, issue_id).await?;
    let Some(first) = places.first() else {
        return Err(Error::NotFound("the issue is not on this board"));
    };
    let left = board
        .full
        .sprints
        .iter()
        .find(|s| s.id == first.sprint)
        .ok_or_else(|| internal("a card of an unknown sprint"))?;
    let records: Vec<Record> = Record::boards(Some(&place(&board.full.board, left)), None)
        .into_iter()
        .collect();
    for p in &places {
        store_cards::remove(&mut tx, p.sprint, issue_id).await?;
    }
    let who = author(actor, origin, false, issue.project_id)?;
    let mut batch = Batch::new();
    finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
    batch.issue_on_board(issue_id, Change::Updated, board_id);
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Puts a just published issue on the boards it joins on its own (BRD-026, BRD-027): boards in
/// mode `auto` with its project and a column for its column value, except those the draft was
/// kept off (ISS-007). Answers the history records; the caller writes them with the creation.
pub(crate) async fn on_published(
    conn: &mut PgConnection,
    batch: &mut Batch,
    issue: &Issue,
) -> Result<Vec<Record>> {
    let targets = store_cards::auto_targets(&mut *conn, issue.id).await?;
    store_cards::clear_skips(conn, issue.id).await?;
    let mut records = Vec::with_capacity(targets.len());
    for t in targets.into_iter().filter(|t| !t.skipped) {
        let last = store_cards::last_rank(&mut *conn, t.sprint_id).await?;
        let rank = rank::between(last.as_deref(), None).map_err(internal)?;
        store_cards::insert(conn, t.sprint_id, issue.id, &rank).await?;
        let joined = BoardPlace {
            board: (t.board_id, &t.board_name),
            sprint: (!t.internal).then_some((t.sprint_id, t.sprint_name.as_str())),
        };
        records.extend(Record::boards(None, Some(&joined)));
        batch.issue_on_board(issue.id, Change::Created, t.board_id);
    }
    Ok(records)
}
