//! The vocabulary (plan §3.3): keywords with stable keys and their English and Russian forms.
//! Both languages work whatever the instance language (D-013). Keys never change (saved searches
//! store them); forms may grow.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `and`, `or`, `not`.
    Operator,
    /// `sort`, `asc`, `desc`.
    Sort,
    /// A built-in attribute in a name position: `project`, `created`, `tag`…
    Attribute,
    /// A keyword in a value position: `me`, `none`, `resolved`…
    Value,
    /// A relative date in a value position: `today`, `last-week`…
    Date,
}

/// What counts in a name position (before `:` or a comparison, and in `sort`).
pub const NAME: &[Kind] = &[Kind::Attribute, Kind::Sort];
/// What counts in a value position.
pub const VALUE: &[Kind] = &[Kind::Value, Kind::Date];

#[derive(Debug, PartialEq, Eq)]
pub struct Keyword {
    pub key: &'static str,
    pub kind: Kind,
    /// Folded forms (see [`fold`]): `"last week"`, not `"last-week"`.
    pub english: &'static [&'static str],
    pub russian: &'static [&'static str],
}

const fn word(
    key: &'static str,
    kind: Kind,
    english: &'static [&'static str],
    russian: &'static [&'static str],
) -> Keyword {
    Keyword {
        key,
        kind,
        english,
        russian,
    }
}

use Kind::{Attribute, Date, Operator, Sort, Value};

pub const VOCABULARY: &[Keyword] = &[
    word("and", Operator, &["and"], &["и"]),
    word("or", Operator, &["or"], &["или"]),
    word("not", Operator, &["not"], &["не"]),
    word("sort", Sort, &["sort"], &["сортировка"]),
    word("asc", Sort, &["asc"], &["возр"]),
    word("desc", Sort, &["desc"], &["убыв"]),
    word("project", Attribute, &["project"], &["проект"]),
    word("id", Attribute, &["id"], &["ид", "номер"]),
    word("reporter", Attribute, &["reporter", "author"], &["автор"]),
    word(
        "updater",
        Attribute,
        &["updater", "updated by"],
        &["изменил"],
    ),
    word(
        "commenter",
        Attribute,
        &["commenter", "commented by"],
        &["комментатор"],
    ),
    word("involves", Attribute, &["involves"], &["участник"]),
    word("created", Attribute, &["created"], &["создана"]),
    word("updated", Attribute, &["updated"], &["изменена"]),
    word("resolved_date", Attribute, &["resolved"], &["решена"]),
    word(
        "commented",
        Attribute,
        &["commented"],
        &["прокомментирована"],
    ),
    word("tag", Attribute, &["tag"], &["тег", "метка"]),
    word("linked", Attribute, &["linked"], &["связана"]),
    word("links", Attribute, &["links"], &["связи"]),
    word("votes", Attribute, &["votes"], &["голоса"]),
    word("comments", Attribute, &["comments"], &["комментарии"]),
    word("attachments", Attribute, &["attachments"], &["вложения"]),
    word("summary", Attribute, &["summary", "title"], &["заголовок"]),
    word("description", Attribute, &["description"], &["описание"]),
    word("text", Attribute, &["text"], &["текст"]),
    word("mentions", Attribute, &["mentions"], &["упоминает"]),
    word(
        "mentioned_in",
        Attribute,
        &["mentioned in"],
        &["упомянута в"],
    ),
    word("is", Attribute, &["is"], &["это"]),
    word("board", Attribute, &["board"], &["доска"]),
    word("sprint", Attribute, &["sprint"], &["спринт"]),
    word(
        "me",
        Value,
        &["me", "my", "mine"],
        &["я", "мой", "моя", "мои", "мне", "меня"],
    ),
    word("none", Value, &["none", "empty"], &["нет", "пусто"]),
    word("any", Value, &["any"], &["любое", "есть"]),
    word("resolved", Value, &["resolved"], &["решена", "решенные"]),
    word(
        "unresolved",
        Value,
        &["unresolved", "open"],
        &["открыта", "открытые"],
    ),
    word("starred", Value, &["starred"], &["отмечена", "отмеченные"]),
    word("current", Value, &["current"], &["текущий"]),
    word("now", Date, &["now"], &["сейчас"]),
    word("today", Date, &["today"], &["сегодня"]),
    word("yesterday", Date, &["yesterday"], &["вчера"]),
    word("tomorrow", Date, &["tomorrow"], &["завтра"]),
    word("monday", Date, &["monday"], &["понедельник"]),
    word("tuesday", Date, &["tuesday"], &["вторник"]),
    word("wednesday", Date, &["wednesday"], &["среда"]),
    word("thursday", Date, &["thursday"], &["четверг"]),
    word("friday", Date, &["friday"], &["пятница"]),
    word("saturday", Date, &["saturday"], &["суббота"]),
    word("sunday", Date, &["sunday"], &["воскресенье"]),
    word("this_week", Date, &["this week"], &["эта неделя"]),
    word("last_week", Date, &["last week"], &["прошлая неделя"]),
    word("next_week", Date, &["next week"], &["следующая неделя"]),
    word(
        "two_weeks_ago",
        Date,
        &["two weeks ago"],
        &["две недели назад"],
    ),
    word(
        "three_weeks_ago",
        Date,
        &["three weeks ago"],
        &["три недели назад"],
    ),
    word("this_month", Date, &["this month"], &["этот месяц"]),
    word("last_month", Date, &["last month"], &["прошлый месяц"]),
    word("next_month", Date, &["next month"], &["следующий месяц"]),
    word(
        "last_working_day",
        Date,
        &["last working day"],
        &["последний рабочий день"],
    ),
    word("older", Date, &["older"], &["давно"]),
];

