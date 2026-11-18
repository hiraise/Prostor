//! Lanes (BRD-049…BRD-057, BRD-096; plan 1.8 §3.14): the lane settings, where a card's lane comes
//! from, moving a card to another lane (a field value, a tag or the parent), parent lanes —
//! new ones, their order, back to the backlog — and parents joining a sprint as lanes.

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::boards::lanes::{
    self as domain, Attribute, Bounds, LaneKey, LaneMode, OtherLane, Source,
};
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::{Origin, Record};
use taskmaster_domain::issues::rights;
use taskmaster_domain::issues::values::Input;
use taskmaster_domain::links::Side;
use taskmaster_domain::live::Change;
use taskmaster_domain::rank;
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{BoardId, FieldId, IssueId, ProjectId, SprintId};
use taskmaster_store::PgPool;
use taskmaster_store::boards::cards as store_cards;
use taskmaster_store::boards::view::{DateOf, LaneScope, LaneSource};
use taskmaster_store::boards::{self as store, Sprint};
use taskmaster_store::fields::{self as store_fields, Field};
use taskmaster_store::issues::Issue;
use time::OffsetDateTime;

use super::cards::{place, sprint_of};
use super::view::{Lane, is_query, zone_name};
use super::{BoardRow, load, rejected};
use crate::issues::{
    Described, FieldChange, FieldWritten, author, facts, finish_change, lock_for_change,
    set_field_locked,
};
use crate::live::Batch;
use crate::seed::internal;
use crate::{Error, Result};

/// The lanes of a board, ready for its statements.
#[derive(Debug, Clone)]
pub(crate) struct LaneSetup {
    pub mode: LaneMode,
    /// `values` only.
    pub source: Option<Source>,
    date_of: Option<DateOf>,
    /// The lanes (`values`) or the values a parent holds (`parents`), in order.
    pub keys: Vec<LaneKey>,
    key_text: Vec<String>,
    ids: Vec<i64>,
    pub other: OtherLane,
    /// The lane field (`values`) or the field limiting parents.
    pub field: Option<Field>,
    zone: String,
    bounds: Bounds,
}

impl LaneSetup {
    pub fn scope(&self) -> LaneScope<'_> {
        let source = match (self.mode, self.source) {
            (LaneMode::Parents, _) => LaneSource::Parents {
                field: self
                    .field
                    .as_ref()
                    .filter(|_| !self.ids.is_empty())
                    .map(|f| (f.id, &self.ids[..])),
            },
            (_, Some(Source::Projects)) => LaneSource::Projects { ids: &self.ids },
            (_, Some(Source::Tags)) => LaneSource::Tags { ids: &self.ids },
            (_, Some(Source::Dates)) => LaneSource::Dates {
                date: self.date_of.unwrap_or(DateOf::Created),
                bounds: self.bounds,
                zone: &self.zone,
                keys: &self.key_text,
            },
            (_, source) => LaneSource::Refs {
                field: self.field.as_ref().map_or(0, |f| f.id),
                prefix: source.unwrap_or(Source::Values).prefix(),
                ids: &self.ids,
            },
        };
        LaneScope {
            source,
            other: self.other != OtherLane::Off,
        }
    }

    /// The lanes of a `values` board in the order shown.
    pub fn value_lanes(&self) -> Vec<LaneKey> {
        domain::ordered(self.keys.clone(), self.other)
    }
}

