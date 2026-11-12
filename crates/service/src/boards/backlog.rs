//! The backlog of a board (BRD-089…BRD-096, D-045): a saved search shown beside the
//! board, with a manual order — its priority — kept on the search, so every board using that
//! search shares it.

use sqlx::PgConnection;
use time::OffsetDateTime;
use trackflow_domain::access::Access;
use trackflow_domain::rank;
use trackflow_domain::search::saved;
use trackflow_domain::settings::Language;
use trackflow_domain::sharing::{Audience, Shared};
use trackflow_domain::validation::Problem;
use trackflow_domain::{BoardId, IssueId, ProjectId, UserId};
use trackflow_store::PgPool;
use trackflow_store::saved_searches::{
    self as store_searches, NewSavedSearch, SavedSearchId, ranks,
};

use super::load;
use crate::live::Batch;
use crate::search::{self, SearchInput, SearchPage};
use crate::seed::internal;
use crate::{Error, Result};

/// How far down the unranked part of a backlog a drop may land (BRD-093): ranks are given to
/// the issues above it, in their order.
pub const RANK_UP_TO_MAX: i64 = 500;

/// The saved search a backlog is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogSearch {
    pub id: SavedSearchId,
    pub name: String,
    /// The caller may change its order (BRD-093): they edit the search.
    pub may_order: bool,
}

/// Why a board shows no backlog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// The board has none (never chosen, or the search was deleted).
    None,
    /// The caller does not view its search.
    Hidden,
}

/// One page of a backlog.
#[derive(Debug, Clone)]
pub struct Backlog {
    pub search: Option<BacklogSearch>,
    pub missing: Option<Missing>,
    pub page: Option<SearchPage>,
}

/// What the backlog panel asks for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BacklogInput {
    /// The panel's own filter, ANDed with the search.
    pub text: String,
    pub cursor: Option<String>,
    pub limit: u32,
    pub time_zone: Option<String>,
    pub now: Option<OffsetDateTime>,
}

/// A page of the board's backlog (BRD-089, BRD-093): ranked issues first by their rank, the rest
/// by the search's sort.
pub async fn get(
    pool: &PgPool,
    viewer: &Access,
    board_id: BoardId,
    input: BacklogInput,
) -> Result<Backlog> {
    let found = {
        let mut conn = pool.acquire().await?;
        let board = load(&mut conn, viewer, board_id, true).await?;
        match board.full.board.backlog_search_id {
            Some(id) => store_searches::get(&mut *conn, id).await?,
            None => None,
        }
    };
    let Some(row) = found else {
        return Ok(Backlog {
            search: None,
            missing: Some(Missing::None),
            page: None,
        });
    };
    let shared = row.sharing();
    if !shared.can_view(viewer) {
        return Ok(Backlog {
            search: None,
            missing: Some(Missing::Hidden),
            page: None,
        });
    }
    let page = search::search(
        pool,
        viewer,
        SearchInput {
            text: input.text,
            saved_search: Some(row.id),
            manual_order: Some(row.id),
            time_zone: input.time_zone,
            cursor: input.cursor,
            limit: input.limit,
            now: input.now,
            ..SearchInput::default()
        },
    )
    .await?;
    Ok(Backlog {
        search: Some(BacklogSearch {
            id: row.id,
            name: row.name,
            may_order: shared.can_edit(viewer),
        }),
        missing: None,
        page: Some(page),
    })
}

/// The backlog search locked for a change of its order: the caller edits it.
async fn lock_ordered(
    conn: &mut PgConnection,
    viewer: &Access,
    board_id: BoardId,
) -> Result<SavedSearchId> {
    let board = load(&mut *conn, viewer, board_id, true).await?;
    let id = board
        .full
        .board
        .backlog_search_id
        .ok_or(Error::Refused("the board has no backlog".into()))?;
    let row = store_searches::lock(&mut *conn, id)
        .await?
        .filter(|r| r.sharing().can_view(viewer))
        .ok_or(Error::Refused(
            "you do not see the backlog of this board".into(),
        ))?;
    if !row.sharing().can_edit(viewer) {
        return Err(Error::Refused(
            "you may not change the order of this backlog".into(),
        ));
    }
    Ok(id)
}

