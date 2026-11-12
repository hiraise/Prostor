//! Groups (ACC-021…ACC-032): sets of accounts that hold roles together. Two built-in groups have
//! implicit membership, so they never need rows per account.

use crate::accounts::plain_text;
use crate::validation::{Problem, Rejection};

/// Longest group name.
pub const GROUP_NAME_MAX: usize = 100;
/// Longest group description.
pub const GROUP_DESCRIPTION_MAX: usize = 500;

/// A group every installation has (ACC-021). Neither can be renamed, deleted or lose auto-join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinGroup {
    /// Every account, the guest included.
    Everyone,
    /// Every account except the guest.
    AllMembers,
}

impl BuiltinGroup {
    pub const ALL: [BuiltinGroup; 2] = [BuiltinGroup::Everyone, BuiltinGroup::AllMembers];

    pub fn key(self) -> &'static str {
        match self {
            BuiltinGroup::Everyone => "everyone",
            BuiltinGroup::AllMembers => "all_members",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.key() == key)
    }

    /// Names in the languages trackflow speaks (English first).
    pub fn names(self) -> [&'static str; 2] {
        match self {
            BuiltinGroup::Everyone => ["Everyone", "Все"],
            BuiltinGroup::AllMembers => ["All members", "Все участники"],
        }
    }

    /// Whether an account belongs to the group.
    pub fn includes(self, is_guest: bool) -> bool {
        match self {
            BuiltinGroup::Everyone => true,
            BuiltinGroup::AllMembers => !is_guest,
        }
    }
}

/// Validates a group name: a single line, not one of the built-in names in any language.
pub fn group_name(raw: &str) -> Result<String, Rejection> {
    let name = plain_text(raw, GROUP_NAME_MAX, "name")?;
    let lower = name.to_lowercase();
    let reserved = BuiltinGroup::ALL
        .iter()
        .flat_map(|group| group.names())
        .any(|taken| taken.to_lowercase() == lower);
    if reserved {
        return Err(Rejection::new(
            Problem::Taken,
            "a built-in group has this name",
        ));
    }
    Ok(name)
}

/// Built-in groups whose name in any language contains `text` (ignoring case), so a search in the
/// person's language finds them although their rows keep the English name.
pub fn builtin_matching(text: &str) -> Vec<BuiltinGroup> {
    let text = text.trim().to_lowercase();
    BuiltinGroup::ALL
        .into_iter()
        .filter(|group| {
            group
                .names()
                .iter()
                .any(|name| name.to_lowercase().contains(&text))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_021_built_in_groups() {
        assert!(BuiltinGroup::Everyone.includes(true));
        assert!(!BuiltinGroup::AllMembers.includes(true));
        assert!(BuiltinGroup::AllMembers.includes(false));
        for group in BuiltinGroup::ALL {
            assert_eq!(BuiltinGroup::parse(group.key()), Some(group));
        }
    }

    #[test]
    fn acc_024_names_cannot_copy_built_in_ones() {
        assert_eq!(group_name(" Developers ").unwrap(), "Developers");
        assert_eq!(group_name("everyone").unwrap_err().problem, Problem::Taken);
        assert_eq!(
            group_name("Все участники").unwrap_err().problem,
            Problem::Taken
        );
        assert_eq!(builtin_matching("участ"), [BuiltinGroup::AllMembers]);
        assert_eq!(builtin_matching("ALL"), [BuiltinGroup::AllMembers]);
        assert_eq!(builtin_matching("Вс").len(), 2);
    }
}
