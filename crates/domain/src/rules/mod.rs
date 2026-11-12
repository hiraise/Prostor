//! Built-in behaviors (AUT-022, AUT-023, step 1.9): the rule registry with its template sets, and
//! what a project lacks for a rule to work. Definitions only — the engine and behaviors live in
//! `service::rules`, the toggles in `store::rules`. A rule with no row in a project never runs
//! there; nothing is on by default at read time.

use crate::fields::FieldKind;
use crate::fields::builtin::Names;
use crate::projects::Template;
use crate::settings::Language;

/// A built-in behavior a project turns on or off (AUT-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    AssigneeVisibility,
    PlusOneVote,
    ComponentOwner,
    ParentFollowsSubtasks,
    InheritFixVersions,
    FixedInBuild,
    Duplicates,
    Scrum,
    CloneIssue,
}

impl Rule {
    pub const ALL: [Rule; 9] = [
        Rule::AssigneeVisibility,
        Rule::PlusOneVote,
        Rule::ComponentOwner,
        Rule::ParentFollowsSubtasks,
        Rule::InheritFixVersions,
        Rule::FixedInBuild,
        Rule::Duplicates,
        Rule::Scrum,
        Rule::CloneIssue,
    ];

    /// Stable key, stored in `project_rules` and used in the API.
    pub fn key(self) -> &'static str {
        match self {
            Rule::AssigneeVisibility => "assignee_visibility",
            Rule::PlusOneVote => "plus_one_vote",
            Rule::ComponentOwner => "component_owner",
            Rule::ParentFollowsSubtasks => "subtasks_follow",
            Rule::InheritFixVersions => "inherit_fix_versions",
            Rule::FixedInBuild => "fixed_in_build",
            Rule::Duplicates => "duplicates",
            Rule::Scrum => "scrum",
            Rule::CloneIssue => "clone_issue",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|rule| rule.key() == key)
    }

    /// The rule's name for the tab, the messages and the history badge.
    pub fn name(self, language: Language) -> &'static str {
        self.names().get(language)
    }

    const fn names(self) -> Names {
        match self {
            Rule::AssigneeVisibility => Names::new("Assignee visibility", "Видимость исполнителя"),
            Rule::PlusOneVote => Names::new("+1 votes", "+1 — это голос"),
            Rule::ComponentOwner => Names::new("Component owner", "Владелец компонента"),
            Rule::ParentFollowsSubtasks => {
                Names::new("Parent follows subtasks", "Родитель по подзадачам")
            }
            Rule::InheritFixVersions => Names::new(
                "Subtasks inherit fix versions",
                "Подзадачи наследуют версии",
            ),
            Rule::FixedInBuild => Names::new("Fixed in build", "Сборка исправления"),
            Rule::Duplicates => Names::new("Duplicates", "Дубликаты"),
            Rule::Scrum => Names::new("Scrum rules", "Правила скрама"),
            Rule::CloneIssue => Names::new("Issue cloning", "Клонирование задач"),
        }
    }
}

/// The rules a template turns on in every project made from it (AUT-022). There is no global
/// auto-attach in P1: the template is the only source.
pub fn template_set(template: Template) -> &'static [Rule] {
    match template {
        // PRJ-022's list; kanban takes it too (PRJ-024), its stage rule arrives in P2 disabled.
        Template::Blank | Template::Kanban => &[
            Rule::AssigneeVisibility,
            Rule::CloneIssue,
            Rule::Duplicates,
            Rule::PlusOneVote,
            Rule::ComponentOwner,
            Rule::InheritFixVersions,
            Rule::ParentFollowsSubtasks,
            Rule::FixedInBuild,
        ],
        Template::Scrum => &[
            Rule::AssigneeVisibility,
            Rule::CloneIssue,
            Rule::Duplicates,
            Rule::PlusOneVote,
            Rule::Scrum,
        ],
    }
}

