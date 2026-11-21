//! Boards (BRD, step 1.8, D-044): create from a preset, read, list, change, share, give away and
//! delete boards, and each person's view of a board. Rules live in `domain::boards`; columns are
//! in [`columns`], the read path of the board in [`view`].

pub mod backlog;
pub mod card_setup;
pub mod cards;
pub mod columns;
pub mod drafts;
pub mod health;
pub mod issue_side;
pub mod lanes;
pub mod membership;
pub mod sprints;
pub mod view;

use prostor_domain::access::Access;
use prostor_domain::access::permission::Permission;
use prostor_domain::boards::access::{BoardAccess, may_create};
use prostor_domain::boards::lanes::LaneKey;
use prostor_domain::boards::presets::{self, Attached as PresetField, ProjectFields, SetValue};
use prostor_domain::boards::sprints::{INTERNAL_NAME, current, free_numbered_name, numbered_name};
use prostor_domain::boards::{self as domain, ColorMode, Mode, Preset};
use prostor_domain::fields::FieldKind;
use prostor_domain::fields::builtin::BuiltinField;
use prostor_domain::live::Change;
use prostor_domain::rank;
use prostor_domain::search::Binding;
use prostor_domain::settings::Language;
use prostor_domain::sharing::Audience;
use prostor_domain::validation::{Problem, Problems};
use prostor_domain::{Actor, BoardId, ColumnId, FieldId, ProjectId, SprintId, UserId};
use prostor_store::boards::{self as store, After, Full, Listed, Profile, Settings, Viewer};
use prostor_store::fields::{self as store_fields, Field};
use prostor_store::projects::{self, Project};
use prostor_store::value_sets::{self, Value};
use prostor_store::{PgPool, audit, users};
use serde_json::json;
use sqlx::PgConnection;

use crate::live::Batch;
use crate::seed::internal;
use crate::tags::check_audiences;
use crate::{Error, Result};

/// A board as its viewer sees it.
#[derive(Debug, Clone)]
pub struct BoardRow {
    pub full: Full,
    /// The column field, if it still exists.
    pub column_field: Option<Field>,
    /// The lane field, if it still exists (BRD-049).
    pub lane_field: Option<Field>,
    /// The value of each column, beside `full.columns` (a column whose value is gone is left out
    /// of both).
    pub column_values: Vec<Value>,
    /// The board projects that still exist, by ID.
    pub projects: Vec<Project>,
    pub rights: Rights,
    pub profile: Profile,
    /// The board query by the viewer's names (mode `query`, BRD-029).
    pub query: Option<String>,
    /// What the board query lost (a deleted value…): the board shows no cards until it is fixed.
    pub query_problems: Vec<prostor_query::Problem>,
    /// The fields the card settings name that exist (BRD-059…BRD-062).
    pub fields: Vec<Field>,
    /// Fields of the board projects the viewer may not read (FLD-062): the settings leave them
    /// out.
    pub hidden: Vec<FieldId>,
    /// The fields attached to the board projects.
    pub attached: Vec<store_fields::Attached>,
    /// What keeps the board from working (BRD-023), errors first.
    pub health: Vec<prostor_domain::boards::health::Finding>,
    /// What the lane values name, so empty lanes have titles too (BRD-053).
    pub lane_names: LaneNames,
}

/// The values, people, groups and tags the lane values name; tags only those the viewer knows.
#[derive(Debug, Clone, Default)]
pub struct LaneNames {
    pub values: Vec<Value>,
    pub users: Vec<users::User>,
    pub groups: Vec<prostor_store::groups::Group>,
    pub tags: Vec<prostor_store::tags::Tag>,
}

impl BoardRow {
    /// A field the viewer may see in the settings (FLD-062).
    pub fn shown(&self, field: FieldId) -> bool {
        !self.hidden.contains(&field)
    }

    /// The configured estimation, or the standard one of an existing kanban board.
    pub fn estimation_field(&self) -> Option<FieldId> {
        self.full.board.estimation_field_id.or_else(|| {
            if Preset::parse(&self.full.board.preset) != Some(Preset::Kanban) {
                return None;
            }
            let field = self
                .fields
                .iter()
                .find(|f| f.builtin.as_deref() == Some(BuiltinField::Estimation.key()))?;
            self.full
                .board
                .projects
                .iter()
                .all(|p| {
                    self.attached
                        .iter()
                        .any(|a| a.project_id == *p && a.field_id == field.id)
                })
                .then_some(field.id)
        })
    }
}

/// What the viewer may do with the board.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rights {
    pub edit: bool,
    pub share: bool,
    pub transfer: bool,
    pub delete: bool,
}

impl Rights {
    fn of(access: &BoardAccess, viewer: &Access) -> Self {
        Self {
            edit: access.can_edit(viewer),
            share: access.may_share(viewer).is_ok(),
            transfer: access.may_transfer(viewer).is_ok(),
            delete: access.may_delete(viewer),
        }
    }
}

/// Who views and who edits a board, and the two audiences over its projects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sharing {
    pub view: Audience,
    pub edit: Audience,
    pub view_readers: bool,
    pub edit_updaters: bool,
}

