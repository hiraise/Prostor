//! Projects (PRJ): names, keys and their suggestion, descriptions, templates and the states a
//! project goes through. Design: decisions.md D-027.

use crate::accounts::plain_text;
use crate::validation::{Problem, Rejection};

/// Longest project name.
pub const PROJECT_NAME_MAX: usize = 100;
/// Longest description (Markdown).
pub const PROJECT_DESCRIPTION_MAX: usize = 10_000;
/// Shortest and longest key.
pub const KEY_MIN: usize = 2;
pub const KEY_MAX: usize = 10;
/// Largest first issue number.
pub const FIRST_NUMBER_MAX: i64 = 1_000_000_000;

/// A project key (PRJ-003): the prefix of issue IDs such as `WEB-42`. Latin capital letters,
/// digits and underscores, starting with a letter, 2–10 characters. Typed in any case; stored in
/// capitals. No hyphen, so `KEY-123` and article IDs `KEY-A-7` always split the same way.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectKey(String);

impl ProjectKey {
    pub fn parse(raw: &str) -> Result<Self, Rejection> {
        let key = raw.trim().to_ascii_uppercase();
        if key.is_empty() {
            return Err(Rejection::new(Problem::Required, "key is required"));
        }
        if key.chars().count() < KEY_MIN {
            return Err(Rejection::new(
                Problem::TooShort,
                format!("key is shorter than {KEY_MIN} characters"),
            ));
        }
        if key.chars().count() > KEY_MAX {
            return Err(Rejection::new(
                Problem::TooLong,
                format!("key is longer than {KEY_MAX} characters"),
            ));
        }
        let allowed = |c: char| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_';
        if !key.starts_with(|c: char| c.is_ascii_uppercase()) || !key.chars().all(allowed) {
            return Err(Rejection::new(
                Problem::Invalid,
                "key may contain Latin letters, digits and underscores and must start with a letter",
            ));
        }
        Ok(Self(key))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

/// Validates a project name: one line, unique ignoring case (checked by the store).
pub fn project_name(raw: &str) -> Result<String, Rejection> {
    plain_text(raw, PROJECT_NAME_MAX, "name")
}

/// Validates a description: Markdown up to the limit; may be empty.
pub fn project_description(raw: &str) -> Result<String, Rejection> {
    let text = raw.trim();
    if text.chars().count() > PROJECT_DESCRIPTION_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("description is longer than {PROJECT_DESCRIPTION_MAX} characters"),
        ));
    }
    Ok(text.to_owned())
}

/// Validates the number of the first issue (PRJ-004).
pub fn first_number(number: i64) -> Result<i64, Rejection> {
    if (1..=FIRST_NUMBER_MAX).contains(&number) {
        Ok(number)
    } else {
        Err(Rejection::new(
            Problem::OutOfRange,
            format!("the first number must be from 1 to {FIRST_NUMBER_MAX}"),
        ))
    }
}

/// Latin letters for a Russian one, so names in Russian get readable keys; `None` for characters
/// that are not Russian letters.
fn transliterate(c: char) -> Option<&'static str> {
    Some(match c {
        'А' => "A",
        'Б' => "B",
        'В' => "V",
        'Г' => "G",
        'Д' => "D",
        'Е' | 'Ё' | 'Э' => "E",
        'Ж' => "ZH",
        'З' => "Z",
        'И' | 'Й' | 'Ы' => "I",
        'К' => "K",
        'Л' => "L",
        'М' => "M",
        'Н' => "N",
        'О' => "O",
        'П' => "P",
        'Р' => "R",
        'С' => "S",
        'Т' => "T",
        'У' => "U",
        'Ф' => "F",
        'Х' => "KH",
        'Ц' => "TS",
        'Ч' => "CH",
        'Ш' => "SH",
        'Щ' => "SHCH",
        'Ю' => "YU",
        'Я' => "YA",
        'Ь' | 'Ъ' => "",
        _ => return None,
    })
}

