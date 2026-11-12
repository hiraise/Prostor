//! The permission catalog (ACC-034…ACC-039, ACC-044, ACC-046): a fixed, closed list. Each
//! permission is an entity × an operation with a stable key, a scope rule and the permissions it
//! implies. Permissions are granted only inside roles. The catalog grows with the features that
//! need new permissions (work items, articles, organizations and extension content in P2); keys
//! never change.

use std::fmt;

/// One permission. Its [`Permission::key`] is a public identifier (API, roles, automation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Permission {
    // System administration (ACC-035).
    SystemRead,
    SystemWrite,
    // Users and profiles (ACC-036).
    UserDirectory,
    UserDetails,
    UserSelf,
    UserUpdate,
    UserCreate,
    UserDelete,
    // Projects (ACC-037).
    ProjectCreate,
    ProjectBasic,
    ProjectFull,
    ProjectUpdate,
    ProjectDelete,
    // Issues (ACC-038).
    IssueRead,
    IssueCreate,
    IssueUpdate,
    IssueDelete,
    IssueLink,
    IssuePrivateRead,
    IssuePrivateUpdate,
    IssueWatchersRead,
    IssueWatchersUpdate,
    IssueVotersRead,
    IssueSilent,
    VisibilityBypass,
    // Comments and attachments (ACC-039).
    CommentCreate,
    CommentRead,
    CommentUpdateOwn,
    CommentDeleteOwn,
    CommentUpdateAny,
    CommentDeleteAny,
    AttachmentCreate,
    AttachmentUpdate,
    AttachmentDelete,
}

/// What a permission is about; the role editor groups by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Entity {
    System,
    User,
    Project,
    Issue,
    Comment,
    Attachment,
    Visibility,
}

impl Entity {
    pub fn key(self) -> &'static str {
        match self {
            Entity::System => "system",
            Entity::User => "user",
            Entity::Project => "project",
            Entity::Issue => "issue",
            Entity::Comment => "comment",
            Entity::Attachment => "attachment",
            Entity::Visibility => "visibility",
        }
    }
}

/// What a permission lets one do; the role editor can group by it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Operation {
    Read,
    Create,
    Update,
    Delete,
    Link,
}

impl Operation {
    pub fn key(self) -> &'static str {
        match self {
            Operation::Read => "read",
            Operation::Create => "create",
            Operation::Update => "update",
            Operation::Delete => "delete",
            Operation::Link => "link",
        }
    }
}

/// Facts about one permission.
struct Info {
    key: &'static str,
    entity: Entity,
    operation: Operation,
    /// Works only when granted at global scope (ACC-044).
    global: bool,
    /// Directly implied permissions (ACC-046); the closure is computed.
    implies: &'static [Permission],
}

impl Permission {
    /// Every permission, in catalog order.
    pub const ALL: [Permission; 34] = [
        Permission::SystemRead,
        Permission::SystemWrite,
        Permission::UserDirectory,
        Permission::UserDetails,
        Permission::UserSelf,
        Permission::UserUpdate,
        Permission::UserCreate,
        Permission::UserDelete,
        Permission::ProjectCreate,
        Permission::ProjectBasic,
        Permission::ProjectFull,
        Permission::ProjectUpdate,
        Permission::ProjectDelete,
        Permission::IssueRead,
        Permission::IssueCreate,
        Permission::IssueUpdate,
        Permission::IssueDelete,
        Permission::IssueLink,
        Permission::IssuePrivateRead,
        Permission::IssuePrivateUpdate,
        Permission::IssueWatchersRead,
        Permission::IssueWatchersUpdate,
        Permission::IssueVotersRead,
        Permission::IssueSilent,
        Permission::VisibilityBypass,
        Permission::CommentCreate,
        Permission::CommentRead,
        Permission::CommentUpdateOwn,
        Permission::CommentDeleteOwn,
        Permission::CommentUpdateAny,
        Permission::CommentDeleteAny,
        Permission::AttachmentCreate,
        Permission::AttachmentUpdate,
        Permission::AttachmentDelete,
    ];

