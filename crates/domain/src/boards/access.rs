//! Who may view, edit, share, transfer, delete and create boards (BRD-003, BRD-010…BRD-013,
//! BRD-016): the board's version of D-038 sharing, with two dynamic audiences over the board's
//! projects.
//!
//! - *View* (and use): the owner, the view and edit audiences (edit implies view), and, with
//!   `view_readers`, everyone who reads issues in every board project.
//! - *Edit settings*: the owner and the edit audience; with `edit_updaters`, viewers who update
//!   issues in at least one board project; `system.write` on any board the person views that is
//!   not private. The two last ones need the view right first (BRD-013: "boards they can see").
//! - *Share* (audiences and flags): an editor with the share-views capability (ACC-066).
//! - *Transfer*: an editor (BRD-011). *Delete*: the owner or `system.write` (BRD-016).

use crate::ProjectId;
use crate::access::Access;
use crate::access::capability::Capability;
use crate::access::permission::Permission;
use crate::sharing::Shared;

/// The sharing of one board. `shared.use_` stays empty: viewing a board is using it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardAccess {
    pub shared: Shared,
    /// Everyone who reads issues in every board project views the board.
    pub view_readers: bool,
    /// Everyone who updates issues in at least one board project edits the settings.
    pub edit_updaters: bool,
    pub projects: Vec<ProjectId>,
}

/// Why a change is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// May not edit the board's settings.
    NotEditor,
    /// Not in a group with the share-views capability (ACC-066).
    NoCapability,
}

impl BoardAccess {
    fn owns(&self, who: &Access) -> bool {
        self.shared.owner == who.user()
    }

    /// Private: nobody but the owner; `system.write` does not reach it (BRD-013).
    pub fn is_private(&self) -> bool {
        !(self.shared.is_shared() || self.view_readers || self.edit_updaters)
    }

    fn reads_every_project(&self, who: &Access) -> bool {
        !self.projects.is_empty()
            && self
                .projects
                .iter()
                .all(|p| who.has_in(Permission::IssueRead, *p))
    }

    pub fn can_view(&self, who: &Access) -> bool {
        self.owns(who)
            || self.shared.view.includes(who)
            || self.shared.edit.includes(who)
            || (self.view_readers && self.reads_every_project(who))
    }

    /// Edits the settings: name, columns, cards, sprints (BRD-013).
    pub fn can_edit(&self, who: &Access) -> bool {
        if self.owns(who) || self.shared.edit.includes(who) {
            return true;
        }
        if !self.can_view(who) {
            return false;
        }
        (self.edit_updaters
            && self
                .projects
                .iter()
                .any(|p| who.has_in(Permission::IssueUpdate, *p)))
            || (who.has(Permission::SystemWrite) && !self.is_private())
    }

    /// Changes the audiences and the two flags.
    pub fn may_share(&self, who: &Access) -> Result<(), Refusal> {
        if !self.can_edit(who) {
            return Err(Refusal::NotEditor);
        }
        if !who.capability(Capability::ShareViews) {
            return Err(Refusal::NoCapability);
        }
        Ok(())
    }

    /// Gives the board to another owner (BRD-011); the previous owner keeps access only through
    /// the lists.
    pub fn may_transfer(&self, who: &Access) -> Result<(), Refusal> {
        if !self.can_edit(who) {
            return Err(Refusal::NotEditor);
        }
        Ok(())
    }

    pub fn may_delete(&self, who: &Access) -> bool {
        self.owns(who) || who.has(Permission::SystemWrite)
    }
}

