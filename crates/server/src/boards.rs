//! `/boards`: boards, their columns, each person's view and the board view (BRD, step 1.8).
//! Rules live in `prostor_service::boards`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use prostor_api::boards::{
    BacklogMissing, BacklogOrder, BacklogSearch, Board, BoardBacklog, BoardBacklogQuery, BoardCell,
    BoardCellQuery, BoardColumn, BoardColumnPatch, BoardImpact, BoardList, BoardListQuery,
    BoardMode, BoardName, BoardPatch, BoardPreset, BoardProfile, BoardProfilePatch, BoardProject,
    BoardRights, BoardSharing, BoardSharingInput, BoardSummary, BoardTotals, BoardView,
    BoardViewQuery, Card, CardBatch, CardBatchAnswer, CardChange, CardField, CardIssue, CardMove,
    CardRefs, ColumnCount, IssueBoard, IssueBoards, Lane, LaneAttribute, LaneMode, LaneMove,
    LaneRemoved, NewBoard, NewBoardColumn, NewCard, NewIssueBoard, NewLane, NewSprint, OtherLane,
    PlacedCard, PlacedLane, Sprint, SprintCarry, SprintList, SprintName, SprintPatch,
    TruncatedCell, WipState,
};
use prostor_api::boards::{
    BoardCards, BoardCardsInput, BoardHealth, BoardLanes, BoardLanesInput, CardColor, CellDraft,
    FieldBoardUse, FieldBoards, HealthKind, HealthSeverity, HealthSlot, LaneLabel, NewCellDraft,
};
use prostor_api::error::ErrorBody;
use prostor_api::page::{Page, PageQuery};
use prostor_domain::access::Access;
use prostor_domain::auth::Scope;
use prostor_domain::boards::ColorMode;
use prostor_domain::boards::columns::WipState as DomainWip;
use prostor_domain::boards::health::{Finding, Severity, Slot};
use prostor_domain::boards::lanes as domain_lanes;
use prostor_domain::boards::query_mode::Step;
use prostor_domain::boards::sprints;
use prostor_domain::boards::{Mode, Preset};
use prostor_domain::settings::SYSTEM_LANGUAGE;
use prostor_domain::sharing::Shared;
use prostor_domain::{BoardId, ColumnId, IssueId, SprintId};
use prostor_service::boards::backlog::{self as service_backlog, BacklogInput};
use prostor_service::boards::card_setup::CardsChange;
use prostor_service::boards::cards::{self as cards, Move, Placed};
use prostor_service::boards::columns::{self as columns, ColumnChange, NewColumn};
use prostor_service::boards::issue_side;
use prostor_service::boards::lanes::{self as service_lanes, LanesChange};
use prostor_service::boards::sprints::{self as service_sprints, SprintChange, SprintRow};
use prostor_service::boards::view as service_view;
use prostor_service::boards::{
    self as service, BoardChange, BoardRow, LaneNames, ProfileChange, Sharing,
};
use prostor_service::boards::{drafts as service_drafts, health as service_health};
use prostor_service::issues::Described;
use prostor_service::people::Names;
use prostor_service::tags;
use prostor_store::boards::{After, Profile};
use time::{Date, OffsetDateTime};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::fields::field_ref;
use crate::issues::{held_value, origin, tag_ref};
use crate::present::{group_ref, project_ref, user_ref};
use crate::sharing::{Shown, shared_with};

const MAX_LIMIT: u32 = 100;

fn preset(p: BoardPreset) -> Preset {
    match p {
        BoardPreset::Scrum => Preset::Scrum,
        BoardPreset::Kanban => Preset::Kanban,
        BoardPreset::Custom => Preset::Custom,
    }
}

fn preset_key(key: &str) -> BoardPreset {
    match Preset::parse(key) {
        Some(Preset::Scrum) => BoardPreset::Scrum,
        Some(Preset::Kanban) => BoardPreset::Kanban,
        _ => BoardPreset::Custom,
    }
}

fn mode(m: BoardMode) -> Mode {
    match m {
        BoardMode::Manual => Mode::Manual,
        BoardMode::Auto => Mode::Auto,
        BoardMode::Query => Mode::Query,
    }
}

fn mode_key(key: &str) -> BoardMode {
    match Mode::parse(key) {
        Some(Mode::Auto) => BoardMode::Auto,
        Some(Mode::Query) => BoardMode::Query,
        _ => BoardMode::Manual,
    }
}

fn lane_mode(key: &str) -> LaneMode {
    match domain_lanes::LaneMode::parse(key) {
        Some(domain_lanes::LaneMode::Values) => LaneMode::Values,
        Some(domain_lanes::LaneMode::Parents) => LaneMode::Parents,
        _ => LaneMode::None,
    }
}

fn attribute_key(key: &str) -> Option<LaneAttribute> {
    Some(match domain_lanes::Attribute::parse(key)? {
        domain_lanes::Attribute::Project => LaneAttribute::Project,
        domain_lanes::Attribute::Tags => LaneAttribute::Tags,
        domain_lanes::Attribute::Created => LaneAttribute::Created,
        domain_lanes::Attribute::Updated => LaneAttribute::Updated,
    })
}

fn other_key(key: &str) -> OtherLane {
    match domain_lanes::OtherLane::parse(key) {
        Some(domain_lanes::OtherLane::Off) => OtherLane::Off,
        Some(domain_lanes::OtherLane::Top) => OtherLane::Top,
        _ => OtherLane::Bottom,
    }
}

fn lanes_change(input: BoardLanesInput) -> LanesChange {
    LanesChange {
        mode: match input.mode {
            LaneMode::None => domain_lanes::LaneMode::None,
            LaneMode::Values => domain_lanes::LaneMode::Values,
            LaneMode::Parents => domain_lanes::LaneMode::Parents,
        },
        field: input.field,
        attribute: input.attribute.map(|a| match a {
            LaneAttribute::Project => domain_lanes::Attribute::Project,
            LaneAttribute::Tags => domain_lanes::Attribute::Tags,
            LaneAttribute::Created => domain_lanes::Attribute::Created,
            LaneAttribute::Updated => domain_lanes::Attribute::Updated,
        }),
        values: input.values,
        other: match input.other {
            OtherLane::Off => domain_lanes::OtherLane::Off,
            OtherLane::Top => domain_lanes::OtherLane::Top,
            OtherLane::Bottom => domain_lanes::OtherLane::Bottom,
        },
        new_card_value: input.new_card_value,
    }
}

