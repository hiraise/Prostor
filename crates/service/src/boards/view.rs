//! The board's read path (BRD-068, BRD-057, BRD-099; D-044): one sprint's cards by
//! column, each cell capped at [`CELL_CAP`], the counts over all cards, and the issues in the
//! short form of lists. A query board's cards are the issues matching its query (BRD-028); the
//! header filter narrows any board and its sort clause orders each cell (BRD-081, BRD-070).
//! A board with lanes shows them in pages of [`lanes::PAGE`] (BRD-057), each cell capped.

use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::boards::columns::{Wip, WipState};
use taskmaster_domain::boards::lanes::{self as lanes, LaneKey, LaneMode};
use taskmaster_domain::boards::{Mode, sprints};
use taskmaster_domain::search::{Filter, SortSpec};
use taskmaster_domain::validation::Problem;
use taskmaster_domain::{BoardId, ColumnId, IssueId, ProjectId, SprintId, ValueId};
use taskmaster_query::Problem as QueryProblem;
use taskmaster_store::boards::view::{self as store_view, After, CardRow, Scope};
use taskmaster_store::boards::{self as store, Sprint};
use taskmaster_store::{PgPool, issues as store_issues, settings};
use time::{Date, OffsetDateTime};

use super::lanes::LaneSetup;
use super::{BoardRow, load};
use crate::issues::{Described, describe};
use crate::search::{QueryBoards, resolve_within, visibility_over};
use crate::seed::internal;
use crate::{Error, Result};

/// Most cards a cell shows before «more» (BRD-057).
pub const CELL_CAP: i64 = 100;

/// One card: where an issue stands on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub issue: IssueId,
    pub column: ColumnId,
    /// The lane key on a board with lanes.
    pub lane: Option<String>,
    pub rank: Option<String>,
}

/// A lane's header (BRD-053).
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub key: String,
    /// Cards in the lane, the hidden ones included.
    pub count: i64,
    /// Their estimation (BRD-062); `None` without an estimation field.
    pub estimate: Option<f64>,
    /// The parent issue of a parent lane.
    pub issue: Option<IssueId>,
}

/// A column's header: how many cards it holds and where it stands against its limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnCount {
    pub column: ColumnId,
    pub count: i64,
    /// The estimation of its cards (BRD-062).
    pub estimate: Option<f64>,
    pub wip: WipState,
}

/// A cell showing fewer cards than it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub column: ColumnId,
    pub lane: Option<String>,
    pub total: i64,
    /// Where the rest of the cell starts (`cell`'s cursor).
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BoardView {
    pub board: BoardRow,
    /// The shown sprint; `None` when a sprint board has no current one (BRD-099).
    pub sprint: Option<Sprint>,
    /// The current sprint, never set by hand (BRD-099).
    pub current: Option<SprintId>,
    pub columns: Vec<ColumnCount>,
    pub cards: Vec<Card>,
    pub cells: Vec<Cell>,
    /// The page of lanes shown, on a board with lanes.
    pub lanes: Vec<Lane>,
    /// Where the next page of lanes starts (`lanes`'s cursor).
    pub lanes_next: Option<String>,
    /// Lanes of the board in the shown sprint, every page.
    pub lanes_total: usize,
    pub issues: Described,
    /// Cards on the board in the shown sprint.
    pub total: i64,
    /// Their estimation (BRD-062), in the field's unit.
    pub estimate: Option<f64>,
    /// The live topic that hears about changes of these cards.
    pub live_topic: String,
    /// Today in the instance time zone: a sprint whose end passed is finished (BRD-100).
    pub today: Date,
    /// The sort ordering each cell, when a sort clause is active (BRD-070).
    pub sort: Option<Vec<SortSpec>>,
    /// Notes about the header filter.
    pub notes: Vec<QueryProblem>,
}

/// What narrows and orders the cards for one viewer: the board query (mode `query`) and the
/// header filter, both resolved within the board projects by the viewer's names and day
/// (BRD-030).
#[derive(Debug, Clone, Default)]
pub(super) struct Lens {
    pub filter: Option<Filter>,
    pub sort: Option<Vec<SortSpec>>,
    pub notes: Vec<QueryProblem>,
    pub query_mode: bool,
    /// The board's lanes (BRD-049).
    pub lanes: Option<LaneSetup>,
}

