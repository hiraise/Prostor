//! Cards (BRD-025, BRD-068, BRD-069, BRD-074; D-044): adding an issue to a sprint of a
//! board, moving its card to another column or place, and taking it off. A move to another column
//! sets the column field through the same function as a field change (rights, required values,
//! history, resolution), all in one transaction with the rank and the live events. On a query
//! board an issue joins by the field changes that make it match the board query (BRD-028).

use prostor_domain::access::Access;
use prostor_domain::boards::query_mode::{self, Facts, Failure, Step};
use prostor_domain::issues::history::{BoardPlace, ChangeKind, Origin, Record};
use prostor_domain::issues::rights;
use prostor_domain::issues::values::Input;
use prostor_domain::live::Change;
use prostor_domain::rank;
use prostor_domain::search::Filter;
use prostor_domain::validation::Problem;
use prostor_domain::{BoardId, ColumnId, IssueId, SprintId, TagId};
use prostor_store::PgPool;
use prostor_store::boards::cards as store_cards;
use prostor_store::boards::view::{self as store_view, After};
use prostor_store::boards::{self as store, Sprint};
use prostor_store::issues::Issue;
use sqlx::PgConnection;

use super::view::{CELL_CAP, Card, Lens, cards_of, is_query, lens, scope, visibility};
use super::{BoardRow, load};
use crate::issues::{
    Described, FieldChange, FieldWritten, author, facts, finish_change, lock_for_change,
    set_field_locked,
};
use crate::live::Batch;
use crate::rules::{Ctx, Event};
use crate::seed::internal;
use crate::{Error, Result};

/// An issue to add to a sprint: where its card goes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewCard {
    pub issue: IssueId,
    /// Sets the column field to this column's value.
    pub column: Option<ColumnId>,
    pub after: Option<IssueId>,
    pub before: Option<IssueId>,
    /// A query board: answer the changes that make the issue match, and change nothing.
    pub preview: bool,
    /// The lane it is dropped into: sets the lane's value or parent (BRD-050, BRD-051).
    pub lane: Option<String>,
    /// A parent joining a parent-lane board as a lane brings its subtasks as cards (BRD-096).
    pub subtasks: bool,
}

/// Most subtasks a parent lane brings along (BRD-096).
pub const SUBTASKS_MAX: usize = 100;

/// Where a card goes; `version` is the issue's version the person saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub column: ColumnId,
    pub after: Option<IssueId>,
    pub before: Option<IssueId>,
    /// The lane it goes to, on a board with lanes; `None` keeps its lane.
    pub lane: Option<String>,
    pub version: i64,
    /// The header filter the person sees: its sort clause forbids reordering (BRD-070).
    pub header: Option<String>,
}

/// The card after a change, with its issue; `card` is `None` when it left the viewer's board.
#[derive(Debug, Clone)]
pub struct Placed {
    pub card: Option<Card>,
    pub issues: Described,
    /// On a query board: the field changes that made the issue match (BRD-028).
    pub changes: Vec<Step>,
}

/// The sprint of the board that holds cards: any of a sprint board, the internal one otherwise.
pub(super) fn sprint_of(board: &BoardRow, id: SprintId) -> Result<&Sprint> {
    let sprints_on = board.full.board.sprints_on;
    board
        .full
        .sprints
        .iter()
        .find(|s| s.id == id && s.internal != sprints_on)
        .ok_or(Error::NotFound("no such sprint"))
}

/// How a board record names a place: the board, and the sprint on a sprint board.
pub(super) fn place<'a>(board: &'a store::Board, sprint: &'a Sprint) -> BoardPlace<'a> {
    BoardPlace {
        board: (board.id, &board.name),
        sprint: (!sprint.internal).then_some((sprint.id, sprint.name.as_str())),
    }
}

