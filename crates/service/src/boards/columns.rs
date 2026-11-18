//! Columns of a board (BRD-042…BRD-046): add a value of the column field's set or a new value,
//! rename a value, move, set WIP limits, remove. A new value and a rename go through
//! `value_sets` and need its rights (FLD-031); they change the set for every project using it.

use serde_json::json;
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::boards::columns::{Wip, may_add};
use taskmaster_domain::rank;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{Actor, BoardId, ColumnId, SetId, ValueId};
use taskmaster_store::boards::{self as store, Column};
use taskmaster_store::fields as store_fields;
use taskmaster_store::value_sets;
use taskmaster_store::{PgPool, audit, unique_violation};

use super::{BoardRow, load, lock_edited};
use crate::seed::internal;
use crate::value_sets::{self as sets, ValueInput};
use crate::{Error, Result};

/// Where a column goes: right after another one, or first.
pub type Position = Option<ColumnId>;

/// A column to add: an existing value of the field's set, or a new value by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewColumn {
    Value(ValueId),
    NewValue(String),
}

/// A change of a column; `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnChange {
    pub position: Option<Position>,
    pub wip_min: Option<Option<i32>>,
    pub wip_max: Option<Option<i32>>,
    /// Renames the value (BRD-044).
    pub name: Option<String>,
}

/// The set hides itself from those who may not edit it; on a board they see its values, so the
/// answer is a refusal.
fn set_refusal(error: Error) -> Error {
    match error {
        Error::NotFound(_) => {
            Error::Refused("you may not change the value set of the column field".into())
        }
        other => other,
    }
}

fn target(id: BoardId) -> String {
    format!("board:{id}")
}

/// The sets of the column field in the board projects, by project ID.
async fn field_sets(conn: &mut PgConnection, board: &store::Board) -> Result<Vec<SetId>> {
    let Some(field) = board.column_field_id else {
        return Err(Error::Conflict("the board has no column field"));
    };
    let mut attached = store_fields::of_projects(&mut *conn, &board.projects).await?;
    attached.sort_by_key(|a| a.project_id);
    let mut out = Vec::new();
    for set in attached
        .iter()
        .filter(|a| a.field_id == field)
        .filter_map(|a| a.set_id)
    {
        if !out.contains(&set) {
            out.push(set);
        }
    }
    if out.is_empty() {
        return Err(Error::Conflict(
            "the column field is not attached to a board project",
        ));
    }
    Ok(out)
}

/// A rank right after `after`, or first; the column `skip` (the moving one) is left out.
fn rank_at(columns: &[Column], after: Position, skip: Option<ColumnId>) -> Result<String> {
    let rest: Vec<&Column> = columns.iter().filter(|c| Some(c.id) != skip).collect();
    let at = match after {
        None => 0,
        Some(id) => {
            rest.iter()
                .position(|c| c.id == id)
                .ok_or_else(|| Error::field("after", Problem::Invalid, "no such column"))?
                + 1
        }
    };
    let lower = at
        .checked_sub(1)
        .and_then(|i| rest.get(i))
        .map(|c| c.rank.as_str());
    let upper = rest.get(at).map(|c| c.rank.as_str());
    rank::between(lower, upper).map_err(internal)
}

/// Adds a column (BRD-043) after another one, or at the end.
pub async fn add(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    column: NewColumn,
    after: Option<ColumnId>,
) -> Result<BoardRow> {
    let value = match column {
        NewColumn::Value(value) => value,
        NewColumn::NewValue(name) => {
            // The value joins the set first, with the set's rights; then the column.
            let mut conn = pool.acquire().await?;
            let board = store::get(&mut *conn, id)
                .await?
                .filter(|b| b.access().can_edit(viewer))
                .ok_or_else(super::not_found)?;
            let set = field_sets(&mut conn, &board).await?[0];
            drop(conn);
            let row = sets::add_value(
                pool,
                viewer,
                set,
                ValueInput {
                    name: Some(name),
                    ..ValueInput::default()
                },
                None,
            )
            .await
            .map_err(set_refusal)?;
            row.values
                .iter()
                .map(|v| v.id)
                .max()
                .ok_or_else(|| internal("the new value is missing"))?
        }
    };
    let mut tx = pool.begin().await?;
    let board = lock_edited(&mut tx, viewer, id).await?;
    let sets = field_sets(&mut tx, &board).await?;
    let found = value_sets::get_values(&mut *tx, &[value]).await?;
    if !found
        .iter()
        .any(|v| v.id == value && sets.contains(&v.set_id))
    {
        return Err(Error::field(
            "value",
            Problem::Invalid,
            "the value is not in the column field's set of a board project",
        ));
    }
    let columns = store::columns::of_board(&mut *tx, id).await?;
    may_add(columns.len()).map_err(|r| Error::field("value", r.problem, r.message))?;
    let after = after.or_else(|| columns.last().map(|c| c.id));
    let rank = rank_at(&columns, after, None)?;
    store::columns::insert(&mut tx, id, value, &rank, None, None)
        .await
        .map_err(|e| match unique_violation(&e) {
            Some(store::columns::VALUE_KEY) => {
                Error::field("value", Problem::Taken, "the board has this column")
            }
            _ => e.into(),
        })?;
    store::touch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        Actor::User(viewer.user()),
        "board.column_added",
        &target(id),
        json!({ "value": value }),
    )
    .await?;
    super::changed(&mut tx, id).await?;
    tx.commit().await?;
    super::get(pool, viewer, id).await
}

