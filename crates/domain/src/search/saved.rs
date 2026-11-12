//! Rules of saved searches (QRY-063…QRY-068, PRJ-019; D-038): names, limits, the predefined set
//! and the "unassigned" search of a new project. Query texts use English keywords, which work in
//! any instance language (D-013); field names come from the database.

use crate::accounts::plain_text;
use crate::settings::Language;
use crate::validation::Rejection;

/// Longest name, in characters.
pub const NAME_MAX: usize = 100;
/// Most visible saved searches of one owner.
pub const PER_OWNER_MAX: usize = 500;

/// Validates a saved search name: one line, 1…[`NAME_MAX`] characters.
pub fn name(raw: &str) -> Result<String, Rejection> {
    plain_text(raw, NAME_MAX, "name")
}

/// The predefined searches every person gets (QRY-068).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    AssignedToMe,
    CommentedByMe,
    ReportedByMe,
    AllIssues,
}

impl Builtin {
    pub const ALL: [Builtin; 4] = [
        Builtin::AssignedToMe,
        Builtin::CommentedByMe,
        Builtin::ReportedByMe,
        Builtin::AllIssues,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Builtin::AssignedToMe => "assigned_to_me",
            Builtin::CommentedByMe => "commented_by_me",
            Builtin::ReportedByMe => "reported_by_me",
            Builtin::AllIssues => "all_issues",
        }
    }

    pub fn name(self, language: Language) -> &'static str {
        match (self, language) {
            (Builtin::AssignedToMe, Language::En) => "Assigned to me",
            (Builtin::AssignedToMe, Language::Ru) => "Назначены мне",
            (Builtin::CommentedByMe, Language::En) => "Commented by me",
            (Builtin::CommentedByMe, Language::Ru) => "С моими комментариями",
            (Builtin::ReportedByMe, Language::En) => "Reported by me",
            (Builtin::ReportedByMe, Language::Ru) => "Созданы мной",
            (Builtin::AllIssues, Language::En) => "All issues",
            (Builtin::AllIssues, Language::Ru) => "Все задачи",
        }
    }

    /// The query text; `assignee` is the current name of the assignee field of the default set.
    pub fn query(self, assignee: &str) -> String {
        match self {
            Builtin::AssignedToMe => format!("{}: me is: unresolved", quoted(assignee)),
            Builtin::CommentedByMe => "commenter: me".to_owned(),
            Builtin::ReportedByMe => "reporter: me".to_owned(),
            Builtin::AllIssues => String::new(),
        }
    }
}

/// The name of a new project's "unassigned" search (PRJ-019).
pub fn unassigned_name(language: Language, key: &str) -> String {
    match language {
        Language::En => format!("Unassigned in {key}"),
        Language::Ru => format!("Неназначенные в {key}"),
    }
}

/// Its query; `assignee` is the project's assignee field name.
pub fn unassigned_query(key: &str, assignee: &str) -> String {
    format!("project: {key} {}: none is: unresolved", quoted(assignee))
}

/// The name of a board's backlog search (BRD-089), at most [`NAME_MAX`] characters: a long
/// board name is cut.
pub fn backlog_name(language: Language, board: &str) -> String {
    let suffix = match language {
        Language::En => " · backlog",
        Language::Ru => " · бэклог",
    };
    let room = NAME_MAX - suffix.chars().count();
    let head: String = board.chars().take(room).collect();
    format!("{}{suffix}", head.trim_end())
}

/// Its query: the unresolved issues of the board projects that are not on the board.
pub fn backlog_query(keys: &[&str], board: &str) -> String {
    format!(
        "project: {} is: unresolved !board: {}",
        keys.join(", "),
        quoted(board)
    )
}

/// A name in quotes; the stored form drops quotes that are not needed when it prints.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trackflow_query::parse;

    #[test]
    fn brd_089_backlog_name_and_query() {
        assert_eq!(backlog_name(Language::Ru, "Команда"), "Команда · бэклог");
        let long = backlog_name(Language::En, &"x".repeat(NAME_MAX));
        assert_eq!(long.chars().count(), NAME_MAX);
        let q = parse(&backlog_query(&["APP", "WEB"], "Team \"A\""));
        assert!(q.problems.is_empty(), "{:?}", q.problems);
    }

    #[test]
    fn qry_068_predefined_queries_parse() {
        for b in Builtin::ALL {
            for assignee in ["Assignee", "Исполнитель", "Who \"does\" it"] {
                let q = parse(&b.query(assignee));
                assert!(!q.has_errors(), "{:?}: {:?}", b, q.problems);
            }
            assert_ne!(b.name(Language::En), b.name(Language::Ru));
        }
        assert!(!parse(&unassigned_query("WEB", "Assignee")).has_errors());
    }

    #[test]
    fn qry_063_names_are_one_line() {
        assert_eq!(name("  Mine ").unwrap(), "Mine");
        assert!(name("").is_err());
        assert!(name("a\nb").is_err());
        assert!(name(&"x".repeat(NAME_MAX + 1)).is_err());
    }
}