/// A rank between the given neighbours of the sprint (BRD-069). A neighbour must be a card of
/// the sprint; an unranked one counts as absent. When `before` is not above `after`, the rank
/// follows `after` alone. No neighbour: after the last card.
async fn rank_between(
    conn: &mut PgConnection,
    sprint: SprintId,
    moving: IssueId,
    after: Option<IssueId>,
    before: Option<IssueId>,
) -> Result<String> {
    let asked: Vec<IssueId> = after.into_iter().chain(before).collect();
    let found = store_cards::ranks(&mut *conn, sprint, &asked).await?;
    let rank_of = |path: &str, id: Option<IssueId>| -> Result<Option<String>> {
        let Some(id) = id else { return Ok(None) };
        if id == moving {
            return Err(Error::field(path, Problem::Invalid, "the card itself"));
        }
        found
            .iter()
            .find(|(issue, _)| *issue == id)
            .map(|(_, rank)| rank.clone())
            .ok_or_else(|| Error::field(path, Problem::Invalid, "not a card of this sprint"))
    };
    let low = rank_of("after", after)?;
    let high = rank_of("before", before)?;
    let (low, high) = match (low, high) {
        (Some(a), Some(b)) if a < b => (Some(a), Some(b)),
        (Some(a), _) => {
            let above = store_cards::rank_above(&mut *conn, sprint, &a).await?;
            (Some(a), above)
        }
        (None, Some(b)) => (
            store_cards::rank_below(&mut *conn, sprint, &b).await?,
            Some(b),
        ),
        (None, None) => (store_cards::last_rank(&mut *conn, sprint).await?, None),
    };
    rank::between(low.as_deref(), high.as_deref()).map_err(internal)
}

/// Sets the column field to the column's value; answers the history records.
async fn set_column(
    conn: &mut PgConnection,
    actor: &Access,
    board: &BoardRow,
    issue: &Issue,
    column: ColumnId,
) -> Result<FieldWritten> {
    let value = board
        .full
        .columns
        .iter()
        .find(|c| c.id == column)
        .map(|c| c.value_id)
        .ok_or_else(|| Error::field("column", Problem::Invalid, "no such column"))?;
    let field = board
        .full
        .board
        .column_field_id
        .ok_or_else(|| Error::field("column", Problem::Invalid, "no such column"))?;
    set_field_locked(
        conn,
        actor,
        issue,
        field,
        FieldChange::Set(Some(Input::Integer(value))),
    )
    .await
}

/// Hears a column write the way the field use case does (§3.5): the behaviors run before the
/// change's own records, so what they change on this issue joins them as one change.
async fn hear(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    written: &FieldWritten,
    rule_records: &mut Vec<Record>,
) -> Result<()> {
    if written.is_empty() {
        return Ok(());
    }
    rule_records.extend(
        crate::rules::on(
            conn,
            batch,
            ctx,
            Event::FieldChanged {
                issue,
                field: &written.field,
                old: written.old.as_ref(),
                new: written.new.as_ref(),
            },
        )
        .await?,
    );
    rule_records
        .extend(crate::rules::on_resolution(conn, batch, ctx, issue, &written.records).await?);
    Ok(())
}

/// Writes a change's joined rule records in the rule's name, and lets their own resolutions be
/// heard (§3.5) — a rule may have settled the issue further.
async fn settle(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    issue: &Issue,
    rule_records: &[Record],
) -> Result<()> {
    if rule_records.is_empty() {
        return Ok(());
    }
    crate::rules::finish(conn, batch, ctx, issue, rule_records).await?;
    if rule_records
        .iter()
        .any(|record| matches!(record.kind, ChangeKind::Resolved | ChangeKind::Reopened))
    {
        crate::rules::on_resolution(conn, batch, ctx, issue, rule_records).await?;
    }
    Ok(())
}

pub(super) async fn placed(
    pool: &PgPool,
    viewer: &Access,
    board: BoardId,
    sprint: SprintId,
    issue: IssueId,
) -> Result<Placed> {
    let mut conn = pool.acquire().await?;
    let row = load(&mut conn, viewer, board, true).await?;
    let sprint = sprint_of(&row, sprint)?.clone();
    let lens = lens(&mut conn, viewer, &row, None, None).await?;
    let (mut cards, issues) =
        cards_of(&mut conn, viewer, &row, Some(&sprint), &lens, &[issue]).await?;
    Ok(Placed {
        card: cards.pop().flatten(),
        issues,
        changes: Vec::new(),
    })
}

