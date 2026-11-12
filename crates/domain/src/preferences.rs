//! Personal interface settings that follow a person across devices (UI-018, D-041): the rail and
//! the editor toolbar, the issue page's panel and activity, the list's sidebar and columns, recent
//! issues and the last project. Every key is optional; an absent key means its default, which the
//! web client knows. Stored are only keys that passed [`validate`].

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::validation::{FieldProblem, Problem, Problems, Rejection};

/// Most bytes of the stored object, serialized.
pub const SIZE_MAX: usize = 16 * 1024;
/// Widths of side panels, in tenths of a rem.
pub const WIDTH_RANGE: RangeInclusive<i32> = 160..=400;
pub const COLUMNS_MAX: usize = 30;
pub const RECENT_MAX: usize = 20;

/// Column keys of the issue list besides `field:<id>`.
const COLUMNS: [&str; 9] = [
    "id", "summary", "project", "reporter", "created", "updated", "resolved", "votes", "tags",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Home {
    Issues,
    Projects,
    Boards,
}

/// Issues, as the web client already opens today; the instance setting `default_home`
/// (`settings::DEFAULT_HOME`) feeds the fallback and agrees with this default (UI-004).
impl Default for Home {
    fn default() -> Self {
        Home::Issues
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelSide {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityOrder {
    OldestFirst,
    NewestFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LongComments {
    Full,
    Collapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListSidebar {
    Open,
    Auto,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarSection {
    Drafts,
    Projects,
    Tags,
    Searches,
}

/// Declares the keys once: the settings, their patch, and how a patch splits.
macro_rules! keys {
    ($($key:ident: $ty:ty),* $(,)?) => {
        /// A person's settings; `None` is the default.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        pub struct Preferences {
            $(
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $key: Option<$ty>,
            )*
        }

        /// A change: absent keeps a key, `Some(None)` resets it to its default.
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct Patch {
            $(pub $key: Option<Option<$ty>>,)*
        }

        impl Patch {
            fn split(self) -> (Preferences, Vec<&'static str>) {
                let mut set = Preferences::default();
                let mut reset = Vec::new();
                $(
                    match self.$key {
                        Some(Some(value)) => set.$key = Some(value),
                        Some(None) => reset.push(stringify!($key)),
                        None => {}
                    }
                )*
                (set, reset)
            }
        }
    };
}

keys! {
    rail_collapsed: bool,
    editor_toolbar: bool,
    home: Home,
    issue_panel_side: PanelSide,
    issue_panel_width: i32,
    issue_panel_collapsed: bool,
    activity_order: ActivityOrder,
    activity_comments: bool,
    activity_changes: bool,
    long_comments: LongComments,
    attachments_collapsed: bool,
    links_collapsed: bool,
    list_hide_resolved: bool,
    list_columns: Vec<String>,
    list_sidebar: ListSidebar,
    list_sidebar_width: i32,
    list_sidebar_folded: Vec<SidebarSection>,
    recent_issues: Vec<i64>,
    last_project: i64,
}

/// A checked patch: the keys to set (absent ones are `None`) and the keys to reset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valid {
    pub set: Preferences,
    pub reset: Vec<&'static str>,
}

/// Checks a patch: widths in range, list lengths, column keys, positive IDs. Lists lose repeated
/// items, keeping the first.
pub fn validate(patch: Patch) -> Result<Valid, Vec<FieldProblem>> {
    let (mut set, reset) = patch.split();
    let mut problems = Problems::new();
    for (path, width) in [
        ("issue_panel_width", set.issue_panel_width),
        ("list_sidebar_width", set.list_sidebar_width),
    ] {
        if let Some(width) = width
            && !WIDTH_RANGE.contains(&width)
        {
            problems.add(
                path,
                Rejection::new(
                    Problem::OutOfRange,
                    format!("between {} and {}", WIDTH_RANGE.start(), WIDTH_RANGE.end()),
                ),
            );
        }
    }
    if let Some(columns) = &mut set.list_columns {
        dedup(columns);
        if columns.len() > COLUMNS_MAX {
            problems.add(
                "list_columns",
                Rejection::new(Problem::TooLong, format!("at most {COLUMNS_MAX} columns")),
            );
        }
        if columns.iter().any(|c| !is_column(c)) {
            problems.add(
                "list_columns",
                Rejection::new(Problem::Invalid, "unknown column"),
            );
        }
    }
    if let Some(folded) = &mut set.list_sidebar_folded {
        dedup(folded);
    }
    if let Some(recent) = &mut set.recent_issues {
        dedup(recent);
        if recent.len() > RECENT_MAX {
            problems.add(
                "recent_issues",
                Rejection::new(Problem::TooLong, format!("at most {RECENT_MAX} issues")),
            );
        }
        if recent.iter().any(|id| *id <= 0) {
            problems.add(
                "recent_issues",
                Rejection::new(Problem::Invalid, "not an issue ID"),
            );
        }
    }
    if set.last_project.is_some_and(|id| id <= 0) {
        problems.add(
            "last_project",
            Rejection::new(Problem::Invalid, "not a project ID"),
        );
    }
    problems.finish()?;
    Ok(Valid { set, reset })
}

/// Whether the stored object, after a change, still fits.
pub fn check_size(stored: &serde_json::Value) -> Result<(), Rejection> {
    if stored.to_string().len() > SIZE_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("settings take at most {SIZE_MAX} bytes"),
        ));
    }
    Ok(())
}

/// Reads what is stored. A key that no longer reads (an older shape) falls back to its default.
pub fn from_stored(stored: serde_json::Value) -> Preferences {
    let serde_json::Value::Object(map) = stored else {
        return Preferences::default();
    };
    let mut kept = serde_json::Map::new();
    for (key, value) in map {
        let one = serde_json::Value::Object(serde_json::Map::from_iter([(key.clone(), value)]));
        if serde_json::from_value::<Preferences>(one.clone()).is_ok()
            && let serde_json::Value::Object(mut one) = one
            && let Some(value) = one.remove(&key)
        {
            kept.insert(key, value);
        }
    }
    serde_json::from_value(serde_json::Value::Object(kept)).unwrap_or_default()
}

fn is_column(key: &str) -> bool {
    COLUMNS.contains(&key)
        || key.strip_prefix("field:").is_some_and(|id| {
            id.bytes().all(|b| b.is_ascii_digit())
                && !id.starts_with('0')
                && id.parse::<i64>().is_ok()
        })
}

fn dedup<T: PartialEq + Clone>(items: &mut Vec<T>) {
    let mut seen: Vec<T> = Vec::with_capacity(items.len());
    items.retain(|item| {
        let fresh = !seen.contains(item);
        if fresh {
            seen.push(item.clone());
        }
        fresh
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(result: Result<Valid, Vec<FieldProblem>>) -> Vec<(String, Problem)> {
        result
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.path, p.problem))
            .collect()
    }

    #[test]
    fn ui_018_patch_splits_into_set_and_reset() {
        let valid = validate(Patch {
            home: Some(Some(Home::Projects)),
            rail_collapsed: Some(None),
            ..Patch::default()
        })
        .ok();
        assert_eq!(
            valid,
            Some(Valid {
                set: Preferences {
                    home: Some(Home::Projects),
                    ..Preferences::default()
                },
                reset: vec!["rail_collapsed"],
            })
        );
    }

    #[test]
    fn ui_018_rejects_out_of_range_and_bad_lists() {
        let found = paths(validate(Patch {
            issue_panel_width: Some(Some(159)),
            list_sidebar_width: Some(Some(401)),
            list_columns: Some(Some(vec!["id".into(), "field:x".into()])),
            recent_issues: Some(Some((1..=21).collect())),
            last_project: Some(Some(0)),
            ..Patch::default()
        }));
        assert_eq!(
            found,
            [
                ("issue_panel_width".to_owned(), Problem::OutOfRange),
                ("list_sidebar_width".to_owned(), Problem::OutOfRange),
                ("list_columns".to_owned(), Problem::Invalid),
                ("recent_issues".to_owned(), Problem::TooLong),
                ("last_project".to_owned(), Problem::Invalid),
            ]
        );
        let edges = validate(Patch {
            issue_panel_width: Some(Some(160)),
            list_sidebar_width: Some(Some(400)),
            list_columns: Some(Some(vec!["field:12".into(), "id".into(), "id".into()])),
            ..Patch::default()
        })
        .ok()
        .map(|v| v.set.list_columns);
        assert_eq!(edges, Some(Some(vec!["field:12".into(), "id".into()])));
        assert!(!is_column("field:+3"));
        assert!(!is_column("field:-3"));
    }

    #[test]
    fn ui_018_size_limit() {
        let small = serde_json::json!({ "home": "issues" });
        assert!(check_size(&small).is_ok());
        let big = serde_json::json!({ "list_columns": ["x".repeat(SIZE_MAX)] });
        assert_eq!(
            check_size(&big).map_err(|r| r.problem),
            Err(Problem::TooLong)
        );
        // The largest valid object fits with room to spare.
        let largest = Preferences {
            list_columns: Some(
                (1..=COLUMNS_MAX as i64)
                    .map(|i| format!("field:{}", i64::MAX - i))
                    .collect(),
            ),
            recent_issues: Some((1..=RECENT_MAX as i64).map(|i| i64::MAX - i).collect()),
            last_project: Some(i64::MAX),
            ..Preferences::default()
        };
        let stored = serde_json::to_value(&largest).unwrap_or_default();
        assert!(check_size(&stored).is_ok());
    }

    #[test]
    fn ui_018_stored_keys_that_no_longer_read_fall_back() {
        let stored = serde_json::json!({
            "home": "dashboards",
            "issue_panel_side": "start",
            "removed_key": true,
        });
        assert_eq!(
            from_stored(stored),
            Preferences {
                issue_panel_side: Some(PanelSide::Start),
                ..Preferences::default()
            }
        );
    }
}