/// Usernames a person may not take (ACC-017): the ASCII forms of the value keywords and the
/// operator words, which a query would read as keywords. Usernames are ASCII, so the Russian
/// forms cannot collide.
pub const RESERVED_USERNAMES: &[&str] = &[
    "and",
    "or",
    "not",
    "me",
    "my",
    "mine",
    "none",
    "empty",
    "any",
    "resolved",
    "unresolved",
    "open",
    "starred",
    "current",
];

/// The keyword of one of `kinds` that `folded` (the output of [`fold`]) names.
pub fn keyword(folded: &str, kinds: &[Kind]) -> Option<&'static Keyword> {
    VOCABULARY.iter().find(|w| {
        kinds.contains(&w.kind) && (w.english.contains(&folded) || w.russian.contains(&folded))
    })
}

/// Whether the text, folded, is one of the operator words `and`, `or`, `not`.
pub fn is_operator(text: &str) -> bool {
    keyword(&fold(text), &[Kind::Operator]).is_some()
}

/// The form for every lookup of names and keywords: Unicode lowercase, `ё` → `е`, runs of
/// whitespace, `-` and `_` as one space, trimmed. `due-date`, `Due_Date` and `due date` match.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for c in text.chars() {
        if c.is_whitespace() || c == '-' || c == '_' {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push(' ');
        }
        gap = false;
        push_lower(&mut out, c);
    }
    out
}

/// The form for comparing values of set fields: lowercase, `ё` → `е`, trimmed.
pub fn fold_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.trim().chars() {
        push_lower(&mut out, c);
    }
    out
}

fn push_lower(out: &mut String, c: char) {
    for lower in c.to_lowercase() {
        out.push(if lower == 'ё' { 'е' } else { lower });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn folding() {
        assert_eq!(fold("Due_Date"), "due date");
        assert_eq!(fold("  Last--Week "), "last week");
        assert_eq!(fold("due - date"), "due date");
        assert_eq!(fold("ЁЛКА"), "елка");
        assert_eq!(fold_value("  In  Progress "), "in  progress");
        assert_eq!(fold_value("Решённые"), "решенные");
    }

    #[test]
    fn forms_are_folded_and_unique_per_position() {
        let mut keys = HashSet::new();
        for w in VOCABULARY {
            assert!(keys.insert(w.key), "key {} twice", w.key);
            for form in w.english.iter().chain(w.russian) {
                assert_eq!(&fold(form), form, "{} is not folded", form);
            }
        }
        for kinds in [NAME, VALUE, &[Kind::Operator][..]] {
            let mut seen = HashSet::new();
            for w in VOCABULARY.iter().filter(|w| kinds.contains(&w.kind)) {
                for form in w.english.iter().chain(w.russian) {
                    assert!(seen.insert(*form), "{form} twice in one position");
                }
            }
        }
    }

    #[test]
    fn lookup_by_position() {
        assert_eq!(
            keyword(&fold("Решена"), NAME).map(|w| w.key),
            Some("resolved_date")
        );
        assert_eq!(
            keyword(&fold("Решена"), VALUE).map(|w| w.key),
            Some("resolved")
        );
        assert_eq!(
            keyword(&fold("решённые"), VALUE).map(|w| w.key),
            Some("resolved")
        );
        assert_eq!(
            keyword(&fold("Updated-By"), NAME).map(|w| w.key),
            Some("updater")
        );
        assert_eq!(
            keyword(&fold("last_week"), VALUE).map(|w| w.key),
            Some("last_week")
        );
        assert_eq!(keyword(&fold("open"), NAME), None);
        assert!(is_operator("ИЛИ"));
    }

    #[test]
    fn acc_017_reserved_usernames_are_the_ascii_value_and_operator_forms() {
        let expected: HashSet<&str> = VOCABULARY
            .iter()
            .filter(|w| matches!(w.kind, Kind::Value | Kind::Operator))
            .flat_map(|w| w.english.iter().copied())
            .collect();
        let reserved: HashSet<&str> = RESERVED_USERNAMES.iter().copied().collect();
        assert_eq!(reserved, expected);
        assert!(RESERVED_USERNAMES.iter().all(|n| n.is_ascii()));
    }
}