/// What a project lacks for a rule (AUT-023): a pure answer over its attached fields, rendered by
/// the settings tab through i18n, so both UI languages work. Nothing is stored; computed on read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    /// No field of this kind and name is attached; `name` is what it is looked up by.
    Field { name: Names, kind: FieldKind },
    /// The field (by its real name) is attached, but its set holds no such value. `None` stands
    /// for "any value": an empty component set, a state set without a resolved value.
    Value { field: String, value: Option<Names> },
}

/// The fields a project has attached, with everything `needs` looks at; the service fills it from
/// the store. Field names are in the instance language, as everywhere in the product.
pub struct ProjectSnapshot<'a> {
    pub language: Language,
    pub fields: Vec<SnapshotField<'a>>,
}

/// One attached field with the values of its set, in set order.
pub struct SnapshotField<'a> {
    pub name: &'a str,
    pub kind: FieldKind,
    /// The built-in key, when the field is one (`fields.builtin`).
    pub builtin: Option<&'a str>,
    pub multi: bool,
    /// The attachment holds a default value, so a reopen value always exists (plan §3.7).
    pub default: bool,
    pub values: Vec<SnapshotValue<'a>>,
}

/// One value of the field's set.
pub struct SnapshotValue<'a> {
    pub name: &'a str,
    /// State values: an issue holding it counts as resolved (FLD-046).
    pub resolved: bool,
}

/// How a rule looks up the field it needs: by built-in key first, else by name in the instance
/// language, kind and multi-value — the rule `seed::template_field` uses to attach fields.
pub struct FieldRef {
    builtin: Option<&'static str>,
    name: Names,
    kind: FieldKind,
    multi: bool,
}

impl FieldRef {
    /// The built-in key the lookup tries first, when the reference has one.
    pub fn builtin_key(&self) -> Option<&'static str> {
        self.builtin
    }

    /// Whether one attached field is this reference (§3.4): the built-in key when the reference
    /// has one — whatever the field is called — or the name in the instance language with the
    /// kind and multi-value. The engine looks its fields up the same way.
    pub fn matches(
        &self,
        language: Language,
        builtin: Option<&str>,
        name: &str,
        kind: FieldKind,
        multi: bool,
    ) -> bool {
        if self.builtin.is_some() && builtin == self.builtin {
            return true;
        }
        kind == self.kind
            && multi == self.multi
            && name.to_lowercase() == self.name.get(language).to_lowercase()
    }
}

/// The assignee field the visibility and owner behaviors look up (§3.6).
pub const ASSIGNEE: FieldRef = FieldRef {
    builtin: Some("assignee"),
    name: Names::new("Assignee", "Исполнитель"),
    kind: FieldKind::User,
    multi: false,
};
/// The owned field whose values hold owners (§3.6, FLD-049).
pub const COMPONENT: FieldRef = FieldRef {
    builtin: Some("component"),
    name: Names::new("Component", "Компонент"),
    kind: FieldKind::Owned,
    multi: false,
};
/// The multi-value version field the inheritance behavior copies (§3.7, AUT-092).
pub const FIX_VERSIONS: FieldRef = FieldRef {
    builtin: Some("fix_versions"),
    name: Names::new("Fix versions", "Исправлено в версиях"),
    kind: FieldKind::Version,
    multi: true,
};
/// The priority field the duplicates rule compares when two issues share one set (§3.8, AUT-071).
pub const PRIORITY: FieldRef = FieldRef {
    builtin: Some("priority"),
    name: Names::new("Priority", "Приоритет"),
    kind: FieldKind::Enum,
    multi: false,
};
/// The build field the fixed-in-build behavior clears, copies and spreads (§3.7, AUT-098).
pub const FIXED_IN_BUILD: FieldRef = FieldRef {
    builtin: Some("fixed_in_build"),
    name: Names::new("Fixed in build", "Исправлено в сборке"),
    kind: FieldKind::Build,
    multi: false,
};
/// The type field whose Epic and Task values the scrum rules read (§3.9, AUT-085).
pub const TYPE: FieldRef = FieldRef {
    builtin: Some("type"),
    name: Names::new("Type", "Тип"),
    kind: FieldKind::Enum,
    multi: false,
};
/// The scrum template's estimation fields, found by name (§3.9, AUT-085).
pub const STORY_POINTS: FieldRef = FieldRef {
    builtin: None,
    name: Names::new("Story points", "Стори-поинты"),
    kind: FieldKind::Integer,
    multi: false,
};
pub const IDEAL_DAYS: FieldRef = FieldRef {
    builtin: None,
    name: Names::new("Ideal days", "Идеальные дни"),
    kind: FieldKind::Integer,
    multi: false,
};