fn lane(l: &service_view::Lane) -> Lane {
    Lane {
        key: l.key.clone(),
        count: l.count,
        estimate: l.estimate,
        issue: l.issue,
    }
}

fn color_mode(c: CardColor) -> ColorMode {
    match c {
        CardColor::None => ColorMode::None,
        CardColor::Project => ColorMode::Project,
        CardColor::Field => ColorMode::Field,
    }
}

fn cards_change(input: BoardCardsInput) -> CardsChange {
    CardsChange {
        fields: input.fields,
        initials: input.initials,
        color: color_mode(input.color),
        color_field: input.color_field,
        estimation: input.estimation,
        original_estimation: input.original_estimation,
        project_colors: input
            .project_colors
            .iter()
            .map(|c| (c.project, c.color))
            .collect(),
    }
}

/// The card settings as the caller may see them (FLD-062).
fn cards_of(row: &BoardRow) -> BoardCards {
    let b = &row.full.board;
    let shown = |id: Option<i64>| {
        let id = id.filter(|id| row.shown(*id))?;
        row.fields.iter().find(|f| f.id == id).map(field_ref)
    };
    let fields: Vec<_> = b
        .card_fields
        .iter()
        .filter_map(|id| shown(Some(*id)))
        .collect();
    BoardCards {
        initials: b
            .card_initials
            .iter()
            .copied()
            .filter(|id| fields.iter().any(|f| f.id == *id))
            .collect(),
        fields,
        color: match ColorMode::parse(&b.color_mode) {
            Some(ColorMode::Project) => CardColor::Project,
            Some(ColorMode::Field) => CardColor::Field,
            _ => CardColor::None,
        },
        color_field: shown(b.color_field_id),
        estimation: shown(b.estimation_field_id),
        original_estimation: shown(b.original_estimation_field_id),
    }
}

fn health(f: Finding) -> BoardHealth {
    let slot = |s: Slot| match s {
        Slot::Lane => HealthSlot::Lane,
        Slot::Color => HealthSlot::Color,
        Slot::Estimation => HealthSlot::Estimation,
        Slot::OriginalEstimation => HealthSlot::OriginalEstimation,
    };
    let (kind, project, value, s) = match f {
        Finding::NoProjects => (HealthKind::NoProjects, None, None, None),
        Finding::NoColumnField => (HealthKind::NoColumnField, None, None, None),
        Finding::NoColumns => (HealthKind::NoColumns, None, None, None),
        Finding::ColumnFieldMissing { project } => {
            (HealthKind::ColumnFieldMissing, Some(project), None, None)
        }
        Finding::ColumnValueMissing { project, value } => (
            HealthKind::ColumnValueMissing,
            Some(project),
            Some(value),
            None,
        ),
        Finding::ArchivedProject { project } => {
            (HealthKind::ArchivedProject, Some(project), None, None)
        }
        Finding::FieldMissing { slot: x } => (HealthKind::FieldMissing, None, None, Some(slot(x))),
        Finding::FieldWrongKind { slot: x } => {
            (HealthKind::FieldWrongKind, None, None, Some(slot(x)))
        }
        Finding::BacklogHidden => (HealthKind::BacklogHidden, None, None, None),
        Finding::DefaultNotColumn { project } => {
            (HealthKind::DefaultNotColumn, Some(project), None, None)
        }
    };
    BoardHealth {
        severity: match f.severity() {
            Severity::Error => HealthSeverity::Error,
            Severity::Warning => HealthSeverity::Warning,
        },
        kind,
        project,
        value,
        slot: s,
    }
}

fn wip(state: DomainWip) -> WipState {
    match state {
        DomainWip::Within => WipState::Within,
        DomainWip::Under => WipState::Under,
        DomainWip::Over => WipState::Over,
    }
}

fn sharing(input: BoardSharingInput) -> Sharing {
    Sharing {
        view: shared_with(input.view),
        edit: shared_with(input.edit),
        view_readers: input.view_readers,
        edit_updaters: input.edit_updaters,
    }
}

fn profile(p: &Profile) -> BoardProfile {
    BoardProfile {
        favorite: p.favorite,
        card_size: p.card_size,
        collapsed_columns: p.collapsed_columns.clone(),
        collapsed_lanes: p.collapsed_lanes.clone(),
    }
}

fn sprint(s: &prostor_store::boards::Sprint, current: Option<i64>, today: Date) -> Sprint {
    Sprint {
        finished: sprints::finished(&s.dates(), today),
        id: s.id,
        name: s.name.clone(),
        goal: s.goal.clone(),
        starts_on: s.starts_on,
        ends_on: s.ends_on,
        finished_at: s.finished_at,
        archived: s.archived_at.is_some(),
        current: current == Some(s.id),
        internal: s.internal,
        version: s.version,
    }
}

/// A board with its owner and audiences: one load of people.
/// The names of the lane keys a board keeps (BRD-053).
fn lane_labels(names: &Names, viewer: &Access, found: &LaneNames) -> Vec<LaneLabel> {
    let values = found.values.iter().map(|v| LaneLabel {
        key: format!("v:{}", v.id),
        name: v.name.clone(),
        color: v.color,
    });
    let users = found.users.iter().map(|u| LaneLabel {
        key: format!("u:{}", u.id),
        name: user_ref(names, viewer, u).full_name,
        color: None,
    });
    let groups = found.groups.iter().map(|g| LaneLabel {
        key: format!("g:{}", g.id),
        name: g.name.clone(),
        color: None,
    });
    let tags = found.tags.iter().map(|t| LaneLabel {
        key: format!("t:{}", t.id),
        name: t.name.clone(),
        color: t.color,
    });
    values.chain(users).chain(groups).chain(tags).collect()
}