/// The board's lanes; `None` without lanes or when their field is gone.
pub(crate) async fn setup(
    conn: &mut PgConnection,
    board: &BoardRow,
    now: OffsetDateTime,
) -> Result<Option<LaneSetup>> {
    let b = &board.full.board;
    let Some(mode) = LaneMode::parse(&b.lane_mode).filter(|m| *m != LaneMode::None) else {
        return Ok(None);
    };
    let field = board.lane_field.clone();
    // FLD-062: a lane field the viewer may not read leaves the view laneless — its value IDs
    // must not ride the lane keys, cards and counts.
    if field.as_ref().is_some_and(|f| !board.shown(f.id)) {
        return Ok(None);
    }
    let kind = field.as_ref().and_then(|f| FieldKind::parse(&f.kind));
    let (source, date_of) = match mode {
        LaneMode::Parents => (None, None),
        _ => match (kind, b.lane_attribute.as_deref().and_then(Attribute::parse)) {
            (Some(kind), _) => {
                let Ok(source) = Source::of_field(kind) else {
                    return Ok(None);
                };
                let id = field.as_ref().map_or(0, |f| f.id);
                let date_of = match kind {
                    FieldKind::Date => Some(DateOf::Date(id)),
                    FieldKind::DateTime => Some(DateOf::DateTime(id)),
                    _ => None,
                };
                (Some(source), date_of)
            }
            (None, Some(attribute)) => (
                Some(Source::of_attribute(attribute)),
                match attribute {
                    Attribute::Created => Some(DateOf::Created),
                    Attribute::Updated => Some(DateOf::Updated),
                    _ => None,
                },
            ),
            (None, None) => return Ok(None),
        },
    };
    let keys = domain::values(source.unwrap_or(Source::Values), &b.lane_values).unwrap_or_default();
    let (zone, bounds) = if source == Some(Source::Dates) {
        let zone = zone_name(&mut *conn).await?;
        let today = super::view::today(&mut *conn, now).await?;
        (zone, Bounds::of(today))
    } else {
        let today = now.date();
        ("UTC".to_owned(), Bounds::of(today))
    };
    Ok(Some(LaneSetup {
        mode,
        source,
        date_of,
        key_text: keys.iter().map(ToString::to_string).collect(),
        ids: keys.iter().filter_map(|k| k.id()).collect(),
        keys,
        other: OtherLane::parse(&b.other_lane).unwrap_or(OtherLane::Bottom),
        field,
        zone,
        bounds,
    }))
}

/// A change of the lane settings (BRD-049, BRD-052, BRD-055).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanesChange {
    pub mode: LaneMode,
    /// `values`: the field, or else the attribute; `parents`: the optional field.
    pub field: Option<FieldId>,
    pub attribute: Option<Attribute>,
    /// Lane keys in order (`v:…`, `u:…`, `d:…`…).
    pub values: Vec<String>,
    pub other: OtherLane,
    /// `parents` only: the value new cards get (BRD-055).
    pub new_card_value: Option<i64>,
}

/// Validates lane settings against the board projects; answers what the store keeps.
pub(super) async fn check_change(
    conn: &mut PgConnection,
    viewer: &Access,
    change: &LanesChange,
    projects: &[ProjectId],
    column_field: Option<FieldId>,
) -> Result<store::Lanes> {
    let none = store::Lanes {
        other: change.other.key().to_owned(),
        ..store::Lanes::default()
    };
    if change.mode == LaneMode::None {
        return Ok(none);
    }
    // The sets the field uses in the board projects: lane values come from them.
    let mut sets = Vec::new();
    let field = match change.field {
        Some(id) => {
            let def = store_fields::get(&mut *conn, id)
                .await?
                .ok_or_else(|| Error::field("lanes.field", Problem::Invalid, "no such field"))?;
            let attached = store_fields::of_projects(&mut *conn, projects).await?;
            sets.extend(
                attached
                    .iter()
                    .filter(|a| a.field_id == id)
                    .filter_map(|a| a.set_id),
            );
            if !attached.iter().any(|a| a.field_id == id) {
                return Err(Error::field(
                    "lanes.field",
                    Problem::Invalid,
                    "the field is not attached to a board project",
                ));
            }
            Some(def)
        }
        None => None,
    };
    let kind = field.as_ref().and_then(|f| FieldKind::parse(&f.kind));
    let source = match change.mode {
        LaneMode::Parents => {
            if let Some(kind) = kind {
                domain::parent_field_allowed(kind).map_err(rejected("lanes.field"))?;
            }
            Source::Values
        }
        _ => match (kind, change.attribute) {
            (Some(kind), _) => {
                if change.field == column_field {
                    return Err(Error::field(
                        "lanes.field",
                        Problem::Invalid,
                        "the columns come from this field already",
                    ));
                }
                Source::of_field(kind).map_err(rejected("lanes.field"))?
            }
            (None, Some(attribute)) => Source::of_attribute(attribute),
            (None, None) => {
                return Err(Error::field(
                    "lanes.field",
                    Problem::Required,
                    "choose a field or an attribute for the lanes",
                ));
            }
        },
    };
    let keys = domain::values(source, &change.values).map_err(rejected("lanes.values"))?;
    let ids: Vec<i64> = keys.iter().filter_map(|k| k.id()).collect();
    match source {
        Source::Values if !ids.is_empty() => {
            let found = taskmaster_store::value_sets::get_values(&mut *conn, &ids).await?;
            let all_of_set = ids.iter().all(|id| {
                found
                    .iter()
                    .any(|v| v.id == *id && sets.contains(&v.set_id))
            });
            if !all_of_set || field.is_none() {
                return Err(Error::field(
                    "lanes.values",
                    Problem::Invalid,
                    "a value is not of the lane field",
                ));
            }
        }
        Source::Projects if ids.iter().any(|p| !projects.contains(p)) => {
            return Err(Error::field(
                "lanes.values",
                Problem::Invalid,
                "a project is not on the board",
            ));
        }
        Source::Tags => {
            let found = taskmaster_store::tags::get_many(&mut *conn, &ids).await?;
            let usable = ids.iter().all(|id| {
                found
                    .iter()
                    .any(|t| t.id == *id && t.sharing().can_use(viewer))
            });
            if !usable {
                return Err(Error::field(
                    "lanes.values",
                    Problem::Invalid,
                    "no such tag",
                ));
            }
        }
        _ => {}
    }
    let new_card_value = match change.new_card_value {
        Some(_) if change.mode != LaneMode::Parents => {
            return Err(Error::field(
                "lanes.new_card_value",
                Problem::Invalid,
                "only parent lanes give new cards a value",
            ));
        }
        Some(id) => {
            if taskmaster_store::value_sets::get_values(&mut *conn, &[id])
                .await?
                .is_empty()
            {
                return Err(Error::field(
                    "lanes.new_card_value",
                    Problem::Invalid,
                    "no such value",
                ));
            }
            Some(id)
        }
        None => None,
    };
    Ok(store::Lanes {
        mode: change.mode.key().to_owned(),
        field: change.field,
        attribute: if field.is_none() && change.mode == LaneMode::Values {
            change.attribute.map(|a| a.key().to_owned())
        } else {
            None
        },
        values: if source == Source::Dates && change.values.is_empty() {
            Vec::new()
        } else {
            keys.iter().map(ToString::to_string).collect()
        },
        other: change.other.key().to_owned(),
        new_card_value,
    })
}

