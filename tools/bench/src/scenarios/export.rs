//! The export scenario (step 2.2 §9): one whole project's issues as a file over
//! `POST /issues/export`, both formats — CSV one variant, XLSX the other. The project is the
//! largest, so the export cap of 500 rows is what stops it, as the toolbar's note tells people.
//! The viewer is a team member: the count they see can be short of the project's issues, some
//! are limited to audiences they are not in.

use super::list::quoted;
use anyhow::Context;
use serde_json::json;

use crate::catalog::Catalog;
use crate::http::Node;
use crate::rng::Rng;
use http::Method;

/// The export cap of the server (`service::issues::export::EXPORT_CAP`), asserted from the
/// answer's headers rather than imported: the benchmark measures the server as it is deployed.
const CAP: i64 = 500;

/// Exports one project's issues in the variant's format; answers the rows in the file.
pub async fn run(
    catalog: &Catalog,
    node: &Node,
    rng: &mut Rng,
    variant: usize,
) -> anyhow::Result<usize> {
    // The largest project, which a team member exports: a whole project is what people export.
    let project = catalog
        .projects
        .iter()
        .copied()
        .max_by_key(|project| {
            (
                catalog.project_sizes.get(project).copied().unwrap_or(0),
                *project,
            )
        })
        .unwrap_or(1);
    let viewer = catalog.member(project, rng);
    let key = catalog.keys.get(&project).cloned().unwrap_or_default();
    let format = if variant.is_multiple_of(2) {
        "csv"
    } else {
        "xlsx"
    };
    let (status, headers, bytes) = node
        .download(
            Method::POST,
            "/issues/export",
            &catalog.token(viewer),
            &json!({ "query": { "q": format!("project: {}", quoted(&key)) }, "format": format }),
        )
        .await?;
    anyhow::ensure!(status.is_success(), "export {format} answered {status}");
    let read = |name: &str| -> anyhow::Result<i64> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
            .with_context(|| format!("no {name} in the export's answer"))
    };
    let exported = read("x-exported-count")?;
    let total = read("x-total-count")?;
    anyhow::ensure!(total >= CAP, "the query found only {total} issues");
    anyhow::ensure!(
        exported == total.min(CAP),
        "the file holds {exported} of {total}"
    );
    anyhow::ensure!(!bytes.is_empty(), "the file is empty");
    Ok(exported as usize)
}