async fn board(state: &AppState, viewer: &Access, row: &BoardRow) -> Result<Board, ApiError> {
    let shown_names = state.names();
    let b = &row.full.board;
    let shared = b.sharing();
    let parties = tags::parties(&state.db, std::slice::from_ref(&shared)).await?;
    let shown = Shown::new(state.names(), viewer, parties);
    // FLD-062: a field the viewer may not read leaves the answer entirely — its name, its
    // values' names, and the IDs that would reveal them. A missing field hides nothing.
    let column_shown = row.column_field.as_ref().is_none_or(|f| row.shown(f.id));
    let lane_shown = row.lane_field.as_ref().is_none_or(|f| row.shown(f.id));
    Ok(Board {
        id: b.id,
        name: b.name.clone(),
        owner: shown.owner(&shared)?,
        preset: preset_key(&b.preset),
        sprints_on: b.sprints_on,
        mode: mode_key(&b.mode),
        auto_sprint: b.auto_sprint_id,
        query: row.query.clone(),
        backlog: b.backlog_search_id,
        query_problems: row
            .query
            .as_deref()
            .map(|text| crate::search::problems(text, &row.query_problems))
            .unwrap_or_default(),
        column_field: row
            .column_field
            .as_ref()
            .filter(|f| row.shown(f.id))
            .map(field_ref),
        cards: cards_of(row),
        health: row
            .health
            .iter()
            .map(|f| {
                let mut found = health(*f);
                if !column_shown && found.kind == HealthKind::ColumnValueMissing {
                    found.value = None;
                }
                found
            })
            .collect(),
        lanes: if lane_shown {
            BoardLanes {
                mode: lane_mode(&b.lane_mode),
                field: row
                    .lane_field
                    .as_ref()
                    .filter(|f| row.shown(f.id))
                    .map(field_ref),
                attribute: b.lane_attribute.as_deref().and_then(attribute_key),
                values: b.lane_values.clone(),
                other: other_key(&b.other_lane),
                new_card_value: b.new_card_value_id,
                labels: lane_labels(&shown_names, viewer, &row.lane_names),
            }
        } else {
            BoardLanes {
                mode: LaneMode::None,
                field: None,
                attribute: None,
                values: Vec::new(),
                other: OtherLane::Off,
                new_card_value: None,
                labels: Vec::new(),
            }
        },
        columns: if column_shown {
            row.full
                .columns
                .iter()
                .zip(&row.column_values)
                .map(|(c, v)| BoardColumn {
                    id: c.id,
                    value: held_value(v),
                    wip_min: c.wip_min,
                    wip_max: c.wip_max,
                })
                .collect()
        } else {
            Vec::new()
        },
        projects: b
            .projects
            .iter()
            .zip(&b.project_colors)
            .filter_map(|(id, color)| {
                let p = row.projects.iter().find(|p| p.id == *id)?;
                Some(BoardProject {
                    project: project_ref(p),
                    color: *color,
                })
            })
            .collect(),
        sharing: row.rights.edit.then(|| BoardSharing {
            view: shown.audience(&shared.view),
            edit: shown.audience(&shared.edit),
            view_readers: b.view_readers,
            edit_updaters: b.edit_updaters,
        }),
        rights: BoardRights {
            edit: row.rights.edit,
            share: row.rights.share,
            transfer: row.rights.transfer,
            delete: row.rights.delete,
        },
        favorite: row.profile.favorite,
        created_at: b.created_at,
        updated_at: b.updated_at,
        version: b.version,
    })
}

/// The start of a description a card shows: three lines, at most 300 characters.
fn excerpt(text: &str) -> String {
    let lines: Vec<&str> = text.lines().take(3).collect();
    lines.join("\n").chars().take(300).collect()
}