/// What a lane move did: the records for the caller's `finish_change`, and the field writes for
/// the behaviors to hear (§3.5) — a lane move is a field change like any other, so board paths
/// fire the same events (step 1.9b-3).
pub(super) struct LaneMoved {
    pub records: Vec<Record>,
    pub writes: Vec<FieldWritten>,
}

/// Moves a locked issue from lane `from` to lane `to` (BRD-050, BRD-051): sets the lane value (a
/// multi-value field or tags: the value of the lane it leaves goes, the new one comes), clears it
/// for `other`, or re-parents. Answers the history records of field and tag changes, and the
/// field writes the behaviors hear; links record themselves.
#[allow(clippy::too_many_arguments)]
pub(super) async fn set_lane(
    conn: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    setup: &LaneSetup,
    issue: &Issue,
    from: Option<&str>,
    to: &str,
) -> Result<LaneMoved> {
    let target =
        LaneKey::parse(to).ok_or_else(|| Error::field("lane", Problem::Invalid, "no such lane"))?;
    let from = from.and_then(LaneKey::parse);
    if from == Some(target) {
        return Ok(LaneMoved {
            records: Vec::new(),
            writes: Vec::new(),
        });
    }
    if setup.mode == LaneMode::Parents {
        let parent = match target {
            LaneKey::Other => None,
            LaneKey::Issue(id) => Some(id),
            _ => return Err(Error::field("lane", Problem::Invalid, "no such lane")),
        };
        set_parent(conn, batch, actor, origin, issue, parent).await?;
        return Ok(LaneMoved {
            records: Vec::new(),
            writes: Vec::new(),
        });
    }
    let source = setup.source.unwrap_or(Source::Values);
    if !source.settable() {
        return Err(Error::Refused(
            "cards do not change their project or dates by moving between these lanes".into(),
        ));
    }
    if target != LaneKey::Other && !setup.keys.contains(&target) {
        return Err(Error::field("lane", Problem::Invalid, "no such lane"));
    }
    let lane_ids: Vec<i64> = setup.keys.iter().filter_map(|k| k.id()).collect();
    let leaving: Vec<i64> = match (from, target) {
        (_, LaneKey::Other) => lane_ids,
        (Some(k), _) => k.id().into_iter().collect(),
        (None, _) => Vec::new(),
    };
    let coming = target.id();
    let mut moved = LaneMoved {
        records: Vec::new(),
        writes: Vec::new(),
    };
    if source == Source::Tags {
        let mut removed = Vec::new();
        for tag in leaving {
            if taskmaster_store::tags::remove(conn, issue.id, tag).await? {
                removed.push(tag);
            }
        }
        let mut added = Vec::new();
        if let Some(tag) = coming
            && taskmaster_store::tags::add(conn, issue.id, tag).await?
        {
            added.push(tag);
        }
        moved.records = Record::tags(&removed, &added).into_iter().collect();
        return Ok(moved);
    }
    let field = setup
        .field
        .as_ref()
        .ok_or_else(|| Error::field("lane", Problem::Invalid, "no such lane"))?;
    match (field.multi, coming) {
        (false, value) => {
            write_lane_field(
                conn,
                actor,
                issue,
                field.id,
                FieldChange::Set(value.map(Input::Integer)),
                &mut moved,
            )
            .await?
        }
        (true, None) => {
            write_lane_field(
                conn,
                actor,
                issue,
                field.id,
                FieldChange::Remove(leaving),
                &mut moved,
            )
            .await?
        }
        (true, Some(value)) => {
            let gone: Vec<i64> = leaving.into_iter().filter(|v| *v != value).collect();
            if !gone.is_empty() {
                write_lane_field(
                    conn,
                    actor,
                    issue,
                    field.id,
                    FieldChange::Remove(gone),
                    &mut moved,
                )
                .await?;
            }
            write_lane_field(
                conn,
                actor,
                issue,
                field.id,
                FieldChange::Add(vec![value]),
                &mut moved,
            )
            .await?;
        }
    }
    Ok(moved)
}