/// Puts an issue of the backlog after `after` or before `before` (neither: first) in its manual
/// order (BRD-093). A neighbour without a rank first ranks the unranked issues up to it, in the
/// order they are shown.
pub async fn order(
    pool: &PgPool,
    viewer: &Access,
    board_id: BoardId,
    issue: IssueId,
    after: Option<IssueId>,
    before: Option<IssueId>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let search = lock_ordered(&mut tx, viewer, board_id).await?;
    let found = trackflow_store::issues::get(&mut *tx, issue).await?;
    crate::issues::checked(&mut tx, viewer, found).await?;
    if after == Some(issue) || before == Some(issue) {
        return Err(Error::field(
            if after == Some(issue) {
                "after"
            } else {
                "before"
            },
            Problem::Invalid,
            "an issue is not its own neighbour",
        ));
    }
    let (rank, follows) = match (after, before) {
        (Some(a), _) => {
            let ra = ranked(&mut tx, viewer, search, a, issue, "after").await?;
            let next = ranks::next(&mut *tx, search, &ra, a, issue).await?;
            let rank = rank::between(Some(&ra), next.as_ref().map(|(_, r)| r.as_str()))
                .map_err(internal)?;
            (rank, Some(a))
        }
        (None, Some(b)) => {
            let rb = ranked(&mut tx, viewer, search, b, issue, "before").await?;
            let prev = ranks::previous(&mut *tx, search, &rb, b, issue).await?;
            let rank = rank::between(prev.as_ref().map(|(_, r)| r.as_str()), Some(&rb))
                .map_err(internal)?;
            (rank, prev.map(|(id, _)| id))
        }
        (None, None) => {
            let first = ranks::next(&mut *tx, search, "", IssueId::MIN, issue).await?;
            let rank =
                rank::between(None, first.as_ref().map(|(_, r)| r.as_str())).map_err(internal)?;
            (rank, None)
        }
    };
    ranks::set(&mut tx, search, issue, &rank).await?;
    let mut batch = Batch::new();
    batch.backlog_order(issue, search, follows);
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// The rank of a neighbour; an unranked one gets ranks for it and the unranked issues above it.
async fn ranked(
    conn: &mut PgConnection,
    viewer: &Access,
    search: SavedSearchId,
    neighbour: IssueId,
    moving: IssueId,
    path: &str,
) -> Result<String> {
    if let Some(r) = ranks::of(&mut *conn, search, neighbour).await? {
        return Ok(r);
    }
    let Some(above) =
        search::unranked_until(conn, viewer, search, neighbour, RANK_UP_TO_MAX).await?
    else {
        return Err(Error::field(
            path,
            Problem::Invalid,
            "no such issue in this backlog",
        ));
    };
    let mut last = ranks::last(&mut *conn, search).await?;
    for id in above.into_iter().filter(|id| *id != moving) {
        let r = rank::between(last.as_deref(), None).map_err(internal)?;
        ranks::set(conn, search, id, &r).await?;
        last = Some(r);
    }
    last.ok_or_else(|| internal("a ranked neighbour without a rank"))
}

/// Forgets the manual order of the board's backlog (BRD-093): the search's sort again.
pub async fn discard(pool: &PgPool, viewer: &Access, board_id: BoardId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let search = lock_ordered(&mut tx, viewer, board_id).await?;
    ranks::discard(&mut tx, search).await?;
    let mut batch = Batch::new();
    for board in ranks::boards_of(&mut *tx, search).await? {
        batch.board(board, trackflow_domain::live::Change::Updated);
    }
    batch.publish(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Checks a search chosen as the backlog (BRD-090): the caller views it and its issues can only
/// be of the board projects.
pub(crate) async fn check_choice(
    conn: &mut PgConnection,
    viewer: &Access,
    id: SavedSearchId,
    projects: &[ProjectId],
) -> Result<()> {
    store_searches::get(&mut *conn, id)
        .await?
        .filter(|r| r.sharing().can_view(viewer))
        .ok_or_else(|| Error::field("backlog", Problem::Invalid, "no such saved search"))?;
    let scope = search::saved_scope(conn, viewer, id).await?;
    if !scope.narrowed || scope.projects.iter().any(|p| !projects.contains(p)) {
        return Err(Error::field(
            "backlog",
            Problem::Invalid,
            "the search must name only projects of this board",
        ));
    }
    Ok(())
}

/// What a new board's backlog is made of.
pub(crate) struct NewBacklog<'a> {
    pub owner: UserId,
    pub board: BoardId,
    pub board_name: &'a str,
    /// Keys of the board projects.
    pub keys: &'a [String],
    pub projects: &'a [ProjectId],
    /// Who views it besides the owner.
    pub view: Audience,
    pub language: Language,
}

/// The default backlog of a new board (BRD-089): «<board> · бэклог», the unresolved issues of
/// the board projects that are not on the board; a free name is found when the owner has one.
/// `None` when the owner has as many saved searches as allowed.
pub(crate) async fn create_default(
    conn: &mut PgConnection,
    new: NewBacklog<'_>,
) -> Result<Option<SavedSearchId>> {
    if store_searches::count_of_owner(&mut *conn, new.owner).await?
        >= i64::try_from(saved::PER_OWNER_MAX).unwrap_or(i64::MAX)
    {
        return Ok(None);
    }
    let base = saved::backlog_name(new.language, new.board_name);
    let mut name = base.clone();
    let mut n = 2;
    while ranks::name_taken(&mut *conn, new.owner, &name).await? {
        let suffix = format!(" {n}");
        let head: String = base
            .chars()
            .take(saved::NAME_MAX - suffix.chars().count())
            .collect();
        name = format!("{head}{suffix}");
        n += 1;
    }
    let keys: Vec<&str> = new.keys.iter().map(String::as_str).collect();
    let text = saved::backlog_query(&keys, new.board_name);
    let stored = crate::saved_searches::bind_board(
        conn,
        new.owner,
        &text,
        new.projects,
        (new.board, new.board_name),
    )
    .await?;
    let id = store_searches::insert(
        conn,
        &NewSavedSearch {
            owner: new.owner,
            name: &name,
            query: &crate::saved_searches::encode(&stored)?,
            builtin: None,
            project: None,
        },
    )
    .await?;
    if !new.view.users.is_empty() || !new.view.groups.is_empty() {
        let shared = Shared {
            view: new.view,
            ..Shared::private(new.owner)
        };
        store_searches::set_sharing(conn, id, &shared).await?;
    }
    Ok(Some(id))
}