/// The issues of an answer as cards show them, with what they name (D-044).
fn card_issues(state: &AppState, viewer: &Access, d: &Described) -> (Vec<CardIssue>, CardRefs) {
    use prostor_domain::fields::FieldKind;
    use prostor_domain::issues::values::Value;
    use std::collections::BTreeSet;
    let names = state.names();
    let (mut fields, mut values, mut users, mut groups) = (
        Vec::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    let mut seen_fields = BTreeSet::new();
    let mut out = Vec::with_capacity(d.issues.len());
    for v in &d.issues {
        let mut shown = Vec::new();
        for f in &v.fields {
            let Some(value) = &f.value else { continue };
            let (mut ids, mut text, mut number) = (Vec::new(), None, None);
            match value {
                Value::Text(_) => continue,
                Value::String(t) => text = Some(t.clone()),
                Value::Integer(n) | Value::Duration(n) => number = Some(*n as f64),
                Value::Float(n) => number = Some(*n),
                Value::Date(day) => text = Some(day.to_string()),
                Value::DateTime(at) => {
                    text = at
                        .format(&time::format_description::well_known::Rfc3339)
                        .ok();
                }
                Value::Refs(refs) => {
                    ids.clone_from(refs);
                    let into = match f.field.kind.kind {
                        FieldKind::User => &mut users,
                        FieldKind::Group => &mut groups,
                        _ => &mut values,
                    };
                    into.extend(refs.iter().copied());
                }
            }
            if seen_fields.insert(f.field.id()) {
                fields.push(field_ref(&f.field.field));
            }
            shown.push(CardField {
                field: f.field.id(),
                ids,
                text,
                number,
            });
        }
        let i = &v.issue;
        out.push(CardIssue {
            id: i.id,
            readable_id: v.readable_id(),
            project: i.project_id,
            summary: i.summary.clone(),
            excerpt: excerpt(&i.description),
            resolved_at: i.resolved_at,
            tags: v.tags.iter().map(tag_ref).collect(),
            fields: shown,
            files: d.files_of(i.id),
            restricted: !i.audience_users.is_empty() || !i.audience_groups.is_empty(),
            starred: v.starred,
            can_update: v.can.update,
            version: i.version,
        });
    }
    let refs = CardRefs {
        fields,
        values: values
            .into_iter()
            .filter_map(|id| d.value(id))
            .map(held_value)
            .collect(),
        users: users
            .into_iter()
            .filter_map(|id| d.user(id))
            .map(|u| user_ref(&names, viewer, u))
            .collect(),
        groups: groups
            .into_iter()
            .filter_map(|id| d.group(id))
            .map(group_ref)
            .collect(),
    };
    (out, refs)
}

fn card(c: &service_view::Card) -> Card {
    Card {
        issue: c.issue,
        column: c.column,
        lane: c.lane.clone(),
        rank: c.rank.clone(),
    }
}

/// Boards the caller views (BRD-001, BRD-002): favorites first, then by name.
#[utoipa::path(
    get,
    path = "/boards",
    operation_id = "list_boards",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(BoardListQuery, PageQuery),
    responses(
        (status = OK, description = "A page of boards", body = BoardList),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<BoardListQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = query
        .resolve::<(bool, String, BoardId)>(MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let after = page
        .after
        .clone()
        .map(|(favorite, name, id)| After { favorite, name, id });
    let found = service::list(
        &state.db,
        &viewer.access,
        filter.q.as_deref(),
        filter.project,
        after.as_ref(),
        page.fetch_limit(),
    )
    .await?;
    let shared: Vec<Shared> = found
        .items
        .iter()
        .map(|b| Shared::private(b.owner_id))
        .collect();
    let parties = tags::parties(&state.db, &shared).await?;
    let shown = Shown::new(state.names(), &viewer.access, parties);
    let rows = Page::from_rows(found.items, page.limit, |b| {
        (b.favorite, b.name.to_lowercase(), b.id)
    });
    let items = rows
        .items
        .iter()
        .map(|b| {
            Ok(BoardSummary {
                id: b.id,
                name: b.name.clone(),
                owner: shown.owner(&Shared::private(b.owner_id))?,
                favorite: b.favorite,
                projects: b
                    .projects
                    .iter()
                    .filter_map(|id| found.projects.iter().find(|p| p.id == *id))
                    .map(project_ref)
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(json(
        StatusCode::OK,
        BoardList {
            items,
            next_cursor: rows.next_cursor,
            last_visited: found.last_visited,
        },
    ))
}

/// Creates a board from a preset (BRD-003…BRD-010).
#[utoipa::path(
    post,
    path = "/boards",
    operation_id = "create_board",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    request_body = NewBoard,
    responses(
        (status = CREATED, description = "Created", body = Board),
        (status = BAD_REQUEST, description = "Invalid name, projects or audiences", body = ErrorBody),
        (status = FORBIDDEN, description = "A guest, or does not read issues in every project", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewBoard>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let language = state.settings.get().get(&SYSTEM_LANGUAGE);
    let row = service::create(
        &state.db,
        &viewer.access,
        language,
        service::NewBoard {
            preset: preset(request.preset),
            name: request.name,
            projects: request.projects,
            sharing: sharing(request.sharing),
        },
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        board(&state, &viewer.access, &row).await?,
    ))
}

/// One board the caller views.
#[utoipa::path(
    get,
    path = "/boards/{id}",
    operation_id = "get_board",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    responses(
        (status = OK, description = "The board", body = Board),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let row = service::get(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        board(&state, &viewer.access, &row).await?,
    ))
}

/// Renames a board, changes its projects or column field, shares it or gives it to another owner
/// (BRD-010…BRD-013, BRD-042).
#[utoipa::path(
    patch,
    path = "/boards/{id}",
    operation_id = "update_board",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = BoardPatch,
    responses(
        (status = OK, description = "The board now", body = Board),
        (status = BAD_REQUEST, description = "Invalid name, projects, field or audiences", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit, share or give it away", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(patch): JsonBody<BoardPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let language = state.settings.get().get(&SYSTEM_LANGUAGE);
    let row = service::update(
        &state.db,
        &viewer.access,
        language,
        id,
        BoardChange {
            name: patch.name,
            projects: patch.projects,
            owner: patch.owner,
            sharing: patch.sharing.map(sharing),
            column_field: patch.column_field,
            sprints_on: patch.sprints_on,
            mode: patch.mode.map(mode),
            query: patch.query.clone(),
            auto_sprint: patch.auto_sprint,
            backlog: patch.backlog,
            new_backlog: patch.new_backlog.unwrap_or(false),
            lanes: patch.lanes.map(lanes_change),
            cards: patch.cards.map(cards_change),
            version: patch.version,
        },
    )
    .await
    .map_err(|e| crate::search::run_error(patch.query.as_deref().unwrap_or(""), e))?;
    Ok(json(
        StatusCode::OK,
        board(&state, &viewer.access, &row).await?,
    ))
}

/// Deletes a board with its sprints and cards (BRD-016): the owner or an administrator.
#[utoipa::path(
    delete,
    path = "/boards/{id}",
    operation_id = "delete_board",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "Neither the owner nor an administrator", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service::delete(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// What deleting the board takes with it, to name before asking (BRD-016).
#[utoipa::path(
    get,
    path = "/boards/{id}/impact",
    operation_id = "board_impact",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    responses(
        (status = OK, description = "What goes with the board", body = BoardImpact),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn impact(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = service::impact(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        BoardImpact {
            sprints: found.sprints,
            cards: found.cards,
            searches: found.searches,
        },
    ))
}

/// Changes the caller's view of a board: favorite, card size, collapsed columns and lanes.
#[utoipa::path(
    patch,
    path = "/boards/{id}/profile",
    operation_id = "update_board_profile",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = BoardProfilePatch,
    responses(
        (status = OK, description = "The caller's view now", body = BoardProfile),
        (status = BAD_REQUEST, description = "Invalid card size", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn update_profile(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(patch): JsonBody<BoardProfilePatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let p = service::set_profile(
        &state.db,
        &viewer.access,
        id,
        ProfileChange {
            favorite: patch.favorite,
            card_size: patch.card_size,
            collapsed_columns: patch.collapsed_columns,
            collapsed_lanes: patch.collapsed_lanes,
        },
    )
    .await?;
    Ok(json(StatusCode::OK, profile(&p)))
}

/// Adds a column (BRD-043): a value of the column field's set, or a new value of it (FLD-031).
#[utoipa::path(
    post,
    path = "/boards/{id}/columns",
    operation_id = "add_board_column",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = NewBoardColumn,
    responses(
        (status = OK, description = "The board now", body = Board),
        (status = BAD_REQUEST, description = "Not a value of the set, a column already, or too many", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit the board or the set", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
        (status = CONFLICT, description = "The board has no column field", body = ErrorBody),
    ),
)]
pub async fn add_column(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(request): JsonBody<NewBoardColumn>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let column = match (request.value, request.new_value) {
        (Some(value), None) => NewColumn::Value(value),
        (None, Some(name)) => NewColumn::NewValue(name),
        _ => {
            return Err(prostor_service::Error::field(
                "value",
                prostor_domain::validation::Problem::Required,
                "give either value or new_value",
            )
            .into());
        }
    };
    let row = columns::add(&state.db, &viewer.access, id, column, request.after).await?;
    Ok(json(
        StatusCode::OK,
        board(&state, &viewer.access, &row).await?,
    ))
}

/// Moves a column, sets its WIP limits or renames its value (BRD-044…BRD-046).
#[utoipa::path(
    patch,
    path = "/boards/{id}/columns/{column}",
    operation_id = "update_board_column",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("column" = i64, Path, description = "Column ID"),
    ),
    request_body = BoardColumnPatch,
    responses(
        (status = OK, description = "The board now", body = Board),
        (status = BAD_REQUEST, description = "Invalid limits, place or name", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit the board or the set", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or column", body = ErrorBody),
        (status = CONFLICT, description = "The value was changed by someone else", body = ErrorBody),
    ),
)]
pub async fn update_column(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, column)): Path<(BoardId, ColumnId)>,
    JsonBody(patch): JsonBody<BoardColumnPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let row = columns::update(
        &state.db,
        &viewer.access,
        id,
        column,
        ColumnChange {
            position: patch.after,
            wip_min: patch.wip_min,
            wip_max: patch.wip_max,
            name: patch.name,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        board(&state, &viewer.access, &row).await?,
    ))
}

/// Removes a column: its cards leave the board and keep their sprint (BRD-043).
#[utoipa::path(
    delete,
    path = "/boards/{id}/columns/{column}",
    operation_id = "delete_board_column",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("column" = i64, Path, description = "Column ID"),
    ),
    responses(
        (status = OK, description = "The board now", body = Board),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or column", body = ErrorBody),
    ),
)]
pub async fn delete_column(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, column)): Path<(BoardId, ColumnId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let row = columns::delete(&state.db, &viewer.access, id, column).await?;
    Ok(json(
        StatusCode::OK,
        board(&state, &viewer.access, &row).await?,
    ))
}

