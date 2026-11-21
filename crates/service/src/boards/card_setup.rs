//! Card settings (BRD-059…BRD-063; plan 1.8 §3.16): the fields cards show, the colors and the
//! estimation, with private fields kept from viewers who cannot read them (FLD-062).

use prostor_domain::access::Access;
use prostor_domain::boards::cards as domain;
use prostor_domain::boards::presets::Attached as PresetField;
use prostor_domain::boards::{self as boards, ColorMode};
use prostor_domain::fields::FieldKind;
use prostor_domain::fields::builtin::BuiltinField;
use prostor_domain::issues::rights;
use prostor_domain::validation::Problem;
use prostor_domain::{FieldId, ProjectId};
use prostor_store::boards::view::Estimate;
use prostor_store::boards::{self as store, Settings};
use prostor_store::fields::{Attached, Field};
use sqlx::PgConnection;

use super::{BoardRow, rejected};
use crate::{Error, Result};

/// The card settings to keep (BRD-059…BRD-062); fields hidden from the caller stay as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardsChange {
    /// Shown on cards, in order (at most [`boards::CARD_FIELDS_MAX`]).
    pub fields: Vec<FieldId>,
    /// Of `fields`, those shown as a first letter.
    pub initials: Vec<FieldId>,
    pub color: ColorMode,
    pub color_field: Option<FieldId>,
    pub estimation: Option<FieldId>,
    pub original_estimation: Option<FieldId>,
    /// Card colors of board projects; projects left out keep theirs.
    pub project_colors: Vec<(ProjectId, Option<i16>)>,
}