impl Sharing {
    fn is_private(&self) -> bool {
        self.view.is_empty() && self.edit.is_empty() && !self.view_readers && !self.edit_updaters
    }
}

/// A board to create (BRD-003…BRD-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBoard {
    pub preset: Preset,
    pub name: String,
    pub projects: Vec<ProjectId>,
    pub sharing: Sharing,
}

/// A change of a board; `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardChange {
    pub name: Option<String>,
    pub projects: Option<Vec<ProjectId>>,
    /// A new owner (BRD-011): an editor gives the board away.
    pub owner: Option<UserId>,
    pub sharing: Option<Sharing>,
    /// `Some(None)` removes the column field; any change drops the columns (BRD-042).
    pub column_field: Option<Option<FieldId>>,
    /// Sprints on or off (BRD-024).
    pub sprints_on: Option<bool>,
    /// How issues join: `manual`, `auto` or `query` (BRD-025…BRD-028). Leaving `query` pins every
    /// card it shows (at most [`PIN_MAX`]).
    pub mode: Option<Mode>,
    /// The board query (mode `query`, BRD-029): any issue-list query; empty means all issues of
    /// the board projects.
    pub query: Option<String>,
    /// The sprint new issues join on a sprint board (BRD-026); `Some(None)` clears it.
    pub auto_sprint: Option<Option<SprintId>>,
    /// The saved search used as the backlog (BRD-090); `Some(None)` leaves the board without.
    pub backlog: Option<Option<prostor_store::saved_searches::SavedSearchId>>,
    /// Makes a new default backlog owned by the caller and uses it (BRD-089); wins over `backlog`.
    pub new_backlog: bool,
    /// The lanes (BRD-049…BRD-055); value lanes reorder here too.
    pub lanes: Option<lanes::LanesChange>,
    /// The card settings (BRD-059…BRD-062).
    pub cards: Option<card_setup::CardsChange>,
    pub version: i64,
}

/// Most cards leaving query mode pins (BRD-028).
pub const PIN_MAX: i64 = 20_000;

/// A change of the person's view; `None` keeps a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileChange {
    pub favorite: Option<bool>,
    pub card_size: Option<i16>,
    pub collapsed_columns: Option<Vec<ColumnId>>,
    pub collapsed_lanes: Option<Vec<String>>,
}

/// One page of the board list.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardList {
    pub items: Vec<Listed>,
    /// The board the person visited last and may still view (BRD-001).
    pub last_visited: Option<BoardId>,
    /// The projects of the listed boards.
    pub projects: Vec<Project>,
}

/// What deleting a board takes with it (BRD-016).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Impact {
    pub sprints: i64,
    pub cards: i64,
    /// Saved searches whose query names the board (conditions on boards come with QRY-049).
    pub searches: i64,
}

impl From<domain::access::Refusal> for Error {
    fn from(refusal: domain::access::Refusal) -> Self {
        use domain::access::Refusal;
        Error::Refused(
            match refusal {
                Refusal::NotEditor => "you may not change this board",
                Refusal::NoCapability => "sharing is not switched on for you",
            }
            .into(),
        )
    }
}

/// Tells the board's sockets that its settings changed (BRD-115); the last statement before the
/// commit.
pub(crate) async fn changed(conn: &mut PgConnection, id: BoardId) -> Result<()> {
    let mut batch = Batch::new();
    batch.board(id, Change::Updated);
    batch.publish(conn).await
}

pub(crate) fn not_found() -> Error {
    Error::NotFound("no such board")
}

fn target(id: BoardId) -> String {
    format!("board:{id}")
}

fn rejected(path: &str) -> impl FnOnce(prostor_domain::validation::Rejection) -> Error + '_ {
    move |r| Error::field(path, r.problem, r.message)
}

/// The board locked for a change, if the viewer views it.
pub(crate) async fn lock_viewed(
    conn: &mut PgConnection,
    viewer: &Access,
    id: BoardId,
) -> Result<store::Board> {
    store::lock(conn, id)
        .await?
        .filter(|b| b.access().can_view(viewer))
        .ok_or_else(not_found)
}

/// The board locked for a change of its settings (BRD-013).
pub(crate) async fn lock_edited(
    conn: &mut PgConnection,
    viewer: &Access,
    id: BoardId,
) -> Result<store::Board> {
    let board = lock_viewed(conn, viewer, id).await?;
    if !board.access().can_edit(viewer) {
        return Err(Error::Refused("you may not change this board".into()));
    }
    Ok(board)
}

/// One board the viewer views, with everything its settings show.
pub async fn get(pool: &PgPool, viewer: &Access, id: BoardId) -> Result<BoardRow> {
    let mut conn = pool.acquire().await?;
    load(&mut conn, viewer, id, true).await
}