    fn info(self) -> Info {
        use Entity as E;
        use Operation as O;
        use Permission as P;
        let (key, entity, operation, global, implies): (_, _, _, _, &'static [Permission]) =
            match self {
                P::SystemRead => ("system.read", E::System, O::Read, true, &[]),
                P::SystemWrite => ("system.write", E::System, O::Update, true, &[P::SystemRead]),
                P::UserDirectory => ("user.directory", E::User, O::Read, true, &[]),
                P::UserDetails => ("user.details", E::User, O::Read, true, &[P::UserDirectory]),
                P::UserSelf => ("user.self", E::User, O::Update, true, &[]),
                P::UserUpdate => (
                    "user.update",
                    E::User,
                    O::Update,
                    true,
                    &[P::UserSelf, P::UserDetails],
                ),
                P::UserCreate => ("user.create", E::User, O::Create, true, &[]),
                P::UserDelete => ("user.delete", E::User, O::Delete, true, &[P::UserDetails]),
                P::ProjectCreate => ("project.create", E::Project, O::Create, true, &[]),
                P::ProjectBasic => ("project.basic", E::Project, O::Read, false, &[]),
                P::ProjectFull => (
                    "project.full",
                    E::Project,
                    O::Read,
                    false,
                    &[P::ProjectBasic],
                ),
                P::ProjectUpdate => (
                    "project.update",
                    E::Project,
                    O::Update,
                    false,
                    &[P::ProjectFull],
                ),
                P::ProjectDelete => (
                    "project.delete",
                    E::Project,
                    O::Delete,
                    false,
                    &[P::ProjectFull],
                ),
                P::IssueRead => ("issue.read", E::Issue, O::Read, false, &[P::ProjectBasic]),
                P::IssueCreate => (
                    "issue.create",
                    E::Issue,
                    O::Create,
                    false,
                    &[P::ProjectBasic],
                ),
                P::IssueUpdate => ("issue.update", E::Issue, O::Update, false, &[]),
                P::IssueDelete => ("issue.delete", E::Issue, O::Delete, false, &[]),
                P::IssueLink => ("issue.link", E::Issue, O::Link, false, &[]),
                P::IssuePrivateRead => (
                    "issue.private.read",
                    E::Issue,
                    O::Read,
                    false,
                    &[P::ProjectBasic],
                ),
                P::IssuePrivateUpdate => (
                    "issue.private.update",
                    E::Issue,
                    O::Update,
                    false,
                    &[P::IssuePrivateRead, P::IssueUpdate],
                ),
                P::IssueWatchersRead => (
                    "issue.watchers.read",
                    E::Issue,
                    O::Read,
                    false,
                    &[P::ProjectBasic],
                ),
                P::IssueWatchersUpdate => {
                    ("issue.watchers.update", E::Issue, O::Update, false, &[])
                }
                P::IssueVotersRead => (
                    "issue.voters.read",
                    E::Issue,
                    O::Read,
                    false,
                    &[P::ProjectBasic],
                ),
                P::IssueSilent => ("issue.silent", E::Issue, O::Update, false, &[]),
                P::VisibilityBypass => (
                    "visibility.bypass",
                    E::Visibility,
                    O::Read,
                    false,
                    &[P::IssuePrivateRead],
                ),
                P::CommentCreate => ("comment.create", E::Comment, O::Create, false, &[]),
                P::CommentRead => ("comment.read", E::Comment, O::Read, false, &[]),
                P::CommentUpdateOwn => ("comment.update.own", E::Comment, O::Update, false, &[]),
                P::CommentDeleteOwn => ("comment.delete.own", E::Comment, O::Delete, false, &[]),
                P::CommentUpdateAny => (
                    "comment.update.any",
                    E::Comment,
                    O::Update,
                    false,
                    &[P::CommentRead],
                ),
                P::CommentDeleteAny => (
                    "comment.delete.any",
                    E::Comment,
                    O::Delete,
                    false,
                    &[P::CommentRead],
                ),
                P::AttachmentCreate => ("attachment.create", E::Attachment, O::Create, false, &[]),
                P::AttachmentUpdate => ("attachment.update", E::Attachment, O::Update, false, &[]),
                P::AttachmentDelete => ("attachment.delete", E::Attachment, O::Delete, false, &[]),
            };
        Info {
            key,
            entity,
            operation,
            global,
            implies,
        }
    }

    /// Stable key, such as `issue.read`.
    pub fn key(self) -> &'static str {
        self.info().key
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.key() == key)
    }

    pub fn entity(self) -> Entity {
        self.info().entity
    }

    pub fn operation(self) -> Operation {
        self.info().operation
    }

    /// Works only when granted at global scope; ignored in a role assigned to a project (ACC-044).
    pub fn global_only(self) -> bool {
        self.info().global
    }

    /// Everything this permission implies, directly or through others; without itself.
    pub fn implies(self) -> PermissionSet {
        let mut set = PermissionSet::of(self).closure();
        set.remove(self);
        set
    }

    /// Everything that implies this permission, directly or through others; switching this one off
    /// in a role switches those off too (ACC-046).
    pub fn dependents(self) -> PermissionSet {
        Self::ALL
            .into_iter()
            .filter(|other| *other != self && other.implies().contains(self))
            .collect()
    }

    fn bit(self) -> u64 {
        1 << (self as u8)
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// A set of permissions, small enough to copy (one bit each).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PermissionSet(u64);

impl PermissionSet {
    pub const EMPTY: PermissionSet = PermissionSet(0);

    pub fn of(permission: Permission) -> Self {
        Self(permission.bit())
    }

    /// Every permission of the catalog.
    pub fn all() -> Self {
        Permission::ALL.into_iter().collect()
    }

    /// Permissions that work in a project: all but the global-only ones.
    pub fn project_scoped() -> Self {
        Permission::ALL
            .into_iter()
            .filter(|permission| !permission.global_only())
            .collect()
    }

    /// Reads stored keys; unknown keys (from a newer version) are ignored.
    pub fn from_keys<S: AsRef<str>>(keys: &[S]) -> Self {
        keys.iter()
            .filter_map(|key| Permission::parse(key.as_ref()))
            .collect()
    }

    pub fn contains(self, permission: Permission) -> bool {
        self.0 & permission.bit() != 0
    }

    pub fn insert(&mut self, permission: Permission) {
        self.0 |= permission.bit();
    }

    pub fn remove(&mut self, permission: Permission) {
        self.0 &= !permission.bit();
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn is_subset(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The set with everything its members imply (ACC-046).
    #[must_use]
    pub fn closure(self) -> Self {
        let mut set = self;
        loop {
            let mut next = set;
            for permission in set.iter() {
                for implied in permission.info().implies {
                    next.insert(*implied);
                }
            }
            if next == set {
                return set;
            }
            set = next;
        }
    }

    /// Members in catalog order.
    pub fn iter(self) -> impl Iterator<Item = Permission> {
        Permission::ALL
            .into_iter()
            .filter(move |permission| self.contains(*permission))
    }

    pub fn keys(self) -> Vec<&'static str> {
        self.iter().map(Permission::key).collect()
    }
}

impl FromIterator<Permission> for PermissionSet {
    fn from_iter<I: IntoIterator<Item = Permission>>(iter: I) -> Self {
        let mut set = Self::EMPTY;
        for permission in iter {
            set.insert(permission);
        }
        set
    }
}

impl fmt::Debug for PermissionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.keys()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_034_the_catalog_has_unique_stable_keys() {
        let keys: std::collections::HashSet<_> = Permission::ALL.iter().map(|p| p.key()).collect();
        assert_eq!(keys.len(), Permission::ALL.len());
        for permission in Permission::ALL {
            assert_eq!(Permission::parse(permission.key()), Some(permission));
            assert!(
                permission.key().starts_with(permission.entity().key()),
                "{permission}"
            );
        }
        assert_eq!(Permission::parse("issue.everything"), None);
        // Bits fit the set.
        assert!(Permission::ALL.len() <= 64);
        assert_eq!(PermissionSet::all().len(), Permission::ALL.len());
    }

    #[test]
    fn acc_035_acc_036_acc_037_acc_038_acc_039_the_catalog_by_area() {
        let by = |entity: Entity| -> Vec<&str> {
            Permission::ALL
                .into_iter()
                .filter(|p| p.entity() == entity)
                .map(Permission::key)
                .collect()
        };
        assert_eq!(by(Entity::System), ["system.read", "system.write"]);
        assert_eq!(
            by(Entity::User),
            [
                "user.directory",
                "user.details",
                "user.self",
                "user.update",
                "user.create",
                "user.delete"
            ]
        );
        assert_eq!(
            by(Entity::Project),
            [
                "project.create",
                "project.basic",
                "project.full",
                "project.update",
                "project.delete"
            ]
        );
        assert_eq!(by(Entity::Issue).len(), 11);
        assert_eq!(by(Entity::Visibility), ["visibility.bypass"]);
        assert_eq!(by(Entity::Comment).len(), 6);
        assert_eq!(
            by(Entity::Attachment),
            [
                "attachment.create",
                "attachment.update",
                "attachment.delete"
            ]
        );
        // ACC-039: "own" and "any" are separate keys; deleting any comment includes deleting
        // comments for good.
        assert!(
            Permission::CommentDeleteAny
                .implies()
                .contains(Permission::CommentRead)
        );
        assert!(Permission::CommentDeleteOwn.implies().is_empty());
    }

    #[test]
    fn acc_044_global_only_permissions() {
        let global: Vec<_> = Permission::ALL
            .into_iter()
            .filter(|p| p.global_only())
            .map(Permission::key)
            .collect();
        assert_eq!(
            global,
            [
                "system.read",
                "system.write",
                "user.directory",
                "user.details",
                "user.self",
                "user.update",
                "user.create",
                "user.delete",
                "project.create",
            ]
        );
        assert!(!PermissionSet::project_scoped().contains(Permission::UserDirectory));
        assert!(PermissionSet::project_scoped().contains(Permission::ProjectBasic));
    }

    #[test]
    fn acc_046_implications_are_transitive() {
        assert_eq!(
            Permission::UserUpdate.implies().keys(),
            ["user.directory", "user.details", "user.self"]
        );
        assert_eq!(
            Permission::ProjectUpdate.implies().keys(),
            ["project.basic", "project.full"]
        );
        assert_eq!(
            Permission::VisibilityBypass.implies().keys(),
            ["project.basic", "issue.private.read"]
        );
        assert_eq!(
            Permission::IssuePrivateUpdate.implies().keys(),
            ["project.basic", "issue.update", "issue.private.read"]
        );
        // The submit-only pattern (ACC-048) needs link without read.
        assert!(
            !Permission::IssueLink
                .implies()
                .contains(Permission::IssueRead)
        );
    }

    #[test]
    fn acc_046_dependents_are_the_reverse() {
        assert_eq!(
            Permission::ProjectBasic.dependents().keys(),
            [
                "project.full",
                "project.update",
                "project.delete",
                "issue.read",
                "issue.create",
                "issue.private.read",
                "issue.private.update",
                "issue.watchers.read",
                "issue.voters.read",
                "visibility.bypass",
            ]
        );
        assert_eq!(
            Permission::CommentRead.dependents().keys(),
            ["comment.update.any", "comment.delete.any"]
        );
        assert!(Permission::IssueSilent.dependents().is_empty());
    }

    #[test]
    fn sets() {
        let mut set = PermissionSet::of(Permission::IssueRead);
        set.insert(Permission::CommentRead);
        assert!(set.contains(Permission::IssueRead));
        assert_eq!(set.len(), 2);
        assert_eq!(
            set.closure().keys(),
            ["project.basic", "issue.read", "comment.read"]
        );
        assert!(PermissionSet::of(Permission::IssueRead).is_subset(set));
        assert!(!set.is_subset(PermissionSet::of(Permission::IssueRead)));
        assert_eq!(
            PermissionSet::from_keys(&["issue.read", "nope", "comment.read"]),
            set
        );
        assert_eq!(format!("{set:?}"), r#"{"issue.read", "comment.read"}"#);
        set.remove(Permission::IssueRead);
        assert_eq!(set.keys(), ["comment.read"]);
    }
}
