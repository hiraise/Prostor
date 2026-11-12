//! Comments (CNT-001…CNT-005): their text, who may write, change, delete and restore them, and who
//! sees them. Seeing a comment needs seeing its issue first (the caller checks that) and then the
//! comment rule of `access::visibility`, so a comment's audience only narrows (CNT-003, ACC-049).

use crate::access::permission::Permission;
use crate::access::visibility::{self, Audience, Item, Kind};
use crate::access::{Access, Denied, Scope};
use crate::validation::{Problem, Rejection};
use crate::{ProjectId, UserId};

/// Largest comment, in bytes of Markdown (64 KiB).
pub const TEXT_MAX: usize = 65_536;

/// Validates a comment's text (CNT-001): Markdown up to the limit, trailing white space removed.
/// It may be empty only when files come with it (step 1.4b).
pub fn text(raw: &str, has_files: bool) -> Result<String, Rejection> {
    let text = raw.trim_end();
    if text.trim_start().is_empty() && !has_files {
        return Err(Rejection::new(Problem::Required, "the comment is empty"));
    }
    if text.len() > TEXT_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("a comment is larger than {TEXT_MAX} bytes"),
        ));
    }
    Ok(text.to_owned())
}

/// What the rules need to know about a comment.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub project: ProjectId,
    pub author: UserId,
    pub audience: Audience<'a>,
}

/// Why a new comment is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Denied(Denied),
    /// Drafts take no comments (CNT-001).
    Draft,
}

/// Whether the viewer sees a comment of an issue they see (CNT-002, CNT-003, ACC-047):
/// `comment.read`, or being its author with `comment.create`; then the audience.
pub fn can_see(viewer: &Access, comment: &Facts<'_>) -> bool {
    visibility::can_see(
        viewer,
        Kind::Comment,
        Item {
            project: comment.project,
            author: Some(comment.author),
            audience: comment.audience,
        },
    )
}

/// Commenting on an issue the viewer sees (CNT-001, CNT-002): `comment.create`; not on drafts.
pub fn may_comment(viewer: &Access, project: ProjectId, draft: bool) -> Result<(), Refusal> {
    if draft {
        return Err(Refusal::Draft);
    }
    viewer
        .require_at(Permission::CommentCreate, Scope::Project(project))
        .map_err(Refusal::Denied)
}

fn own_or_any(
    viewer: &Access,
    comment: &Facts<'_>,
    own: Permission,
    any: Permission,
) -> Result<(), Denied> {
    let permission = if comment.author == viewer.user() {
        own
    } else {
        any
    };
    viewer.require_at(permission, Scope::Project(comment.project))
}

/// Changing a comment's text or audience (CNT-004): own ones with `comment.update.own`, others'
/// with `comment.update.any`.
pub fn may_update(viewer: &Access, comment: &Facts<'_>) -> Result<(), Denied> {
    own_or_any(
        viewer,
        comment,
        Permission::CommentUpdateOwn,
        Permission::CommentUpdateAny,
    )
}

/// Deleting a comment (CNT-005): own ones with `comment.delete.own`, others' with
/// `comment.delete.any`.
pub fn may_delete(viewer: &Access, comment: &Facts<'_>) -> Result<(), Denied> {
    own_or_any(
        viewer,
        comment,
        Permission::CommentDeleteOwn,
        Permission::CommentDeleteAny,
    )
}

/// Restoring a deleted comment (CNT-005): whoever could delete it (D-013).
pub fn may_restore(viewer: &Access, comment: &Facts<'_>) -> Result<(), Denied> {
    may_delete(viewer, comment)
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

    fn comment(author: UserId) -> Facts<'static> {
        Facts {
            project: 1,
            author,
            audience: Audience::default(),
        }
    }

    fn set(permissions: &[Permission]) -> PermissionSet {
        permissions.iter().copied().collect()
    }

    #[test]
    fn cnt_001_texts() {
        assert_eq!(text("hello \n\n", false).unwrap(), "hello");
        assert_eq!(text(" \n ", false).unwrap_err().problem, Problem::Required);
        assert_eq!(text(" \n ", true).unwrap(), "");
        assert!(text(&"x".repeat(TEXT_MAX), false).is_ok());
        assert_eq!(
            text(&"x".repeat(TEXT_MAX + 1), false).unwrap_err().problem,
            Problem::TooLong
        );
    }

    #[test]
    fn cnt_002_each_permission() {
        let writer = person(5, set(&[Permission::CommentCreate]));
        assert!(may_comment(&writer, 1, false).is_ok());
        assert_eq!(may_comment(&writer, 1, true), Err(Refusal::Draft));
        assert!(matches!(
            may_comment(&writer, 2, false),
            Err(Refusal::Denied(_))
        ));
        // Writers see their own comments, not others'.
        assert!(can_see(&writer, &comment(5)));
        assert!(!can_see(&writer, &comment(6)));
        let reader = person(7, set(&[Permission::CommentRead]));
        assert!(can_see(&reader, &comment(5)));
        assert!(may_comment(&reader, 1, false).is_err());

        let own = person(
            5,
            set(&[Permission::CommentUpdateOwn, Permission::CommentDeleteOwn]),
        );
        assert!(may_update(&own, &comment(5)).is_ok());
        assert!(may_update(&own, &comment(6)).is_err());
        assert!(may_delete(&own, &comment(5)).is_ok());
        assert!(may_restore(&own, &comment(5)).is_ok());
        assert!(may_delete(&own, &comment(6)).is_err());
        let any = person(
            5,
            set(&[Permission::CommentUpdateAny, Permission::CommentDeleteAny]),
        );
        // "Any" covers others' comments only; one's own need the "own" permissions.
        assert!(may_update(&any, &comment(6)).is_ok());
        assert!(may_delete(&any, &comment(6)).is_ok());
        assert!(may_update(&any, &comment(5)).is_err());
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        assert!(may_update(&member, &comment(7)).is_ok());
    }

    #[test]
    fn cnt_003_audiences_narrow() {
        let member = person(7, PredefinedRole::ProjectMember.permissions());
        let restricted = Facts {
            audience: Audience {
                users: &[9],
                groups: &[],
            },
            ..comment(5)
        };
        assert!(!can_see(&member, &restricted));
        assert!(can_see(
            &person(9, set(&[Permission::CommentRead])),
            &restricted
        ));
        // The author always sees it.
        assert!(can_see(
            &person(5, set(&[Permission::CommentCreate])),
            &restricted
        ));
        let admin = person(8, PredefinedRole::SystemAdmin.permissions());
        assert!(can_see(&admin, &restricted));
    }
}
