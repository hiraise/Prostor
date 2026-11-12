//! Who may do what with an issue (ISS-022, ISS-041, ISS-044, FLD-060, ACC-047…ACC-049). Reading
//! follows the visibility rule of `access::visibility`; drafts are their reporter's alone.

use crate::access::permission::Permission;
use crate::access::visibility::{self, Audience, Item, Kind};
use crate::access::{Access, Denied, Scope};
use crate::{ProjectId, UserId};

/// What the rules need to know about an issue.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub project: ProjectId,
    pub reporter: UserId,
    /// Not published yet.
    pub draft: bool,
    pub audience: Audience<'a>,
}

impl Facts<'_> {
    fn reported_by(&self, viewer: &Access) -> bool {
        self.reporter == viewer.user()
    }
}

fn require(viewer: &Access, permission: Permission, project: ProjectId) -> Result<(), Denied> {
    viewer.require_at(permission, Scope::Project(project))
}

/// Whether the viewer may see the issue (ACC-049): the read permission in its project, or being
/// its reporter with the create permission (ACC-047), and the audience. A draft only its reporter
/// sees, while they may still create issues there.
pub fn can_read(viewer: &Access, issue: &Facts<'_>) -> bool {
    if issue.draft {
        return issue.reported_by(viewer) && viewer.has_in(Permission::IssueCreate, issue.project);
    }
    visibility::can_see(
        viewer,
        Kind::Issue,
        Item {
            project: issue.project,
            author: Some(issue.reporter),
            audience: issue.audience,
        },
    )
}

/// Creating issues in a project (ISS-005).
pub fn may_create(viewer: &Access, project: ProjectId) -> Result<(), Denied> {
    require(viewer, Permission::IssueCreate, project)
}

/// Changing the summary, description, public fields, tags and audience of an issue the viewer
/// sees: `issue.update`, or being its reporter with `issue.create` (ISS-022, ACC-047). A draft is
/// its reporter's to change.
pub fn may_update(viewer: &Access, issue: &Facts<'_>) -> Result<(), Denied> {
    if issue.draft || viewer.has_in(Permission::IssueUpdate, issue.project) {
        return Ok(());
    }
    if issue.reported_by(viewer) && viewer.has_in(Permission::IssueCreate, issue.project) {
        return Ok(());
    }
    Err(Denied::of(Permission::IssueUpdate))
}

/// Reading private fields (FLD-060); `visibility.bypass` implies it (ISS-044).
pub fn may_read_private(viewer: &Access, project: ProjectId) -> bool {
    viewer.has_in(Permission::IssuePrivateRead, project)
}

/// Changing private fields (FLD-060): `issue.private.update`, which implies `issue.update`.
/// Reporters with only `issue.create` change public fields alone.
pub fn may_update_private(viewer: &Access, project: ProjectId) -> Result<(), Denied> {
    require(viewer, Permission::IssuePrivateUpdate, project)
}

/// Deleting an issue (ISS-028): `issue.delete`, also for the reporter.
pub fn may_delete(viewer: &Access, issue: &Facts<'_>) -> Result<(), Denied> {
    require(viewer, Permission::IssueDelete, issue.project)
}

/// Moving an issue to another project (ISS-025): `issue.update` where it is and `issue.create`
/// where it goes.
pub fn may_move(viewer: &Access, from: ProjectId, to: ProjectId) -> Result<(), Denied> {
    require(viewer, Permission::IssueUpdate, from)?;
    require(viewer, Permission::IssueCreate, to)
}

/// Adding or removing other people as watchers (ISS-007).
pub fn may_manage_watchers(viewer: &Access, project: ProjectId) -> Result<(), Denied> {
    require(viewer, Permission::IssueWatchersUpdate, project)
}

/// Seeing how many people watch an issue (ISS-016).
pub fn may_see_watchers(viewer: &Access, project: ProjectId) -> bool {
    viewer.has_in(Permission::IssueWatchersRead, project)
}

/// Making changes without notifications (HIST-012).
pub fn may_be_silent(viewer: &Access, project: ProjectId) -> Result<(), Denied> {
    require(viewer, Permission::IssueSilent, project)
}

