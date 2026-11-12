//! What one person may learn about another (ACC-015, ACC-016): profile data comes in tiers, and
//! without the directory permission other people appear under pseudonyms.

use super::Access;
use super::permission::Permission;
use crate::UserId;

/// How much of a profile a viewer sees (ACC-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Not even that the account exists: lookups answer "not found".
    Hidden,
    /// ID, username, full name, avatar (`user.directory`).
    Basic,
    /// Also email, groups and roles (`user.details`). Never personal settings or security.
    Details,
    /// Everything, personal settings and security included: the owner, or `user.update`.
    Full,
}

/// The tier of `target`'s profile for `viewer`.
pub fn tier(viewer: &Access, target: UserId) -> Tier {
    if viewer.user() == target || viewer.has(Permission::UserUpdate) {
        Tier::Full
    } else if viewer.has(Permission::UserDetails) {
        Tier::Details
    } else if viewer.has(Permission::UserDirectory) {
        Tier::Basic
    } else {
        Tier::Hidden
    }
}

/// Whether the viewer sees `target`'s real name wherever it appears (authors, assignees,
/// pickers); otherwise a pseudonym (ACC-016).
pub fn sees_name(viewer: &Access, target: UserId) -> bool {
    tier(viewer, target) >= Tier::Basic
}

/// The pseudonym number of a person for one viewer, from a keyed hash of both IDs: stable for
/// that viewer, so a conversation stays readable, and unrelated between viewers.
pub fn pseudonym(digest: [u8; 4]) -> u32 {
    // Six digits: a collision between two people one viewer sees is unlikely.
    100_000 + u32::from_be_bytes(digest) % 900_000
}

#[cfg(test)]
mod tests {
    use super::super::capability::CapabilitySet;
    use super::super::role::PredefinedRole;
    use super::super::{Grant, Scope};
    use super::*;

    fn with(role: PredefinedRole) -> Access {
        Access::compile(
            1,
            [Grant {
                permissions: role.permissions(),
                scope: Scope::Global,
            }],
            vec![],
            CapabilitySet::default(),
        )
    }

    #[test]
    fn acc_015_profile_tiers() {
        let nobody = Access::none(1);
        assert_eq!(tier(&nobody, 1), Tier::Full);
        assert_eq!(tier(&nobody, 2), Tier::Hidden);
        assert_eq!(
            tier(&with(PredefinedRole::DirectoryViewer), 2),
            Tier::Details
        );
        assert_eq!(tier(&with(PredefinedRole::SystemAdmin), 2), Tier::Full);
        let basic = Access::compile(
            1,
            [Grant {
                permissions: crate::access::permission::PermissionSet::of(
                    Permission::UserDirectory,
                ),
                scope: Scope::Global,
            }],
            vec![],
            CapabilitySet::default(),
        );
        assert_eq!(tier(&basic, 2), Tier::Basic);
    }

    #[test]
    fn acc_016_names_are_masked_without_the_directory() {
        assert!(!sees_name(&Access::none(1), 2));
        assert!(sees_name(&Access::none(1), 1));
        assert!(sees_name(&with(PredefinedRole::DirectoryViewer), 2));
        assert_eq!(pseudonym([0, 0, 0, 0]), 100_000);
        assert!((100_000..1_000_000).contains(&pseudonym([255, 255, 255, 255])));
    }
}