pub(super) fn is_query(board: &BoardRow) -> bool {
    Mode::parse(&board.full.board.mode) == Some(Mode::Query)
}

/// The board's lens for this viewer; `header` is the header filter's text.
pub(super) async fn lens(
    conn: &mut PgConnection,
    viewer: &Access,
    board: &BoardRow,
    header: Option<&str>,
    now: Option<OffsetDateTime>,
) -> Result<Lens> {
    let projects = &board.full.board.projects;
    let mut lens = Lens {
        query_mode: is_query(board),
        lanes: super::lanes::setup(
            &mut *conn,
            board,
            now.unwrap_or_else(OffsetDateTime::now_utc),
        )
        .await?,
        ..Lens::default()
    };
    let mut parts = Vec::new();
    if lens.query_mode {
        // A query that lost a name, or that this viewer cannot resolve, shows nothing rather
        // than more than it should.
        let text = board.query.as_deref().unwrap_or("");
        let resolved = if board.query_problems.is_empty() {
            match resolve_within(conn, viewer, text, projects, now, QueryBoards::Leave).await {
                Ok(w) => Some(w),
                Err(Error::InvalidQuery(_)) => None,
                Err(other) => return Err(other),
            }
        } else {
            None
        };
        match resolved {
            Some(w) => {
                lens.sort = w.sort;
                parts.push(w.filter);
            }
            None => parts.push(Filter::False),
        }
    }
    if let Some(text) = header.filter(|t| !t.trim().is_empty()) {
        let w = resolve_within(conn, viewer, text, projects, now, QueryBoards::Expand).await?;
        if w.sort.is_some() {
            lens.sort = w.sort;
        }
        lens.notes = w.notes;
        parts.push(w.filter);
    }
    if !parts.is_empty() {
        lens.filter = Some(Filter::and(parts));
    }
    Ok(lens)
}

pub(super) fn scope<'a>(
    board: &'a BoardRow,
    sprint: SprintId,
    field: taskmaster_domain::FieldId,
    values: &'a [ValueId],
    lens: &'a Lens,
) -> Scope<'a> {
    Scope {
        sprint,
        column_field: field,
        column_values: values,
        projects: &board.full.board.projects,
        query_mode: lens.query_mode,
        filter: lens.filter.as_ref(),
        sort: lens.sort.as_deref(),
        lanes: lens.lanes.as_ref().map(LaneSetup::scope),
        lane_page: None,
        estimate: super::card_setup::estimate(board),
    }
}

/// A page of one cell.
#[derive(Debug, Clone)]
pub struct CellPage {
    pub cards: Vec<Card>,
    pub issues: Described,
    pub next_cursor: Option<String>,
}

/// The instance time zone's name.
pub(super) async fn zone_name(conn: &mut PgConnection) -> Result<String> {
    Ok(settings::value(&mut *conn, "default_time_zone")
        .await?
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "UTC".to_owned()))
}

/// Today in the instance time zone (sprint dates are calendar days there).
pub(super) async fn today(conn: &mut PgConnection, now: OffsetDateTime) -> Result<Date> {
    let name = zone_name(&mut *conn).await?;
    let zone = jiff::tz::TimeZone::get(&name).unwrap_or(jiff::tz::TimeZone::UTC);
    let stamp = jiff::Timestamp::new(now.unix_timestamp(), 0).map_err(internal)?;
    let day = stamp.to_zoned(zone).date();
    let month =
        time::Month::try_from(u8::try_from(day.month()).map_err(internal)?).map_err(internal)?;
    Date::from_calendar_date(
        i32::from(day.year()),
        month,
        u8::try_from(day.day()).map_err(internal)?,
    )
    .map_err(internal)
}

/// The sprint asked for, or the one the board shows by default.
pub(super) fn pick(
    board: &BoardRow,
    asked: Option<SprintId>,
    today: Date,
) -> Result<(Option<Sprint>, Option<SprintId>)> {
    let all = &board.full.sprints;
    let dates: Vec<_> = all.iter().map(Sprint::dates).collect();
    let current = sprints::current(&dates, today);
    let id = match asked {
        Some(id) => Some(id),
        None => sprints::shown(board.full.board.sprints_on, &dates, today),
    };
    let sprint = match id {
        Some(id) => Some(
            all.iter()
                .find(|s| s.id == id && (!s.internal || !board.full.board.sprints_on))
                .cloned()
                .ok_or(Error::NotFound("no such sprint"))?,
        ),
        None => None,
    };
    Ok((sprint, current))
}