/// Why a vote is refused (ISS-032).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteRefusal {
    /// Nobody votes for an issue they reported.
    OwnIssue,
    /// Drafts take no votes.
    Draft,
}

/// Voting for an issue the viewer sees (ISS-032): anyone who reads it, except its reporter.
pub fn may_vote(viewer: &Access, issue: &Facts<'_>) -> Result<(), VoteRefusal> {
    if issue.draft {
        return Err(VoteRefusal::Draft);
    }
    if issue.reported_by(viewer) {
        return Err(VoteRefusal::OwnIssue);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::capability::CapabilitySet;
    use crate::access::permission::PermissionSet;
    use crate::access::role::PredefinedRole;
    use crate::access::{Grant, Scope};

    fn person(user: UserId, permissions: PermissionSet) -> Access {
        Access::compile(
            user,
            [Grant {
                permissions,
                scope: Scope::Project(1),
            }],
            vec![],
            CapabilitySet::default(),
        )
    }

    fn issue(reporter: UserId, draft: bool) -> Facts<'static> {
        Facts {
            project: 1,
            reporter,
            draft,
            audience: Audience::default(),
        }
    }

    fn submit_only() -> PermissionSet {
        [
            Permission::ProjectBasic,
            Permission::IssueCreate,
            Permission::IssueLink,
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn iss_022_reporters_update_their_own_issues() {
        let reporter = person(5, submit_only());
        assert!(can_read(&reporter, &issue(5, false)));
        assert!(!can_read(&reporter, &issue(6, false)));
        assert!(may_update(&reporter, &issue(5, false)).is_ok());
        assert!(may_update(&reporter, &issue(6, false)).is_err());
        assert!(may_delete(&reporter, &issue(5, false)).is_err());
        assert!(may_update_private(&reporter, 1).is_err());
        assert!(!may_read_private(&reporter, 1));
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        assert!(may_update(&member, &issue(5, false)).is_ok());
        assert!(may_delete(&member, &issue(5, false)).is_ok());
        assert!(may_update_private(&member, 1).is_ok());
    }

    #[test]
    fn iss_002_drafts_are_their_reporters() {
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        assert!(can_read(&member, &issue(7, true)));
        assert!(!can_read(&member, &issue(8, true)));
        assert!(may_update(&member, &issue(7, true)).is_ok());
        let reader = person(
            7,
            [Permission::ProjectBasic, Permission::IssueRead]
                .into_iter()
                .collect(),
        );
        // Without the create permission a draft is out of reach, even one's own.
        assert!(!can_read(&reader, &issue(7, true)));
    }

    #[test]
    fn iss_041_audience_narrows_reading() {
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        let restricted = Facts {
            audience: Audience {
                users: &[9],
                groups: &[],
            },
            ..issue(5, false)
        };
        assert!(!can_read(&member, &restricted));
        let listed = person(9, PredefinedRole::ProjectMember.permissions());
        assert!(can_read(&listed, &restricted));
        // The reporter is always in the audience.
        let reporter = person(5, submit_only());
        assert!(can_read(&reporter, &restricted));
    }

    #[test]
    fn iss_032_votes_and_iss_025_moves() {
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        assert_eq!(
            may_vote(&member, &issue(7, false)),
            Err(VoteRefusal::OwnIssue)
        );
        assert_eq!(may_vote(&member, &issue(5, true)), Err(VoteRefusal::Draft));
        assert!(may_vote(&member, &issue(5, false)).is_ok());
        assert!(may_move(&member, 1, 2).is_err());
        let both = Access::compile(
            7,
            [1, 2].map(|project| Grant {
                permissions: PredefinedRole::ProjectMember.permissions(),
                scope: Scope::Project(project),
            }),
            vec![],
            CapabilitySet::default(),
        );
        assert!(may_move(&both, 1, 2).is_ok());
        assert!(may_manage_watchers(&member, 1).is_ok());
        assert!(may_see_watchers(&member, 1));
        assert!(may_be_silent(&member, 1).is_err());
    }
}