/// The board with what its settings show; `checked` refuses a board the viewer does not view.
pub(crate) async fn load(
    conn: &mut PgConnection,
    viewer: &Access,
    id: BoardId,
    checked: bool,
) -> Result<BoardRow> {
    let full = store::get_full(&mut *conn, id)
        .await?
        .filter(|f| !checked || f.board.access().can_view(viewer))
        .ok_or_else(not_found)?;
    let column_field = match full.board.column_field_id {
        Some(f) => store_fields::get(&mut *conn, f).await?,
        None => None,
    };
    let lane_field = match full.board.lane_field_id {
        Some(f) => store_fields::get(&mut *conn, f).await?,
        None => None,
    };
    let lane_keys: Vec<LaneKey> = full
        .board
        .lane_values
        .iter()
        .filter_map(|k| LaneKey::parse(k))
        .collect();
    let lane_ids = |pick: fn(LaneKey) -> Option<i64>| -> Vec<i64> {
        lane_keys.iter().filter_map(|k| pick(*k)).collect()
    };
    let lane_value_ids = lane_ids(|k| match k {
        LaneKey::Value(id) => Some(id),
        _ => None,
    });
    // Column and lane values in one statement.
    let mut value_ids: Vec<i64> = full.columns.iter().map(|c| c.value_id).collect();
    value_ids.extend(&lane_value_ids);
    let found = if value_ids.is_empty() {
        Vec::new()
    } else {
        value_sets::get_values(&mut *conn, &value_ids).await?
    };
    let mut full = full;
    let mut column_values = Vec::with_capacity(full.columns.len());
    full.columns
        .retain(|c| match found.iter().find(|v| v.id == c.value_id) {
            Some(v) => {
                column_values.push(v.clone());
                true
            }
            None => false,
        });
    let user_ids = lane_ids(|k| match k {
        LaneKey::User(id) => Some(id),
        _ => None,
    });
    let group_ids = lane_ids(|k| match k {
        LaneKey::Group(id) => Some(id),
        _ => None,
    });
    let tag_ids = lane_ids(|k| match k {
        LaneKey::Tag(id) => Some(id),
        _ => None,
    });
    let lane_names = LaneNames {
        values: found
            .iter()
            .filter(|v| lane_value_ids.contains(&v.id))
            .cloned()
            .collect(),
        users: if user_ids.is_empty() {
            Vec::new()
        } else {
            users::get_many(&mut *conn, &user_ids).await?
        },
        groups: if group_ids.is_empty() {
            Vec::new()
        } else {
            prostor_store::groups::get_many(&mut *conn, &group_ids).await?
        },
        tags: if tag_ids.is_empty() {
            Vec::new()
        } else {
            prostor_store::tags::get_many(&mut *conn, &tag_ids)
                .await?
                .into_iter()
                .filter(|t| {
                    let shared = t.sharing();
                    shared.can_view(viewer)
                        || shared.can_use(viewer)
                        || shared.can_edit(viewer)
                        || shared.may_delete(viewer)
                })
                .collect()
        },
    };
    let projects = projects::get_many(&mut *conn, &full.board.projects).await?;
    let profile = store::profiles::get(&mut *conn, viewer.user(), id).await?;
    let rights = Rights::of(&full.board.access(), viewer);
    let (query, query_problems) = match &full.board.query {
        Some(stored) if Mode::parse(&full.board.mode) == Some(Mode::Query) => {
            let (text, problems) =
                crate::saved_searches::text_of(&mut *conn, viewer, stored).await?;
            (Some(text), problems)
        }
        _ => (None, Vec::new()),
    };
    let attached = store_fields::of_projects(&mut *conn, &full.board.projects).await?;
    let hidden = card_setup::hidden_fields(viewer, &attached);
    let mut named = card_setup::named(&full.board);
    if full.board.estimation_field_id.is_none()
        && Preset::parse(&full.board.preset) == Some(Preset::Kanban)
        && let Some(field) =
            store_fields::by_builtin(&mut *conn, BuiltinField::Estimation.key()).await?
        && full.board.projects.iter().all(|p| {
            attached
                .iter()
                .any(|a| a.project_id == *p && a.field_id == field.id)
        })
        && !named.contains(&field.id)
    {
        named.push(field.id);
    }
    let fields = if named.is_empty() {
        Vec::new()
    } else {
        store_fields::get_many(&mut *conn, &named).await?
    };
    let mut checked_fields = fields.clone();
    checked_fields.extend(lane_field.clone());
    let health = health::check(
        &mut *conn,
        viewer,
        health::Inputs {
            full: &full,
            column_values: &column_values,
            projects: &projects,
            attached: &attached,
            fields: &checked_fields,
            query_mode: Mode::parse(&full.board.mode) == Some(Mode::Query),
        },
    )
    .await?;
    Ok(BoardRow {
        full,
        column_field,
        lane_field,
        column_values,
        projects,
        rights,
        profile,
        query,
        query_problems,
        fields,
        hidden,
        attached,
        health,
        lane_names,
    })
}