/// The projects of the board the viewer's visibility is computed over.
pub(super) fn visibility(
    viewer: &Access,
    board: &BoardRow,
) -> taskmaster_store::visibility::Visibility {
    let all: Vec<ProjectId> = board.projects.iter().map(|p| p.id).collect();
    let deleting: Vec<ProjectId> = board
        .projects
        .iter()
        .filter(|p| p.is_deleting())
        .map(|p| p.id)
        .collect();
    visibility_over(viewer, &all, &deleting)
}

/// Loads the issues of the cards in the order of the rows: one query per relation.
pub(super) async fn issues_of(
    conn: &mut PgConnection,
    viewer: &Access,
    rows: &[CardRow],
) -> Result<Described> {
    let ids: Vec<IssueId> = rows.iter().map(|r| r.issue_id).collect();
    describe_ids(conn, viewer, ids).await
}

/// Loads issues in the given order.
pub(super) async fn describe_ids(
    conn: &mut PgConnection,
    viewer: &Access,
    ids: Vec<IssueId>,
) -> Result<Described> {
    if ids.is_empty() {
        return Ok(Described::default());
    }
    let mut found = store_issues::get_many(&mut *conn, &ids).await?;
    found.sort_by_key(|i| ids.iter().position(|id| *id == i.id));
    let mut described = describe(conn, viewer, found).await?;
    described.files = file_counts(conn, viewer, &described).await?;
    Ok(described)
}

/// How many files of each card's issue the viewer sees (ISS-024): one statement per answer.
async fn file_counts(
    conn: &mut PgConnection,
    viewer: &Access,
    d: &Described,
) -> Result<Vec<(IssueId, i64)>> {
    use taskmaster_domain::access::visibility::Audience;
    use taskmaster_domain::attachments::{self as rules, Facts};
    let ids: Vec<IssueId> = d.issues.iter().map(|v| v.issue.id).collect();
    let files = taskmaster_store::attachments::audiences_of_issues(&mut *conn, &ids).await?;
    let mut counts: Vec<(IssueId, i64)> = Vec::new();
    for f in &files {
        let Some(v) = d.issues.iter().find(|v| v.issue.id == f.issue_id) else {
            continue;
        };
        let facts = Facts {
            project: v.issue.project_id,
            author: f.author_id,
            audience: Audience {
                users: &f.audience_users,
                groups: &f.audience_groups,
            },
        };
        if !rules::can_see(viewer, &facts) {
            continue;
        }
        match counts.iter_mut().find(|(issue, _)| *issue == f.issue_id) {
            Some((_, n)) => *n += 1,
            None => counts.push((f.issue_id, 1)),
        }
    }
    Ok(counts)
}

fn card(board: &BoardRow, row: &CardRow) -> Option<Card> {
    let column = board
        .full
        .columns
        .iter()
        .find(|c| c.value_id == row.column_value)?;
    Some(Card {
        issue: row.issue_id,
        column: column.id,
        lane: row.lane.clone(),
        rank: row.rank.clone(),
    })
}

