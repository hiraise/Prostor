//! Personal interface settings (UI-018): `GET` and `PATCH /users/me/preferences`. Every key is
//! optional: an absent key means its default, which the client knows. In a patch `null` resets a
//! key; unknown keys are ignored.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HomePage {
    Issues,
    Projects,
    Boards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PanelSide {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityOrder {
    OldestFirst,
    NewestFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LongComments {
    Full,
    Collapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListSidebarMode {
    Open,
    Auto,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SidebarSection {
    Drafts,
    Projects,
    Tags,
    Searches,
}

/// What came of the prompt after a first bulk apply without silent (plan 2.1 §6): `silent` —
/// bulk applies start without notifications; `dismissed`/`never` — it asked, the answer was no.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommandSilentPrompt {
    Silent,
    Dismissed,
    Never,
}

/// Declares the keys once for the settings and their patch.
macro_rules! keys {
    ($($(#[doc = $doc:literal])* $key:ident: $ty:ty),* $(,)?) => {
        /// A person's interface settings; an absent key has its default.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
        pub struct Preferences {
            $(
                $(#[doc = $doc])*
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $key: Option<$ty>,
            )*
        }

        /// A change of settings: an absent key stays, `null` resets it to its default.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
        pub struct PreferencesPatch {
            $(
                $(#[doc = $doc])*
                #[serde(
                    default,
                    deserialize_with = "crate::patch::nullable",
                    skip_serializing_if = "Option::is_none"
                )]
                #[schema(value_type = Option<$ty>, nullable)]
                pub $key: Option<Option<$ty>>,
            )*
        }
    };
}

keys! {
    /// The rail is folded to icons (UI-001).
    rail_collapsed: bool,
    /// The expanded rail's width in tenths of a rem, 160…400.
    rail_width: i32,
    /// The Markdown editor shows its toolbar.
    editor_toolbar: bool,
    /// The page after signing in (UI-004).
    home: HomePage,
    /// The side of the issue page's field panel (ISS-020).
    issue_panel_side: PanelSide,
    /// The field panel's width in tenths of a rem, 160…400.
    issue_panel_width: i32,
    issue_panel_collapsed: bool,
    /// The order of the activity stream (HIST-009).
    activity_order: ActivityOrder,
    /// The activity stream shows comments (HIST-008).
    activity_comments: bool,
    /// The activity stream shows changes (HIST-008).
    activity_changes: bool,
    long_comments: LongComments,
    attachments_collapsed: bool,
    links_collapsed: bool,
    /// The issue list hides resolved issues (QRY-054).
    list_hide_resolved: bool,
    /// Columns of the issue list, at most 30: `id`, `summary`, `project`, `reporter`, `created`,
    /// `updated`, `resolved`, `votes`, `tags`, `field:<id>`. Empty means the defaults.
    list_columns: Vec<String>,
    /// The issue list's sidebar (QRY-082).
    list_sidebar: ListSidebarMode,
    /// The sidebar's width in tenths of a rem, 160…400.
    list_sidebar_width: i32,
    /// Folded sections of the sidebar.
    list_sidebar_folded: Vec<SidebarSection>,
    /// Issues the person opened lately, newest first, at most 20 internal IDs.
    recent_issues: Vec<i64>,
    /// The project of the person's last new issue (ISS-005).
    last_project: i64,
    /// Typing a letter or digit on the issue page opens the command dialog (plan 2.1 §6),
    /// default off.
    commands_on_typing: bool,
    /// The answer of the silent-by-default prompt for bulk applies (plan 2.1 §6); unset = not
    /// asked yet.
    commands_silent_prompt: CommandSilentPrompt,
}
