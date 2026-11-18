//! The command scenario (step 2.1 §8): one command line — `state … priority … tag …` plus a
//! comment — applied to one issue over `POST /commands`, and the bulk job over a query of ≈50
//! issues. The value and tag names are read first by primary key on the benchmark's own
//! connection, a fraction of a millisecond, inside the measurement (the way the state cascade
//! does it).

use super::list::quoted;
use anyhow::Context;
use serde_json::json;
use sqlx::Row;

use crate::catalog::Catalog;
use crate::dataset::model::names;
use crate::http::Node;
use crate::rng::Rng;
use http::Method;

/// The names of the values the line names, by their IDs.
async fn value_names(conn: &mut sqlx::PgConnection, ids: &[i64]) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query("select name from field_values where id = any($1)")
        .bind(ids)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect())
}

/// One of the author's own tags, the ones they may use.
async fn own_tag(
    conn: &mut sqlx::PgConnection,
    author: i64,
    rng: &mut Rng,
) -> anyhow::Result<Option<String>> {
    let rows: Vec<String> =
        sqlx::query_scalar("select name from tags where owner_id = $1 order by id")
            .bind(author)
            .fetch_all(&mut *conn)
            .await?;
    let index = Catalog::pick(&(0..rows.len()).collect::<Vec<_>>(), rng);
    Ok(rows.get(index).cloned())
}

/// The command line of one operation: the state flipped the other way (every operation is a
/// real change), a priority of the project, the author's tag, a comment.
#[allow(clippy::too_many_arguments)]
async fn line(
    catalog: &Catalog,
    conn: &mut sqlx::PgConnection,
    rng: &mut Rng,
    issue: i64,
    project: i32,
    author: i64,
) -> anyhow::Result<String> {
    let Some((open, resolved)) = catalog.project_states.get(&project) else {
        return Ok(String::new());
    };
    let is_resolved: bool =
        sqlx::query_scalar("select resolved_at is not null from issues where id = $1")
            .bind(issue)
            .fetch_one(&mut *conn)
            .await?;
    let flip = Catalog::pick(if is_resolved { open } else { resolved }, rng);
    let priorities = catalog
        .project_priorities
        .get(&project)
        .map_or(&[][..], Vec::as_slice);
    let priority = Catalog::pick(priorities, rng);
    let state = value_names(conn, &[flip]).await?;
    let priority = value_names(conn, &[priority]).await?;
    let (Some(state), Some(priority)) = (state.first(), priority.first()) else {
        return Ok(String::new());
    };
    let mut parts: Vec<String> = vec![format!("state {state}")];
    parts.push(format!("priority {priority}"));
    if let Some(tag) = own_tag(conn, author, rng).await? {
        parts.push(format!("tag {tag}"));
    }
    parts.push("comment".to_owned());
    Ok(parts.join(" "))
}

/// Applies the line to one issue (plan §8): the change budget covers this variant.
pub async fn one(
    catalog: &Catalog,
    node: &Node,
    conn: &mut sqlx::PgConnection,
    rng: &mut Rng,
) -> anyhow::Result<usize> {
    let (issue, project, author) = catalog.issue_and_member(rng);
    let text = line(catalog, conn, rng, issue, project, author).await?;
    if text.is_empty() {
        return Ok(0);
    }
    node.send(
        Method::POST,
        "/commands",
        &catalog.token(author),
        Some(&json!({
            "text": text,
            "issues": [issue],
            "comment": "by command",
        })),
    )
    .await?;
    Ok(1)
}

/// The bulk query of ≈50 issues (plan §8): one fix version and one assignee together. A single
/// version holds hundreds even in the smallest project — the dataset spreads them by the issue's
/// progress through the project's life (`dataset::issues`), and one assignee holds a team's
/// share — so the pair picks the batch: the count closest to 50, a deterministic order, the same
/// batch every operation. The command does not touch versions or assignees, so the batch stays,
/// and the probe comes from the batch, so every job flips its state the other way — a real
/// change. Returns the query text and the probe issue.
async fn batch(
    conn: &mut sqlx::PgConnection,
    catalog: &Catalog,
    project: i32,
) -> anyhow::Result<Option<(String, i64)>> {
    let row = sqlx::query(
        "select v.name as version, u.username as username, min(r.issue_id) as probe \
         from issue_ref_values r \
         join field_values v on v.id = r.value \
         join issue_ref_values a on a.issue_id = r.issue_id and a.field_id = $2::bigint \
         join users u on u.id = a.value \
         where r.field_id = $1::bigint \
           and r.issue_id in (select id from issues where project_id = $3::bigint) \
         group by v.name, u.username \
         order by abs(count(*) - 50), v.name, u.username \
         limit 1",
    )
    .bind(catalog.fix_versions)
    .bind(catalog.assignee)
    .bind(project)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|row| {
        (
            format!(
                "{}: {} {}: {}",
                quoted(names::FIX_VERSIONS),
                quoted(row.get::<String, _>("version").as_str()),
                quoted(names::ASSIGNEE),
                quoted(row.get::<String, _>("username").as_str()),
            ),
            row.get("probe"),
        )
    }))
}

/// Enqueues the bulk job over ≈50 issues of the smallest project and waits it out (plan §8): one
/// measurement is the whole job — its p95 is the job's, not one apply's.
pub async fn bulk(
    catalog: &Catalog,
    node: &Node,
    conn: &mut sqlx::PgConnection,
    rng: &mut Rng,
) -> anyhow::Result<usize> {
    let Some(project) = catalog
        .project_sizes
        .iter()
        .min_by_key(|(_, size)| **size)
        .map(|(project, _)| *project)
    else {
        return Ok(0);
    };
    let author = catalog.member(project, rng);
    let Some((query, issue)) = batch(conn, catalog, project).await? else {
        return Ok(0);
    };
    let text = line(catalog, conn, rng, issue, project, author).await?;
    if text.is_empty() {
        return Ok(0);
    }
    let queued = node
        .send(
            Method::POST,
            "/commands/bulk",
            &catalog.token(author),
            Some(&json!({
                "text": text,
                "query": query,
                "project": project,
                "comment": "by command",
            })),
        )
        .await?;
    let job = queued
        .get("job")
        .and_then(|id| id.as_i64())
        .context("the bulk enqueue answered no job")?;
    let token = catalog.token(author);
    let mut last_seen = String::new();
    for _ in 0..1500 {
        let progress = node
            .send(Method::GET, &format!("/commands/bulk/{job}"), &token, None)
            .await?;
        let state = progress.get("state").and_then(|state| state.as_str());
        if state == Some("finished") {
            return Ok(progress
                .get("done")
                .and_then(|done| done.as_i64())
                .map_or(0, |done| usize::try_from(done).unwrap_or(0)));
        }
        if matches!(state, Some("failed") | Some("cancelled")) {
            anyhow::bail!(
                "the bulk job {job} ended {state:?}: {:?}",
                progress.get("last_error")
            );
        }
        if let Some(seen) = state {
            last_seen = seen.to_owned();
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    anyhow::bail!("the bulk job {job} never finished (last state {last_seen})")
}
