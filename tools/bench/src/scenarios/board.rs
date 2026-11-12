//! Boards over HTTP (step 1.8a, D-035):
//!
//! - Board with 500 cards: `GET /boards/{id}/view?sprint=` by a member of the board's project
//!   team: the sprint's cards by column, their issues in the short form of lists.
//! - The same with a header filter (`q=is: unresolved`, BRD-081), and a query board (BRD-028)
//!   whose query names that sprint (`sprint: "board/sprint"`): the same cards through the query
//!   path. The query board is made once over HTTP by a member and kept for later runs.
//! - The same query board with lanes by parent issues (BRD-051, step 1.8c): each card's parent
//!   through the subtask links, the lanes ordered and paged, cells by column and lane.
//! - Card move (the `change` scenario): `POST …/cards/{issue}/move` to another column by a member
//!   of the team: the column field changes with history, rank and live events in one commit. The
//!   issue's version and column are read first by primary key on the benchmark's own connection
//!   (a fraction of a millisecond, inside the measurement).

use std::sync::OnceLock;

use http::Method;
use serde_json::json;
use sqlx::PgConnection;

use crate::catalog::Catalog;
use crate::http::Node;
use crate::rng::Rng;

pub const VARIANTS: &[&str] = &[
    "sprint board",
    "header filter",
    "query board",
    "parent lanes",
];

const QUERY_BOARD: &str = "Bench query board";
const LANES_BOARD: &str = "Bench lanes board";

/// The query boards and their owners, made on first use: without lanes, with parent lanes.
static MADE: OnceLock<(i64, i64)> = OnceLock::new();
static MADE_LANES: OnceLock<(i64, i64)> = OnceLock::new();

pub async fn view(
    catalog: &Catalog,
    node: &Node,
    conn: &mut PgConnection,
    rng: &mut Rng,
    variant: usize,
) -> anyhow::Result<usize> {
    let (viewer, path) = if variant >= 2 {
        let (board, owner) = query_board(catalog, node, conn, rng, variant == 3).await?;
        (owner, format!("/boards/{board}/view"))
    } else {
        let (board, project, sprint, _) = Catalog::pick(&catalog.sprints, rng);
        let filter = if variant == 1 {
            "&q=is%3A%20unresolved"
        } else {
            ""
        };
        (
            catalog.member(project, rng),
            format!("/boards/{board}/view?sprint={sprint}{filter}"),
        )
    };
    let answer = node
        .send(Method::GET, &path, &catalog.token(viewer), None)
        .await?;
    Ok(answer["cards"].as_array().map_or(0, Vec::len))
}

/// A kanban board in mode `query` over the project of the sprint closest to the card target,
/// showing that sprint's issues; `lanes` adds lanes by parent issues.
async fn query_board(
    catalog: &Catalog,
    node: &Node,
    conn: &mut PgConnection,
    rng: &mut Rng,
    lanes: bool,
) -> anyhow::Result<(i64, i64)> {
    let (cell, name) = if lanes {
        (&MADE_LANES, LANES_BOARD)
    } else {
        (&MADE, QUERY_BOARD)
    };
    if let Some(made) = cell.get() {
        return Ok(*made);
    }
    let found: Option<(i64, i64)> =
        sqlx::query_as("select id, owner_id from boards where name = $1 order by id limit 1")
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?;
    let made = match found {
        Some(made) => made,
        None => {
            let (_, project, sprint, _) = *catalog
                .sprints
                .first()
                .ok_or_else(|| anyhow::anyhow!("no sprint boards in the dataset"))?;
            let (board_name, sprint_name): (String, String) = sqlx::query_as(
                "select b.name, s.name from sprints s join boards b on b.id = s.board_id
                 where s.id = $1",
            )
            .bind(sprint)
            .fetch_one(&mut *conn)
            .await?;
            let owner = catalog.member(project, rng);
            let token = catalog.token(owner);
            let board = node
                .send(
                    Method::POST,
                    "/boards",
                    &token,
                    Some(&json!({
                        "preset": "kanban",
                        "name": name,
                        "projects": [project],
                    })),
                )
                .await?;
            let id = board["id"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("no board made: {board}"))?;
            let mut change = json!({
                "mode": "query",
                "query": format!("sprint: \"{board_name}/{sprint_name}\""),
                "version": board["version"],
            });
            if lanes {
                change["lanes"] = json!({ "mode": "parents" });
            }
            node.send(
                Method::PATCH,
                &format!("/boards/{id}"),
                &token,
                Some(&change),
            )
            .await?;
            (id, owner)
        }
    };
    Ok(*cell.get_or_init(|| made))
}

pub async fn move_card(
    catalog: &Catalog,
    node: &Node,
    conn: &mut PgConnection,
    rng: &mut Rng,
) -> anyhow::Result<usize> {
    // A scrum project's board when the pick holds one: the move answers assign-on-move (§3.14).
    let sprints = if catalog.scrum_sprints.is_empty() {
        &catalog.sprints
    } else {
        &catalog.scrum_sprints
    };
    let (board, project, sprint, _) = Catalog::pick(sprints, rng);
    let cards = catalog
        .open_cards
        .get(&sprint)
        .map_or(&[][..], Vec::as_slice);
    let issue = Catalog::pick(cards, rng);
    let (version, column): (i64, i64) = sqlx::query_as(
        "select i.version, c.id
         from issues i
         join issue_ref_values r on r.issue_id = i.id and r.field_id = $2
         join board_columns c on c.board_id = $3 and c.value_id = r.value
         where i.id = $1",
    )
    .bind(issue)
    .bind(i64::from(catalog.state))
    .bind(board)
    .fetch_one(&mut *conn)
    .await?;
    let columns = catalog
        .board_columns
        .get(&board)
        .map_or(&[][..], Vec::as_slice);
    let others: Vec<i64> = columns.iter().copied().filter(|c| *c != column).collect();
    let to = Catalog::pick(&others, rng);
    let author = catalog.member(project, rng);
    node.send(
        Method::POST,
        &format!("/boards/{board}/sprints/{sprint}/cards/{issue}/move"),
        &catalog.token(author),
        Some(&json!({ "column": to, "version": version })),
    )
    .await?;
    Ok(1)
}