/// The changes that make the issue match the board query (BRD-028): the leaves are evaluated
/// over the issue in one statement, then walked.
async fn plan_for(
    conn: &mut PgConnection,
    actor: &Access,
    board: &BoardRow,
    issue: &Issue,
    filter: &Filter,
) -> Result<Vec<Step>> {
    let leaves = query_mode::leaves(filter);
    let vis = visibility(actor, board);
    let holds = prostor_store::search::holds_each(&mut *conn, issue.id, &leaves, &vis).await?;
    let fields = crate::issues::values::of_project(&mut *conn, issue.project_id).await?;
    let wanted: Vec<TagId> = leaves
        .iter()
        .filter_map(|l| match l {
            Filter::Tags(ids) => Some(ids.iter().copied()),
            _ => None,
        })
        .flatten()
        .collect();
    let usable: Vec<TagId> = prostor_store::tags::get_many(&mut *conn, &wanted)
        .await?
        .into_iter()
        .filter(|t| t.sharing().can_use(actor))
        .map(|t| t.id)
        .collect();
    let attached = |f| fields.get(f).is_some();
    let usable_tag = |t| usable.contains(&t);
    query_mode::plan(
        filter,
        &Facts {
            holds: &holds,
            attached: &attached,
            usable_tag: &usable_tag,
        },
    )
    .map_err(|f| Error::Refused(f.message().into()))
}

/// Applies the plan through the same functions as field and tag changes; answers the records and,
/// for the behaviors to hear (§3.5), the field writes it made — a query board's cell create is a
/// field change like any other.
async fn apply(
    conn: &mut PgConnection,
    actor: &Access,
    issue: &Issue,
    steps: &[Step],
    writes: &mut Vec<FieldWritten>,
) -> Result<Vec<Record>> {
    let fields = crate::issues::values::of_project(&mut *conn, issue.project_id).await?;
    let mut records = Vec::new();
    for step in steps {
        match *step {
            Step::Set { field, value } => {
                let multi = fields.get(field).is_some_and(|f| f.kind.multi);
                let change = if multi {
                    FieldChange::Add(vec![value])
                } else {
                    FieldChange::Set(Some(Input::Integer(value)))
                };
                let written = set_field_locked(conn, actor, issue, field, change).await?;
                records.extend(written.records.iter().cloned());
                writes.push(written);
            }
            Step::Clear { field } => {
                let written =
                    set_field_locked(conn, actor, issue, field, FieldChange::Set(None)).await?;
                records.extend(written.records.iter().cloned());
                writes.push(written);
            }
            Step::Tag(tag) => {
                if prostor_store::tags::add(conn, issue.id, tag).await? {
                    records.extend(Record::tags(&[], &[tag]));
                }
            }
        }
    }
    Ok(records)
}

/// Gives ranks to the unranked cards of a query board's cell up to `neighbour`, in the order
/// shown (bounded by the cell cap), so a card can be placed beside it (D-044).
async fn rank_up_to(
    conn: &mut PgConnection,
    actor: &Access,
    board: &BoardRow,
    sprint: &Sprint,
    lens: &Lens,
    neighbour: IssueId,
) -> Result<()> {
    let ranked = store_cards::ranks(&mut *conn, sprint.id, &[neighbour]).await?;
    if ranked.iter().any(|(_, r)| r.is_some()) {
        return Ok(());
    }
    let (mut found, _) =
        cards_of(&mut *conn, actor, board, Some(sprint), lens, &[neighbour]).await?;
    let (Some(card), Some(field)) = (found.pop().flatten(), board.full.board.column_field_id)
    else {
        return Ok(());
    };
    let Some(value) = board
        .full
        .columns
        .iter()
        .find(|c| c.id == card.column)
        .map(|c| c.value_id)
    else {
        return Ok(());
    };
    let values = [value];
    let rows = store_view::cell(
        &mut *conn,
        &scope(board, sprint.id, field, &values, lens),
        value,
        &visibility(actor, board),
        Some(&After::Card {
            rank: None,
            issue_id: 0,
        }),
        CELL_CAP,
    )
    .await?;
    let Some(upto) = rows.iter().position(|r| r.issue_id == neighbour) else {
        return Ok(());
    };
    let mut last = store_cards::last_rank(&mut *conn, sprint.id).await?;
    for row in &rows[..=upto] {
        let rank = rank::between(last.as_deref(), None).map_err(internal)?;
        store_cards::set_rank(&mut *conn, sprint.id, row.issue_id, &rank).await?;
        last = Some(rank);
    }
    Ok(())
}