/// The board as the viewer sees it (BRD-068): records the visit (BRD-001). `lanes` is the
/// cursor of a page of lanes (BRD-057).
#[allow(clippy::too_many_arguments)]
pub async fn view(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    sprint: Option<SprintId>,
    header: Option<&str>,
    lanes_cursor: Option<&str>,
    now: Option<OffsetDateTime>,
) -> Result<BoardView> {
    let mut conn = pool.acquire().await?;
    let board = load(&mut conn, viewer, id, true).await?;
    let today = today(&mut conn, now.unwrap_or_else(OffsetDateTime::now_utc)).await?;
    let (sprint, current) = pick(&board, sprint, today)?;
    let lens = lens(&mut conn, viewer, &board, header, now).await?;
    let vis = visibility(viewer, &board);
    let values: Vec<i64> = board.full.columns.iter().map(|c| c.value_id).collect();
    let offset = match lanes_cursor {
        Some(c) => c
            .parse::<usize>()
            .map_err(|_| Error::field("lanes", Problem::Invalid, "a cursor of another list"))?,
        None => 0,
    };
    let mut paged = Paged::default();
    let rows = match (&sprint, board.full.board.column_field_id) {
        (Some(s), Some(field)) => {
            let mut sc = scope(&board, s.id, field, &values, &lens);
            if let Some(setup) = &lens.lanes {
                paged = page_lanes(&mut conn, &board, s.id, setup, &sc, &vis, offset).await?;
                sc.lane_page = Some(&paged.page);
            }
            store_view::cards(&mut conn, &sc, &vis, CELL_CAP).await?
        }
        _ => Vec::new(),
    };
    let sorted = lens.sort.is_some();
    let mut ids: Vec<IssueId> = rows.iter().map(|r| r.issue_id).collect();
    ids.extend(paged.lanes.iter().filter_map(|l| l.issue));
    let issues = describe_ids(&mut conn, viewer, ids).await?;
    // A card the describe step left out (read right lost meanwhile) is not shown.
    let shown: Vec<&CardRow> = rows
        .iter()
        .filter(|r| issues.issues.iter().any(|v| v.issue.id == r.issue_id))
        .collect();
    let cards: Vec<Card> = shown.iter().filter_map(|r| card(&board, r)).collect();
    let mut columns = Vec::with_capacity(board.full.columns.len());
    let mut cells = Vec::new();
    let mut total = 0;
    let estimating = super::card_setup::estimate(&board).is_some();
    let mut estimate = 0.0;
    let lanes_on = lens.lanes.is_some();
    for column in &board.full.columns {
        // Cell sizes: from the counts of every lane, or from the rows of a board without lanes.
        let sizes: Vec<(Option<String>, i64, Option<f64>)> = if lanes_on {
            paged
                .counts
                .iter()
                .filter(|c| c.column_value == column.value_id)
                .map(|c| (Some(c.lane.clone()), c.count, c.estimate))
                .collect()
        } else {
            rows.iter()
                .find(|r| r.column_value == column.value_id)
                .map(|r| vec![(None, r.total, r.estimate)])
                .unwrap_or_default()
        };
        let count: i64 = sizes.iter().map(|(_, n, _)| n).sum();
        let column_estimate = estimating.then(|| sizes.iter().filter_map(|s| s.2).sum::<f64>());
        estimate += column_estimate.unwrap_or(0.0);
        for (lane, size, _) in &sizes {
            if lanes_on && !paged.page.iter().any(|k| Some(k) == lane.as_ref()) {
                continue;
            }
            let in_cell = |r: &&CardRow| r.column_value == column.value_id && &r.lane == lane;
            let showing = cards
                .iter()
                .filter(|c| c.column == column.id && &c.lane == lane)
                .count();
            if usize::try_from(*size).unwrap_or(usize::MAX) > showing {
                // Rows come by cell and the cell order, so the cell's last row ends the page.
                cells.push(Cell {
                    column: column.id,
                    lane: lane.clone(),
                    total: *size,
                    next_cursor: if sorted {
                        Some(write_offset(CELL_CAP))
                    } else {
                        rows.iter().rfind(in_cell).map(write_cursor)
                    },
                });
            }
        }
        total += count;
        columns.push(ColumnCount {
            column: column.id,
            count,
            estimate: column_estimate,
            wip: Wip {
                min: column.wip_min,
                max: column.wip_max,
            }
            .state(count),
        });
    }
    store::profiles::visit(&mut *conn, viewer.user(), id).await?;
    let live_topic = taskmaster_domain::live::Topic::Board(id).to_string();
    Ok(BoardView {
        board,
        sprint,
        current,
        columns,
        cards,
        cells,
        lanes: paged.lanes,
        lanes_next: paged.next,
        lanes_total: paged.total,
        issues,
        total,
        estimate: estimating.then_some(estimate),
        live_topic,
        today,
        sort: lens.sort,
        notes: lens.notes,
    })
}

/// A page of lanes with the cell sizes over every lane.
#[derive(Debug, Default)]
struct Paged {
    counts: Vec<store_view::CellCount>,
    /// The keys of the page.
    page: Vec<String>,
    lanes: Vec<Lane>,
    next: Option<String>,
    total: usize,
}