/// Fields of the board projects the viewer may not read (FLD-062): private wherever they are
/// attached to a board project the viewer does not read private fields in.
pub(super) fn hidden_fields(viewer: &Access, attached: &[Attached]) -> Vec<FieldId> {
    let mut ids: Vec<FieldId> = attached.iter().map(|a| a.field_id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.retain(|id| {
        !attached
            .iter()
            .filter(|a| a.field_id == *id)
            .any(|a| !a.private || rights::may_read_private(viewer, a.project_id))
    });
    ids
}

/// The fields the card settings name, for one batch read.
pub(super) fn named(board: &store::Board) -> Vec<FieldId> {
    let mut ids = board.card_fields.clone();
    ids.extend(board.color_field_id);
    ids.extend(board.estimation_field_id);
    ids.extend(board.original_estimation_field_id);
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn kind_of(fields: &[Field], id: FieldId) -> Option<FieldKind> {
    fields
        .iter()
        .find(|f| f.id == id)
        .and_then(|f| FieldKind::parse(&f.kind))
}

/// What the view sums (BRD-062): the estimation field when it exists, fits and the viewer reads
/// it.
pub(super) fn estimate(board: &BoardRow) -> Option<Estimate> {
    let field = board.estimation_field()?;
    if board.hidden.contains(&field) {
        return None;
    }
    match kind_of(&board.fields, field)? {
        FieldKind::Float => Some(Estimate { field, float: true }),
        FieldKind::Integer | FieldKind::Duration => Some(Estimate {
            field,
            float: false,
        }),
        _ => None,
    }
}

/// The card fields of a new board (BRD-059) from the first project's fields.
pub(super) fn default_fields(first: &[PresetField], taken: &[FieldId]) -> Vec<FieldId> {
    let candidates: Vec<domain::Candidate> = first
        .iter()
        .map(|f| domain::Candidate {
            id: f.field,
            kind: f.kind,
            assignee: f.builtin == Some(BuiltinField::Assignee),
        })
        .collect();
    domain::default_fields(&candidates, taken)
}

/// A field one of the settings may name: attached to a board project and readable by the caller.
fn offered(path: &str, id: FieldId, attached: &[Attached], hidden: &[FieldId]) -> Result<()> {
    if !attached.iter().any(|a| a.field_id == id) || hidden.contains(&id) {
        return Err(Error::field(
            path,
            Problem::Invalid,
            "the field is not in a board project",
        ));
    }
    Ok(())
}

/// A slot's new field: a hidden field there stays when the caller sends none, since they did not
/// see it.
fn slot(
    path: &str,
    stored: Option<FieldId>,
    given: Option<FieldId>,
    attached: &[Attached],
    hidden: &[FieldId],
) -> Result<Option<FieldId>> {
    match given {
        None if stored.is_some_and(|s| hidden.contains(&s)) => Ok(stored),
        None => Ok(None),
        Some(id) if Some(id) == stored => Ok(Some(id)),
        Some(id) => {
            offered(path, id, attached, hidden)?;
            Ok(Some(id))
        }
    }
}

/// Validates the card settings and writes them into `settings`; answers the project colors to set.
pub(super) async fn check_change(
    conn: &mut PgConnection,
    viewer: &Access,
    board: &store::Board,
    projects: &[ProjectId],
    change: CardsChange,
    settings: &mut Settings,
) -> Result<Vec<(ProjectId, Option<i16>)>> {
    let attached = prostor_store::fields::of_projects(&mut *conn, projects).await?;
    let hidden = hidden_fields(viewer, &attached);
    let given = boards::card_fields(change.fields).map_err(rejected("cards.fields"))?;
    for id in &given {
        if !board.card_fields.contains(id) {
            offered("cards.fields", *id, &attached, &hidden)?;
        }
    }
    // Hidden fields the caller could not see stay where they were.
    let mut fields = given.clone();
    let kept: Vec<FieldId> = board
        .card_fields
        .iter()
        .copied()
        .filter(|f| hidden.contains(f) && !given.contains(f))
        .collect();
    fields.extend(kept.iter().copied());
    let fields = boards::card_fields(fields).map_err(rejected("cards.fields"))?;
    let mut initials = change.initials;
    initials.extend(
        board
            .card_initials
            .iter()
            .copied()
            .filter(|f| kept.contains(f)),
    );
    let initials = domain::initials(&fields, &initials).map_err(rejected("cards.initials"))?;
    let color_field = slot(
        "cards.color_field",
        board.color_field_id,
        change.color_field,
        &attached,
        &hidden,
    )?;
    let estimation = slot(
        "cards.estimation",
        board.estimation_field_id,
        change.estimation,
        &attached,
        &hidden,
    )?;
    let original = slot(
        "cards.original_estimation",
        board.original_estimation_field_id,
        change.original_estimation,
        &attached,
        &hidden,
    )?;
    let mut ids: Vec<FieldId> = color_field.into_iter().collect();
    ids.extend(estimation);
    ids.extend(original);
    let defs = prostor_store::fields::get_many(&mut *conn, &ids).await?;
    let kind = |id: FieldId, path: &str| {
        kind_of(&defs, id).ok_or_else(|| Error::field(path, Problem::Invalid, "no such field"))
    };
    if change.color == ColorMode::Field {
        let Some(id) = color_field else {
            return Err(Error::field(
                "cards.color_field",
                Problem::Required,
                "choose the field whose values color the cards",
            ));
        };
        domain::color_field_allowed(kind(id, "cards.color_field")?)
            .map_err(rejected("cards.color_field"))?;
    }
    for (id, path) in [
        (estimation, "cards.estimation"),
        (original, "cards.original_estimation"),
    ] {
        if let Some(id) = id
            && Some(id) != stored_slot(board, path)
        {
            domain::estimation_field_allowed(kind(id, path)?).map_err(rejected(path))?;
        }
    }
    let mut colors = Vec::with_capacity(change.project_colors.len());
    for (project, color) in change.project_colors {
        if !projects.contains(&project) {
            return Err(Error::field(
                "cards.project_colors",
                Problem::Invalid,
                "the project is not on the board",
            ));
        }
        let color = domain::project_color(color).map_err(rejected("cards.project_colors"))?;
        colors.push((project, color));
    }
    settings.card_fields = fields;
    settings.card_initials = initials;
    settings.color_mode = change.color.key().to_owned();
    settings.color_field = color_field;
    settings.estimation_field = estimation;
    settings.original_estimation_field = original;
    Ok(colors)
}

fn stored_slot(board: &store::Board, path: &str) -> Option<FieldId> {
    if path == "cards.estimation" {
        board.estimation_field_id
    } else {
        board.original_estimation_field_id
    }
}