/// Boards the viewer can view (BRD-001, BRD-002), favorites first, then by name; `project` keeps
/// boards that include it.
pub async fn list(
    pool: &PgPool,
    viewer: &Access,
    q: Option<&str>,
    project: Option<ProjectId>,
    after: Option<&After>,
    limit: i64,
) -> Result<BoardList> {
    let mut conn = pool.acquire().await?;
    let read = viewer.projects_with(Permission::IssueRead);
    let who = Viewer {
        me: viewer.user(),
        groups: viewer.groups(),
        read: &read,
    };
    let items = store::listed(&mut *conn, &who, q, project, after, limit).await?;
    let visited = store::profiles::last_visited(&mut *conn, viewer.user(), 10).await?;
    let boards = if visited.is_empty() {
        Vec::new()
    } else {
        store::get_many(&mut *conn, &visited).await?
    };
    let last_visited = visited.into_iter().find(|id| {
        boards
            .iter()
            .any(|b| b.id == *id && b.access().can_view(viewer))
    });
    let mut ids: Vec<ProjectId> = items
        .iter()
        .flat_map(|b| b.projects.iter().copied())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let projects = if ids.is_empty() {
        Vec::new()
    } else {
        projects::get_many(&mut *conn, &ids).await?
    };
    Ok(BoardList {
        items,
        last_visited,
        projects,
    })
}

/// The projects exist, are not being deleted, and the viewer reads issues in each (BRD-010).
async fn check_projects(
    conn: &mut PgConnection,
    viewer: &Access,
    ids: &[ProjectId],
) -> Result<Vec<Project>> {
    let found = projects::get_many(&mut *conn, ids).await?;
    let readable = |p: &&Project| !p.is_deleting() && viewer.has_in(Permission::IssueRead, p.id);
    if found.iter().filter(readable).count() != ids.len() {
        return Err(Error::field(
            "projects",
            Problem::Invalid,
            "no such project, or you do not read its issues",
        ));
    }
    Ok(found)
}

/// The audiences checked (D-038): accounts active, groups visible; sharing beyond the owner
/// needs the share-views capability (ACC-066).
async fn check_sharing(
    conn: &mut PgConnection,
    viewer: &Access,
    owner: UserId,
    sharing: Sharing,
) -> Result<Sharing> {
    let mut problems = Problems::new();
    let view = problems.check("sharing.view", sharing.view.validated(owner));
    let edit = problems.check("sharing.edit", sharing.edit.validated(owner));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(view), Some(edit)) = (view, edit) else {
        return Err(internal("validated values are missing"));
    };
    check_audiences(conn, viewer, &[&view, &edit]).await?;
    Ok(Sharing {
        view,
        edit,
        ..sharing
    })
}

/// The fields of the projects as presets read them: three queries.
async fn preset_fields(
    conn: &mut PgConnection,
    projects: &[ProjectId],
) -> Result<Vec<ProjectFields>> {
    let attached = store_fields::of_projects(&mut *conn, projects).await?;
    let mut ids: Vec<FieldId> = attached.iter().map(|a| a.field_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let defs = store_fields::get_many(&mut *conn, &ids).await?;
    let mut sets: Vec<i64> = attached.iter().filter_map(|a| a.set_id).collect();
    sets.sort_unstable();
    sets.dedup();
    let values = if sets.is_empty() {
        Vec::new()
    } else {
        value_sets::values_of(&mut *conn, &sets).await?
    };
    Ok(projects
        .iter()
        .map(|p| {
            attached
                .iter()
                .filter(|a| a.project_id == *p)
                .filter_map(|a| {
                    let def = defs.iter().find(|f| f.id == a.field_id)?;
                    let kind = FieldKind::parse(&def.kind)?;
                    Some(PresetField {
                        field: def.id,
                        kind,
                        multi: def.multi,
                        builtin: def.builtin.as_deref().and_then(BuiltinField::parse),
                        values: values
                            .iter()
                            .filter(|v| Some(v.set_id) == a.set_id)
                            .map(|v| SetValue {
                                id: v.id,
                                resolved: v.resolved,
                                archived: v.archived,
                            })
                            .collect(),
                    })
                })
                .collect()
        })
        .collect())
}

/// Creates a board from a preset (BRD-003…BRD-010): its first sprint (or the internal one of a
/// sprintless board) and its columns.
pub async fn create(
    pool: &PgPool,
    viewer: &Access,
    language: Language,
    input: NewBoard,
) -> Result<BoardRow> {
    let mut problems = Problems::new();
    let name = problems.check("name", domain::board_name(&input.name));
    let project_ids = problems.check("projects", domain::projects(input.projects));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(project_ids)) = (name, project_ids) else {
        return Err(internal("validated values are missing"));
    };
    let mut tx = pool.begin().await?;
    let me = users::get(&mut *tx, viewer.user())
        .await?
        .ok_or_else(not_found)?;
    if me.is_guest() {
        return Err(Error::Refused("guests do not create boards".into()));
    }
    check_projects(&mut tx, viewer, &project_ids).await?;
    if !may_create(viewer, me.is_guest(), &project_ids) {
        return Err(Error::Refused(
            "you need to read issues in every board project".into(),
        ));
    }
    let sharing = if input.sharing.is_private() {
        input.sharing
    } else {
        if !viewer.capability(prostor_domain::access::capability::Capability::ShareViews) {
            return Err(Error::Refused("sharing is not switched on for you".into()));
        }
        check_sharing(&mut tx, viewer, viewer.user(), input.sharing).await?
    };
    let backlog_view = sharing.view.clone();
    let id = found(
        &mut tx,
        Founding {
            owner: viewer.user(),
            preset: input.preset,
            name,
            projects: project_ids,
            sharing,
            backlog_view,
            language,
        },
    )
    .await?;
    tx.commit().await?;
    get(pool, viewer, id).await
}