const STATE: Names = Names::new("State", "Состояние");
/// The reopen value by name (§3.7, AUT-094): the value a reopened parent takes.
pub const REOPENED: Names = Names::new("Reopened", "Открыта повторно");
/// The duplicate state value by name (§3.7, AUT-098): the state that marks an issue a duplicate
/// of its original (§3.8).
pub const DUPLICATE: Names = Names::new("Duplicate", "Дубликат");
/// The type values the scrum rules distinguish (§3.9): an epic counts no ideal days, a task no
/// story points.
pub const EPIC: Names = Names::new("Epic", "Эпик");
pub const TASK: Names = Names::new("Task", "Задача");

/// What a project lacks for a rule to work (AUT-023): a rule stays inert while anything is
/// missing, and the tab shows why.
pub fn needs(rule: Rule, snapshot: &ProjectSnapshot<'_>) -> Vec<Need> {
    match rule {
        // Votes and cloning are built-in: nothing to lack.
        Rule::PlusOneVote | Rule::CloneIssue => Vec::new(),
        Rule::AssigneeVisibility => field_needs(snapshot, &ASSIGNEE),
        Rule::ComponentOwner => {
            let mut missing = field_needs(snapshot, &COMPONENT);
            if let Some(field) = find_field(snapshot, &COMPONENT)
                && field.values.is_empty()
            {
                missing.push(Need::Value {
                    field: field.name.to_owned(),
                    value: None,
                });
            }
            missing
        }
        Rule::ParentFollowsSubtasks => state_needs(snapshot),
        Rule::InheritFixVersions => field_needs(snapshot, &FIX_VERSIONS),
        Rule::FixedInBuild => {
            let mut missing = field_needs(snapshot, &FIXED_IN_BUILD);
            if !snapshot
                .fields
                .iter()
                .any(|field| field.kind == FieldKind::State)
            {
                missing.push(Need::Field {
                    name: STATE,
                    kind: FieldKind::State,
                });
            }
            missing
        }
        Rule::Duplicates => {
            // The Duplicate link type is built-in, always present and locked (D-034): only the
            // state value is a project's to provide.
            let states = snapshot
                .fields
                .iter()
                .filter(|field| field.kind == FieldKind::State)
                .collect::<Vec<_>>();
            match states.first() {
                None => vec![Need::Field {
                    name: STATE,
                    kind: FieldKind::State,
                }],
                Some(_) if states.iter().any(|field| holds_value(field, DUPLICATE)) => Vec::new(),
                Some(first) => vec![Need::Value {
                    field: first.name.to_owned(),
                    value: Some(DUPLICATE),
                }],
            }
        }
        Rule::Scrum => {
            let mut missing = state_needs(snapshot);
            match find_field(snapshot, &TYPE) {
                None => missing.push(Need::Field {
                    name: TYPE.name,
                    kind: TYPE.kind,
                }),
                Some(field) => {
                    for needle in [EPIC, TASK] {
                        if !holds_value(field, needle) {
                            missing.push(Need::Value {
                                field: field.name.to_owned(),
                                value: Some(needle),
                            });
                        }
                    }
                }
            }
            missing.extend(field_needs(snapshot, &STORY_POINTS));
            missing.extend(field_needs(snapshot, &IDEAL_DAYS));
            missing
        }
    }
}

/// The field need when the needle is not attached, nothing when it is.
fn field_needs(snapshot: &ProjectSnapshot<'_>, needle: &FieldRef) -> Vec<Need> {
    match find_field(snapshot, needle) {
        Some(_) => Vec::new(),
        None => vec![Need::Field {
            name: needle.name,
            kind: needle.kind,
        }],
    }
}

