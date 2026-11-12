//! Sharing of tags and saved searches (ISS-037, QRY-065, ACC-066; D-038): who sees, uses and
//! edits what one person owns. One model for both; boards and dashboards reuse it later.
//!
//! - The owner may do everything; the audiences are independent (using or editing a tag does not
//!   let one see it on issues, ISS-037).
//! - Changing the audiences needs the share-views capability (ACC-066), and being the owner or a
//!   user administrator (`user.update`); only a user administrator gives the thing to someone else.
//! - Non-owners with edit rights change the content (name, color, query), never the sharing, and
//!   do not delete (ISS-036, QRY-064).

use crate::access::Access;
use crate::access::capability::Capability;
use crate::access::permission::Permission;
use crate::validation::{Problem, Rejection};
use crate::{GroupId, UserId};

/// Entries in one audience list, accounts and groups together.
pub const AUDIENCE_MAX: usize = 50;

/// Accounts and groups (project teams are groups, D-027).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Audience {
    pub users: Vec<UserId>,
    pub groups: Vec<GroupId>,
}

impl Audience {
    pub fn is_empty(&self) -> bool {
        self.users.is_empty() && self.groups.is_empty()
    }

    pub fn includes(&self, who: &Access) -> bool {
        self.users.contains(&who.user()) || self.groups.iter().any(|g| who.in_group(*g))
    }

    /// Sorted, without duplicates and without the owner; at most [`AUDIENCE_MAX`] entries.
    pub fn validated(mut self, owner: UserId) -> Result<Self, Rejection> {
        self.users.retain(|u| *u != owner);
        self.users.sort_unstable();
        self.users.dedup();
        self.groups.sort_unstable();
        self.groups.dedup();
        if self.users.len() + self.groups.len() > AUDIENCE_MAX {
            return Err(Rejection::new(
                Problem::TooLong,
                format!("at most {AUDIENCE_MAX} accounts and groups"),
            ));
        }
        Ok(self)
    }
}

/// The owner and audiences of one shared thing. Saved searches leave `use_` empty: viewing one is
/// using it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shared {
    pub owner: UserId,
    pub view: Audience,
    pub use_: Audience,
    pub edit: Audience,
}

impl Shared {
    pub fn private(owner: UserId) -> Self {
        Self {
            owner,
            ..Self::default()
        }
    }

    pub fn is_shared(&self) -> bool {
        !(self.view.is_empty() && self.use_.is_empty() && self.edit.is_empty())
    }

    fn owns(&self, who: &Access) -> bool {
        self.owner == who.user()
    }

    /// Sees it: a tag on issues and in their history (HIST-007), a saved search in the list.
    pub fn can_view(&self, who: &Access) -> bool {
        self.owns(who) || self.view.includes(who)
    }

    /// Puts a tag on issues and takes it off (with the right to update the issue, ISS-035).
    pub fn can_use(&self, who: &Access) -> bool {
        self.owns(who) || self.use_.includes(who)
    }

    /// Changes the content: the name, the color, the query.
    pub fn can_edit(&self, who: &Access) -> bool {
        self.owns(who) || self.edit.includes(who) || who.has(Permission::UserUpdate)
    }

    /// Changes the audiences.
    pub fn may_share(&self, who: &Access) -> Result<(), Refusal> {
        if !(self.owns(who) || who.has(Permission::UserUpdate)) {
            return Err(Refusal::NotOwner);
        }
        if !who.capability(Capability::ShareViews) {
            return Err(Refusal::NoCapability);
        }
        Ok(())
    }

    /// Gives it to another owner.
    pub fn may_transfer(&self, who: &Access) -> Result<(), Refusal> {
        if !who.has(Permission::UserUpdate) {
            return Err(Refusal::NotOwner);
        }
        if !who.capability(Capability::ShareViews) {
            return Err(Refusal::NoCapability);
        }
        Ok(())
    }

    pub fn may_delete(&self, who: &Access) -> bool {
        self.owns(who) || who.has(Permission::UserUpdate)
    }
}

/// Why a change of sharing is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Neither the owner nor a user administrator (transfer: not a user administrator).
    NotOwner,
    /// Not in a group with the share-views capability (ACC-066).
    NoCapability,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::capability::CapabilitySet;
    use crate::access::permission::PermissionSet;
    use crate::access::{Grant, Scope};

    fn person(id: UserId, groups: &[GroupId], admin: bool, share: bool) -> Access {
        let grants = admin.then_some(Grant {
            permissions: PermissionSet::of(Permission::UserUpdate),
            scope: Scope::Global,
        });
        let capabilities: CapabilitySet = if share {
            [Capability::ShareViews].into_iter().collect()
        } else {
            CapabilitySet::default()
        };
        Access::compile(id, grants, groups.to_vec(), capabilities)
    }

    fn tag() -> Shared {
        Shared {
            owner: 1,
            view: Audience {
                users: vec![2],
                groups: vec![],
            },
            use_: Audience {
                users: vec![3],
                groups: vec![],
            },
            edit: Audience {
                users: vec![],
                groups: vec![10],
            },
        }
    }

    #[test]
    fn iss_037_audiences_are_independent() {
        let t = tag();
        let owner = person(1, &[], false, true);
        let viewer = person(2, &[], false, true);
        let user = person(3, &[], false, true);
        let editor = person(4, &[10], false, true);
        assert!(t.can_view(&owner) && t.can_use(&owner) && t.can_edit(&owner));
        assert!(t.can_view(&viewer) && !t.can_use(&viewer) && !t.can_edit(&viewer));
        assert!(!t.can_view(&user) && t.can_use(&user) && !t.can_edit(&user));
        assert!(!t.can_view(&editor) && !t.can_use(&editor) && t.can_edit(&editor));
    }

    #[test]
    fn iss_036_editors_neither_share_nor_delete() {
        let t = tag();
        let editor = person(4, &[10], false, true);
        assert_eq!(t.may_share(&editor), Err(Refusal::NotOwner));
        assert!(!t.may_delete(&editor));
        let admin = person(9, &[], true, true);
        assert!(t.can_edit(&admin) && t.may_delete(&admin) && t.may_share(&admin).is_ok());
        assert!(
            !t.can_view(&admin),
            "administrators do not see others' tags on issues"
        );
    }

    #[test]
    fn acc_066_sharing_needs_the_capability() {
        let t = tag();
        assert_eq!(
            t.may_share(&person(1, &[], false, false)),
            Err(Refusal::NoCapability)
        );
        assert!(t.may_share(&person(1, &[], false, true)).is_ok());
        assert_eq!(
            t.may_transfer(&person(1, &[], false, true)),
            Err(Refusal::NotOwner)
        );
        assert_eq!(
            t.may_transfer(&person(9, &[], true, false)),
            Err(Refusal::NoCapability)
        );
        assert!(t.may_transfer(&person(9, &[], true, true)).is_ok());
    }

    #[test]
    fn audience_is_cleaned_and_limited() {
        let a = Audience {
            users: vec![5, 1, 5, 3],
            groups: vec![7, 7],
        }
        .validated(1)
        .unwrap();
        assert_eq!(a.users, vec![3, 5]);
        assert_eq!(a.groups, vec![7]);
        let big = Audience {
            users: (100..151).collect(),
            groups: vec![],
        };
        assert_eq!(big.validated(1).unwrap_err().problem, Problem::TooLong);
        assert!(Shared::private(1).can_view(&person(1, &[], false, false)));
        assert!(!Shared::private(1).is_shared());
        assert!(tag().is_shared());
    }
}