/// Creates a board over these projects (BRD-003, BRD-010): any signed-in account but the guest,
/// reading issues in every chosen project.
pub fn may_create(who: &Access, is_guest: bool, projects: &[ProjectId]) -> bool {
    !is_guest
        && !projects.is_empty()
        && projects
            .iter()
            .all(|p| who.has_in(Permission::IssueRead, *p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::capability::CapabilitySet;
    use crate::access::permission::PermissionSet;
    use crate::access::{Grant, Scope};
    use crate::sharing::Audience;
    use crate::{GroupId, UserId};

    struct Who {
        id: UserId,
        groups: Vec<GroupId>,
        grants: Vec<Grant>,
        share: bool,
    }

    impl Who {
        fn new(id: UserId) -> Self {
            Self {
                id,
                groups: vec![],
                grants: vec![],
                share: false,
            }
        }
        fn group(mut self, g: GroupId) -> Self {
            self.groups.push(g);
            self
        }
        fn grant(mut self, p: Permission, scope: Scope) -> Self {
            self.grants.push(Grant {
                permissions: PermissionSet::of(p),
                scope,
            });
            self
        }
        fn sharer(mut self) -> Self {
            self.share = true;
            self
        }
        fn access(self) -> Access {
            let caps: CapabilitySet = if self.share {
                [Capability::ShareViews].into_iter().collect()
            } else {
                CapabilitySet::default()
            };
            Access::compile(self.id, self.grants, self.groups, caps)
        }
    }

    fn board() -> BoardAccess {
        BoardAccess {
            shared: Shared {
                owner: 1,
                view: Audience {
                    users: vec![2],
                    groups: vec![20],
                },
                use_: Audience::default(),
                edit: Audience {
                    users: vec![3],
                    groups: vec![30],
                },
            },
            view_readers: false,
            edit_updaters: false,
            projects: vec![100, 200],
        }
    }

    #[test]
    fn brd_012_view_audiences() {
        let b = board();
        let cases = [
            (Who::new(1).access(), true, "owner"),
            (Who::new(2).access(), true, "view user"),
            (Who::new(9).group(20).access(), true, "view group"),
            (Who::new(3).access(), true, "edit implies view"),
            (
                Who::new(9).group(30).access(),
                true,
                "edit group implies view",
            ),
            (Who::new(9).access(), false, "stranger"),
        ];
        for (who, expected, case) in cases {
            assert_eq!(b.can_view(&who), expected, "{case}");
        }
    }

    #[test]
    fn brd_012_readers_of_every_project() {
        let mut b = board();
        b.view_readers = true;
        let both = Who::new(9)
            .grant(Permission::IssueRead, Scope::Project(100))
            .grant(Permission::IssueRead, Scope::Project(200))
            .access();
        let one = Who::new(9)
            .grant(Permission::IssueRead, Scope::Project(100))
            .access();
        let global = Who::new(9)
            .grant(Permission::IssueRead, Scope::Global)
            .access();
        assert!(b.can_view(&both));
        assert!(!b.can_view(&one), "reads only one board project");
        assert!(b.can_view(&global));
        b.view_readers = false;
        assert!(!b.can_view(&both), "flag off");
        b.view_readers = true;
        b.projects.clear();
        assert!(
            !b.can_view(&global),
            "a board without projects has no readers"
        );
    }

    #[test]
    fn brd_013_edit_rights() {
        let mut b = board();
        assert!(b.can_edit(&Who::new(1).access()), "owner");
        assert!(b.can_edit(&Who::new(3).access()), "edit user");
        assert!(b.can_edit(&Who::new(9).group(30).access()), "edit group");
        assert!(!b.can_edit(&Who::new(2).access()), "viewer only");

        let updater = Who::new(2)
            .grant(Permission::IssueUpdate, Scope::Project(200))
            .access();
        assert!(!b.can_edit(&updater), "flag off");
        b.edit_updaters = true;
        assert!(b.can_edit(&updater), "updates one board project");
        let outsider = Who::new(9)
            .grant(Permission::IssueUpdate, Scope::Project(200))
            .access();
        assert!(!b.can_edit(&outsider), "updaters must view the board first");
    }

    #[test]
    fn brd_013_system_write_on_shared_boards_only() {
        let admin = |id| {
            Who::new(id)
                .grant(Permission::SystemWrite, Scope::Global)
                .access()
        };
        let b = board();
        assert!(!b.can_edit(&admin(9)), "a board the admin does not see");
        assert!(b.can_edit(&admin(2)), "shared board the admin sees");
        let private = BoardAccess {
            shared: Shared::private(1),
            projects: vec![100],
            ..BoardAccess::default()
        };
        assert!(private.is_private());
        assert!(!private.can_edit(&admin(9)));
        assert!(private.may_delete(&admin(9)), "delete reaches every board");
    }

    #[test]
    fn brd_013_share_needs_edit_and_capability() {
        let b = board();
        assert_eq!(
            b.may_share(&Who::new(3).access()),
            Err(Refusal::NoCapability)
        );
        assert_eq!(b.may_share(&Who::new(3).sharer().access()), Ok(()));
        assert_eq!(
            b.may_share(&Who::new(2).sharer().access()),
            Err(Refusal::NotEditor)
        );
    }

    #[test]
    fn brd_011_transfer_by_editors() {
        let b = board();
        assert_eq!(b.may_transfer(&Who::new(1).access()), Ok(()));
        assert_eq!(b.may_transfer(&Who::new(3).access()), Ok(()));
        assert_eq!(
            b.may_transfer(&Who::new(2).access()),
            Err(Refusal::NotEditor)
        );
        // After the transfer the previous owner keeps only what the lists give.
        let mut after = b.clone();
        after.shared.owner = 3;
        assert!(!after.can_view(&Who::new(1).access()));
        after.shared.view.users.push(1);
        assert!(after.can_view(&Who::new(1).access()));
    }

    #[test]
    fn brd_016_delete_by_owner_or_admin() {
        let b = board();
        assert!(b.may_delete(&Who::new(1).access()));
        assert!(
            !b.may_delete(&Who::new(3).access()),
            "editors do not delete"
        );
    }

    #[test]
    fn brd_010_create() {
        let reader = Who::new(5)
            .grant(Permission::IssueRead, Scope::Project(100))
            .access();
        assert!(may_create(&reader, false, &[100]));
        assert!(!may_create(&reader, false, &[100, 200]));
        assert!(!may_create(&reader, true, &[100]), "guest");
        assert!(!may_create(&reader, false, &[]));
    }
}
