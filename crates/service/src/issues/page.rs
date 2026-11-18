//! The issue page in one request (backlog 2026-10-23, step 1.7b): the issue, the first page of its
//! activity, its links panel and its files. The issue, its project and the viewer's access are
//! loaded and checked once, on one connection, instead of once per part.

use taskmaster_domain::IssueId;
use taskmaster_domain::access::Access;
use taskmaster_store::PgPool;
use taskmaster_store::issues;

use super::activity::{self, ActivityPage};
use super::{One, checked, one};
use crate::Result;
use crate::attachments::{self, Listed};
use crate::issue_links::{self, Panel};

/// What the issue page shows on opening.
#[derive(Debug, Clone)]
pub struct Page {
    pub issue: One,
    pub activity: ActivityPage,
    pub links: Panel,
    pub attachments: Listed,
    /// The boards and sprints holding it (BRD-033).
    pub boards: Vec<crate::boards::issue_side::IssueBoard>,
}

/// The page of an issue the viewer may see; `query` is the activity's first page.
pub async fn page(
    pool: &PgPool,
    viewer: &Access,
    id: IssueId,
    query: activity::Query,
) -> Result<Page> {
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, id).await?;
    let (issue, project) = checked(&mut conn, viewer, found).await?;
    let links = issue_links::panel_of(&mut conn, viewer, &issue).await?;
    let attachments = attachments::list_of(
        &mut conn,
        viewer,
        &issue,
        &project,
        None,
        attachments::PAGE_MAX,
    )
    .await?;
    let boards = crate::boards::issue_side::boards_of(&mut conn, viewer, &issue).await?;
    let activity = activity::page_of(&mut conn, viewer, issue.clone(), &project, query).await?;
    let issue = one(&mut conn, viewer, issue).await?;
    Ok(Page {
        issue,
        activity,
        links,
        attachments,
        boards,
    })
}