/// One lane-field write of [`set_lane`]: its records join the move's, its `FieldWritten` waits
/// for the behaviors to hear.
async fn write_lane_field(
    conn: &mut PgConnection,
    actor: &Access,
    issue: &Issue,
    field: FieldId,
    change: FieldChange,
    moved: &mut LaneMoved,
) -> Result<()> {
    let written = set_field_locked(conn, actor, issue, field, change).await?;
    moved.records.extend(written.records.iter().cloned());
    moved.writes.push(written);
    Ok(())
}

/// Replaces the issue's parent through the subtask type (`None` removes it), with the rights and
/// records of links.
async fn set_parent(
    conn: &mut PgConnection,
    batch: &mut Batch,
    actor: &Access,
    origin: Origin,
    issue: &Issue,
    parent: Option<IssueId>,
) -> Result<()> {
    let current = taskmaster_store::links::parent_link(&mut *conn, issue.id).await?;
    if current.as_ref().map(|l| l.source_id) == parent {
        return Ok(());
    }
    if let Some(link) = current {
        crate::issue_links::remove_locked(conn, batch, actor, origin, issue, &link).await?;
    }
    if let Some(parent) = parent {
        let link_type = taskmaster_store::links::subtask_type(&mut *conn)
            .await?
            .ok_or_else(|| internal("the subtask link type is missing"))?;
        crate::issue_links::add_locked(
            conn,
            batch,
            actor,
            origin,
            issue,
            crate::issue_links::NewLinks {
                link_type,
                side: Side::Target,
                issues: vec![parent],
            },
            false,
        )
        .await?;
    }
    Ok(())
}

/// Whether an issue joining a parent-lane board is a lane (BRD-096): it holds a lane value of the
/// lane field, or, without one, has subtasks.
pub(super) async fn is_lane(
    conn: &mut PgConnection,
    setup: &LaneSetup,
    issue: IssueId,
) -> Result<bool> {
    if setup.mode != LaneMode::Parents {
        return Ok(false);
    }
    let field = setup.field.as_ref().filter(|_| !setup.ids.is_empty());
    Ok(taskmaster_store::boards::view::is_parent_lane(
        &mut *conn,
        issue,
        field.map(|f| (f.id, &setup.ids[..])),
    )
    .await?)
}

/// A lane rank after the sprint's last lane.
pub(super) async fn last_lane(conn: &mut PgConnection, sprint: SprintId) -> Result<String> {
    let last = store_cards::last_lane_rank(&mut *conn, sprint).await?;
    rank::between(last.as_deref(), None).map_err(internal)
}