/// The board with one sprint's cards (BRD-068): the current sprint unless one is asked for
/// (BRD-099); each cell shows at most 100 cards (BRD-057).
#[utoipa::path(
    get,
    path = "/boards/{id}/view",
    operation_id = "view_board",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID"), BoardViewQuery),
    responses(
        (status = OK, description = "The board view", body = BoardView),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn view(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    QueryParams(query): QueryParams<BoardViewQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let header = query.q.as_deref();
    let v = service_view::view(
        &state.db,
        &viewer.access,
        id,
        query.sprint,
        header,
        query.lanes.as_deref(),
        None,
    )
    .await
    .map_err(|e| crate::search::run_error(header.unwrap_or(""), e))?;
    let (shown, refs) = card_issues(&state, &viewer.access, &v.issues);
    let sprints = v
        .board
        .full
        .sprints
        .iter()
        .filter(|s| !s.internal && s.archived_at.is_none())
        .map(|s| sprint(s, v.current, v.today))
        .collect();
    Ok(json(
        StatusCode::OK,
        BoardView {
            board: board(&state, &viewer.access, &v.board).await?,
            sprint: v.sprint.as_ref().map(|s| sprint(s, v.current, v.today)),
            sprints,
            columns: v
                .columns
                .iter()
                .map(|c| ColumnCount {
                    column: c.column,
                    count: c.count,
                    estimate: c.estimate,
                    wip: wip(c.wip),
                })
                .collect(),
            cards: v.cards.iter().map(card).collect(),
            cells: v
                .cells
                .iter()
                .map(|c| TruncatedCell {
                    column: c.column,
                    lane: c.lane.clone(),
                    total: c.total,
                    next_cursor: c.next_cursor.clone(),
                })
                .collect(),
            lanes: v.lanes.iter().map(lane).collect(),
            lanes_next: v.lanes_next.clone(),
            lanes_total: i64::try_from(v.lanes_total).unwrap_or(i64::MAX),
            issues: shown,
            refs,
            totals: BoardTotals {
                count: v.total,
                estimate: v.estimate,
            },
            sort: v
                .sort
                .as_ref()
                .map(|sort| sort.iter().map(crate::search::sort_used).collect()),
            notes: crate::search::problems(header.unwrap_or(""), &v.notes),
            live_topic: v.live_topic,
            profile: profile(&v.board.profile),
        },
    ))
}

/// The rest of a truncated cell (BRD-057).
#[utoipa::path(
    get,
    path = "/boards/{id}/cell",
    operation_id = "board_cell",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID"), BoardCellQuery),
    responses(
        (status = OK, description = "A page of the cell", body = BoardCell),
        (status = BAD_REQUEST, description = "Invalid cursor", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board, sprint or column", body = ErrorBody),
    ),
)]
pub async fn cell(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    QueryParams(query): QueryParams<BoardCellQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = service_view::cell(
        &state.db,
        &viewer.access,
        id,
        query.sprint,
        query.column,
        query.lane.as_deref(),
        query.cursor.as_deref(),
        query.q.as_deref(),
        None,
    )
    .await
    .map_err(|e| crate::search::run_error(query.q.as_deref().unwrap_or(""), e))?;
    let (shown, refs) = card_issues(&state, &viewer.access, &page.issues);
    Ok(json(
        StatusCode::OK,
        BoardCell {
            cards: page.cards.iter().map(card).collect(),
            issues: shown,
            refs,
            next_cursor: page.next_cursor,
        },
    ))
}

fn placed(
    state: &AppState,
    viewer: &Access,
    p: &Placed,
    messages: &[prostor_service::rules::Message],
) -> PlacedCard {
    let (mut shown, refs) = card_issues(state, viewer, &p.issues);
    PlacedCard {
        card: p.card.as_ref().map(card),
        issue: shown.pop(),
        refs,
        changes: p
            .changes
            .iter()
            .map(|step| match *step {
                Step::Set { field, value } => CardChange::Set { field, value },
                Step::Clear { field } => CardChange::Clear { field },
                Step::Tag(tag) => CardChange::Tag { tag },
            })
            .collect(),
        messages: crate::present::rule_messages(messages),
    }
}

