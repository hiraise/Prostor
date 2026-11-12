//! The default field set (FLD-070), in our own words, in each language trackflow speaks. It is
//! created once, in the instance language, when the instance is set up; afterwards these are
//! ordinary fields that administrators may rename or change. The built-in key stays with a field
//! so templates and later features (time tracking, due dates, team assignees) can find it.

use super::FieldKind;
use crate::settings::Language;

/// Text in each language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Names {
    pub en: &'static str,
    pub ru: &'static str,
}

impl Names {
    pub const fn new(en: &'static str, ru: &'static str) -> Self {
        Self { en, ru }
    }

    pub fn get(self, language: Language) -> &'static str {
        match language {
            Language::En => self.en,
            Language::Ru => self.ru,
        }
    }
}

/// Aliases in each language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aliases {
    pub en: &'static [&'static str],
    pub ru: &'static [&'static str],
}

impl Aliases {
    pub const NONE: Aliases = Aliases { en: &[], ru: &[] };

    pub fn get(self, language: Language) -> &'static [&'static str] {
        match language {
            Language::En => self.en,
            Language::Ru => self.ru,
        }
    }
}

/// Indexes of the value color palette (FLD-044); the web app draws them.
pub mod color {
    pub const GRAY: i16 = 0;
    pub const RED: i16 = 1;
    pub const ORANGE: i16 = 2;
    pub const AMBER: i16 = 3;
    pub const LIME: i16 = 4;
    pub const GREEN: i16 = 5;
    pub const TEAL: i16 = 6;
    pub const SKY: i16 = 7;
    pub const BLUE: i16 = 8;
    pub const VIOLET: i16 = 9;
    pub const PINK: i16 = 10;
    pub const BROWN: i16 = 11;
}

/// A value to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueSeed {
    pub name: Names,
    pub color: Option<i16>,
    /// For state values: counts as resolved.
    pub resolved: bool,
}

const fn value(en: &'static str, ru: &'static str, color: i16) -> ValueSeed {
    ValueSeed {
        name: Names::new(en, ru),
        color: Some(color),
        resolved: false,
    }
}

const fn resolved(en: &'static str, ru: &'static str, color: i16) -> ValueSeed {
    ValueSeed {
        name: Names::new(en, ru),
        color: Some(color),
        resolved: true,
    }
}

use color::*;

const PRIORITIES: &[ValueSeed] = &[
    value("Blocking", "Блокирующий", RED),
    value("High", "Высокий", ORANGE),
    value("Elevated", "Повышенный", AMBER),
    value("Normal", "Обычный", SKY),
    value("Low", "Низкий", GRAY),
];

const TYPES: &[ValueSeed] = &[
    value("Bug", "Ошибка", RED),
    value("Feature", "Новая функция", GREEN),
    value("Task", "Задача", BLUE),
    value("Epic", "Эпик", VIOLET),
    value("Story", "История", TEAL),
    value("Improvement", "Улучшение", SKY),
    value("Performance", "Производительность", ORANGE),
    value("Documentation", "Документация", GRAY),
];

const STATES: &[ValueSeed] = &[
    value("New", "Новая", SKY),
    value("Open", "Открыта", BLUE),
    value("In progress", "В работе", AMBER),
    value("Needs discussion", "Требует обсуждения", VIOLET),
    value("Reopened", "Открыта повторно", ORANGE),
    resolved("Fixed", "Исправлена", GREEN),
    resolved("Verified", "Проверена", TEAL),
    resolved("Duplicate", "Дубликат", GRAY),
    resolved("Won’t fix", "Не будет исправлена", GRAY),
    resolved("Can’t reproduce", "Не воспроизводится", GRAY),
    resolved("Incomplete", "Не хватает данных", BROWN),
    resolved("Obsolete", "Неактуальна", GRAY),
];

/// A value set to create, with the index of its default value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetSeed {
    pub name: Names,
    pub values: &'static [ValueSeed],
    pub default: Option<usize>,
}

/// A field of the default set (FLD-070).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinField {
    Priority,
    Type,
    State,
    Assignee,
    Component,
    FixVersions,
    AffectedVersions,
    FixedInBuild,
    DueDate,
    Estimation,
    SpentTime,
}

