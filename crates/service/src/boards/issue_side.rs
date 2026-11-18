//! Boards from the issue's side (BRD-033, ISS-020, ISS-007): which boards and sprints hold an
//! issue, adding it to one and taking it off. A draft can be put on boards too; the boards it
//! would join on its own when published are shown, and it can be kept off them.

use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::issues::rights;
use taskmaster_domain::{BoardId, IssueId, SprintId};
use taskmaster_store::PgPool;
use taskmaster_store::boards::cards as store_cards;
use taskmaster_store::boards::{self as store, Viewer};
use taskmaster_store::issues::{self as store_issues, Issue};
use time::OffsetDateTime;

use super::cards::NewCard;
use super::membership::{self, Target};
use crate::issues::{checked, facts, lock_for_change};
use crate::{Error, Result};

/// A board of the issue as its page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueBoard {
    pub board: BoardId,
    pub name: String,
    pub sprints_on: bool,
    /// The sprint holding it; `None` on a sprintless board.
    pub sprint: Option<(SprintId, String)>,
    /// A draft joins this board on its own when it is published (ISS-007).
    pub auto: bool,
    /// The draft was kept off this board: it will not join when published.
    pub skipped: bool,
}

/// The boards of an issue the viewer views (BRD-033): the sprints holding it, then, for a draft,
/// the boards it would join on publishing (kept-off ones marked).
pub async fn of_issue(pool: &PgPool, viewer: &Access, id: IssueId) -> Result<Vec<IssueBoard>> {
    let mut conn = pool.acquire().await?;
    let found = store_issues::get(&mut *conn, id).await?;
    let (issue, _) = checked(&mut conn, viewer, found).await?;
    boards_of(&mut conn, viewer, &issue).await
}

/// The same on a connection, for an issue already checked.
pub(crate) async fn boards_of(
    conn: &mut sqlx::PgConnection,
    viewer: &Access,
    issue: &Issue,
) -> Result<Vec<IssueBoard>> {
    let held = store_cards::of_issue(&mut *conn, issue.id).await?;
    let auto = if issue.is_draft() {
        store_cards::auto_targets(&mut *conn, issue.id).await?
    } else {
        Vec::new()
    };
    let mut ids: Vec<BoardId> = held.iter().map(|m| m.board_id).collect();
    ids.extend(auto.iter().map(|t| t.board_id));
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let boards = store::get_many(&mut *conn, &ids).await?;
    let read = viewer.projects_with(Permission::IssueRead);
    let reader = Viewer {
        me: viewer.user(),
        groups: viewer.groups(),
        read: &read,
    };
    let seen = |id: BoardId| boards.iter().find(|b| b.id == id && reader.views(b));
    let mut out: Vec<IssueBoard> = held
        .iter()
        .filter_map(|m| {
            let b = seen(m.board_id)?;
            Some(IssueBoard {
                board: b.id,
                name: b.name.clone(),
                sprints_on: b.sprints_on,
                sprint: (!m.internal).then(|| (m.sprint_id, m.sprint_name.clone())),
                auto: false,
                skipped: false,
            })
        })
        .collect();
    out.extend(auto.iter().filter_map(|t| {
        let b = seen(t.board_id)?;
        Some(IssueBoard {
            board: b.id,
            name: b.name.clone(),
            sprints_on: b.sprints_on,
            sprint: (!t.internal).then(|| (t.sprint_id, t.sprint_name.clone())),
            auto: true,
            skipped: t.skipped,
        })
    }));
    Ok(out)
}

/// Puts the issue on a board (BRD-033, ISS-020): into `sprint`, else the current sprint of a
/// sprint board. A draft kept off a board it joins on its own is let back on instead.
pub async fn add(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: IssueId,
    board: BoardId,
    sprint: Option<SprintId>,
    now: OffsetDateTime,
) -> Result<Vec<IssueBoard>> {
    let issue = {
        let mut conn = pool.acquire().await?;
        let found = store_issues::get(&mut *conn, id).await?;
        checked(&mut conn, actor, found).await?.0
    };
    if issue.is_draft() && sprint.is_none() {
        let mut tx = pool.begin().await?;
        super::load(&mut tx, actor, board, true).await?;
        let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
        rights::may_update(actor, &facts(&issue))?;
        let auto = store_cards::auto_targets(&mut *tx, id).await?;
        if auto.iter().any(|t| t.board_id == board) {
            store_cards::set_skip(&mut tx, id, board, false).await?;
            tx.commit().await?;
            return of_issue(pool, actor, id).await;
        }
    }
    let target = sprint.map_or(Target::Current, Target::Sprint);
    let card = NewCard {
        issue: id,
        ..NewCard::default()
    };
    membership::add(pool, actor, origin, board, target, card, now).await?;
    of_issue(pool, actor, id).await
}

/// Takes the issue off a board (BRD-033, ISS-020). A draft is also kept off the board if it would
/// join it on its own when published (ISS-007).
pub async fn remove(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    id: IssueId,
    board: BoardId,
) -> Result<Vec<IssueBoard>> {
    let draft = {
        let mut conn = pool.acquire().await?;
        let found = store_issues::get(&mut *conn, id).await?;
        checked(&mut conn, actor, found).await?.0.is_draft()
    };
    if !draft {
        membership::remove_from_board(pool, actor, origin, board, id).await?;
        return of_issue(pool, actor, id).await;
    }
    let mut tx = pool.begin().await?;
    super::load(&mut tx, actor, board, true).await?;
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    rights::may_update(actor, &facts(&issue))?;
    let places = store_cards::places(&mut *tx, board, id).await?;
    for p in &places {
        store_cards::remove(&mut tx, p.sprint, id).await?;
    }
    let auto = store_cards::auto_targets(&mut *tx, id).await?;
    let joins = auto.iter().any(|t| t.board_id == board);
    if joins {
        store_cards::set_skip(&mut tx, id, board, true).await?;
    }
    if places.is_empty() && !joins {
        return Err(Error::NotFound("the issue is not on this board"));
    }
    tx.commit().await?;
    of_issue(pool, actor, id).await
}
