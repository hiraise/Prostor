//! Full-text search over HTTP: `GET /issues?q=<words>`, a page of 50 by relevance (D-037). Words
//! are drawn by frequency: rare words such as names and error codes, frequent ones, and two words
//! together; then a word within one project and a word among unresolved issues. The hundred most
//! frequent words of the vocabulary are left out, as people do not search for them. Who searches
//! across projects is someone in many project teams, as for lists.

use std::ops::Range;

use crate::catalog::Catalog;
use crate::http::Node;
use crate::rng::Rng;

use super::list::{PAGE, encode};

pub const VARIANTS: &[&str] = &[
    "rare word",
    "frequent word",
    "two words",
    "word in a project",
    "word, unresolved",
];

const RARE: Range<usize> = 2_000..20_000;
const FREQUENT: Range<usize> = 100..1_000;
const ANY: Range<usize> = 100..5_000;

/// A word of the vocabulary that is not an operator of the language (`not` can be made up).
fn word(catalog: &Catalog, rng: &mut Rng, ranks: Range<usize>) -> String {
    loop {
        let word = catalog.word(rng, ranks.clone());
        if !matches!(word, "not" | "and" | "or") {
            return word.to_owned();
        }
    }
}

/// The query of variant `variant` and the person who runs it.
pub fn query(catalog: &Catalog, rng: &mut Rng, variant: usize) -> (String, i64) {
    let c = catalog;
    let busy = |rng: &mut Rng| Catalog::pick(&c.busy_members, rng);
    match variant % VARIANTS.len() {
        0 => (word(c, rng, RARE), busy(rng)),
        1 => (word(c, rng, FREQUENT), busy(rng)),
        2 => (
            format!("{} {}", word(c, rng, ANY), word(c, rng, ANY)),
            busy(rng),
        ),
        3 => {
            let project = c.project(rng);
            let key = c.keys.get(&project).cloned().unwrap_or_default();
            (
                format!("{} project: {key}", word(c, rng, ANY)),
                c.member(project, rng),
            )
        }
        _ => (format!("{} is: unresolved", word(c, rng, ANY)), busy(rng)),
    }
}

pub async fn run(
    catalog: &Catalog,
    node: &Node,
    rng: &mut Rng,
    variant: usize,
) -> anyhow::Result<usize> {
    let (q, viewer) = query(catalog, rng, variant);
    let page = node
        .send(
            http::Method::GET,
            &format!("/issues?q={}&limit={PAGE}", encode(&q)),
            &catalog.token(viewer),
            None,
        )
        .await?;
    let items = page["items"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("search for {q}: {page}"))?;
    Ok(items.len())
}