/// Adds an issue to a sprint of the board (BRD-025): it needs the update right on the issue, an
/// issue of a board project (a draft too, ISS-007, but not on a query board). A sprint board holds an issue in
/// one sprint: adding it to another moves it there. An issue already in the sprint stays as it is.
pub async fn add(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    sprint_id: SprintId,
    card: NewCard,
) -> Result<crate::rules::Answer<Placed>> {
    let mut ctx = Ctx::new(actor);
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    let sprint = sprint_of(&board, sprint_id)?.clone();
    if card.preview && !is_query(&board) {
        return Err(Error::field(
            "preview",
            Problem::Invalid,
            "only a query board changes fields to add an issue",
        ));
    }
    let (issue, _) = lock_for_change(&mut tx, actor, card.issue).await?;
    rights::may_update(actor, &facts(&issue))?;
    // A draft may join (ISS-007): its card shows once it is published.
    if issue.is_draft() && is_query(&board) {
        return Err(Error::field(
            "issue",
            Problem::Invalid,
            "a draft joins a query board through its fields once it is published",
        ));
    }
    if !board.full.board.projects.contains(&issue.project_id) {
        return Err(Error::field(
            "issue",
            Problem::Invalid,
            "the issue's project is not on this board",
        ));
    }
    if is_query(&board) {
        return add_by_query(
            pool, tx, &mut ctx, actor, origin, board, sprint, issue, card,
        )
        .await;
    }
    let setup = super::lanes::setup(&mut tx, &board, time::OffsetDateTime::now_utc()).await?;
    if let Some(setup) = &setup
        && super::lanes::is_lane(&mut tx, setup, issue.id).await?
    {
        super::lanes::join_as_lane(&mut tx, actor, origin, &board, &sprint, &issue).await?;
        if card.subtasks {
            join_subtasks(&mut tx, &mut ctx, actor, origin, &board, &sprint, issue.id).await?;
        }
        tx.commit().await?;
        let done = placed(pool, actor, board_id, sprint_id, issue.id).await?;
        return Ok(crate::rules::Answer {
            done,
            messages: ctx.messages,
        });
    }
    let mut batch = Batch::new();
    join_card(
        &mut tx, &mut batch, &mut ctx, actor, origin, &board, &sprint, &issue, &card,
    )
    .await?;
    if let (Some(setup), Some(lane)) = (&setup, card.lane.as_deref()) {
        let moved = super::lanes::set_lane(
            &mut tx, &mut batch, actor, origin, setup, &issue, None, lane,
        )
        .await?;
        // The lane's field writes are field changes the behaviors hear, like the column's.
        let mut rule_records = Vec::new();
        for written in &moved.writes {
            hear(
                &mut tx,
                &mut batch,
                &mut ctx,
                &issue,
                written,
                &mut rule_records,
            )
            .await?;
        }
        if !moved.records.is_empty() {
            let who = author(actor, origin, false, issue.project_id)?;
            finish_change(&mut tx, &mut batch, &issue, who, &moved.records, &[]).await?;
        }
        settle(&mut tx, &mut batch, &mut ctx, &issue, &rule_records).await?;
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = placed(pool, actor, board_id, sprint_id, issue.id).await?;
    Ok(crate::rules::Answer {
        done,
        messages: ctx.messages,
    })
}

/// Makes a locked issue a card of the sprint (leaving the board's other sprints) at the place
/// asked, setting the column; an issue in the sprint already stays as it is.
#[allow(clippy::too_many_arguments)]
async fn join_card(
    tx: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    actor: &Access,
    origin: Origin,
    board: &BoardRow,
    sprint: &Sprint,
    issue: &Issue,
    card: &NewCard,
) -> Result<()> {
    let board_id = board.full.board.id;
    let places = store_cards::places(&mut *tx, board_id, issue.id).await?;
    if places.iter().any(|p| p.sprint == sprint.id) {
        return Ok(());
    }
    let mut left = None;
    for p in &places {
        store_cards::remove(&mut *tx, p.sprint, issue.id).await?;
        left = left.or_else(|| board.full.sprints.iter().find(|s| s.id == p.sprint));
    }
    let mut rule_records = Vec::new();
    let mut records = match card.column {
        Some(column) => {
            let written = set_column(&mut *tx, actor, board, issue, column).await?;
            hear(&mut *tx, batch, ctx, issue, &written, &mut rule_records).await?;
            written.records
        }
        None => Vec::new(),
    };
    let rank = rank_between(&mut *tx, sprint.id, issue.id, card.after, card.before).await?;
    store_cards::insert(&mut *tx, sprint.id, issue.id, &rank).await?;
    let b = &board.full.board;
    records.extend(Record::boards(
        left.map(|s| place(b, s)).as_ref(),
        Some(&place(b, sprint)),
    ));
    let who = author(actor, origin, false, issue.project_id)?;
    finish_change(&mut *tx, batch, issue, who, &records, &[]).await?;
    settle(&mut *tx, batch, ctx, issue, &rule_records).await?;
    batch.issue_on_board(issue.id, Change::Updated, board_id);
    Ok(())
}

/// The subtasks of a parent lane join the sprint as cards (BRD-096): those of board projects the
/// actor may update, at most [`SUBTASKS_MAX`]; others are left where they are.
async fn join_subtasks(
    tx: &mut PgConnection,
    ctx: &mut Ctx<'_>,
    actor: &Access,
    origin: Origin,
    board: &BoardRow,
    sprint: &Sprint,
    parent: IssueId,
) -> Result<()> {
    let mut ids: Vec<IssueId> = prostor_store::links::children_of(&mut *tx, &[parent])
        .await?
        .into_iter()
        .map(|(_, child)| child)
        .filter(|c| {
            c.deleted_at.is_none()
                && c.number.is_some()
                && board.full.board.projects.contains(&c.project_id)
        })
        .map(|c| c.id)
        .collect();
    ids.sort_unstable();
    ids.truncate(SUBTASKS_MAX);
    let mut batch = Batch::new();
    for id in ids {
        let Ok((issue, _)) = lock_for_change(&mut *tx, actor, id).await else {
            continue;
        };
        if rights::may_update(actor, &facts(&issue)).is_err() {
            continue;
        }
        let card = NewCard {
            issue: id,
            ..NewCard::default()
        };
        join_card(
            &mut *tx, &mut batch, ctx, actor, origin, board, sprint, &issue, &card,
        )
        .await?;
    }
    batch.publish(&mut *tx).await?;
    Ok(())
}

/// Adds an issue to a query board (BRD-028, owner decision 2): the field changes that make it
/// match are applied in a savepoint and the whole query checked again; a preview answers them
/// and rolls back. Nothing to change: the issue is a card already.
#[allow(clippy::too_many_arguments)]
async fn add_by_query(
    pool: &PgPool,
    mut tx: prostor_store::Transaction<'_, prostor_store::Postgres>,
    ctx: &mut Ctx<'_>,
    actor: &Access,
    origin: Origin,
    board: BoardRow,
    sprint: Sprint,
    issue: Issue,
    card: NewCard,
) -> Result<crate::rules::Answer<Placed>> {
    let lens = lens(&mut tx, actor, &board, None, None).await?;
    let filter = lens.filter.clone().unwrap_or(Filter::True);
    let steps = plan_for(&mut tx, actor, &board, &issue, &filter).await?;
    let mut sp = sqlx::Connection::begin(&mut *tx).await?;
    let mut plan_writes = Vec::new();
    let mut records = apply(&mut sp, actor, &issue, &steps, &mut plan_writes).await?;
    let mut column_written = None;
    if let Some(column) = card.column {
        let written = set_column(&mut sp, actor, &board, &issue, column).await?;
        records.extend(written.records.iter().cloned());
        column_written = Some(written);
    }
    let (mut now, described) =
        cards_of(&mut sp, actor, &board, Some(&sprint), &lens, &[issue.id]).await?;
    let Some(shown) = now.pop().flatten() else {
        return Err(Error::Refused(Failure::Unsettable.message().into()));
    };
    if card.preview {
        sp.rollback().await?;
        tx.rollback().await?;
        return Ok(crate::rules::Answer {
            done: Placed {
                card: Some(shown),
                issues: described,
                changes: steps,
            },
            messages: Vec::new(),
        });
    }
    // The behaviors hear the plan's and the column's writes inside the savepoint: a refusal of
    // theirs rolls back with the change (§3.5).
    let mut rule_records = Vec::new();
    let mut batch = Batch::new();
    for written in &plan_writes {
        hear(&mut sp, &mut batch, ctx, &issue, written, &mut rule_records).await?;
    }
    if let Some(written) = column_written.as_ref()
        && !written.is_empty()
    {
        hear(&mut sp, &mut batch, ctx, &issue, written, &mut rule_records).await?;
    }
    sp.commit().await?;
    if card.after.is_some() || card.before.is_some() {
        for n in card.after.into_iter().chain(card.before) {
            rank_up_to(&mut tx, actor, &board, &sprint, &lens, n).await?;
        }
        let rank = rank_between(&mut tx, sprint.id, issue.id, card.after, card.before).await?;
        store_cards::set_rank(&mut tx, sprint.id, issue.id, &rank).await?;
    }
    if !records.is_empty() {
        let who = author(actor, origin, false, issue.project_id)?;
        finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
    }
    settle(&mut tx, &mut batch, ctx, &issue, &rule_records).await?;
    batch.issue_on_board(issue.id, Change::Updated, board.full.board.id);
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let mut done = placed(pool, actor, board.full.board.id, sprint.id, issue.id).await?;
    done.changes = steps;
    Ok(crate::rules::Answer {
        done,
        messages: std::mem::take(&mut ctx.messages),
    })
}

/// Moves a card (BRD-068, BRD-069): to another column sets the column field's value; `after` and
/// `before` give its place in the shared order (neither: it keeps its rank). Refused without the
/// update right on the issue; a stale `version` is a conflict.
pub async fn move_card(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    sprint_id: SprintId,
    issue_id: IssueId,
    to: Move,
) -> Result<crate::rules::Answer<Placed>> {
    let mut ctx = Ctx::new(actor);
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    let sprint = sprint_of(&board, sprint_id)?.clone();
    let lens = lens(&mut tx, actor, &board, to.header.as_deref(), None).await?;
    if lens.sort.is_some() && (to.after.is_some() || to.before.is_some()) {
        return Err(Error::field(
            "after",
            Problem::Invalid,
            "the cards are sorted by a query: their order does not change by hand",
        ));
    }
    let (issue, _) = lock_for_change(&mut tx, actor, issue_id).await?;
    rights::may_update(actor, &facts(&issue))?;
    if issue.version != to.version {
        return Err(Error::Conflict("the issue was changed by someone else"));
    }
    let current = match store_cards::ranks(&mut *tx, sprint_id, &[issue_id])
        .await?
        .pop()
    {
        Some((_, rank)) => rank,
        // A query board's card has a row once it is ranked.
        None if lens.query_mode
            && cards_of(&mut tx, actor, &board, Some(&sprint), &lens, &[issue_id])
                .await?
                .0
                .pop()
                .flatten()
                .is_some() =>
        {
            None
        }
        None => return Err(Error::NotFound("the issue is not on this board")),
    };
    if lens.query_mode {
        for n in to.after.into_iter().chain(to.before) {
            rank_up_to(&mut tx, actor, &board, &sprint, &lens, n).await?;
        }
    }
    let mut batch = Batch::new();
    let mut rule_records = Vec::new();
    let written = set_column(&mut tx, actor, &board, &issue, to.column).await?;
    hear(
        &mut tx,
        &mut batch,
        &mut ctx,
        &issue,
        &written,
        &mut rule_records,
    )
    .await?;
    let mut records = written.records;
    if let (Some(setup), Some(lane)) = (&lens.lanes, to.lane.as_deref()) {
        let (mut found, _) =
            cards_of(&mut tx, actor, &board, Some(&sprint), &lens, &[issue_id]).await?;
        let from = found.pop().flatten().and_then(|c| c.lane);
        let moved = super::lanes::set_lane(
            &mut tx,
            &mut batch,
            actor,
            origin,
            setup,
            &issue,
            from.as_deref(),
            lane,
        )
        .await?;
        // The lane's field writes are field changes the behaviors hear, like the column's.
        for written in &moved.writes {
            hear(
                &mut tx,
                &mut batch,
                &mut ctx,
                &issue,
                written,
                &mut rule_records,
            )
            .await?;
        }
        records.extend(moved.records);
    }
    let rank = if to.after.is_some() || to.before.is_some() || current.is_none() {
        Some(rank_between(&mut tx, sprint_id, issue_id, to.after, to.before).await?)
    } else {
        None
    };
    let reordered = rank.is_some() && rank != current;
    if let Some(rank) = rank.as_deref().filter(|_| reordered) {
        store_cards::set_rank(&mut tx, sprint_id, issue_id, rank).await?;
    }
    let relinked = !batch.is_empty();
    if !records.is_empty() || relinked {
        if !records.is_empty() {
            let who = author(actor, origin, false, issue.project_id)?;
            finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
        }
        settle(&mut tx, &mut batch, &mut ctx, &issue, &rule_records).await?;
        batch.issue_on_board(issue_id, Change::Updated, board_id);
    } else if reordered {
        let after = match to.after {
            Some(after) => Some(after),
            None => store_cards::previous(&mut *tx, sprint_id, issue_id).await?,
        };
        batch.card_order(issue_id, board_id, sprint_id, after);
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    let done = placed(pool, actor, board_id, sprint_id, issue_id).await?;
    Ok(crate::rules::Answer {
        done,
        messages: ctx.messages,
    })
}

/// Takes a card off the sprint (BRD-074): the update right on the issue; not on a query board.
pub async fn remove(
    pool: &PgPool,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    sprint_id: SprintId,
    issue_id: IssueId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let board = load(&mut tx, actor, board_id, true).await?;
    let sprint = sprint_of(&board, sprint_id)?.clone();
    if is_query(&board) {
        return Err(Error::Refused(
            "issues leave this board through its query; change their fields instead".into(),
        ));
    }
    let (issue, _) = lock_for_change(&mut tx, actor, issue_id).await?;
    rights::may_update(actor, &facts(&issue))?;
    if !store_cards::remove(&mut tx, sprint_id, issue_id).await? {
        return Err(Error::NotFound("the issue is not on this board"));
    }
    let records: Vec<Record> = Record::boards(Some(&place(&board.full.board, &sprint)), None)
        .into_iter()
        .collect();
    let who = author(actor, origin, false, issue.project_id)?;
    let mut batch = Batch::new();
    finish_change(&mut tx, &mut batch, &issue, who, &records, &[]).await?;
    batch.issue_on_board(issue_id, Change::Updated, board_id);
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// What a board membership command could not do (step 2.1 §4.5).
pub(crate) enum Membership {
    Done,
    /// The board has no sprint the command could mean, or takes no removals (a query board).
    NoSprints,
}

/// Puts a locked issue on a board, or takes it off, inside the caller's transaction (step 2.1
/// §4.3, the `board` command): the same checks and writes as [`add`] and [`remove`] for a card
/// with no column and no place — manual adding, BRD-025 — plus the field rewrite a query board
/// needs (BRD-028). `None` as the sprint means the current one, else the auto-add sprint
/// (BRD-034); a removal without a sprint leaves every sprint of the board.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn membership(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    actor: &Access,
    origin: Origin,
    board_id: BoardId,
    sprint: Option<SprintId>,
    issue: &Issue,
    on: bool,
) -> Result<Membership> {
    let board = load(conn, actor, board_id, true).await?;
    if on {
        let sprint = match sprint {
            Some(id) => sprint_of(&board, id)?.clone(),
            None => {
                if is_query(&board) || !board.full.board.sprints_on {
                    // A sprintless or query board keeps one hidden sprint that holds its issues.
                    board
                        .full
                        .sprints
                        .iter()
                        .find(|s| s.internal)
                        .cloned()
                        .ok_or(Error::NotFound("no such sprint"))?
                } else {
                    let today = super::view::today(conn, time::OffsetDateTime::now_utc()).await?;
                    let dates: Vec<_> = board.full.sprints.iter().map(|s| s.dates()).collect();
                    let chosen = prostor_domain::boards::sprints::current(&dates, today)
                        .or(board.full.board.auto_sprint_id)
                        .and_then(|id| board.full.sprints.iter().find(|s| s.id == id).cloned());
                    let Some(chosen) = chosen else {
                        return Ok(Membership::NoSprints);
                    };
                    chosen
                }
            }
        };
        rights::may_update(actor, &crate::issues::facts(issue))?;
        if issue.is_draft() && is_query(&board) {
            return Err(Error::field(
                "issue",
                Problem::Invalid,
                "a draft joins a query board through its fields once it is published",
            ));
        }
        if !board.full.board.projects.contains(&issue.project_id) {
            return Err(Error::field(
                "issue",
                Problem::Invalid,
                "the issue's project is not on this board",
            ));
        }
        if is_query(&board) {
            return add_by_query_conn(conn, batch, ctx, actor, origin, &board, &sprint, issue)
                .await;
        }
        let setup = super::lanes::setup(conn, &board, time::OffsetDateTime::now_utc()).await?;
        if let Some(setup) = &setup
            && super::lanes::is_lane(conn, setup, issue.id).await?
        {
            super::lanes::join_as_lane(conn, actor, origin, &board, &sprint, issue).await?;
            return Ok(Membership::Done);
        }
        join_card(
            conn,
            batch,
            ctx,
            actor,
            origin,
            &board,
            &sprint,
            issue,
            &NewCard {
                issue: issue.id,
                ..NewCard::default()
            },
        )
        .await?;
        return Ok(Membership::Done);
    }
    // Removal: a query board's issues come and go with its query (BRD-028).
    if is_query(&board) {
        return Ok(Membership::NoSprints);
    }
    rights::may_update(actor, &crate::issues::facts(issue))?;
    let mut records = Vec::new();
    if let Some(id) = sprint {
        let left = sprint_of(&board, id)?.clone();
        if !store_cards::remove(conn, id, issue.id).await? {
            return Err(Error::NotFound("the issue is not on this board"));
        }
        records.extend(Record::boards(Some(&place(&board.full.board, &left)), None));
    } else {
        let places = store_cards::places(&mut *conn, board_id, issue.id).await?;
        let mut left_any = false;
        for at in &places {
            let Some(left) = board.full.sprints.iter().find(|s| s.id == at.sprint) else {
                continue;
            };
            if store_cards::remove(conn, at.sprint, issue.id).await? {
                left_any = true;
                records.extend(Record::boards(Some(&place(&board.full.board, left)), None));
            }
        }
        if !left_any {
            return Err(Error::NotFound("the issue is not on this board"));
        }
    }
    finish_change(
        conn,
        batch,
        issue,
        author(actor, origin, false, issue.project_id)?,
        &records,
        &[],
    )
    .await?;
    batch.issue_on_board(issue.id, Change::Updated, board_id);
    Ok(Membership::Done)
}

/// The query-board half of [`membership`]: the field changes that make the issue match the
/// board query (BRD-028), in a savepoint, with the behaviors hearing them (§3.5).
#[allow(clippy::too_many_arguments)]
async fn add_by_query_conn(
    conn: &mut PgConnection,
    batch: &mut Batch,
    ctx: &mut Ctx<'_>,
    actor: &Access,
    origin: Origin,
    board: &BoardRow,
    sprint: &Sprint,
    issue: &Issue,
) -> Result<Membership> {
    let lens = lens(conn, actor, board, None, None).await?;
    let filter = lens.filter.clone().unwrap_or(Filter::True);
    let steps = plan_for(conn, actor, board, issue, &filter).await?;
    let mut sp = sqlx::Connection::begin(&mut *conn).await?;
    let mut writes = Vec::new();
    let records = apply(&mut sp, actor, issue, &steps, &mut writes).await?;
    let (mut shown, _) = cards_of(&mut sp, actor, board, Some(sprint), &lens, &[issue.id]).await?;
    if shown.pop().flatten().is_none() {
        return Err(Error::Refused(Failure::Unsettable.message().into()));
    }
    let mut rule_records = Vec::new();
    for written in &writes {
        hear(&mut sp, batch, ctx, issue, written, &mut rule_records).await?;
    }
    sp.commit().await?;
    if !records.is_empty() {
        finish_change(
            conn,
            batch,
            issue,
            author(actor, origin, false, issue.project_id)?,
            &records,
            &[],
        )
        .await?;
    }
    settle(conn, batch, ctx, issue, &rule_records).await?;
    batch.issue_on_board(issue.id, Change::Updated, board.full.board.id);
    Ok(Membership::Done)
}
