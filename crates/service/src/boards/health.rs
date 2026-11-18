//! Board health (BRD-023) computed with the board, the check of query mode against the projects'
//! default column values (BRD-028), and which boards use a field (BRD-022).

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::boards::cards::{color_field_allowed, estimation_field_allowed};
use taskmaster_domain::boards::health::{self as domain, Facts, Finding, Slot, SlotField};
use taskmaster_domain::boards::lanes::{LaneMode, Source, parent_field_allowed};
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{FieldId, ProjectId, ValueId};
use taskmaster_store::PgPool;
use taskmaster_store::boards::{self as store, Full};
use taskmaster_store::fields::{Attached, Field};
use taskmaster_store::projects::Project;
use taskmaster_store::saved_searches as store_searches;
use taskmaster_store::value_sets::Value;

use crate::{Error, Result};

/// What the board is checked from; the parts `load` has read already.
pub(super) struct Inputs<'a> {
    pub full: &'a Full,
    pub column_values: &'a [Value],
    pub projects: &'a [Project],
    pub attached: &'a [Attached],
    /// The fields the card settings name and the lane field, those that exist.
    pub fields: &'a [Field],
    pub query_mode: bool,
}

fn kind_of(fields: &[Field], id: FieldId) -> Option<FieldKind> {
    fields
        .iter()
        .find(|f| f.id == id)
        .and_then(|f| FieldKind::parse(&f.kind))
}

/// The findings of the board for this viewer (BRD-023): one query when the board has a backlog.
pub(super) async fn check(
    conn: &mut PgConnection,
    viewer: &Access,
    i: Inputs<'_>,
) -> Result<Vec<Finding>> {
    let b = &i.full.board;
    let column = b.column_field_id;
    let projects = i
        .projects
        .iter()
        .map(|p| domain::ProjectFacts {
            id: p.id,
            archived: p.is_archived(),
            column: column.and_then(|c| {
                i.attached
                    .iter()
                    .find(|a| a.project_id == p.id && a.field_id == c)
                    .map(|a| domain::ColumnAttachment {
                        set: a.set_id,
                        defaults: a.default_values.clone(),
                    })
            }),
            fields: i
                .attached
                .iter()
                .filter(|a| a.project_id == p.id)
                .map(|a| a.field_id)
                .collect(),
        })
        .collect();
    let mut slots = Vec::new();
    let mut push = |slot, field: Option<FieldId>, fits: fn(FieldKind) -> bool| {
        if let Some(field) = field {
            let kind = kind_of(i.fields, field);
            slots.push(SlotField {
                slot,
                field,
                kind,
                fits: kind.is_some_and(fits),
            });
        }
    };
    let lane_fits: fn(FieldKind) -> bool =
        if LaneMode::parse(&b.lane_mode) == Some(LaneMode::Parents) {
            |k| parent_field_allowed(k).is_ok()
        } else {
            |k| Source::of_field(k).is_ok()
        };
    if LaneMode::parse(&b.lane_mode).is_some_and(|m| m != LaneMode::None) {
        push(Slot::Lane, b.lane_field_id, lane_fits);
    }
    if b.color_mode == taskmaster_domain::boards::ColorMode::Field.key() {
        push(Slot::Color, b.color_field_id, |k| {
            color_field_allowed(k).is_ok()
        });
    }
    push(Slot::Estimation, b.estimation_field_id, |k| {
        estimation_field_allowed(k).is_ok()
    });
    push(
        Slot::OriginalEstimation,
        b.original_estimation_field_id,
        |k| estimation_field_allowed(k).is_ok(),
    );
    let backlog_hidden = match b.backlog_search_id {
        Some(id) if b.sprints_on => store_searches::get(&mut *conn, id)
            .await?
            .is_some_and(|s| !s.sharing().can_view(viewer)),
        _ => false,
    };
    let columns = i
        .column_values
        .iter()
        .map(|v| domain::ColumnValue {
            id: v.id,
            set: v.set_id,
        })
        .collect();
    Ok(domain::check(&Facts {
        projects,
        column_field: column,
        columns,
        query_mode: i.query_mode,
        slots,
        backlog_hidden,
    }))
}

/// Refuses query mode while a board project gives new issues a column value that is no column
/// (BRD-028): they would never show on the board.
pub(super) async fn defaults_are_columns(
    conn: &mut PgConnection,
    column_field: Option<FieldId>,
    projects: &[ProjectId],
    columns: &[ValueId],
    path: &str,
) -> Result<()> {
    let Some(field) = column_field else {
        return Ok(());
    };
    let attached = taskmaster_store::fields::of_projects(&mut *conn, projects).await?;
    let stray = attached.iter().find(|a| {
        a.field_id == field
            && a.default_values
                .first()
                .is_some_and(|d| !columns.contains(d))
    });
    match stray {
        Some(a) => {
            let project = taskmaster_store::projects::get(&mut *conn, a.project_id).await?;
            let key = project.map(|p| p.key).unwrap_or_default();
            Err(Error::field(
                path,
                Problem::Invalid,
                format!(
                    "new issues of {key} get a column value that is no column of the board; \
                     add it as a column or change the project's default first"
                ),
            ))
        }
        None => Ok(()),
    }
}

pub use taskmaster_store::boards::FieldUse;

/// The boards using a field (BRD-022): those the viewer views, and how many others there are.
pub async fn field_usage(
    pool: &PgPool,
    viewer: &Access,
    field: FieldId,
) -> Result<(Vec<FieldUse>, i64)> {
    taskmaster_domain::fields::may_see_fields(viewer)?;
    let mut conn = pool.acquire().await?;
    taskmaster_store::fields::get(&mut *conn, field)
        .await?
        .ok_or(Error::NotFound("no such field"))?;
    let (seen, others): (Vec<FieldUse>, Vec<FieldUse>) = store::field_usage(&mut conn, field)
        .await?
        .into_iter()
        .partition(|u| u.board.access().can_view(viewer));
    Ok((seen, i64::try_from(others.len()).unwrap_or(i64::MAX)))
}

/// How many boards use a field, for its delete impact (BRD-022).
pub async fn field_boards(conn: &mut PgConnection, field: FieldId) -> Result<i64> {
    let found = store::field_usage(&mut *conn, field).await?;
    Ok(i64::try_from(found.len()).unwrap_or(i64::MAX))
}