/// Where a parent lane goes: after the lane `after`, before `before`; neither: last.
async fn lane_between(
    conn: &mut PgConnection,
    sprint: SprintId,
    moving: IssueId,
    after: Option<IssueId>,
    before: Option<IssueId>,
) -> Result<String> {
    let asked: Vec<IssueId> = after.into_iter().chain(before).collect();
    let found = store_cards::lane_ranks(&mut *conn, sprint, &asked).await?;
    let rank_of = |path: &str, id: Option<IssueId>| -> Result<Option<String>> {
        let Some(id) = id else { return Ok(None) };
        if id == moving {
            return Err(Error::field(path, Problem::Invalid, "the lane itself"));
        }
        found
            .iter()
            .find(|(issue, _)| *issue == id)
            .map(|(_, rank)| Some(rank.clone()))
            .ok_or_else(|| Error::field(path, Problem::Invalid, "not a ranked lane of this sprint"))
    };
    let low = rank_of("after", after)?;
    let high = rank_of("before", before)?;
    let (low, high) = match (low, high) {
        (Some(a), Some(b)) if a < b => (Some(a), Some(b)),
        (Some(a), _) => {
            let above = store_cards::lane_rank_above(&mut *conn, sprint, &a).await?;
            (Some(a), above)
        }
        (None, Some(b)) => (
            store_cards::lane_rank_below(&mut *conn, sprint, &b).await?,
            Some(b),
        ),
        (None, None) => (store_cards::last_lane_rank(&mut *conn, sprint).await?, None),
    };
    rank::between(low.as_deref(), high.as_deref()).map_err(internal)
}

fn parent_board<'a>(board: &'a BoardRow, setup: Option<&LaneSetup>) -> Result<&'a store::Board> {
    if setup.is_none_or(|s| s.mode != LaneMode::Parents) {
        return Err(Error::Refused("this board has no parent lanes".into()));
    }
    Ok(&board.full.board)
}

