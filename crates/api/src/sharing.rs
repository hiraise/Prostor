//! Sharing of tags and saved searches (ISS-037, QRY-065), gated by the share-views capability
//! (ACC-066): who views, uses and edits what one person owns.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::access::GroupRef;
use crate::accounts::UserRef;

/// People and groups an item is shared with, as a change sends them. The owner is never listed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SharedWithInput {
    #[serde(default)]
    pub users: Vec<i64>,
    #[serde(default)]
    pub groups: Vec<i64>,
}

/// People and groups an item is shared with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SharedWith {
    pub users: Vec<UserRef>,
    pub groups: Vec<GroupRef>,
}

/// What the caller may do with a shared item; the UI hides the other controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SharingAbilities {
    /// Put it on issues and take it off (tags, ISS-035); always `false` for saved searches.
    pub apply: bool,
    /// Change the content: name, color, query.
    pub edit: bool,
    pub delete: bool,
    /// Change who it is shared with.
    pub share: bool,
    /// Give it to another owner.
    pub transfer: bool,
}

/// Whose items a listing shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingScope {
    /// The caller's own.
    #[default]
    Mine,
    /// Others' items shared with the caller.
    Shared,
    /// Both.
    All,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ScopeQuery {
    /// `mine` by default.
    #[param(inline)]
    pub scope: Option<ListingScope>,
}