/// Adds an issue to a sprint of the board (BRD-025); on a sprint board an issue in another
/// sprint moves to this one. An issue already there stays as it is.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/cards",
    operation_id = "add_board_card",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    request_body = NewCard,
    responses(
        (status = OK, description = "The card", body = PlacedCard),
        (status = BAD_REQUEST, description = "Not an issue of a board project, or a bad neighbour", body = ErrorBody),
        (status = FORBIDDEN, description = "May not update the issue, or a query board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board, sprint or issue", body = ErrorBody),
    ),
)]
pub async fn add_card(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint)): Path<(BoardId, SprintId)>,
    JsonBody(input): JsonBody<NewCard>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let p = cards::add(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        sprint,
        cards::NewCard {
            issue: input.issue,
            column: input.column,
            after: input.after,
            before: input.before,
            preview: input.preview,
            lane: input.lane,
            subtasks: input.subtasks,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        placed(&state, &viewer.access, &p.done, &p.messages),
    ))
}

/// Moves a card (BRD-068, BRD-069): another column sets the column field's value, with the rights,
/// required values and history of a field change; the neighbours give its place.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/cards/{issue}/move",
    operation_id = "move_board_card",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
        ("issue" = i64, Path, description = "Issue ID"),
    ),
    request_body = CardMove,
    responses(
        (status = OK, description = "The card", body = PlacedCard),
        (status = BAD_REQUEST, description = "No such column, a bad neighbour or a value the field refuses", body = ErrorBody),
        (status = FORBIDDEN, description = "May not update the issue", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint, or the issue is not on it", body = ErrorBody),
        (status = CONFLICT, description = "The issue changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn move_card(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint, issue_id)): Path<(BoardId, SprintId, IssueId)>,
    JsonBody(input): JsonBody<CardMove>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let p = cards::move_card(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        sprint,
        issue_id,
        Move {
            column: input.column,
            after: input.after,
            before: input.before,
            lane: input.lane.clone(),
            version: input.version,
            header: input.q.clone(),
        },
    )
    .await
    .map_err(|e| crate::search::run_error(input.q.as_deref().unwrap_or(""), e))?;
    Ok(json(
        StatusCode::OK,
        placed(&state, &viewer.access, &p.done, &p.messages),
    ))
}

/// Takes a card off the sprint (BRD-074); not on a query board.
#[utoipa::path(
    delete,
    path = "/boards/{id}/sprints/{sprint}/cards/{issue}",
    operation_id = "remove_board_card",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
        ("issue" = i64, Path, description = "Issue ID"),
    ),
    responses(
        (status = NO_CONTENT, description = "Removed"),
        (status = FORBIDDEN, description = "May not update the issue, or a query board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint, or the issue is not on it", body = ErrorBody),
    ),
)]
pub async fn remove_card(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint, issue_id)): Path<(BoardId, SprintId, IssueId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    cards::remove(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        sprint,
        issue_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// A new parent lane (BRD-054): an issue in the first board project with the first lane value
/// and the title as its summary, the last lane of the sprint. Parent-lane boards only.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/lanes",
    operation_id = "add_board_lane",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    request_body = NewLane,
    responses(
        (status = CREATED, description = "The lane", body = PlacedLane),
        (status = BAD_REQUEST, description = "A bad title or a value the field refuses", body = ErrorBody),
        (status = FORBIDDEN, description = "No parent lanes, a query board, or may not create issues", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn add_lane(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint)): Path<(BoardId, SprintId)>,
    JsonBody(input): JsonBody<NewLane>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let (made, issues) = service_lanes::create_lane(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        sprint,
        &input.title,
    )
    .await?;
    let (mut shown, refs) = card_issues(&state, &viewer.access, &issues);
    let issue = shown
        .pop()
        .ok_or_else(|| ApiError::internal("the new lane's issue is missing"))?;
    Ok(json(
        StatusCode::CREATED,
        PlacedLane {
            lane: lane(&made),
            issue,
            refs,
        },
    ))
}

/// Puts a parent lane at another place (BRD-053): the board's edit right; only the order of lanes
/// changes, no field.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/lanes/{issue}/move",
    operation_id = "move_board_lane",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
        ("issue" = i64, Path, description = "The lane's issue ID"),
    ),
    request_body = LaneMove,
    responses(
        (status = NO_CONTENT, description = "Moved"),
        (status = BAD_REQUEST, description = "A bad neighbour", body = ErrorBody),
        (status = FORBIDDEN, description = "No parent lanes, or may not change the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn move_lane(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint, issue_id)): Path<(BoardId, SprintId, IssueId)>,
    JsonBody(input): JsonBody<LaneMove>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service_lanes::move_lane(
        &state.db,
        &viewer.access,
        id,
        sprint,
        issue_id,
        input.after,
        input.before,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Takes a parent lane back to the backlog (BRD-056): the parent and its subtasks leave the
/// sprint; each needs the update right. Not on a query board.
#[utoipa::path(
    delete,
    path = "/boards/{id}/sprints/{sprint}/lanes/{issue}",
    operation_id = "remove_board_lane",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
        ("issue" = i64, Path, description = "The lane's issue ID"),
    ),
    responses(
        (status = OK, description = "The issues that left", body = LaneRemoved),
        (status = FORBIDDEN, description = "No parent lanes, a query board, or may not update an issue", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board, sprint or issue", body = ErrorBody),
    ),
)]
pub async fn remove_lane(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint, issue_id)): Path<(BoardId, SprintId, IssueId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let issues = service_lanes::lane_to_backlog(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        sprint,
        issue_id,
    )
    .await?;
    Ok(json(StatusCode::OK, LaneRemoved { issues }))
}

/// Where these issues are on the board now (BRD-114): the live board asks after events.
#[utoipa::path(
    post,
    path = "/boards/{id}/cards/batch",
    operation_id = "board_cards_batch",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = CardBatch,
    responses(
        (status = OK, description = "Their cards", body = CardBatchAnswer),
        (status = BAD_REQUEST, description = "More than 100 issues", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn cards_batch(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(input): JsonBody<CardBatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let (found, described) = service_view::batch(
        &state.db,
        &viewer.access,
        id,
        input.sprint,
        &input.issues,
        input.q.as_deref(),
        None,
    )
    .await
    .map_err(|e| crate::search::run_error(input.q.as_deref().unwrap_or(""), e))?;
    let (shown, refs) = card_issues(&state, &viewer.access, &described);
    Ok(json(
        StatusCode::OK,
        CardBatchAnswer {
            cards: found.iter().map(|c| c.as_ref().map(card)).collect(),
            issues: shown,
            refs,
        },
    ))
}

fn sprint_row(r: &SprintRow) -> Sprint {
    sprint(&r.sprint, r.current, r.today)
}

/// The sprints of a board, archived ones included (BRD-103).
#[utoipa::path(
    get,
    path = "/boards/{id}/sprints",
    operation_id = "list_board_sprints",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    responses(
        (status = OK, description = "The sprints", body = SprintList),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn list_sprints(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let l = service_sprints::list(&state.db, &viewer.access, id, OffsetDateTime::now_utc()).await?;
    Ok(json(
        StatusCode::OK,
        SprintList {
            items: l
                .sprints
                .iter()
                .map(|s| sprint(s, l.current, l.today))
                .collect(),
        },
    ))
}

/// Creates a sprint (BRD-098): settings editors of a sprint board.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints",
    operation_id = "create_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = NewSprint,
    responses(
        (status = CREATED, description = "The sprint", body = Sprint),
        (status = BAD_REQUEST, description = "Invalid name, goal, dates or sprint to carry from", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit the board, or it has no sprints", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn create_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(input): JsonBody<NewSprint>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let r = service_sprints::create(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        service_sprints::NewSprint {
            name: input.name,
            goal: input.goal,
            starts_on: input.starts_on,
            ends_on: input.ends_on,
            carry_from: input.carry_from,
            auto_target: input.auto_target,
        },
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::CREATED, sprint_row(&r)))
}

/// Edits a sprint's name, goal and dates (BRD-102).
#[utoipa::path(
    patch,
    path = "/boards/{id}/sprints/{sprint}",
    operation_id = "update_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    request_body = SprintPatch,
    responses(
        (status = OK, description = "The sprint now", body = Sprint),
        (status = BAD_REQUEST, description = "Invalid name, goal or dates", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else", body = ErrorBody),
    ),
)]
pub async fn update_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
    JsonBody(patch): JsonBody<SprintPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let r = service_sprints::update(
        &state.db,
        &viewer.access,
        id,
        sprint_id,
        SprintChange {
            name: patch.name,
            goal: patch.goal,
            starts_on: patch.starts_on,
            ends_on: patch.ends_on,
            version: patch.version,
        },
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::OK, sprint_row(&r)))
}

