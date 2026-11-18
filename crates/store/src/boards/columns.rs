//! Board columns (BRD-042…BRD-046): one field value each, a rank, WIP limits.

use prostor_domain::{BoardId, ColumnId, ValueId};
use sqlx::{PgConnection, PgExecutor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub id: ColumnId,
    pub board_id: BoardId,
    pub value_id: ValueId,
    pub rank: String,
    pub wip_min: Option<i32>,
    pub wip_max: Option<i32>,
}

/// Unique constraint on (board, value).
pub const VALUE_KEY: &str = "board_columns_board_id_value_id_key";

/// In rank order (ties by ID).
pub async fn of_board(db: impl PgExecutor<'_>, board: BoardId) -> Result<Vec<Column>, sqlx::Error> {
    sqlx::query_as!(
        Column,
        "select id, board_id, value_id, rank, wip_min, wip_max from board_columns
         where board_id = $1 order by rank, id",
        board
    )
    .fetch_all(db)
    .await
}

pub async fn insert(
    conn: &mut PgConnection,
    board: BoardId,
    value: ValueId,
    rank: &str,
    wip_min: Option<i32>,
    wip_max: Option<i32>,
) -> Result<ColumnId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into board_columns (board_id, value_id, rank, wip_min, wip_max)
         values ($1, $2, $3, $4, $5) returning id",
        board,
        value,
        rank,
        wip_min,
        wip_max
    )
    .fetch_one(conn)
    .await
}

/// Inserts columns in one statement; `ranks` beside `values`, limits per column.
pub async fn insert_many(
    conn: &mut PgConnection,
    board: BoardId,
    values: &[ValueId],
    ranks: &[String],
    wip_min: &[Option<i32>],
    wip_max: &[Option<i32>],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into board_columns (board_id, value_id, rank, wip_min, wip_max)
         select $1, v, r, mn, mx
         from unnest($2::bigint[], $3::text[], $4::int[], $5::int[]) as t(v, r, mn, mx)",
        board,
        values,
        ranks,
        wip_min as &[Option<i32>],
        wip_max as &[Option<i32>]
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Moves a column and sets its limits.
pub async fn update(
    conn: &mut PgConnection,
    column: ColumnId,
    rank: &str,
    wip_min: Option<i32>,
    wip_max: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update board_columns set rank = $2, wip_min = $3, wip_max = $4 where id = $1",
        column,
        rank,
        wip_min,
        wip_max
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes a column; it leaves the collapsed columns of every profile.
pub async fn delete(
    conn: &mut PgConnection,
    board: BoardId,
    column: ColumnId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from board_columns where id = $1 and board_id = $2",
        column,
        board
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update board_profiles set collapsed_columns = array_remove(collapsed_columns, $2)
         where board_id = $1 and $2 = any(collapsed_columns)",
        board,
        column
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes every column (a new column field).
pub async fn delete_all(conn: &mut PgConnection, board: BoardId) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from board_columns where board_id = $1", board)
        .execute(&mut *conn)
        .await?;
    sqlx::query!(
        "update board_profiles set collapsed_columns = '{}'
         where board_id = $1 and cardinality(collapsed_columns) > 0",
        board
    )
    .execute(conn)
    .await?;
    Ok(())
}
