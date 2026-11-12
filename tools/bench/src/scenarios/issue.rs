//! One issue over HTTP, as the issue page asks for it (D-035):
//!
//! - Open: the issue page in one request (`GET /issues/{id}/page`, step 1.7b): the issue, the
//!   first page of its activity stream, its links panel with the parent, and its files, by a
//!   member of the issue's project. Before 1.7b the page sent four requests at once (D-035).
//! - Change: a new priority by a member of the project, committed and answered with the issue.
//! - State cascade (step 1.9 §3.14): the state of the bottom issue of a closed living chain,
//!   toggled between an open and a resolved value, so the parent-follows walk (AUT-094) climbs
//!   all three levels — resolving up, reopening down — with the rule writes, records and
//!   messages of every step.
//! - Assignee warning (AUT-063): the assignee of an issue with an audience becomes another team
//!   member, outside the listed people, so the visibility check runs its membership query and
//!   warns the actor.

use http::Method;
use serde_json::json;
use sqlx::PgConnection;

use crate::catalog::Catalog;
use crate::http::Node;
use crate::rng::Rng;

/// Items on the first page of the stream.
const HISTORY_PAGE: u32 = 50;

pub async fn open(catalog: &Catalog, node: &Node, rng: &mut Rng) -> anyhow::Result<usize> {
    let (id, _, viewer) = catalog.issue_and_member(rng);
    let token = catalog.token(viewer);
    let path = format!("/issues/{id}/page?limit={HISTORY_PAGE}");
    let page = node.send(Method::GET, &path, &token, None).await?;
    Ok(page["activity"]["items"].as_array().map_or(0, Vec::len))
}

pub async fn change(catalog: &Catalog, node: &Node, rng: &mut Rng) -> anyhow::Result<usize> {
    let (id, project, author) = catalog.issue_and_member(rng);
    let values = catalog
        .project_priorities
        .get(&project)
        .map_or(&[][..], Vec::as_slice);
    let to = Catalog::pick(values, rng);
    node.send(
        Method::PUT,
        &format!("/issues/{id}/fields/{}", catalog.priority),
        &catalog.token(author),
        Some(&json!({ "value": to, "silent": false })),
    )
    .await?;
    Ok(1)
}

/// Toggles the bottom issue's state the other way, so every operation is a real change and the
/// walk runs in both directions. The state and the resolution are read first by primary key on
/// the benchmark's own connection, a fraction of a millisecond, inside the measurement.
pub async fn state_cascade(
    catalog: &Catalog,
    node: &Node,
    conn: &mut PgConnection,
    rng: &mut Rng,
) -> anyhow::Result<usize> {
    let (id, project) = Catalog::pick(&catalog.chains, rng);
    let Some((open, resolved)) = catalog.project_states.get(&project) else {
        return Ok(0);
    };
    let is_resolved: bool =
        sqlx::query_scalar("select resolved_at is not null from issues where id = $1")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
    let to = Catalog::pick(if is_resolved { open } else { resolved }, rng);
    node.send(
        Method::PUT,
        &format!("/issues/{id}/fields/{}", catalog.state),
        &catalog.token(catalog.member(project, rng)),
        Some(&json!({ "value": to, "silent": false })),
    )
    .await?;
    Ok(1)
}

/// Gives the issue another team member as its assignee — not the one it holds, so the write
/// always changes the value — by a team member the audience lists.
pub async fn assignee_warning(
    catalog: &Catalog,
    node: &Node,
    conn: &mut PgConnection,
    rng: &mut Rng,
) -> anyhow::Result<usize> {
    let (id, project, actor) = Catalog::pick(&catalog.audience_issues, rng);
    let current: Option<i64> = sqlx::query_scalar(
        "select value from issue_ref_values where issue_id = $1 and field_id = $2",
    )
    .bind(id)
    .bind(i64::from(catalog.assignee))
    .fetch_optional(&mut *conn)
    .await?;
    let team = catalog.teams.get(&project).map_or(&[][..], Vec::as_slice);
    let others: Vec<i64> = team
        .iter()
        .copied()
        .filter(|user| Some(*user) != current)
        .collect();
    node.send(
        Method::PUT,
        &format!("/issues/{id}/fields/{}", catalog.assignee),
        &catalog.token(actor),
        Some(&json!({ "value": Catalog::pick(&others, rng), "silent": false })),
    )
    .await?;
    Ok(1)
}
