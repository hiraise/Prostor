//! Cell drafts (BRD-077): the «+» of a cell opens the new-issue form on a draft that already has
//! the cell's column and lane, like the form's own drafts (D-041). Each person keeps one draft
//! per cell until it is published or deleted.

use trackflow_domain::access::Access;
use trackflow_domain::boards::lanes::LaneKey;
use trackflow_domain::issues::history::Origin;
use trackflow_domain::issues::rights;
use trackflow_domain::validation::Problem;
use trackflow_domain::{BoardId, ColumnId, IssueId, ProjectId, SprintId};
use trackflow_store::PgPool;
use trackflow_store::boards as store;
use trackflow_store::issues as store_issues;

use super::cards::{self, NewCard};
use super::view::is_query;
use super::{load, lock_viewed};
use crate::issues::{FieldInput, NewIssue};
use crate::{Error, Result};

/// The cell a draft is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub sprint: SprintId,
    pub column: ColumnId,
    pub lane: Option<String>,
    /// The project of a new draft; absent takes the lane's project, else the first board project
    /// the caller creates issues in.
    pub project: Option<ProjectId>,
}

fn key(cell: &Cell) -> String {
    format!(
        "{}:{}:{}",
        cell.sprint,
        cell.column,
        cell.lane.as_deref().unwrap_or("-")
    )
}

/// The caller's draft of the cell: the kept one while it is still their draft, else a new one
/// with the project's defaults, the cell's column value and lane, already a card of the sprint
/// (it shows once published, ISS-007). Answers the draft and whether it is new.
pub async fn cell_draft(
    pool: &PgPool,
    viewer: &Access,
    origin: Origin,
    board_id: BoardId,
    cell: Cell,
) -> Result<(IssueId, bool)> {
    let mut conn = pool.acquire().await?;
    let board = load(&mut conn, viewer, board_id, true).await?;
    super::cards::sprint_of(&board, cell.sprint)?;
    let column = board
        .full
        .columns
        .iter()
        .find(|c| c.id == cell.column)
        .ok_or_else(|| Error::field("column", Problem::Invalid, "no such column"))?;
    let profile = store::profiles::get(&mut *conn, viewer.user(), board_id).await?;
    let slot = key(&cell);
    if let Some(kept) = profile.cell_drafts.0.get(&slot).copied() {
        let found = store_issues::get(&mut *conn, kept).await?;
        if found.is_some_and(|i| i.is_draft() && i.reporter_id == viewer.user()) {
            return Ok((kept, false));
        }
    }
    let lane_project = match cell.lane.as_deref().and_then(LaneKey::parse) {
        Some(LaneKey::Project(p)) => Some(p),
        _ => None,
    };
    let projects = &board.full.board.projects;
    let project = match cell.project.or(lane_project) {
        Some(p) if projects.contains(&p) => p,
        Some(_) => {
            return Err(Error::field(
                "project",
                Problem::Invalid,
                "the project is not on the board",
            ));
        }
        None => board
            .projects
            .iter()
            .find(|p| !p.is_archived() && rights::may_create(viewer, p.id).is_ok())
            .map(|p| p.id)
            .ok_or(Error::Refused(
                "you do not create issues in any project of this board".into(),
            ))?,
    };
    let query = is_query(&board);
    // A query board takes the draft by its fields: the column value is set on the draft itself.
    let fields = match (query, board.full.board.column_field_id) {
        (true, Some(field)) => vec![FieldInput {
            field,
            value: Some(trackflow_domain::issues::values::Input::Ids(vec![
                column.value_id,
            ])),
        }],
        _ => Vec::new(),
    };
    drop(conn);
    let draft = crate::issues::create(
        pool,
        viewer,
        origin,
        NewIssue {
            project,
            fields,
            ..NewIssue::default()
        },
    )
    .await?;
    let id = draft.done.0.issue.id;
    if !query {
        cards::add(
            pool,
            viewer,
            origin,
            board_id,
            cell.sprint,
            NewCard {
                issue: id,
                column: Some(cell.column),
                lane: cell.lane.clone(),
                ..NewCard::default()
            },
        )
        .await?;
    }
    let mut tx = pool.begin().await?;
    lock_viewed(&mut tx, viewer, board_id).await?;
    let mut profile = store::profiles::get(&mut *tx, viewer.user(), board_id).await?;
    // Drafts of cells whose drafts are gone are forgotten along the way.
    let mine = store_issues::draft_ids_of(&mut *tx, &[viewer.user()]).await?;
    profile.cell_drafts.0.retain(|_, d| mine.contains(d));
    profile.cell_drafts.0.insert(slot, id);
    store::profiles::upsert(&mut tx, viewer.user(), board_id, &profile).await?;
    tx.commit().await?;
    Ok((id, true))
}
