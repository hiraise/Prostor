//! Attachments (CNT-017, CNT-018, CNT-020, CNT-024, ACC-047…ACC-049): who may add, change and
//! delete files of an issue, and who sees them. Seeing a file needs seeing its issue first (the
//! caller checks that), for a comment's file also the comment, and then the attachment rule of
//! `access::visibility`, so a file's audience only narrows.

use crate::access::permission::Permission;
use crate::access::visibility::{self, Audience, Item, Kind};
use crate::access::{Access, Denied, Scope};
use crate::issues::rights::{self, Facts as IssueFacts};
use crate::{ProjectId, UserId};

/// What the rules need to know about an attachment.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub project: ProjectId,
    pub author: UserId,
    pub audience: Audience<'a>,
}

/// Whether the viewer sees a file of an issue (and comment) they see (ACC-047, ACC-049): the
/// issue's read permission, or being its author with `attachment.create`; then the audience.
pub fn can_see(viewer: &Access, file: &Facts<'_>) -> bool {
    visibility::can_see(
        viewer,
        Kind::Attachment,
        Item {
            project: file.project,
            author: Some(file.author),
            audience: file.audience,
        },
    )
}

/// Attaching files to an issue or draft the viewer sees (CNT-017, ISS-007): changing the issue
/// (`issue.update`, or its reporter with `issue.create`) and `attachment.create`.
pub fn may_attach(viewer: &Access, issue: &IssueFacts<'_>) -> Result<(), Denied> {
    rights::may_update(viewer, issue)?;
    viewer.require_at(Permission::AttachmentCreate, Scope::Project(issue.project))
}

/// Posting files with a comment (CNT-018): the comment's own right is checked by the comment;
/// the files need `attachment.create`.
pub fn may_attach_to_comment(viewer: &Access, project: ProjectId) -> Result<(), Denied> {
    viewer.require_at(Permission::AttachmentCreate, Scope::Project(project))
}

/// Renaming a file or changing its audience (CNT-020, CNT-023): one's own with
/// `attachment.create`, others' with `attachment.update`.
pub fn may_update(viewer: &Access, file: &Facts<'_>) -> Result<(), Denied> {
    let permission = if file.author == viewer.user() {
        Permission::AttachmentCreate
    } else {
        Permission::AttachmentUpdate
    };
    viewer.require_at(permission, Scope::Project(file.project))
}

/// Deleting a file (CNT-024): one's own always, others' with `attachment.delete`.
pub fn may_delete(viewer: &Access, file: &Facts<'_>) -> Result<(), Denied> {
    if file.author == viewer.user() {
        return Ok(());
    }
    viewer.require_at(Permission::AttachmentDelete, Scope::Project(file.project))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Grant;
    use crate::access::capability::CapabilitySet;
    use crate::access::permission::PermissionSet;
    use crate::access::role::PredefinedRole;

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

    fn set(permissions: &[Permission]) -> PermissionSet {
        permissions.iter().copied().collect()
    }

    fn issue(reporter: UserId) -> IssueFacts<'static> {
        IssueFacts {
            project: 1,
            reporter,
            draft: false,
            audience: Audience::default(),
        }
    }

    fn file(author: UserId) -> Facts<'static> {
        Facts {
            project: 1,
            author,
            audience: Audience::default(),
        }
    }

    #[test]
    fn cnt_017_attaching_needs_changing_the_issue() {
        let member = person(5, PredefinedRole::ProjectMember.permissions());
        assert!(may_attach(&member, &issue(9)).is_ok());
        let updater = person(5, set(&[Permission::IssueUpdate]));
        assert!(may_attach(&updater, &issue(9)).is_err());
        // The reporter rule: own issues with `issue.create` (ACC-048).
        let reporter = person(
            5,
            set(&[Permission::IssueCreate, Permission::AttachmentCreate]),
        );
        assert!(may_attach(&reporter, &issue(5)).is_ok());
        assert!(may_attach(&reporter, &issue(9)).is_err());
        assert!(may_attach_to_comment(&reporter, 1).is_ok());
        assert!(may_attach_to_comment(&reporter, 2).is_err());
    }

    #[test]
    fn cnt_020_cnt_024_own_and_others() {
        let author = person(5, set(&[Permission::AttachmentCreate]));
        assert!(may_update(&author, &file(5)).is_ok());
        assert!(may_update(&author, &file(6)).is_err());
        assert!(may_delete(&author, &file(5)).is_ok());
        assert!(may_delete(&author, &file(6)).is_err());
        // One's own files stay one's own to delete, whatever the role says now.
        let nobody = person(5, PermissionSet::EMPTY);
        assert!(may_delete(&nobody, &file(5)).is_ok());
        let manager = person(
            7,
            set(&[Permission::AttachmentUpdate, Permission::AttachmentDelete]),
        );
        assert!(may_update(&manager, &file(5)).is_ok());
        assert!(may_delete(&manager, &file(5)).is_ok());
    }

    #[test]
    fn acc_049_audiences_narrow() {
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        assert!(can_see(&member, &file(5)));
        let restricted = Facts {
            audience: Audience {
                users: &[9],
                groups: &[],
            },
            ..file(5)
        };
        assert!(!can_see(&member, &restricted));
        assert!(can_see(
            &person(5, set(&[Permission::AttachmentCreate])),
            &restricted
        ));
        assert!(can_see(
            &person(8, PredefinedRole::SystemAdmin.permissions()),
            &restricted
        ));
    }
}
