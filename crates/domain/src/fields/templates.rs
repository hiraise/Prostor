//! The fields of the built-in project templates (PRJ-021…PRJ-024) and the rule for attaching a
//! field to a new project. A template is a read-only definition; the project gets a copy that it
//! may change freely, and later template changes never reach it (PRJ-021). Boards of the
//! templates come with boards (step 1.8), their rules with built-in behaviors (step 1.9).

use super::FieldKind;
use super::builtin::{
    Aliases, BuiltinField, Names, SetSeed, ValueSeed,
    color::{AMBER, BLUE, GRAY, GREEN, RED, SKY, TEAL, VIOLET},
};
use crate::projects::Template;

/// Which set a field attached to a new project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetChoice {
    /// The field's own set, shared with every project that shares it.
    Shared,
    /// A new set of the project holding the project team (FLD-040, PRJ-054).
    Team,
    /// A new empty set of the project. Fields with the same own set share one new set, so fix and
    /// affected versions keep one list of versions per project.
    Empty,
    /// A new set of the project with these values.
    Values(SetSeed),
}

/// How a field that is attached to every new blank project gets its set (FLD-020): values that
/// mean the same everywhere (enumerations, states) are shared; people, components, versions and
/// builds belong to each project.
pub fn auto_attach_set(kind: FieldKind) -> SetChoice {
    match kind {
        FieldKind::User => SetChoice::Team,
        FieldKind::Version | FieldKind::Build | FieldKind::Owned => SetChoice::Empty,
        _ => SetChoice::Shared,
    }
}

/// A field a template creates if the instance has no field of that name and type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewField {
    pub name: Names,
    pub kind: FieldKind,
    pub multi: bool,
    pub aliases: Aliases,
}

/// Which field a template attaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateFieldRef {
    /// A field of the default set, found by its built-in key.
    Builtin(BuiltinField),
    /// A field found by name (in the instance language) and type, or created.
    Named(NewField),
}

/// One field of a template with its settings in the new project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateField {
    pub field: TemplateFieldRef,
    pub set: SetChoice,
    pub can_be_empty: bool,
    pub empty_label: Option<Names>,
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

const fn builtin(field: BuiltinField, set: SetChoice, can_be_empty: bool) -> TemplateField {
    TemplateField {
        field: TemplateFieldRef::Builtin(field),
        set,
        can_be_empty,
        empty_label: None,
    }
}

const SCRUM: &[TemplateField] = &[
    builtin(BuiltinField::Priority, SetChoice::Shared, false),
    builtin(
        BuiltinField::Type,
        SetChoice::Values(SetSeed {
            name: Names::new("Types", "Типы"),
            values: &[
                value("Epic", "Эпик", VIOLET),
                value("User story", "Пользовательская история", TEAL),
                value("Task", "Задача", BLUE),
                value("Bug", "Ошибка", RED),
            ],
            default: Some(2),
        }),
        false,
    ),
    builtin(
        BuiltinField::State,
        SetChoice::Values(SetSeed {
            name: Names::new("States", "Состояния"),
            values: &[
                value("Open", "Открыта", BLUE),
                value("In progress", "В работе", AMBER),
                value("To verify", "На проверке", VIOLET),
                resolved("Done", "Готово", GREEN),
                resolved("Duplicate", "Дубликат", GRAY),
            ],
            default: Some(0),
        }),
        false,
    ),
    builtin(BuiltinField::Assignee, SetChoice::Team, true),
    TemplateField {
        field: TemplateFieldRef::Named(NewField {
            name: Names::new("Sprints", "Спринты"),
            kind: FieldKind::Version,
            multi: true,
            aliases: Aliases::NONE,
        }),
        set: SetChoice::Empty,
        can_be_empty: true,
        empty_label: Some(Names::new("Not planned", "Не запланировано")),
    },
    TemplateField {
        field: TemplateFieldRef::Named(NewField {
            name: Names::new("Story points", "Стори-поинты"),
            kind: FieldKind::Integer,
            multi: false,
            aliases: Aliases::NONE,
        }),
        set: SetChoice::Shared,
        can_be_empty: true,
        empty_label: Some(Names::new("?", "?")),
    },
    TemplateField {
        field: TemplateFieldRef::Named(NewField {
            name: Names::new("Ideal days", "Идеальные дни"),
            kind: FieldKind::Integer,
            multi: false,
            aliases: Aliases::NONE,
        }),
        set: SetChoice::Shared,
        can_be_empty: true,
        empty_label: Some(Names::new("?", "?")),
    },
    builtin(BuiltinField::Estimation, SetChoice::Shared, true),
];

const KANBAN: &[TemplateField] = &[
    TemplateField {
        field: TemplateFieldRef::Named(NewField {
            name: Names::new("Stage", "Этап"),
            kind: FieldKind::State,
            multi: false,
            aliases: Aliases::NONE,
        }),
        set: SetChoice::Values(SetSeed {
            name: Names::new("Stages", "Этапы"),
            values: &[
                value("Backlog", "Бэклог", GRAY),
                value("Develop", "Разработка", BLUE),
                value("Review", "Ревью", VIOLET),
                value("Test", "Тестирование", AMBER),
                value("Staging", "Предрелиз", SKY),
                resolved("Done", "Готово", GREEN),
            ],
            default: Some(0),
        }),
        can_be_empty: false,
        empty_label: None,
    },
    TemplateField {
        field: TemplateFieldRef::Named(NewField {
            name: Names::new("Readiness", "Готовность"),
            kind: FieldKind::Enum,
            multi: false,
            aliases: Aliases::NONE,
        }),
        set: SetChoice::Values(SetSeed {
            name: Names::new("Readiness", "Готовность"),
            values: &[
                value("Ready to pull", "Можно брать", GREEN),
                value("Blocked", "Заблокирована", RED),
            ],
            default: Some(0),
        }),
        can_be_empty: false,
        empty_label: None,
    },
    builtin(BuiltinField::Priority, SetChoice::Shared, false),
    builtin(BuiltinField::Assignee, SetChoice::Team, true),
];

/// The fields a template attaches, in order; `None` for the blank template, which attaches the
/// fields marked for new projects (FLD-020).
pub fn fields_of(template: Template) -> Option<&'static [TemplateField]> {
    match template {
        Template::Blank => None,
        Template::Scrum => Some(SCRUM),
        Template::Kanban => Some(KANBAN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{FieldType, field_name, value_name};
    use crate::settings::Language;

    #[test]
    fn prj_023_024_templates_are_valid() {
        for template in [Template::Scrum, Template::Kanban] {
            let fields = fields_of(template).unwrap();
            for language in [Language::En, Language::Ru] {
                for field in fields {
                    if let TemplateFieldRef::Named(new) = field.field {
                        field_name(new.name.get(language)).unwrap();
                        FieldType::new(new.kind, new.multi).unwrap();
                        let has_set = new.kind.set_kind().is_some();
                        assert_eq!(has_set, field.set != SetChoice::Shared, "{:?}", new.name);
                    }
                    if let SetChoice::Values(set) = field.set {
                        for value in set.values {
                            value_name(value.name.get(language)).unwrap();
                        }
                        assert!(set.default.is_some_and(|i| i < set.values.len()));
                    }
                    assert!(field.can_be_empty || field.empty_label.is_none());
                }
            }
        }
        assert!(fields_of(Template::Blank).is_none());
        assert_eq!(auto_attach_set(FieldKind::Enum), SetChoice::Shared);
        assert_eq!(auto_attach_set(FieldKind::User), SetChoice::Team);
        assert_eq!(auto_attach_set(FieldKind::Version), SetChoice::Empty);
    }
}
