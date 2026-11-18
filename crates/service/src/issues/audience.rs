//! Whom an issue's audience may name (ISS-043): the picker offers the project team as one entry,
//! the project's default audience, the groups and teams of the actor, and people of the team.
//! Others can still be added by ID: the list only shortens the choice.

use taskmaster_domain::access::Access;
use taskmaster_domain::{GroupId, ProjectId, UserId};
use taskmaster_store::groups::{self, Group};
use taskmaster_store::teams;
use taskmaster_store::users::{self, User};
use taskmaster_store::{PgPool, projects};

use crate::groups::team_visible;
use crate::{Error, Result};

/// Most people offered.
const PEOPLE_MAX: i64 = 20;
const SEARCH_MAX: usize = 100;

/// What the audience picker offers.
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    /// The project's team first, then its default audience, then the actor's groups.
    pub groups: Vec<Group>,
    pub users: Vec<User>,
}

/// Candidates for the audience of issues in a project, filtered by `text` (ISS-043). Needs
/// `issue.create` or `issue.update` there.
pub async fn candidates(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    text: &str,
) -> Result<Candidates> {
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let mut conn = pool.acquire().await?;
    let project = projects::get(&mut *conn, project)
        .await?
        .filter(|p| crate::projects::can_see(actor, p))
        .ok_or(Error::NotFound("no such project"))?;
    if taskmaster_domain::issues::rights::may_create(actor, project.id).is_err()
        && !actor.has_in(
            taskmaster_domain::access::permission::Permission::IssueUpdate,
            project.id,
        )
    {
        return Err(Error::NotFound("no such project"));
    }
    let team = teams::of_project(&mut *conn, project.id).await?;
    let mut ids: Vec<GroupId> = team.into_iter().collect();
    ids.extend(project.default_audience_group_id);
    ids.extend(actor.groups().iter().copied());
    let mut seen = Vec::new();
    ids.retain(|id| {
        let fresh = !seen.contains(id);
        seen.push(*id);
        fresh
    });
    let found = groups::get_many(&mut *conn, &ids).await?;
    let groups_list: Vec<Group> = ids
        .iter()
        .filter_map(|id| found.iter().find(|g| g.id == *id))
        .filter(|g| g.builtin.is_none() && team_visible(actor, g))
        .filter(|g| text.is_empty() || g.name.to_lowercase().contains(&text))
        .cloned()
        .collect();
    let people: Vec<UserId> = match team {
        Some(team) => groups::members(&mut conn, team, &text, ("", 0), PEOPLE_MAX).await?,
        None => Vec::new(),
    };
    let found_users = users::get_many(&mut *conn, &people).await?;
    let users_list = people
        .iter()
        .filter_map(|id| found_users.iter().find(|u| u.id == *id))
        .cloned()
        .collect();
    Ok(Candidates {
        groups: groups_list,
        users: users_list,
    })
}