/// What creating a built-in field needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSeed {
    pub name: Names,
    pub kind: FieldKind,
    pub multi: bool,
    pub aliases: Aliases,
    pub auto_attach: bool,
    pub can_be_empty: bool,
    pub empty_label: Names,
    /// The field's own set. Fields naming the same set share it.
    pub set: Option<SharedSet>,
}

/// The sets of the default field set; fix and affected versions share one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SharedSet {
    Priorities,
    Types,
    States,
    Assignees,
    Components,
    Versions,
    Builds,
}

impl SharedSet {
    pub const ALL: [SharedSet; 7] = [
        SharedSet::Priorities,
        SharedSet::Types,
        SharedSet::States,
        SharedSet::Assignees,
        SharedSet::Components,
        SharedSet::Versions,
        SharedSet::Builds,
    ];

    pub fn seed(self) -> SetSeed {
        match self {
            SharedSet::Priorities => SetSeed {
                name: Names::new("Priorities", "Приоритеты"),
                values: PRIORITIES,
                default: Some(3),
            },
            SharedSet::Types => SetSeed {
                name: Names::new("Types", "Типы"),
                values: TYPES,
                default: Some(2),
            },
            SharedSet::States => SetSeed {
                name: Names::new("States", "Состояния"),
                values: STATES,
                default: Some(0),
            },
            SharedSet::Assignees => SetSeed {
                name: Names::new("Assignees", "Исполнители"),
                values: &[],
                default: None,
            },
            SharedSet::Components => SetSeed {
                name: Names::new("Components", "Компоненты"),
                values: &[],
                default: None,
            },
            SharedSet::Versions => SetSeed {
                name: Names::new("Versions", "Версии"),
                values: &[],
                default: None,
            },
            SharedSet::Builds => SetSeed {
                name: Names::new("Builds", "Сборки"),
                values: &[],
                default: None,
            },
        }
    }
}

impl BuiltinField {
    /// In the order of the default field list.
    pub const ALL: [BuiltinField; 11] = [
        BuiltinField::Priority,
        BuiltinField::Type,
        BuiltinField::State,
        BuiltinField::Assignee,
        BuiltinField::Component,
        BuiltinField::FixVersions,
        BuiltinField::AffectedVersions,
        BuiltinField::FixedInBuild,
        BuiltinField::DueDate,
        BuiltinField::Estimation,
        BuiltinField::SpentTime,
    ];