/// Every lane of the sprint in order, and the page after `offset` (BRD-052, BRD-053, BRD-057):
/// value lanes as set; parent lanes by lane rank, then in the order of their first card.
#[allow(clippy::too_many_arguments)]
async fn page_lanes(
    conn: &mut PgConnection,
    board: &BoardRow,
    sprint: SprintId,
    setup: &LaneSetup,
    sc: &Scope<'_>,
    vis: &taskmaster_store::visibility::Visibility,
    offset: usize,
) -> Result<Paged> {
    let counts = store_view::cell_counts(&mut *conn, sc, vis).await?;
    let keys: Vec<LaneKey> = match setup.mode {
        LaneMode::Parents => {
            let ranked =
                store_view::lane_rows(&mut *conn, sprint, &board.full.board.projects, vis).await?;
            let mut keys: Vec<LaneKey> = ranked.iter().map(|(id, _)| LaneKey::Issue(*id)).collect();
            let mut found: Vec<(Option<&str>, IssueId)> = Vec::new();
            for c in &counts {
                let Some(LaneKey::Issue(parent)) = LaneKey::parse(&c.lane) else {
                    continue;
                };
                match found.iter_mut().find(|(_, p)| *p == parent) {
                    Some(entry) => {
                        entry.0 = match (entry.0, c.first_rank.as_deref()) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (a, b) => a.or(b),
                        };
                    }
                    None => found.push((c.first_rank.as_deref(), parent)),
                }
            }
            found.sort_by(|a, b| match (a.0, b.0) {
                (Some(x), Some(y)) => x.cmp(y).then(a.1.cmp(&b.1)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.1.cmp(&b.1),
            });
            for (_, parent) in found {
                if !keys.contains(&LaneKey::Issue(parent)) {
                    keys.push(LaneKey::Issue(parent));
                }
            }
            lanes::ordered(keys, setup.other)
        }
        _ => setup.value_lanes(),
    };
    let estimating = sc.estimate.is_some();
    let total = keys.len();
    let end = offset.saturating_add(lanes::PAGE).min(total);
    let shown = keys.get(offset.min(total)..end).unwrap_or_default();
    let lanes = shown
        .iter()
        .map(|k| {
            let key = k.to_string();
            Lane {
                count: counts
                    .iter()
                    .filter(|c| c.lane == key)
                    .map(|c| c.count)
                    .sum(),
                estimate: estimating.then(|| {
                    counts
                        .iter()
                        .filter(|c| c.lane == key)
                        .filter_map(|c| c.estimate)
                        .sum()
                }),
                issue: match k {
                    LaneKey::Issue(id) => Some(*id),
                    _ => None,
                },
                key,
            }
        })
        .collect::<Vec<_>>();
    Ok(Paged {
        page: lanes.iter().map(|l| l.key.clone()).collect(),
        lanes,
        next: (end < total).then(|| end.to_string()),
        total,
        counts,
    })
}

fn write_cursor(row: &CardRow) -> String {
    use base64::Engine as _;
    let text = serde_json::json!([row.rank, row.issue_id]).to_string();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text)
}

/// A sorted cell pages by position: its keys are not a cursor here.
fn write_offset(offset: i64) -> String {
    use base64::Engine as _;
    let text = serde_json::json!({ "offset": offset }).to_string();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text)
}

fn read_cursor(cursor: &str, sorted: bool) -> Result<After> {
    use base64::Engine as _;
    #[derive(serde::Deserialize)]
    struct Offset {
        offset: i64,
    }
    let bad = || Error::field("cursor", Problem::Invalid, "a cursor of another list");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| bad())?;
    if sorted {
        let o: Offset = serde_json::from_slice(&bytes).map_err(|_| bad())?;
        return if o.offset >= 0 {
            Ok(After::Offset(o.offset))
        } else {
            Err(bad())
        };
    }
    let (rank, issue_id): (Option<String>, IssueId) =
        serde_json::from_slice(&bytes).map_err(|_| bad())?;
    Ok(After::Card { rank, issue_id })
}