/// What a board is founded with; the checks are the caller's.
pub(crate) struct Founding {
    pub owner: UserId,
    pub preset: Preset,
    pub name: String,
    pub projects: Vec<ProjectId>,
    pub sharing: Sharing,
    /// Who views the default backlog besides the owner.
    pub backlog_view: Audience,
    pub language: Language,
}

/// Inserts a board from a preset (BRD-003…BRD-010): its columns, its first sprint (or the
/// internal one of a sprintless board) and its default backlog (BRD-089).
pub(crate) async fn found(conn: &mut PgConnection, f: Founding) -> Result<BoardId> {
    let fields = preset_fields(&mut *conn, &f.projects).await?;
    let setup = presets::setup(f.preset, &fields);
    let mut taken: Vec<FieldId> = setup.column_field.into_iter().collect();
    taken.extend(setup.color_field);
    taken.extend(setup.estimation_field);
    let card_fields = fields
        .first()
        .map(|first| card_setup::default_fields(first, &taken))
        .unwrap_or_default();
    let settings = Settings {
        name: f.name.clone(),
        owner: f.owner,
        preset: f.preset.key().to_owned(),
        sprints_on: setup.sprints_on,
        mode: Mode::Manual.key().to_owned(),
        query: None,
        auto_sprint: None,
        column_field: setup.column_field,
        card_fields,
        card_initials: Vec::new(),
        color_mode: if setup.color_field.is_some() {
            ColorMode::Field
        } else {
            ColorMode::None
        }
        .key()
        .to_owned(),
        color_field: setup.color_field,
        estimation_field: setup.estimation_field,
        original_estimation_field: None,
        backlog_search: None,
        lanes: store::Lanes::default(),
        view: f.sharing.view,
        edit: f.sharing.edit,
        view_readers: f.sharing.view_readers,
        edit_updaters: f.sharing.edit_updaters,
    };
    let id = store::insert(&mut *conn, &settings, &f.projects).await?;
    if !setup.columns.is_empty() {
        let values: Vec<i64> = setup.columns.iter().map(|(v, _)| *v).collect();
        let ranks = rank::sequence(values.len());
        let min: Vec<Option<i32>> = setup.columns.iter().map(|(_, w)| w.min).collect();
        let max: Vec<Option<i32>> = setup.columns.iter().map(|(_, w)| w.max).collect();
        store::columns::insert_many(&mut *conn, id, &values, &ranks, &min, &max).await?;
    }
    if setup.sprints_on {
        store::sprints::insert(&mut *conn, id, &numbered_name(f.language, 1), false).await?;
    } else {
        store::sprints::insert(&mut *conn, id, INTERNAL_NAME, true).await?;
    }
    let mut found = projects::get_many(&mut *conn, &f.projects).await?;
    found.sort_by_key(|p| f.projects.iter().position(|id| *id == p.id));
    let keys: Vec<String> = found.into_iter().map(|p| p.key).collect();
    let backlog = backlog::create_default(
        &mut *conn,
        backlog::NewBacklog {
            owner: f.owner,
            board: id,
            board_name: &f.name,
            keys: &keys,
            projects: &f.projects,
            view: f.backlog_view,
            language: f.language,
        },
    )
    .await?;
    if backlog.is_some() {
        store::set_backlog(&mut *conn, id, backlog).await?;
    }
    audit::record(
        &mut *conn,
        Actor::User(f.owner),
        "board.created",
        &target(id),
        json!({ "name": f.name, "preset": f.preset.key(), "projects": f.projects }),
    )
    .await?;
    Ok(id)
}

