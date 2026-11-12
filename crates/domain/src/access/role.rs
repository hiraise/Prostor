//! Roles (ACC-052…ACC-057): named sets of permissions. Six predefined roles are fixed in code and
//! cannot be edited or deleted; custom roles keep their permissions in the database.

use super::permission::{Permission, PermissionSet};
use crate::accounts::plain_text;
use crate::validation::{Problem, Rejection};

/// Longest role name.
pub const ROLE_NAME_MAX: usize = 100;
/// Longest role description.
pub const ROLE_DESCRIPTION_MAX: usize = 500;

/// A role that comes with every installation (ACC-052).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PredefinedRole {
    /// Every permission at every scope.
    SystemAdmin,
    /// Every project permission except deleting projects and seeing restricted items.
    ProjectAdmin,
    /// Daily work with issues, comments and attachments; the default team role.
    ProjectMember,
    /// Sees other people and edits one's own profile; held by All members.
    DirectoryViewer,
    ProjectCreator,
    AccountCreator,
}

impl PredefinedRole {
    pub const ALL: [PredefinedRole; 6] = [
        PredefinedRole::SystemAdmin,
        PredefinedRole::ProjectAdmin,
        PredefinedRole::ProjectMember,
        PredefinedRole::DirectoryViewer,
        PredefinedRole::ProjectCreator,
        PredefinedRole::AccountCreator,
    ];

    /// Stable key, stored with the role's row.
    pub fn key(self) -> &'static str {
        match self {
            PredefinedRole::SystemAdmin => "system_admin",
            PredefinedRole::ProjectAdmin => "project_admin",
            PredefinedRole::ProjectMember => "project_member",
            PredefinedRole::DirectoryViewer => "directory_viewer",
            PredefinedRole::ProjectCreator => "project_creator",
            PredefinedRole::AccountCreator => "account_creator",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.key() == key)
    }

    /// Names in the languages trackflow speaks (English first), for display in the instance
    /// language and so nobody names a custom role the same.
    pub fn names(self) -> [&'static str; 2] {
        match self {
            PredefinedRole::SystemAdmin => ["System administrator", "Системный администратор"],
            PredefinedRole::ProjectAdmin => ["Project administrator", "Администратор проекта"],
            PredefinedRole::ProjectMember => ["Project member", "Участник проекта"],
            PredefinedRole::DirectoryViewer => ["Directory viewer", "Просмотр справочника"],
            PredefinedRole::ProjectCreator => ["Project creator", "Создатель проектов"],
            PredefinedRole::AccountCreator => ["Account creator", "Создатель учётных записей"],
        }
    }

    pub fn permissions(self) -> PermissionSet {
        use Permission as P;
        match self {
            PredefinedRole::SystemAdmin => PermissionSet::all(),
            PredefinedRole::ProjectAdmin => {
                let mut set = PermissionSet::project_scoped();
                set.remove(P::ProjectDelete);
                set.remove(P::VisibilityBypass);
                set
            }
            PredefinedRole::ProjectMember => [
                P::ProjectBasic,
                P::IssueRead,
                P::IssueCreate,
                P::IssueUpdate,
                P::IssueDelete,
                P::IssueLink,
                P::IssuePrivateRead,
                P::IssuePrivateUpdate,
                P::IssueWatchersRead,
                P::IssueWatchersUpdate,
                P::IssueVotersRead,
                P::AttachmentCreate,
                P::AttachmentUpdate,
                P::AttachmentDelete,
                P::CommentCreate,
                P::CommentRead,
                P::CommentUpdateOwn,
                P::CommentDeleteOwn,
            ]
            .into_iter()
            .collect(),
            PredefinedRole::DirectoryViewer => [P::UserDirectory, P::UserDetails, P::UserSelf]
                .into_iter()
                .collect(),
            PredefinedRole::ProjectCreator => PermissionSet::of(P::ProjectCreate),
            PredefinedRole::AccountCreator => PermissionSet::of(P::UserCreate),
        }
    }
}