/// Deletes a sprint with its cards (BRD-104); a sprint board keeps at least one.
#[utoipa::path(
    delete,
    path = "/boards/{id}/sprints/{sprint}",
    operation_id = "delete_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "May not edit the board, or its last sprint", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn delete_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service_sprints::delete(&state.db, &viewer.access, id, sprint_id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Starts a sprint by hand (D-013): it starts today unless it started already.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/start",
    operation_id = "start_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    responses(
        (status = OK, description = "The sprint now", body = Sprint),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn start_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let r = service_sprints::start(
        &state.db,
        &viewer.access,
        id,
        sprint_id,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::OK, sprint_row(&r)))
}

/// Finishes a sprint by hand (D-013).
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/finish",
    operation_id = "finish_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    responses(
        (status = OK, description = "The sprint now", body = Sprint),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn finish_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let r = service_sprints::finish(
        &state.db,
        &viewer.access,
        id,
        sprint_id,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::OK, sprint_row(&r)))
}

/// Archives a sprint (BRD-103): it leaves detection and the suggestions.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/archive",
    operation_id = "archive_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    responses(
        (status = OK, description = "The sprint now", body = Sprint),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn archive_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
) -> Result<Response, ApiError> {
    archived(state, viewer, id, sprint_id, true).await
}

/// Restores an archived sprint (BRD-103).
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/restore",
    operation_id = "restore_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    responses(
        (status = OK, description = "The sprint now", body = Sprint),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn restore_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
) -> Result<Response, ApiError> {
    archived(state, viewer, id, sprint_id, false).await
}

async fn archived(
    state: AppState,
    viewer: Viewer,
    id: BoardId,
    sprint_id: SprintId,
    archived: bool,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let r = service_sprints::archive(
        &state.db,
        &viewer.access,
        id,
        sprint_id,
        archived,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::OK, sprint_row(&r)))
}

/// Carries the unresolved issues of a sprint to another (BRD-101); answers the target.
#[utoipa::path(
    post,
    path = "/boards/{id}/sprints/{sprint}/carry",
    operation_id = "carry_board_sprint",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Board ID"),
        ("sprint" = i64, Path, description = "Sprint ID"),
    ),
    request_body = SprintCarry,
    responses(
        (status = OK, description = "The sprint they went to", body = Sprint),
        (status = BAD_REQUEST, description = "No such target sprint", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit the board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or sprint", body = ErrorBody),
    ),
)]
pub async fn carry_sprint(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, sprint_id)): Path<(BoardId, SprintId)>,
    JsonBody(input): JsonBody<SprintCarry>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let r = service_sprints::carry(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        sprint_id,
        input.to,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::OK, sprint_row(&r)))
}

/// An issue's board on the wire.
pub(crate) fn issue_board(b: &issue_side::IssueBoard) -> IssueBoard {
    IssueBoard {
        board: BoardName {
            id: b.board,
            name: b.name.clone(),
        },
        sprints_on: b.sprints_on,
        sprint: b.sprint.as_ref().map(|(id, name)| SprintName {
            id: *id,
            name: name.clone(),
        }),
        auto: b.auto,
        skipped: b.skipped,
    }
}

fn issue_boards(found: &[issue_side::IssueBoard]) -> IssueBoards {
    IssueBoards {
        items: found.iter().map(issue_board).collect(),
    }
}

