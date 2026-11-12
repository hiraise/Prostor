//! Who sees an issue, comment or attachment (ACC-047…ACC-049): the role permission in the item's
//! project, the rights authors have without it, and the item's audience, which only narrows.
//!
//! These rules decide single items. Lists apply the same rules in SQL through one predicate
//! built from [`Access::projects_with`] and the viewer's groups (architecture.md §4), so pages
//! never leak or skip; issues bring it in step 1.3, comments and attachments in step 1.4.

use serde::{Deserialize, Serialize};

use super::Access;
use super::permission::Permission;
use crate::{GroupId, ProjectId, UserId};

/// The people and groups an item is limited to (ACC-049). Empty lists mean "no restriction".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Audience<'a> {
    pub users: &'a [UserId],
    pub groups: &'a [GroupId],
}

impl Audience<'_> {
    pub fn is_restricted(&self) -> bool {
        !self.users.is_empty() || !self.groups.is_empty()
    }

    /// Whether the viewer is in the audience: listed, or in a listed group or team.
    fn includes(&self, viewer: &Access) -> bool {
        self.users.contains(&viewer.user()) || self.groups.iter().any(|g| viewer.in_group(*g))
    }
}

/// One item as the visibility rules see it.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a> {
    pub project: ProjectId,
    /// Reporter of an issue, author of a comment or attachment.
    pub author: Option<UserId>,
    pub audience: Audience<'a>,
}

/// The kinds of items with these rules: each has its read permission and the permission whose
/// holders may read what they wrote themselves (ACC-047).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Issue,
    Comment,
    Attachment,
}

impl Kind {
    fn read(self) -> Permission {
        match self {
            Kind::Issue | Kind::Attachment => Permission::IssueRead,
            Kind::Comment => Permission::CommentRead,
        }
    }

    fn create(self) -> Permission {
        match self {
            Kind::Issue => Permission::IssueCreate,
            Kind::Comment => Permission::CommentCreate,
            Kind::Attachment => Permission::AttachmentCreate,
        }
    }
}

/// Whether the viewer may see the item (ACC-049): the read permission in its project — or being
/// its author with the create permission (ACC-047, which makes ACC-048's submit-only role work) —
/// and, when it has an audience, being in it, being its author or holding `visibility.bypass`.
pub fn can_see(viewer: &Access, kind: Kind, item: Item<'_>) -> bool {
    let own = item.author == Some(viewer.user());
    let permitted = viewer.has_in(kind.read(), item.project)
        || (own && viewer.has_in(kind.create(), item.project));
    if !permitted {
        return false;
    }
    !item.audience.is_restricted()
        || own
        || item.audience.includes(viewer)
        || viewer.has_in(Permission::VisibilityBypass, item.project)
}

#[cfg(test)]
mod tests {
    use super::super::capability::CapabilitySet;
    use super::super::permission::PermissionSet;
    use super::super::role::PredefinedRole;
    use super::super::{Grant, Scope};
    use super::*;

    fn viewer(user: UserId, permissions: PermissionSet, groups: Vec<GroupId>) -> Access {
        Access::compile(
            user,
            [Grant {
                permissions,
                scope: Scope::Project(1),
            }],
            groups,
            CapabilitySet::default(),
        )
    }

    fn issue(author: UserId, audience: Audience<'_>) -> Item<'_> {
        Item {
            project: 1,
            author: Some(author),
            audience,
        }
    }

    #[test]
    fn acc_049_an_audience_only_narrows() {
        let member = viewer(5, PredefinedRole::ProjectMember.permissions(), vec![20]);
        let open = Audience::default();
        assert!(can_see(&member, Kind::Issue, issue(9, open)));
        let restricted = Audience {
            users: &[7],
            groups: &[30],
        };
        assert!(!can_see(&member, Kind::Issue, issue(9, restricted)));
        let via_group = Audience {
            users: &[],
            groups: &[20],
        };
        assert!(can_see(&member, Kind::Issue, issue(9, via_group)));
        // Listed but without the read permission: still hidden.
        let outsider = viewer(7, PermissionSet::EMPTY, vec![]);
        assert!(!can_see(&outsider, Kind::Issue, issue(9, restricted)));
        // The bypass reveals restricted items to those who may read them.
        let admin = viewer(8, PredefinedRole::SystemAdmin.permissions(), vec![]);
        assert!(can_see(&admin, Kind::Issue, issue(9, restricted)));
        // Another project is another project.
        let elsewhere = Item {
            project: 2,
            ..issue(9, open)
        };
        assert!(!can_see(&member, Kind::Issue, elsewhere));
    }

    #[test]
    fn acc_047_acc_048_authors_see_their_own_items() {
        // The submit-only pattern: create, link, attach and comment, but no issue read.
        let submit_only: PermissionSet = [
            Permission::ProjectBasic,
            Permission::IssueCreate,
            Permission::IssueLink,
            Permission::AttachmentCreate,
            Permission::CommentCreate,
            Permission::CommentRead,
            Permission::CommentUpdateOwn,
            Permission::CommentDeleteOwn,
        ]
        .into_iter()
        .collect();
        let reporter = viewer(5, submit_only, vec![]);
        let restricted = Audience {
            users: &[7],
            groups: &[],
        };
        assert!(can_see(
            &reporter,
            Kind::Issue,
            issue(5, Audience::default())
        ));
        assert!(can_see(&reporter, Kind::Issue, issue(5, restricted)));
        assert!(!can_see(
            &reporter,
            Kind::Issue,
            issue(6, Audience::default())
        ));
        assert!(can_see(
            &reporter,
            Kind::Attachment,
            issue(5, Audience::default())
        ));
        assert!(can_see(
            &reporter,
            Kind::Comment,
            issue(6, Audience::default())
        ));
    }
}