    /// Stable key, stored with the field.
    pub fn key(self) -> &'static str {
        match self {
            BuiltinField::Priority => "priority",
            BuiltinField::Type => "type",
            BuiltinField::State => "state",
            BuiltinField::Assignee => "assignee",
            BuiltinField::Component => "component",
            BuiltinField::FixVersions => "fix_versions",
            BuiltinField::AffectedVersions => "affected_versions",
            BuiltinField::FixedInBuild => "fixed_in_build",
            BuiltinField::DueDate => "due_date",
            BuiltinField::Estimation => "estimation",
            BuiltinField::SpentTime => "spent_time",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.key() == key)
    }

    pub fn seed(self) -> FieldSeed {
        let none = Names::new("", "");
        let base = FieldSeed {
            name: none,
            kind: FieldKind::Enum,
            multi: false,
            aliases: Aliases::NONE,
            auto_attach: true,
            can_be_empty: true,
            empty_label: none,
            set: None,
        };
        match self {
            BuiltinField::Priority => FieldSeed {
                name: Names::new("Priority", "Приоритет"),
                can_be_empty: false,
                set: Some(SharedSet::Priorities),
                ..base
            },
            BuiltinField::Type => FieldSeed {
                name: Names::new("Type", "Тип"),
                can_be_empty: false,
                set: Some(SharedSet::Types),
                ..base
            },
            BuiltinField::State => FieldSeed {
                name: Names::new("State", "Состояние"),
                kind: FieldKind::State,
                can_be_empty: false,
                set: Some(SharedSet::States),
                ..base
            },
            BuiltinField::Assignee => FieldSeed {
                name: Names::new("Assignee", "Исполнитель"),
                kind: FieldKind::User,
                aliases: Aliases {
                    en: &["assigned to"],
                    ru: &["назначена"],
                },
                empty_label: Names::new("Unassigned", "Не назначена"),
                set: Some(SharedSet::Assignees),
                ..base
            },
            BuiltinField::Component => FieldSeed {
                name: Names::new("Component", "Компонент"),
                kind: FieldKind::Owned,
                aliases: Aliases {
                    en: &["area"],
                    ru: &["область"],
                },
                empty_label: Names::new("No component", "Без компонента"),
                set: Some(SharedSet::Components),
                ..base
            },
            BuiltinField::FixVersions => FieldSeed {
                name: Names::new("Fix versions", "Исправлено в версиях"),
                kind: FieldKind::Version,
                multi: true,
                aliases: Aliases {
                    en: &["fixed in"],
                    ru: &["исправлено в"],
                },
                empty_label: Names::new("Unscheduled", "Не запланировано"),
                set: Some(SharedSet::Versions),
                ..base
            },
            BuiltinField::AffectedVersions => FieldSeed {
                name: Names::new("Affected versions", "Затронутые версии"),
                kind: FieldKind::Version,
                multi: true,
                aliases: Aliases {
                    en: &["affects"],
                    ru: &["затрагивает"],
                },
                empty_label: Names::new("Unknown", "Неизвестно"),
                set: Some(SharedSet::Versions),
                ..base
            },
            BuiltinField::FixedInBuild => FieldSeed {
                name: Names::new("Fixed in build", "Исправлено в сборке"),
                kind: FieldKind::Build,
                aliases: Aliases {
                    en: &["build"],
                    ru: &["сборка"],
                },
                empty_label: Names::new("Not built yet", "Ещё не собрано"),
                set: Some(SharedSet::Builds),
                ..base
            },
            BuiltinField::DueDate => FieldSeed {
                name: Names::new("Due date", "Срок"),
                kind: FieldKind::Date,
                auto_attach: false,
                empty_label: Names::new("No due date", "Без срока"),
                ..base
            },
            BuiltinField::Estimation => FieldSeed {
                name: Names::new("Estimation", "Оценка"),
                kind: FieldKind::Duration,
                empty_label: Names::new("?", "?"),
                ..base
            },
            BuiltinField::SpentTime => FieldSeed {
                name: Names::new("Spent time", "Затрачено"),
                kind: FieldKind::Duration,
                empty_label: Names::new("?", "?"),
                ..base
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FieldType, aliases, empty_label, field_name, set_name, value_name};

    #[test]
    fn fld_070_the_default_field_set_is_valid_in_every_language() {
        for language in [Language::En, Language::Ru] {
            let mut names = Vec::new();
            for field in BuiltinField::ALL {
                let seed = field.seed();
                assert_eq!(BuiltinField::parse(field.key()), Some(field));
                let name = field_name(seed.name.get(language)).unwrap();
                FieldType::new(seed.kind, seed.multi).unwrap();
                let list: Vec<String> = seed
                    .aliases
                    .get(language)
                    .iter()
                    .map(|a| (*a).to_owned())
                    .collect();
                assert_eq!(aliases(&list, &name).unwrap().len(), list.len());
                empty_label(seed.empty_label.get(language)).unwrap();
                assert_eq!(seed.kind.set_kind().is_some(), seed.set.is_some());
                assert!(seed.can_be_empty || seed.empty_label.get(language).is_empty());
                names.push(name.to_lowercase());
            }
            names.sort();
            names.dedup();
            assert_eq!(names.len(), BuiltinField::ALL.len());
            for set in SharedSet::ALL {
                let seed = set.seed();
                set_name(seed.name.get(language)).unwrap();
                let mut values: Vec<String> = seed
                    .values
                    .iter()
                    .map(|v| value_name(v.name.get(language)).unwrap().to_lowercase())
                    .collect();
                values.sort();
                values.dedup();
                assert_eq!(values.len(), seed.values.len());
                assert!(seed.default.is_none_or(|i| i < seed.values.len()));
            }
        }
        // Only due date waits to be attached by hand; states have both kinds of values.
        assert!(!BuiltinField::DueDate.seed().auto_attach);
        let states = SharedSet::States.seed();
        assert_eq!(states.values.iter().filter(|v| v.resolved).count(), 7);
        assert_eq!(states.values.iter().filter(|v| !v.resolved).count(), 5);
    }
}