fn find_field<'a>(
    snapshot: &'a ProjectSnapshot<'_>,
    needle: &FieldRef,
) -> Option<&'a SnapshotField<'a>> {
    if let Some(key) = needle.builtin
        && let Some(found) = snapshot
            .fields
            .iter()
            .find(|field| field.builtin == Some(key))
    {
        return Some(found);
    }
    snapshot.fields.iter().find(|field| {
        needle.matches(
            snapshot.language,
            field.builtin,
            field.name,
            field.kind,
            field.multi,
        )
    })
}

/// What the state field of a project lacks (plan §3.7): a resolved value and a reopen value —
/// a value named «Открыта повторно»/Reopened, or the set's default.
fn state_needs(snapshot: &ProjectSnapshot<'_>) -> Vec<Need> {
    let fields = snapshot
        .fields
        .iter()
        .filter(|field| field.kind == FieldKind::State)
        .collect::<Vec<_>>();
    let Some(first) = fields.first() else {
        return vec![Need::Field {
            name: STATE,
            kind: FieldKind::State,
        }];
    };
    // The state field of a project: its first attached state field that has a resolved value.
    let field = fields
        .iter()
        .find(|field| field.values.iter().any(|value| value.resolved))
        .unwrap_or(first);
    let mut missing = Vec::new();
    if !field.values.iter().any(|value| value.resolved) {
        missing.push(Need::Value {
            field: field.name.to_owned(),
            value: None,
        });
    }
    if !field.default && !holds_value(field, REOPENED) {
        missing.push(Need::Value {
            field: field.name.to_owned(),
            value: Some(REOPENED),
        });
    }
    missing
}