/// The cards of one cell after a cursor (a truncated cell's rest, BRD-057).
#[allow(clippy::too_many_arguments)]
pub async fn cell(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    sprint: Option<SprintId>,
    column: ColumnId,
    lane: Option<&str>,
    cursor: Option<&str>,
    header: Option<&str>,
    now: Option<OffsetDateTime>,
) -> Result<CellPage> {
    let mut conn = pool.acquire().await?;
    let board = load(&mut conn, viewer, id, true).await?;
    let today = today(&mut conn, now.unwrap_or_else(OffsetDateTime::now_utc)).await?;
    let (sprint, _) = pick(&board, sprint, today)?;
    let lens = lens(&mut conn, viewer, &board, header, now).await?;
    let sorted = lens.sort.is_some();
    let after = cursor.map(|c| read_cursor(c, sorted)).transpose()?;
    let value = board
        .full
        .columns
        .iter()
        .find(|c| c.id == column)
        .map(|c| c.value_id)
        .ok_or(Error::NotFound("no such column"))?;
    let (Some(sprint), Some(field)) = (sprint, board.full.board.column_field_id) else {
        return Ok(CellPage {
            cards: Vec::new(),
            issues: Described::default(),
            next_cursor: None,
        });
    };
    let vis = visibility(viewer, &board);
    let values = [value];
    let page: Vec<String> = match (&lens.lanes, lane) {
        (Some(_), Some(lane)) => vec![lane.to_owned()],
        (Some(_), None) => {
            return Err(Error::field(
                "lane",
                Problem::Required,
                "a cell of a board with lanes is in a lane",
            ));
        }
        (None, _) => Vec::new(),
    };
    let mut sc = scope(&board, sprint.id, field, &values, &lens);
    if lens.lanes.is_some() {
        sc.lane_page = Some(&page);
    }
    let mut rows =
        store_view::cell(&mut conn, &sc, value, &vis, after.as_ref(), CELL_CAP + 1).await?;
    let next_cursor = if rows.len() > CELL_CAP as usize {
        rows.truncate(CELL_CAP as usize);
        match after {
            Some(After::Offset(n)) => Some(write_offset(n + CELL_CAP)),
            _ if sorted => Some(write_offset(CELL_CAP)),
            _ => rows.last().map(write_cursor),
        }
    } else {
        None
    };
    let issues = issues_of(&mut conn, viewer, &rows).await?;
    let cards = rows
        .iter()
        .filter(|r| issues.issues.iter().any(|v| v.issue.id == r.issue_id))
        .filter_map(|r| card(&board, r))
        .collect();
    Ok(CellPage {
        cards,
        issues,
        next_cursor,
    })
}

/// Where these issues stand on the board in the sprint now, in the order asked: `None` for an
/// issue that is not a card the viewer sees (BRD-114).
pub(super) async fn cards_of(
    conn: &mut PgConnection,
    viewer: &Access,
    board: &BoardRow,
    sprint: Option<&Sprint>,
    lens: &Lens,
    ids: &[IssueId],
) -> Result<(Vec<Option<Card>>, Described)> {
    let rows = match (sprint, board.full.board.column_field_id) {
        (Some(s), Some(field)) => {
            let values: Vec<i64> = board.full.columns.iter().map(|c| c.value_id).collect();
            store_view::of_issues(
                &mut *conn,
                &scope(board, s.id, field, &values, lens),
                &visibility(viewer, board),
                ids,
            )
            .await?
        }
        _ => Vec::new(),
    };
    let issues = issues_of(&mut *conn, viewer, &rows).await?;
    let cards = ids
        .iter()
        .map(|id| {
            rows.iter()
                .find(|r| r.issue_id == *id)
                .filter(|r| issues.issues.iter().any(|v| v.issue.id == r.issue_id))
                .and_then(|r| card(board, r))
        })
        .collect();
    Ok((cards, issues))
}

/// Most issues one batch asks about.
pub const BATCH_MAX: usize = 100;

/// Where these issues are now on the shown sprint (the live board refetches the cards an event
/// names, BRD-114).
pub async fn batch(
    pool: &PgPool,
    viewer: &Access,
    id: BoardId,
    sprint: Option<SprintId>,
    ids: &[IssueId],
    header: Option<&str>,
    now: Option<OffsetDateTime>,
) -> Result<(Vec<Option<Card>>, Described)> {
    if ids.len() > BATCH_MAX {
        return Err(Error::field(
            "issues",
            Problem::Invalid,
            format!("at most {BATCH_MAX} issues"),
        ));
    }
    let mut conn = pool.acquire().await?;
    let board = load(&mut conn, viewer, id, true).await?;
    let today = today(&mut conn, now.unwrap_or_else(OffsetDateTime::now_utc)).await?;
    let (sprint, _) = pick(&board, sprint, today)?;
    let lens = lens(&mut conn, viewer, &board, header, now).await?;
    cards_of(&mut conn, viewer, &board, sprint.as_ref(), &lens, ids).await
}