/// Changes a board (BRD-010…BRD-013, BRD-042): name, projects and column field need the edit
/// right, sharing the share right, a new owner the transfer right.
pub async fn update(
    pool: &PgPool,
    viewer: &Access,
    language: Language,
    id: BoardId,
    change: BoardChange,
) -> Result<BoardRow> {
    let mut problems = Problems::new();
    let name = change
        .name
        .as_deref()
        .and_then(|n| problems.check("name", domain::board_name(n)));
    let project_ids = change
        .projects
        .clone()
        .and_then(|p| problems.check("projects", domain::projects(p)));
    problems.finish().map_err(Error::Invalid)?;
    let mut tx = pool.begin().await?;
    let board = lock_viewed(&mut tx, viewer, id).await?;
    if board.version != change.version {
        return Err(Error::Conflict("the board was changed by someone else"));
    }
    let access = board.access();
    let settles = change.name.is_some()
        || change.projects.is_some()
        || change.column_field.is_some()
        || change.sprints_on.is_some()
        || change.mode.is_some()
        || change.query.is_some()
        || change.auto_sprint.is_some()
        || change.backlog.is_some()
        || change.new_backlog
        || change.lanes.is_some()
        || change.cards.is_some();
    if settles && !access.can_edit(viewer) {
        return Err(Error::Refused("you may not change this board".into()));
    }
    let mut settings = Settings::of(&board);
    if let Some(name) = name {
        settings.name = name;
    }
    let projects = project_ids.unwrap_or_else(|| board.projects.clone());
    if projects != board.projects {
        // Only the added projects: a board keeps the projects others chose (BRD-010).
        let added: Vec<ProjectId> = projects
            .iter()
            .copied()
            .filter(|p| !board.projects.contains(p))
            .collect();
        check_projects(&mut tx, viewer, &added).await?;
    }
    if let Some(field) = change.column_field
        && field != board.column_field_id
    {
        if let Some(field) = field {
            column_field_allowed(&mut tx, field, &projects).await?;
        }
        settings.column_field = field;
        store::columns::delete_all(&mut tx, id).await?;
    }
    if let Some(owner) = change.owner.filter(|o| *o != board.owner_id) {
        access.may_transfer(viewer)?;
        let found = users::get(&mut *tx, owner).await?;
        if found.is_none_or(|u| u.is_deleted() || u.is_guest()) {
            return Err(Error::field("owner", Problem::Invalid, "no such account"));
        }
        settings.owner = owner;
    }
    if let Some(sharing) = change.sharing {
        access.may_share(viewer)?;
        let sharing = check_sharing(&mut tx, viewer, settings.owner, sharing).await?;
        settings.view = sharing.view;
        settings.edit = sharing.edit;
        settings.view_readers = sharing.view_readers;
        settings.edit_updaters = sharing.edit_updaters;
    } else if settings.owner != board.owner_id {
        // The new owner leaves the lists: owning gives every right.
        let owner = settings.owner;
        settings.view.users.retain(|u| *u != owner);
        settings.edit.users.retain(|u| *u != owner);
    }
    let sprints_switched = change.sprints_on.is_some_and(|on| on != board.sprints_on);
    if sprints_switched {
        settings.sprints_on = !board.sprints_on;
        switch_sprints(&mut tx, &board, language, settings.sprints_on).await?;
        settings.auto_sprint = None;
    }
    if let Some(mode) = change.mode {
        settings.mode = mode.key().to_owned();
    }
    let was_query = Mode::parse(&board.mode) == Some(Mode::Query);
    let is_query = Mode::parse(&settings.mode) == Some(Mode::Query);
    if is_query && settings.sprints_on {
        let path = if sprints_switched {
            "sprints_on"
        } else {
            "mode"
        };
        return Err(Error::field(
            path,
            Problem::Invalid,
            "a query board has no sprints",
        ));
    }
    if is_query && (change.query.is_some() || !was_query) {
        let text = change.query.as_deref().unwrap_or("");
        let stored = crate::search::bind_within(&mut tx, viewer, text, &projects).await?;
        if stored
            .bindings
            .iter()
            .any(|(_, b)| *b == Binding::Board(board.id))
        {
            return Err(Error::field(
                "query",
                Problem::Invalid,
                "a board query cannot name its own board",
            ));
        }
        settings.query = Some(crate::saved_searches::encode(&stored)?);
    } else if change.query.is_some() && !is_query {
        return Err(Error::field(
            "query",
            Problem::Invalid,
            "only a board in mode query has a query",
        ));
    }
    if is_query && (!was_query || projects != board.projects) {
        let columns: Vec<i64> = store::columns::of_board(&mut *tx, id)
            .await?
            .iter()
            .map(|c| c.value_id)
            .collect();
        let path = if was_query { "projects" } else { "mode" };
        health::defaults_are_columns(&mut tx, settings.column_field, &projects, &columns, path)
            .await?;
    }
    if was_query && !is_query {
        pin_cards(&mut tx, viewer, &board).await?;
        settings.query = None;
    }
    if let Some(sprint) = change.auto_sprint {
        if let Some(sprint) = sprint {
            let all = store::sprints::of_board(&mut *tx, id).await?;
            let usable = settings.sprints_on
                && all
                    .iter()
                    .any(|s| s.id == sprint && !s.internal && s.archived_at.is_none());
            if !usable {
                return Err(Error::field(
                    "auto_sprint",
                    Problem::Invalid,
                    "no such sprint",
                ));
            }
        }
        settings.auto_sprint = sprint;
    }
    if let Some(choice) = change.backlog
        && choice != board.backlog_search_id
    {
        if let Some(search) = choice {
            backlog::check_choice(&mut tx, viewer, search, &projects).await?;
        }
        settings.backlog_search = choice;
    }
    if change.new_backlog {
        let mut found = projects::get_many(&mut *tx, &projects).await?;
        found.sort_by_key(|p| projects.iter().position(|id| *id == p.id));
        let keys: Vec<String> = found.into_iter().map(|p| p.key).collect();
        // Shared like a new board's backlog: with the board's viewers when the caller may share.
        let view = if access.may_share(viewer).is_ok() {
            settings.view.clone()
        } else {
            Audience::default()
        };
        let made = backlog::create_default(
            &mut tx,
            backlog::NewBacklog {
                owner: viewer.user(),
                board: id,
                board_name: &settings.name,
                keys: &keys,
                projects: &projects,
                view,
                language,
            },
        )
        .await?
        .ok_or(Error::Refused(
            "you have as many saved searches as allowed".into(),
        ))?;
        settings.backlog_search = Some(made);
    }
    if let Some(lanes) = &change.lanes {
        settings.lanes =
            lanes::check_change(&mut tx, viewer, lanes, &projects, settings.column_field).await?;
    }
    let colors = match change.cards {
        Some(cards) => {
            card_setup::check_change(&mut tx, viewer, &board, &projects, cards, &mut settings)
                .await?
        }
        None => Vec::new(),
    };
    store::update(&mut tx, id, &settings).await?;
    if projects != board.projects {
        store::set_projects(&mut tx, id, &projects).await?;
    }
    if !colors.is_empty() {
        store::set_project_colors(&mut tx, id, &colors).await?;
    }
    audit::record(
        &mut tx,
        Actor::User(viewer.user()),
        "board.changed",
        &target(id),
        json!({
            "name": change.name,
            "projects": change.projects,
            "owner": settings.owner,
            "column_field": change.column_field,
            "sprints_on": settings.sprints_on,
            "mode": settings.mode,
            "auto_sprint": settings.auto_sprint,
            "backlog": settings.backlog_search,
            "cards": {
                "fields": settings.card_fields,
                "initials": settings.card_initials,
                "color": settings.color_mode,
                "color_field": settings.color_field,
                "estimation": settings.estimation_field,
                "original_estimation": settings.original_estimation_field,
                "project_colors": colors,
            },
            "lanes": {
                "mode": settings.lanes.mode,
                "field": settings.lanes.field,
                "attribute": settings.lanes.attribute,
                "values": settings.lanes.values,
                "other": settings.lanes.other,
                "new_card_value": settings.lanes.new_card_value,
            },
            "view": { "users": settings.view.users, "groups": settings.view.groups },
            "edit": { "users": settings.edit.users, "groups": settings.edit.groups },
            "view_readers": settings.view_readers,
            "edit_updaters": settings.edit_updaters,
        }),
    )
    .await?;
    changed(&mut tx, id).await?;
    tx.commit().await?;
    // The viewer may have given away their last right to view it (BRD-011): the answer still
    // shows the board as it is now, with no rights.
    let mut conn = pool.acquire().await?;
    load(&mut conn, viewer, id, false).await
}