/// Moves a column, sets its WIP limits (BRD-045, BRD-046) or renames its value (BRD-044).
pub async fn update(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    column: ColumnId,
    change: ColumnChange,
) -> Result<BoardRow> {
    if let Some(name) = &change.name {
        let mut conn = pool.acquire().await?;
        let board = store::get(&mut *conn, id)
            .await?
            .filter(|b| b.access().can_edit(viewer))
            .ok_or_else(super::not_found)?;
        let columns = store::columns::of_board(&mut *conn, board.id).await?;
        let found = columns
            .iter()
            .find(|c| c.id == column)
            .ok_or(Error::NotFound("no such column"))?;
        let value = value_sets::get_values(&mut *conn, &[found.value_id])
            .await?
            .pop()
            .ok_or(Error::NotFound("no such column"))?;
        drop(conn);
        sets::update_value(
            pool,
            viewer,
            value.set_id,
            value.id,
            ValueInput {
                name: Some(name.clone()),
                ..ValueInput::default()
            },
            value.version,
        )
        .await
        .map_err(set_refusal)?;
    }
    let mut tx = pool.begin().await?;
    lock_edited(&mut tx, viewer, id).await?;
    let columns = store::columns::of_board(&mut *tx, id).await?;
    let current = columns
        .iter()
        .find(|c| c.id == column)
        .ok_or(Error::NotFound("no such column"))?;
    let wip = Wip::new(
        change.wip_min.unwrap_or(current.wip_min),
        change.wip_max.unwrap_or(current.wip_max),
    )
    .map_err(|r| Error::field("wip", r.problem, r.message))?;
    let rank = match change.position {
        Some(after) if after == Some(column) => {
            return Err(Error::field(
                "after",
                Problem::Invalid,
                "a column cannot follow itself",
            ));
        }
        Some(after) => rank_at(&columns, after, Some(column))?,
        None => current.rank.clone(),
    };
    store::columns::update(&mut tx, column, &rank, wip.min, wip.max).await?;
    store::touch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        Actor::User(viewer.user()),
        "board.column_changed",
        &target(id),
        json!({
            "column": column,
            "moved": change.position.is_some(),
            "wip_min": wip.min,
            "wip_max": wip.max,
            "name": change.name,
        }),
    )
    .await?;
    super::changed(&mut tx, id).await?;
    tx.commit().await?;
    super::get(pool, viewer, id).await
}

/// Removes a column: its cards leave the board but keep their sprint (BRD-043).
pub async fn delete(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    column: ColumnId,
) -> Result<BoardRow> {
    let mut tx = pool.begin().await?;
    let board = lock_edited(&mut tx, viewer, id).await?;
    let columns = store::columns::of_board(&mut *tx, id).await?;
    if !columns.iter().any(|c| c.id == column) {
        return Err(Error::NotFound("no such column"));
    }
    if taskmaster_domain::boards::Mode::parse(&board.mode)
        == Some(taskmaster_domain::boards::Mode::Query)
    {
        // New issues of a query board must land in a column (BRD-028).
        let left: Vec<i64> = columns
            .iter()
            .filter(|c| c.id != column)
            .map(|c| c.value_id)
            .collect();
        super::health::defaults_are_columns(
            &mut tx,
            board.column_field_id,
            &board.projects,
            &left,
            "column",
        )
        .await?;
    }
    store::columns::delete(&mut tx, id, column).await?;
    store::touch(&mut tx, id).await?;
    audit::record(
        &mut tx,
        Actor::User(viewer.user()),
        "board.column_removed",
        &target(id),
        json!({ "column": column }),
    )
    .await?;
    super::changed(&mut tx, id).await?;
    tx.commit().await?;
    let mut conn = pool.acquire().await?;
    load(&mut conn, viewer, id, true).await
}
