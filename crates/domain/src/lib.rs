//! Domain model and rules. No HTTP or SQL types here: the store and the server adapt to this crate
//! (conventions.md §2).

pub mod access;
pub mod accounts;
pub mod attachments;
pub mod auth;
pub mod boards;
pub mod comments;
pub mod duration;
pub mod fields;
pub mod files;
pub mod issues;
pub mod links;
pub mod live;
pub mod notifications;
pub mod preferences;
pub mod projects;
pub mod rank;
pub mod rules;
pub mod search;
pub mod secret;
pub mod settings;
pub mod sharing;
pub mod text;
pub mod validation;

/// Internal user ID.
pub type UserId = i64;
/// Internal group ID.
pub type GroupId = i64;
/// Internal role ID.
pub type RoleId = i64;
/// Internal project ID.
pub type ProjectId = i64;
/// Internal field ID.
pub type FieldId = i64;
/// Internal value set ID.
pub type SetId = i64;
/// Internal ID of a value in a set.
pub type ValueId = i64;
/// Internal issue ID; never changes, whatever happens to the readable ID.
pub type IssueId = i64;
/// Internal tag ID.
pub type TagId = i64;
/// Internal ID of a saved search.
pub type SavedSearchId = i64;
/// Internal ID of a notification subscription.
pub type SubscriptionId = i64;
/// Internal comment ID.
pub type CommentId = i64;
/// Internal attachment ID.
pub type AttachmentId = i64;
/// Internal link type ID.
pub type LinkTypeId = i64;
/// Internal link ID.
pub type LinkId = i64;
/// Internal ID of a staged upload.
pub type UploadId = i64;
/// Internal board ID.
pub type BoardId = i64;
/// Internal sprint ID; every board has at least one sprint row (D-044).
pub type SprintId = i64;
/// Internal ID of a board column.
pub type ColumnId = i64;

/// Who makes a change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Actor {
    /// The server itself: setup, migrations, scheduled work.
    #[default]
    System,
    User(UserId),
}

impl Actor {
    /// The user behind the change; `None` for the system.
    pub fn user_id(self) -> Option<UserId> {
        match self {
            Actor::System => None,
            Actor::User(id) => Some(id),
        }
    }
}