/// Leaving query mode (BRD-028): every issue the board query shows now becomes a card, for every
/// viewer (the actor's names and day, no visibility); refused above [`PIN_MAX`].
async fn pin_cards(conn: &mut PgConnection, actor: &Access, board: &store::Board) -> Result<()> {
    let (Some(field), Some(stored)) = (board.column_field_id, &board.query) else {
        return Ok(());
    };
    let (text, lost) = crate::saved_searches::text_of(&mut *conn, actor, stored).await?;
    if !lost.is_empty() {
        return Err(Error::Refused(
            "the board query lost a name; fix it before leaving query mode".into(),
        ));
    }
    let within = crate::search::resolve_within(
        &mut *conn,
        actor,
        &text,
        &board.projects,
        None,
        crate::search::QueryBoards::Leave,
    )
    .await?;
    let sprint = store::sprints::of_board(&mut *conn, board.id)
        .await?
        .into_iter()
        .find(|s| s.internal)
        .ok_or_else(|| internal("a sprintless board without its sprint"))?;
    let values: Vec<i64> = store::columns::of_board(&mut *conn, board.id)
        .await?
        .iter()
        .map(|c| c.value_id)
        .collect();
    let everyone = prostor_store::visibility::Visibility {
        me: actor.user(),
        read: prostor_domain::access::Projects::All,
        create_only: Vec::new(),
        bypass: prostor_domain::access::Projects::All,
        comment_read: prostor_domain::access::Projects::All,
        groups: actor.groups().to_vec(),
        within: None,
    };
    let scope = store::view::Scope {
        sprint: sprint.id,
        column_field: field,
        column_values: &values,
        projects: &board.projects,
        query_mode: true,
        filter: Some(&within.filter),
        sort: None,
        // Every card is pinned, whatever its lane.
        lanes: None,
        lane_page: None,
        estimate: None,
    };
    match store::view::pin(&mut *conn, &scope, &everyone, PIN_MAX).await? {
        store::view::Pinned::Done(_) => Ok(()),
        store::view::Pinned::TooMany(n) => Err(Error::field(
            "mode",
            Problem::OutOfRange,
            format!("the board query shows {n} issues; at most {PIN_MAX} can stay as cards"),
        )),
    }
}

