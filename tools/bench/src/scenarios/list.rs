//! Issue list page of 50 over HTTP: `GET /issues?q=…` with the query language, two or three
//! filters sorted by an enumeration field (Priority), as a person would write them. The server
//! parses, resolves names and compiles the query (`store::search`, the rules of D-024);
//! `--breakdown` shows the statements a request ran.

use http::Method;

use crate::catalog::Catalog;
use crate::dataset::model::names;
use crate::http::Node;
use crate::rng::Rng;

pub const VARIANTS: &[&str] = &[
    "assigned to me, unresolved",
    "project, type, unresolved",
    "project, fix version, type",
    "urgent and open in all projects",
    "fixed bugs in all projects",
    "project, default order",
    "count: assigned to me, unresolved",
];

/// Issues on a page.
pub const PAGE: i64 = 50;

/// A name as a query value: always quoted, so spaces and keywords read as the name.
pub(super) fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

fn list(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| quoted(n))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The query of variant `variant`, the person who runs it and whether it is a count.
pub fn query(catalog: &Catalog, rng: &mut Rng, variant: usize) -> (String, i64, bool) {
    let c = catalog;
    let kind = quoted(names::TYPE);
    let priority = quoted(names::PRIORITY);
    let state = quoted(names::STATE);
    let bug = quoted(names::BUG);
    let sort = format!("sort: {priority}");
    let key = |project: i32| c.keys.get(&project).cloned().unwrap_or_default();
    // Across projects: someone in many project teams.
    let busy = |rng: &mut Rng| Catalog::pick(&c.busy_members, rng);
    match variant % VARIANTS.len() {
        v @ (0 | 6) => {
            let me = Catalog::pick(&c.assignees, rng);
            let assignee = quoted(names::ASSIGNEE);
            let q = format!("{assignee}: me is: unresolved");
            if v == 0 {
                (format!("{q} {sort}"), me, false)
            } else {
                (q, me, true)
            }
        }
        1 => {
            // A project that has bugs.
            let project = (0..20)
                .map(|_| c.project(rng))
                .find(|p| c.bug_projects.contains(p))
                .unwrap_or_default();
            let q = format!(
                "project: {} {kind}: {bug} is: unresolved {sort}",
                key(project)
            );
            (q, c.member(project, rng), false)
        }
        2 => {
            // A project that has versions and bugs.
            let project = (0..20)
                .map(|_| c.project(rng))
                .find(|p| c.versions.contains_key(p) && c.bug_projects.contains(p))
                .unwrap_or_default();
            let versions = c.versions.get(&project).map_or(&[][..], Vec::as_slice);
            let version = Catalog::pick(versions, rng);
            let name = c.version_names.get(&version).cloned().unwrap_or_default();
            let q = format!(
                "project: {} {}: {} {kind}: {bug} {sort}",
                key(project),
                quoted(names::FIX_VERSIONS),
                quoted(&name),
            );
            (q, c.member(project, rng), false)
        }
        3 => {
            let q = format!(
                "{priority}: {} {state}: {} {sort}",
                list(&names::URGENT),
                list(&names::OPEN)
            );
            (q, busy(rng), false)
        }
        4 => {
            let q = format!("{kind}: {bug} {state}: {} {sort}", quoted(names::FIXED));
            (q, busy(rng), false)
        }
        _ => {
            let project = c.project(rng);
            (
                format!("project: {}", key(project)),
                c.member(project, rng),
                false,
            )
        }
    }
}

pub(super) fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

pub async fn run(
    catalog: &Catalog,
    node: &Node,
    rng: &mut Rng,
    variant: usize,
) -> anyhow::Result<usize> {
    let (q, viewer, count) = query(catalog, rng, variant);
    let token = catalog.token(viewer);
    if count {
        let answer = node
            .send(
                Method::GET,
                &format!("/issues/count?q={}", encode(&q)),
                &token,
                None,
            )
            .await?;
        anyhow::ensure!(answer["all"].is_i64(), "count of {q}: {answer}");
        return Ok(1);
    }
    let page = node
        .send(
            Method::GET,
            &format!("/issues?q={}&limit={PAGE}", encode(&q)),
            &token,
            None,
        )
        .await?;
    let items = page["items"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("list of {q}: {page}"))?;
    Ok(items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_quoted_as_values() {
        assert_eq!(quoted("Fix versions"), "\"Fix versions\"");
        assert_eq!(quoted("a\"b\\"), "\"a\\\"b\\\\\"");
        assert_eq!(list(&["Open", "In progress"]), "\"Open\", \"In progress\"");
    }
}