/// A page of the board's backlog (BRD-089…BRD-093): issues ordered by hand first, then the rest
/// in the search's sort; `missing` says why there is none.
#[utoipa::path(
    get,
    path = "/boards/{id}/backlog",
    operation_id = "board_backlog",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID"), BoardBacklogQuery),
    responses(
        (status = OK, description = "A page of the backlog", body = BoardBacklog),
        (status = BAD_REQUEST, description = "The filter has errors (`invalid_query`), or a bad limit or cursor", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn backlog(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    QueryParams(query): QueryParams<BoardBacklogQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let text = query.q.unwrap_or_default();
    let found = service_backlog::get(
        &state.db,
        &viewer.access,
        id,
        BacklogInput {
            text: text.clone(),
            cursor: query.cursor,
            limit: query.limit.unwrap_or(BACKLOG_LIMIT),
            time_zone: query.time_zone,
            now: None,
        },
    )
    .await
    .map_err(|e| crate::search::run_error(&text, e))?;
    Ok(json(
        StatusCode::OK,
        BoardBacklog {
            search: found.search.map(|s| BacklogSearch {
                id: s.id,
                name: s.name,
                may_order: s.may_order,
            }),
            missing: found.missing.map(|m| match m {
                service_backlog::Missing::None => BacklogMissing::None,
                service_backlog::Missing::Hidden => BacklogMissing::Hidden,
            }),
            page: found
                .page
                .map(|p| crate::search::search_page(&state, &viewer.access, &text, p)),
        },
    ))
}

/// Default page size of the backlog panel.
const BACKLOG_LIMIT: u32 = 50;

/// Puts an issue at a place in the backlog's order (BRD-093): those who edit the backlog search.
#[utoipa::path(
    post,
    path = "/boards/{id}/backlog/order",
    operation_id = "order_board_backlog",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = BacklogOrder,
    responses(
        (status = NO_CONTENT, description = "Placed"),
        (status = BAD_REQUEST, description = "A neighbour that is not in the backlog", body = ErrorBody),
        (status = FORBIDDEN, description = "No backlog, or may not edit its search", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board or issue", body = ErrorBody),
    ),
)]
pub async fn order_backlog(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(order): JsonBody<BacklogOrder>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service_backlog::order(
        &state.db,
        &viewer.access,
        id,
        order.issue,
        order.after,
        order.before,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Forgets the backlog's order (BRD-093): the search's sort again.
#[utoipa::path(
    delete,
    path = "/boards/{id}/backlog/order",
    operation_id = "discard_board_backlog_order",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    responses(
        (status = NO_CONTENT, description = "Discarded"),
        (status = FORBIDDEN, description = "No backlog, or may not edit its search", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn discard_backlog_order(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service_backlog::discard(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The boards and sprints of an issue (BRD-033, ISS-020); for a draft also the boards it joins
/// on its own when published (ISS-007).
#[utoipa::path(
    get,
    path = "/issues/{id}/boards",
    operation_id = "issue_boards",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = OK, description = "Its boards", body = IssueBoards),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn list_issue_boards(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = issue_side::of_issue(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, issue_boards(&found)))
}

/// Puts an issue on a board (BRD-033, ISS-020): into the sprint given, else the current one. A
/// draft kept off a board it joins on its own is let back on.
#[utoipa::path(
    post,
    path = "/issues/{id}/boards",
    operation_id = "add_issue_board",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = NewIssueBoard,
    responses(
        (status = OK, description = "Its boards now", body = IssueBoards),
        (status = BAD_REQUEST, description = "The issue's project is not on the board", body = ErrorBody),
        (status = FORBIDDEN, description = "May not update the issue, a query board, or no current sprint", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue, board or sprint", body = ErrorBody),
    ),
)]
pub async fn add_issue_board(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    JsonBody(input): JsonBody<NewIssueBoard>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let found = issue_side::add(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        input.board,
        input.sprint,
        OffsetDateTime::now_utc(),
    )
    .await?;
    Ok(json(StatusCode::OK, issue_boards(&found)))
}

/// Takes an issue off a board (BRD-033, ISS-020); a draft is kept off a board it would join on
/// its own (ISS-007).
#[utoipa::path(
    delete,
    path = "/issues/{id}/boards/{board}",
    operation_id = "remove_issue_board",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Issue ID"),
        ("board" = i64, Path, description = "Board ID"),
    ),
    responses(
        (status = OK, description = "Its boards now", body = IssueBoards),
        (status = FORBIDDEN, description = "May not update the issue, or a query board", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue or board, or the issue is not on it", body = ErrorBody),
    ),
)]
pub async fn remove_issue_board(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, board)): Path<(IssueId, BoardId)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let found = issue_side::remove(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        board,
    )
    .await?;
    Ok(json(StatusCode::OK, issue_boards(&found)))
}

/// The caller's draft of a cell (BRD-077): the kept one, or a new draft with the project's
/// defaults, the cell's column and lane, already a card that shows once published.
#[utoipa::path(
    post,
    path = "/boards/{id}/cell-draft",
    operation_id = "board_cell_draft",
    tag = "boards",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Board ID")),
    request_body = NewCellDraft,
    responses(
        (status = OK, description = "The kept draft", body = CellDraft),
        (status = CREATED, description = "A new draft", body = CellDraft),
        (status = BAD_REQUEST, description = "No such column or sprint, or a project off the board", body = ErrorBody),
        (status = FORBIDDEN, description = "May not create issues in a board project", body = ErrorBody),
        (status = NOT_FOUND, description = "No such board", body = ErrorBody),
    ),
)]
pub async fn cell_draft(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<BoardId>,
    JsonBody(input): JsonBody<NewCellDraft>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let (issue, created) = service_drafts::cell_draft(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        service_drafts::Cell {
            sprint: input.sprint,
            column: input.column,
            lane: input.lane,
            project: input.project,
        },
    )
    .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok(json(status, CellDraft { issue, created }))
}

/// The boards using a field (BRD-022): those the caller views, by name, and how many others.
#[utoipa::path(
    get,
    path = "/fields/{id}/boards",
    operation_id = "list_field_boards",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    responses(
        (status = OK, description = "The boards", body = FieldBoards),
        (status = FORBIDDEN, description = "May not see fields", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
    ),
)]
pub async fn field_boards(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let (found, others) = service_health::field_usage(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        FieldBoards {
            items: found
                .into_iter()
                .map(|u| FieldBoardUse {
                    board: BoardName {
                        id: u.board.id,
                        name: u.board.name,
                    },
                    column: u.column,
                    lane: u.lane,
                    color: u.color,
                    estimation: u.estimation,
                    original_estimation: u.original_estimation,
                    cards: u.cards,
                })
                .collect(),
            others,
        },
    ))
}