/// Puts a parent lane at a place among the lanes (BRD-053): the board's edit right. Lanes before
/// it that have no lane rank yet get one, so the order shown stays.
pub async fn move_lane(
    pool: &PgPool,
    actor: &Access,
    board_id: BoardId,
    sprint_id: SprintId,
    lane: IssueId,
    after: Option<IssueId>,
    before: Option<IssueId>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    if !board.rights.edit {
        return Err(Error::Refused("you may not change this board".into()));
    }
    let sprint = sprint_of(&board, sprint_id)?.clone();
    let setup = setup(&mut tx, &board, OffsetDateTime::now_utc()).await?;
    parent_board(&board, setup.as_ref())?;
    // Lanes named as neighbours but not ranked yet get a rank at the end, in the order given.
    for n in after.into_iter().chain(before) {
        if store_cards::lane_ranks(&mut *tx, sprint.id, &[n])
            .await?
            .is_empty()
        {
            let rank = last_lane(&mut tx, sprint.id).await?;
            store_cards::set_lane_rank(&mut tx, sprint.id, n, &rank).await?;
        }
    }
    let rank = lane_between(&mut tx, sprint.id, lane, after, before).await?;
    store_cards::set_lane_rank(&mut tx, sprint.id, lane, &rank).await?;
    let mut batch = Batch::new();
    batch.sprint(board_id, sprint.id, Change::Updated);
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// A new parent lane (BRD-054): an issue in the first board project with the first lane value,
/// `title` as its summary, joining the sprint as the last lane.
pub async fn create_lane(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    sprint_id: SprintId,
    title: &str,
) -> Result<(Lane, Described)> {
    let summary = domain::title(title).map_err(rejected("title"))?;
    let mut conn = pool.acquire().await?;
    let board = load(&mut conn, actor, board_id, true).await?;
    sprint_of(&board, sprint_id)?;
    let setup = setup(&mut conn, &board, OffsetDateTime::now_utc()).await?;
    let b = parent_board(&board, setup.as_ref())?;
    if is_query(&board) {
        return Err(Error::Refused(
            "issues join this board through its query; a new lane is not added by hand".into(),
        ));
    }
    let project = *b
        .projects
        .first()
        .ok_or(Error::Refused("the board has no project".into()))?;
    let fields = match setup.as_ref().and_then(|s| {
        let value = s.keys.first().and_then(|k| k.id())?;
        Some((s.field.as_ref()?, value))
    }) {
        Some((field, value)) => vec![crate::issues::FieldInput {
            field: field.id,
            value: Some(if field.multi {
                Input::Ids(vec![value])
            } else {
                Input::Integer(value)
            }),
        }],
        None => Vec::new(),
    };
    drop(conn);
    let made = crate::issues::create(
        pool,
        actor,
        origin,
        crate::issues::NewIssue {
            project,
            summary,
            fields,
            publish: true,
            ..crate::issues::NewIssue::default()
        },
    )
    .await?;
    let id = made.done.0.issue.id;
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    let sprint = sprint_of(&board, sprint_id)?.clone();
    let (issue, _) = lock_for_change(&mut tx, actor, id).await?;
    join_as_lane(&mut tx, actor, origin, &board, &sprint, &issue).await?;
    let count = store_cards::subtasks_in(&mut *tx, sprint.id, id)
        .await?
        .len();
    tx.commit().await?;
    let mut conn = pool.acquire().await?;
    let described = super::view::describe_ids(&mut conn, actor, vec![id]).await?;
    let lane = Lane {
        key: LaneKey::Issue(id).to_string(),
        count: i64::try_from(count).unwrap_or(i64::MAX),
        // The view sums it with the next read.
        estimate: None,
        issue: Some(id),
    };
    Ok((lane, described))
}

/// Makes a locked issue a lane of the sprint, the last one, leaving the other sprints of the
/// board (a sprint board holds an issue once), with the board record and the live event.
pub(super) async fn join_as_lane(
    tx: &mut PgConnection,
    actor: &Access,
    origin: Origin,
    board: &BoardRow,
    sprint: &Sprint,
    issue: &Issue,
) -> Result<()> {
    let b = &board.full.board;
    let places = store_cards::places(&mut *tx, b.id, issue.id).await?;
    let mut left = None;
    for p in places.iter().filter(|p| p.sprint != sprint.id) {
        store_cards::remove(&mut *tx, p.sprint, issue.id).await?;
        left = left.or_else(|| board.full.sprints.iter().find(|s| s.id == p.sprint));
    }
    let joined = !places.iter().any(|p| p.sprint == sprint.id);
    if store_cards::lane_ranks(&mut *tx, sprint.id, &[issue.id])
        .await?
        .is_empty()
    {
        let rank = last_lane(&mut *tx, sprint.id).await?;
        store_cards::set_lane_rank(&mut *tx, sprint.id, issue.id, &rank).await?;
    }
    let mut batch = Batch::new();
    if joined {
        let records: Vec<Record> =
            Record::boards(left.map(|s| place(b, s)).as_ref(), Some(&place(b, sprint)))
                .into_iter()
                .collect();
        let who = author(actor, origin, false, issue.project_id)?;
        finish_change(&mut *tx, &mut batch, issue, who, &records, &[]).await?;
    }
    batch.issue_on_board(issue.id, Change::Updated, b.id);
    batch.publish(&mut *tx).await?;
    Ok(())
}

/// Takes a parent lane back to the backlog (BRD-056): the parent and its subtasks leave the
/// sprint; each needs the update right. Not on a query board.
pub async fn lane_to_backlog(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    sprint_id: SprintId,
    lane: IssueId,
) -> Result<Vec<IssueId>> {
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    let sprint = sprint_of(&board, sprint_id)?.clone();
    let setup = setup(&mut tx, &board, OffsetDateTime::now_utc()).await?;
    let b = parent_board(&board, setup.as_ref())?.clone();
    if is_query(&board) {
        return Err(Error::Refused(
            "issues leave this board through its query; change their fields instead".into(),
        ));
    }
    let mut ids = store_cards::subtasks_in(&mut *tx, sprint.id, lane).await?;
    ids.push(lane);
    ids.sort_unstable();
    let mut gone = Vec::new();
    let mut batch = Batch::new();
    for id in ids {
        let (issue, _) = match lock_for_change(&mut tx, actor, id).await {
            Ok(found) => found,
            Err(Error::NotFound(_)) if id != lane => continue,
            Err(other) => return Err(other),
        };
        rights::may_update(actor, &facts(&issue))?;
        if !store_cards::remove(&mut tx, sprint.id, id).await? {
            continue;
        }
        let records: Vec<Record> = Record::boards(Some(&place(&b, &sprint)), None)
            .into_iter()
            .collect();
        let who = author(actor, origin, false, issue.project_id)?;
        finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
        batch.issue_on_board(id, Change::Updated, board_id);
        gone.push(id);
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(gone)
}