/// Switches sprints off or on (BRD-024). Off: the current sprint (else the latest visible one)
/// becomes the hidden sprint holding the cards, the others are archived. On: the hidden sprint
/// becomes «Спринт N».
async fn switch_sprints(
    conn: &mut PgConnection,
    board: &store::Board,
    language: Language,
    on: bool,
) -> Result<()> {
    let all = store::sprints::of_board(&mut *conn, board.id).await?;
    let mut batch = Batch::new();
    if on {
        let hidden = all
            .iter()
            .find(|s| s.internal)
            .ok_or_else(|| internal("a sprintless board without its sprint"))?;
        let taken: Vec<&str> = all
            .iter()
            .filter(|s| !s.internal)
            .map(|s| s.name.as_str())
            .collect();
        let name = free_numbered_name(language, &taken);
        store::sprints::set_internal(conn, hidden.id, false, &name).await?;
        batch.sprint(board.id, hidden.id, Change::Updated);
    } else {
        let now = time::OffsetDateTime::now_utc();
        let today = view::today(&mut *conn, now).await?;
        let dates: Vec<_> = all.iter().map(store::Sprint::dates).collect();
        let kept = current(&dates, today)
            .or_else(|| {
                all.iter()
                    .rev()
                    .find(|s| !s.internal && s.archived_at.is_none())
                    .map(|s| s.id)
            })
            .or_else(|| all.iter().rev().find(|s| !s.internal).map(|s| s.id))
            .ok_or_else(|| internal("a sprint board without sprints"))?;
        store::sprints::set_internal(conn, kept, true, INTERNAL_NAME).await?;
        batch.sprint(board.id, kept, Change::Updated);
        for archived in store::sprints::archive_others(conn, board.id, kept).await? {
            batch.sprint(board.id, archived, Change::Updated);
        }
    }
    batch.publish(conn).await
}

/// A column field (BRD-042): single-value, of a list kind, attached to a board project.
async fn column_field_allowed(
    conn: &mut PgConnection,
    field: FieldId,
    projects: &[ProjectId],
) -> Result<Field> {
    let def = store_fields::get(&mut *conn, field)
        .await?
        .ok_or_else(|| Error::field("column_field", Problem::Invalid, "no such field"))?;
    let kind = FieldKind::parse(&def.kind).ok_or_else(|| internal("unknown field kind"))?;
    domain::columns::field_allowed(kind, def.multi).map_err(rejected("column_field"))?;
    let attached = store_fields::of_projects(&mut *conn, projects).await?;
    if !attached.iter().any(|a| a.field_id == field) {
        return Err(Error::field(
            "column_field",
            Problem::Invalid,
            "the field is not attached to a board project",
        ));
    }
    Ok(def)
}

/// What deleting the board takes with it (BRD-016, D-013).
pub async fn impact(pool: &PgPool, viewer: &Access, id: BoardId) -> Result<Impact> {
    let mut conn = pool.acquire().await?;
    let board = store::get(&mut *conn, id)
        .await?
        .filter(|b| b.access().can_view(viewer))
        .ok_or_else(not_found)?;
    let found = store::impact(&mut *conn, board.id).await?;
    Ok(Impact {
        sprints: found.sprints,
        cards: found.cards,
        searches: 0,
    })
}

/// Deletes a board with its sprints and cards (BRD-016): the owner or `system.write`. Issues keep
/// their history.
pub async fn delete(pool: &PgPool, viewer: &Access, id: BoardId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let board = store::lock(&mut tx, id)
        .await?
        .filter(|b| {
            let a = b.access();
            a.can_view(viewer) || a.may_delete(viewer)
        })
        .ok_or_else(not_found)?;
    if !board.access().may_delete(viewer) {
        return Err(Error::Refused(
            "only the owner or an administrator deletes a board".into(),
        ));
    }
    let mut batch = Batch::new();
    batch.board_deleted(&board);
    // Query boards naming this board hear of the cards that leave (BRD-114).
    for issue in store::cards::named_rows(&mut *tx, id, None).await? {
        batch.issue_on_board(issue, Change::Updated, id);
    }
    store::delete(&mut tx, id).await?;
    audit::record(
        &mut tx,
        Actor::User(viewer.user()),
        "board.deleted",
        &target(id),
        json!({ "name": board.name, "owner": board.owner_id }),
    )
    .await?;
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Changes the person's view of a board (BRD-001, BRD-047, BRD-058); synced across devices.
pub async fn set_profile(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    change: ProfileChange,
) -> Result<Profile> {
    let card_size = change
        .card_size
        .map(domain::card_size)
        .transpose()
        .map_err(rejected("card_size"))?;
    let mut tx = pool.begin().await?;
    lock_viewed(&mut tx, viewer, id).await?;
    let mut profile = store::profiles::get(&mut *tx, viewer.user(), id).await?;
    if let Some(favorite) = change.favorite {
        profile.favorite = favorite;
    }
    if let Some(size) = card_size {
        profile.card_size = size;
    }
    if let Some(mut columns) = change.collapsed_columns {
        let known = store::columns::of_board(&mut *tx, id).await?;
        columns.retain(|c| known.iter().any(|k| k.id == *c));
        columns.sort_unstable();
        columns.dedup();
        profile.collapsed_columns = columns;
    }
    if let Some(mut lanes) = change.collapsed_lanes {
        lanes.sort_unstable();
        lanes.dedup();
        lanes.truncate(1_000);
        profile.collapsed_lanes = lanes;
    }
    store::profiles::upsert(&mut tx, viewer.user(), id, &profile).await?;
    tx.commit().await?;
    Ok(profile)
}