/// Value lookup (plan §3.4): by name, case-insensitively, English and Russian alike.
fn holds_value(field: &SnapshotField<'_>, needle: Names) -> bool {
    field.values.iter().any(|value| {
        value.name.to_lowercase() == needle.en.to_lowercase()
            || value.name.to_lowercase() == needle.ru.to_lowercase()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(language: Language, fields: Vec<SnapshotField<'_>>) -> ProjectSnapshot<'_> {
        ProjectSnapshot { language, fields }
    }

    fn field<'a>(
        name: &'a str,
        kind: FieldKind,
        builtin: Option<&'a str>,
        multi: bool,
        default: bool,
        values: &'a [(&'a str, bool)],
    ) -> SnapshotField<'a> {
        SnapshotField {
            name,
            kind,
            builtin,
            multi,
            default,
            values: values
                .iter()
                .map(|(name, resolved)| SnapshotValue {
                    name,
                    resolved: *resolved,
                })
                .collect(),
        }
    }

    #[test]
    fn prj_022_blank_set() {
        assert_eq!(
            template_set(Template::Blank),
            &[
                Rule::AssigneeVisibility,
                Rule::CloneIssue,
                Rule::Duplicates,
                Rule::PlusOneVote,
                Rule::ComponentOwner,
                Rule::InheritFixVersions,
                Rule::ParentFollowsSubtasks,
                Rule::FixedInBuild,
            ]
        );
        // Kanban takes the blank set (PRJ-024); its stage rule arrives in P2, disabled.
        assert_eq!(
            template_set(Template::Kanban),
            template_set(Template::Blank)
        );
    }

    #[test]
    fn prj_023_scrum_set() {
        assert_eq!(
            template_set(Template::Scrum),
            &[
                Rule::AssigneeVisibility,
                Rule::CloneIssue,
                Rule::Duplicates,
                Rule::PlusOneVote,
                Rule::Scrum,
            ]
        );
    }

    #[test]
    fn aut_023_matches_field() {
        // The built-in key wins whatever the field is called; a name match needs the kind and
        // the multi-value, in the instance language, case-insensitively.
        assert!(ASSIGNEE.matches(
            Language::En,
            Some("assignee"),
            "Whom to ask",
            FieldKind::User,
            false
        ));
        assert!(ASSIGNEE.matches(Language::Ru, None, "исполнитель", FieldKind::User, false));
        assert!(!ASSIGNEE.matches(Language::En, None, "Assignee", FieldKind::User, true));
        assert!(!ASSIGNEE.matches(Language::En, None, "Assignee", FieldKind::Enum, false));
        // A field of another kind under the name does not count.
        assert!(!COMPONENT.matches(Language::En, None, "Component", FieldKind::User, false));
        assert!(!COMPONENT.matches(
            Language::En,
            Some("state"),
            "State",
            FieldKind::State,
            false
        ));
    }

    #[test]
    fn aut_023_keys_roundtrip() {
        for rule in Rule::ALL {
            assert_eq!(Rule::parse(rule.key()), Some(rule));
            assert!(!rule.name(Language::En).is_empty());
            assert!(!rule.name(Language::Ru).is_empty());
        }
        assert_eq!(Rule::parse("no_such_rule"), None);
    }

    #[test]
    fn aut_023_needs_builtin_rules() {
        // Votes and cloning are built-in: nothing to lack.
        let empty = snapshot(Language::En, Vec::new());
        assert!(needs(Rule::PlusOneVote, &empty).is_empty());
        assert!(needs(Rule::CloneIssue, &empty).is_empty());
        // The assignee is found by its built-in key whatever it is called.
        let renamed = snapshot(
            Language::En,
            vec![field(
                "Whom",
                FieldKind::User,
                Some("assignee"),
                false,
                false,
                &[],
            )],
        );
        assert!(needs(Rule::AssigneeVisibility, &renamed).is_empty());
        // …or by name in the instance language, case-insensitively.
        let named = snapshot(
            Language::Ru,
            vec![field(
                "исполнитель",
                FieldKind::User,
                None,
                false,
                false,
                &[],
            )],
        );
        assert!(needs(Rule::AssigneeVisibility, &named).is_empty());
        // A user field under another name does not count.
        assert_eq!(
            needs(Rule::AssigneeVisibility, &empty),
            vec![Need::Field {
                name: ASSIGNEE.name,
                kind: FieldKind::User,
            }]
        );
    }

    #[test]
    fn aut_023_needs_component_owner() {
        let with_values = snapshot(
            Language::En,
            vec![field(
                "Component",
                FieldKind::Owned,
                Some("component"),
                false,
                false,
                &[("Server", false)],
            )],
        );
        assert!(needs(Rule::ComponentOwner, &with_values).is_empty());
        // An owned field without values cannot assign anyone.
        let empty_set = snapshot(
            Language::En,
            vec![field(
                "Component",
                FieldKind::Owned,
                Some("component"),
                false,
                false,
                &[],
            )],
        );
        assert_eq!(
            needs(Rule::ComponentOwner, &empty_set),
            vec![Need::Value {
                field: "Component".to_owned(),
                value: None,
            }]
        );
    }

    #[test]
    fn aut_023_needs_state_rules() {
        let state = |name: &'static str, values: &'static [(&'static str, bool)], default: bool| {
            field(name, FieldKind::State, None, false, default, values)
        };
        // A state field with a resolved value and a reopen value (here: the set's default).
        let full = snapshot(
            Language::En,
            vec![state("Stage", &[("Backlog", false), ("Done", true)], true)],
        );
        assert!(needs(Rule::ParentFollowsSubtasks, &full).is_empty());
        // No state field at all.
        let none = snapshot(Language::En, Vec::new());
        assert_eq!(
            needs(Rule::ParentFollowsSubtasks, &none),
            vec![Need::Field {
                name: STATE,
                kind: FieldKind::State,
            }]
        );
        // A state field without a resolved value, without a reopen value and without a default.
        let bare = snapshot(
            Language::En,
            vec![state("Stage", &[("Backlog", false)], false)],
        );
        assert_eq!(
            needs(Rule::ParentFollowsSubtasks, &bare),
            vec![
                Need::Value {
                    field: "Stage".to_owned(),
                    value: None,
                },
                Need::Value {
                    field: "Stage".to_owned(),
                    value: Some(REOPENED),
                },
            ]
        );
        // The reopen value may be a value named «Открыта повторно», in either language.
        let reopened = snapshot(
            Language::En,
            vec![state(
                "State",
                &[("Open", false), ("Reopened", false), ("Done", true)],
                false,
            )],
        );
        assert!(needs(Rule::ParentFollowsSubtasks, &reopened).is_empty());
    }

    #[test]
    fn aut_023_needs_version_and_build() {
        let versions = snapshot(
            Language::En,
            vec![field(
                "Fix versions",
                FieldKind::Version,
                Some("fix_versions"),
                true,
                false,
                &[],
            )],
        );
        assert!(needs(Rule::InheritFixVersions, &versions).is_empty());
        // A single-value version field of the same name does not count: the rule copies a set.
        // (The built-in key always wins, but the built-in fix-versions field is multi itself.)
        let single = snapshot(
            Language::En,
            vec![field(
                "Fix versions",
                FieldKind::Version,
                None,
                false,
                false,
                &[],
            )],
        );
        assert_eq!(
            needs(Rule::InheritFixVersions, &single),
            vec![Need::Field {
                name: FIX_VERSIONS.name,
                kind: FieldKind::Version,
            }]
        );
        // Fixed in build needs the build field and any state field.
        let build_only = snapshot(
            Language::En,
            vec![field(
                "Fixed in build",
                FieldKind::Build,
                Some("fixed_in_build"),
                false,
                false,
                &[],
            )],
        );
        assert_eq!(
            needs(Rule::FixedInBuild, &build_only),
            vec![Need::Field {
                name: STATE,
                kind: FieldKind::State,
            }]
        );
    }

    #[test]
    fn aut_023_needs_duplicates() {
        // The duplicate value is looked up by name, in either language.
        let with = snapshot(
            Language::En,
            vec![field(
                "State",
                FieldKind::State,
                Some("state"),
                false,
                true,
                &[("Open", false), ("Дубликат", false), ("Done", true)],
            )],
        );
        assert!(needs(Rule::Duplicates, &with).is_empty());
        let without = snapshot(
            Language::En,
            vec![field(
                "State",
                FieldKind::State,
                Some("state"),
                false,
                true,
                &[("Open", false), ("Done", true)],
            )],
        );
        assert_eq!(
            needs(Rule::Duplicates, &without),
            vec![Need::Value {
                field: "State".to_owned(),
                value: Some(DUPLICATE),
            }]
        );
    }

    #[test]
    fn aut_023_needs_scrum() {
        let full = snapshot(
            Language::En,
            vec![
                field(
                    "State",
                    FieldKind::State,
                    Some("state"),
                    false,
                    true,
                    &[("Open", false), ("Done", true)],
                ),
                field(
                    "Type",
                    FieldKind::Enum,
                    Some("type"),
                    false,
                    false,
                    &[("Epic", false), ("Task", false)],
                ),
                field("Story points", FieldKind::Integer, None, false, false, &[]),
                field("Ideal days", FieldKind::Integer, None, false, false, &[]),
            ],
        );
        assert!(needs(Rule::Scrum, &full).is_empty());
        // Missing estimation fields and a type without the values are named one by one.
        let partial = snapshot(
            Language::Ru,
            vec![
                field(
                    "Состояния",
                    FieldKind::State,
                    None,
                    false,
                    true,
                    &[("Открыта", false), ("Готово", true)],
                ),
                field(
                    "Тип",
                    FieldKind::Enum,
                    Some("type"),
                    false,
                    false,
                    &[("Эпик", false)],
                ),
            ],
        );
        assert_eq!(
            needs(Rule::Scrum, &partial),
            vec![
                Need::Value {
                    field: "Тип".to_owned(),
                    value: Some(TASK),
                },
                Need::Field {
                    name: STORY_POINTS.name,
                    kind: FieldKind::Integer,
                },
                Need::Field {
                    name: IDEAL_DAYS.name,
                    kind: FieldKind::Integer,
                },
            ]
        );
    }
}