/// The permissions of a role: fixed in code for predefined roles, stored for custom ones.
pub fn role_permissions(builtin: Option<&str>, stored: &[String]) -> PermissionSet {
    match builtin.and_then(PredefinedRole::parse) {
        Some(role) => role.permissions(),
        None => PermissionSet::from_keys(stored).closure(),
    }
}

/// Validates a custom role's name: a single line, not one of the predefined names in any language.
pub fn role_name(raw: &str) -> Result<String, Rejection> {
    let name = plain_text(raw, ROLE_NAME_MAX, "name")?;
    let lower = name.to_lowercase();
    let reserved = PredefinedRole::ALL
        .iter()
        .flat_map(|role| role.names())
        .any(|taken| taken.to_lowercase() == lower);
    if reserved {
        return Err(Rejection::new(
            Problem::Taken,
            "a predefined role has this name",
        ));
    }
    Ok(name)
}

/// Validates a description; empty is fine.
pub fn description(raw: &str, max: usize) -> Result<String, Rejection> {
    let text = raw.trim();
    if text.chars().count() > max {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("description is longer than {max} characters"),
        ));
    }
    Ok(text.to_owned())
}

/// A role's permissions after switching one on or off (ACC-046): on adds everything it implies,
/// off removes everything that depends on it.
pub fn toggle(current: PermissionSet, permission: Permission, on: bool) -> PermissionSet {
    if on {
        current.union(PermissionSet::of(permission).closure())
    } else {
        let mut gone = permission.dependents();
        gone.insert(permission);
        current.difference(gone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_052_predefined_roles() {
        assert_eq!(
            PredefinedRole::SystemAdmin.permissions().len(),
            Permission::ALL.len()
        );
        let admin = PredefinedRole::ProjectAdmin.permissions();
        assert_eq!(admin.len(), 23);
        assert!(!admin.contains(Permission::ProjectDelete));
        assert!(!admin.contains(Permission::VisibilityBypass));
        assert!(!admin.contains(Permission::UserDirectory));
        assert_eq!(PredefinedRole::ProjectMember.permissions().len(), 18);
        assert_eq!(
            PredefinedRole::DirectoryViewer.permissions().keys(),
            ["user.directory", "user.details", "user.self"]
        );
        // Every predefined set is closed under implication.
        for role in PredefinedRole::ALL {
            assert_eq!(role.permissions(), role.permissions().closure(), "{role:?}");
            assert_eq!(PredefinedRole::parse(role.key()), Some(role));
        }
    }

    #[test]
    fn acc_054_custom_names_cannot_copy_predefined_ones() {
        assert_eq!(role_name(" Reviewer ").unwrap(), "Reviewer");
        assert_eq!(
            role_name("project MEMBER").unwrap_err().problem,
            Problem::Taken
        );
        assert_eq!(
            role_name("Участник проекта").unwrap_err().problem,
            Problem::Taken
        );
        assert_eq!(role_name("").unwrap_err().problem, Problem::Required);
        assert_eq!(
            description(&"x".repeat(501), ROLE_DESCRIPTION_MAX)
                .unwrap_err()
                .problem,
            Problem::TooLong
        );
    }

    #[test]
    fn acc_046_toggling_follows_implications() {
        let on = toggle(PermissionSet::EMPTY, Permission::IssueRead, true);
        assert_eq!(on.keys(), ["project.basic", "issue.read"]);
        let more = toggle(on, Permission::CommentUpdateAny, true);
        assert_eq!(
            more.keys(),
            [
                "project.basic",
                "issue.read",
                "comment.read",
                "comment.update.any"
            ]
        );
        // Switching project.basic off removes what depends on it, and nothing else.
        let off = toggle(more, Permission::ProjectBasic, false);
        assert_eq!(off.keys(), ["comment.read", "comment.update.any"]);
        assert_eq!(
            role_permissions(None, &["issue.read".to_owned()]).keys(),
            ["project.basic", "issue.read"]
        );
        assert_eq!(
            role_permissions(Some("account_creator"), &[]).keys(),
            ["user.create"]
        );
    }
}
