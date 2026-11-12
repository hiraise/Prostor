//! Each person's view of a board (BRD-001, BRD-002, BRD-047, BRD-058), synced across devices.

use sqlx::{PgConnection, PgExecutor};
use std::collections::BTreeMap;
use time::OffsetDateTime;

use trackflow_domain::{BoardId, ColumnId, IssueId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub favorite: bool,
    pub card_size: i16,
    pub collapsed_columns: Vec<ColumnId>,
    pub collapsed_lanes: Vec<String>,
    /// The draft of each cell's «+» (BRD-077), by `<sprint>:<column>:<lane>`.
    pub cell_drafts: sqlx::types::Json<BTreeMap<String, IssueId>>,
    pub visited_at: Option<OffsetDateTime>,
}

impl Profile {
    /// What a person sees before they change anything.
    pub fn initial() -> Self {
        Self {
            card_size: 1,
            ..Self::default()
        }
    }
}

/// The person's view, or the initial one.
pub async fn get(
    db: impl PgExecutor<'_>,
    user: UserId,
    board: BoardId,
) -> Result<Profile, sqlx::Error> {
    let found = sqlx::query_as!(
        Profile,
        r#"select favorite, card_size, collapsed_columns, collapsed_lanes,
                cell_drafts as "cell_drafts: sqlx::types::Json<BTreeMap<String, IssueId>>",
                visited_at
         from board_profiles where user_id = $1 and board_id = $2"#,
        user,
        board
    )
    .fetch_optional(db)
    .await?;
    Ok(found.unwrap_or_else(Profile::initial))
}

/// Writes the person's choices; `visited_at` stays.
pub async fn upsert(
    conn: &mut PgConnection,
    user: UserId,
    board: BoardId,
    profile: &Profile,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into board_profiles
           (user_id, board_id, favorite, card_size, collapsed_columns, collapsed_lanes,
            cell_drafts)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict (user_id, board_id) do update set favorite = excluded.favorite,
           card_size = excluded.card_size, collapsed_columns = excluded.collapsed_columns,
           collapsed_lanes = excluded.collapsed_lanes, cell_drafts = excluded.cell_drafts",
        user,
        board,
        profile.favorite,
        profile.card_size,
        &profile.collapsed_columns,
        &profile.collapsed_lanes,
        &profile.cell_drafts as &sqlx::types::Json<BTreeMap<String, IssueId>>
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Records a visit: one statement per view of the board.
pub async fn visit(
    db: impl PgExecutor<'_>,
    user: UserId,
    board: BoardId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into board_profiles (user_id, board_id, visited_at) values ($1, $2, now())
         on conflict (user_id, board_id) do update set visited_at = now()",
        user,
        board
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The boards the person visited last, newest first; the caller keeps the first one they can
/// still view.
pub async fn last_visited(
    db: impl PgExecutor<'_>,
    user: UserId,
    limit: i64,
) -> Result<Vec<BoardId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select board_id from board_profiles
         where user_id = $1 and visited_at is not null
         order by visited_at desc limit $2",
        user,
        limit
    )
    .fetch_all(db)
    .await
}