/// The words of a name in Latin capitals.
fn words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for c in name.chars().flat_map(char::to_uppercase) {
        if c.is_ascii_alphanumeric() {
            current.push(c);
        } else if let Some(latin) = transliterate(c) {
            current.push_str(latin);
        } else if !current.is_empty() && c != '\'' && c != '’' {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Suggests a key for a project name (PRJ-003, PRJ-018): the initials of its words, or the start
/// of a single word, transliterated, without leading digits; `taken` says which keys are in use,
/// and a number is added until one is free. Deterministic, so the wizard and the server agree.
pub fn suggest_key(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let words = words(name);
    let mut base: String = if words.len() >= 2 {
        words
            .iter()
            .filter_map(|word| word.chars().next())
            .collect()
    } else {
        words
            .first()
            .map(|word| word.chars().take(4).collect())
            .unwrap_or_default()
    };
    base = base
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .to_owned();
    if base.len() < KEY_MIN {
        // Too short: take more letters of the whole name.
        let all: String = words
            .concat()
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .chars()
            .take(4)
            .collect();
        base = if all.len() >= KEY_MIN {
            all
        } else {
            "PRJ".to_owned()
        };
    }
    base.truncate(KEY_MAX);
    if !taken(&base) {
        return base;
    }
    for n in 2..10_000_u32 {
        let suffix = n.to_string();
        let mut candidate = base.clone();
        candidate.truncate(KEY_MAX - suffix.len());
        candidate.push_str(&suffix);
        if !taken(&candidate) {
            return candidate;
        }
    }
    base
}

/// A built-in template (PRJ-021…PRJ-024). Custom templates are PRJ-030 (P2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Template {
    /// The fields marked for new projects (FLD-020), nothing else.
    Blank,
    /// Sprints with stories and tasks.
    Scrum,
    /// A flow of stages without sprints.
    Kanban,
}

impl Template {
    pub const ALL: [Template; 3] = [Template::Blank, Template::Scrum, Template::Kanban];

    pub fn key(self) -> &'static str {
        match self {
            Template::Blank => "blank",
            Template::Scrum => "scrum",
            Template::Kanban => "kanban",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|template| template.key() == key)
    }
}

/// Where a project is in its life (PRJ-061, PRJ-065).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Active,
    /// Read-only; out of pickers and searches; can be restored.
    Archived,
    /// Being deleted in the background; nobody can use it.
    Deleting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prj_003_key_format() {
        assert_eq!(ProjectKey::parse(" web ").unwrap().as_str(), "WEB");
        assert_eq!(ProjectKey::parse("Ab_12").unwrap().as_str(), "AB_12");
        let problem = |raw: &str| ProjectKey::parse(raw).unwrap_err().problem;
        assert_eq!(problem(""), Problem::Required);
        assert_eq!(problem("A"), Problem::TooShort);
        assert_eq!(problem("ABCDEFGHIJK"), Problem::TooLong);
        assert_eq!(problem("1AB"), Problem::Invalid);
        assert_eq!(problem("AB-1"), Problem::Invalid);
        assert_eq!(problem("ПР"), Problem::Invalid);
        assert_eq!(problem("_AB"), Problem::Invalid);
    }

    #[test]
    fn prj_018_key_suggestions() {
        let free = |_: &str| false;
        assert_eq!(suggest_key("Website redesign", free), "WR");
        assert_eq!(suggest_key("Phoenix", free), "PHOE");
        assert_eq!(suggest_key("Мобильное приложение", free), "MP");
        assert_eq!(suggest_key("Склад", free), "SKLA");
        assert_eq!(suggest_key("Customer's portal 2", free), "CP2");
        assert_eq!(suggest_key("2048 game", free), "GAME");
        assert_eq!(suggest_key("X", free), "PRJ");
        assert_eq!(suggest_key("!!!", free), "PRJ");
        assert_eq!(suggest_key("a b c d e f g h i j k l", free), "ABCDEFGHIJ");
        let taken = |key: &str| ["WR", "WR2"].contains(&key);
        assert_eq!(suggest_key("Website redesign", taken), "WR3");
        let all_taken = |key: &str| key.starts_with("ABCDEFGH");
        assert_eq!(
            suggest_key("a b c d e f g h i j k l", all_taken),
            "ABCDEFG100"
        );
        // Every suggestion is a valid key.
        for name in ["Website redesign", "Phoenix", "Мобильное приложение", "X"]
        {
            assert!(
                ProjectKey::parse(&suggest_key(name, free)).is_ok(),
                "{name}"
            );
        }
    }

    #[test]
    fn prj_004_first_number() {
        assert_eq!(first_number(1), Ok(1));
        assert!(first_number(0).is_err());
        assert!(first_number(FIRST_NUMBER_MAX + 1).is_err());
    }
}
