//! The reference dataset as plain data: fields, value sets, projects and their teams, and which
//! project each issue belongs to. Built from the plan and the seed alone, without the database,
//! so every machine gets the same dataset.

use std::collections::{HashMap, HashSet};

use crate::rng::{Rng, Weighted};
use crate::text::Vocabulary;

use super::Plan;

/// Field names the scenarios look up.
pub mod names {
    pub const STATE: &str = "State";
    pub const PRIORITY: &str = "Priority";
    pub const TYPE: &str = "Type";
    pub const ASSIGNEE: &str = "Assignee";
    pub const FIX_VERSIONS: &str = "Fix versions";
    pub const SPRINT: &str = "Sprint";
    /// The issue type most queries filter by.
    pub const BUG: &str = "Bug";
    /// A resolved state of the main state set.
    pub const FIXED: &str = "Fixed";
    /// The two highest priorities.
    pub const URGENT: [&str; 2] = ["Blocker", "Critical"];
    /// Unresolved states of the main state set.
    pub const OPEN: [&str; 2] = ["Open", "In progress"];
}

/// Fields per instance (architecture.md §7).
pub const FIELDS: usize = 60;
/// Projects with this many or more issues are the busiest: all of them run sprints.
const SPRINT_PROJECTS: usize = 20;
/// Zipf exponent of project sizes: the largest project has about a tenth of all issues.
const PROJECT_EXPONENT: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Enum,
    State,
    User,
    Group,
    Version,
    Build,
    Owned,
    Integer,
    Duration,
    Float,
    Date,
    DateTime,
    String,
    Text,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Enum => "enum",
            Self::State => "state",
            Self::User => "user",
            Self::Group => "group",
            Self::Version => "version",
            Self::Build => "build",
            Self::Owned => "owned",
            Self::Integer => "integer",
            Self::Duration => "duration",
            Self::Float => "float",
            Self::Date => "date",
            Self::DateTime => "date_time",
            Self::String => "string",
            Self::Text => "text",
        }
    }

    /// The kind of the value sets of fields of this kind (`value_sets.kind`); none for kinds
    /// without sets, group fields included: their values are groups.
    pub fn set_kind(self) -> Option<&'static str> {
        match self {
            Self::Enum => Some("enum"),
            Self::State => Some("state"),
            Self::User => Some("user"),
            Self::Version => Some("version"),
            Self::Build => Some("build"),
            Self::Owned => Some("owned"),
            _ => None,
        }
    }

    /// Values come from a list of the model: a value set, or groups for group fields. User fields
    /// refer to users instead.
    pub fn has_set(self) -> bool {
        matches!(
            self,
            Self::Enum | Self::State | Self::Group | Self::Version | Self::Build | Self::Owned
        )
    }
}

/// Fields whose values follow a rule of their own; the rest get random values of their kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    State,
    Assignee,
    FixVersions,
    AffectedVersions,
    FixedInBuild,
    Sprint,
    SpentTime,
    Other,
}

/// Where a field's value set comes from.
#[derive(Debug, Clone, Copy)]
enum Sets {
    /// None: user and scalar fields.
    None,
    /// One set for all projects, built from fixed names.
    Fixed(&'static [(&'static str, bool, f64)]),
    /// One set for all projects with this many generated names.
    Shared(usize),
    /// A set of its own in each project, sized by the project.
    PerProject,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: i32,
    pub name: String,
    pub kind: Kind,
    pub multi: bool,
    pub role: Role,
    /// Share of projects that use the field.
    pub attach: f64,
    /// Share of a project's issues that have a value.
    pub fill: f64,
    /// Index into [`Model::sets`]: the set new attachments start from (`fields.set_id`); none for
    /// group and scalar fields.
    pub set: Option<usize>,
    sets: Sets,
}

#[derive(Debug, Clone)]
pub struct Value {
    /// A `field_values` ID, or a group ID in the list of a group field.
    pub id: i64,
    pub name: String,
    pub resolved: bool,
}

/// Allowed values of a reference field in one or more projects, in their order: a value set, or
/// the list of groups of a group field (which is no value set: group fields refer to groups).
#[derive(Debug, Clone)]
pub struct ValueSet {
    /// The `value_sets` ID; 0 for a list of groups.
    pub id: i32,
    /// Unique per kind ignoring case.
    pub name: String,
    /// The kind of the fields that use it.
    pub kind: Kind,
    /// Values in their order; empty in sets of people, which hold groups ([`ValueSet::team`]).
    pub values: Vec<Value>,
    /// Sets of people: the index of the project whose team they hold; none for all members.
    pub team: Option<usize>,
    /// How often each value is used.
    pub weights: Weighted,
    /// The same weights, limited to resolved or unresolved values (for states).
    pub resolved_weights: Weighted,
    pub unresolved_weights: Weighted,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectField {
    /// Index into [`Model::fields`].
    pub field: usize,
    /// Index into [`Model::sets`]: where the field's values on issues come from; none for user
    /// and scalar fields.
    pub set: Option<usize>,
    /// Index into [`Model::sets`]: the value set of the attachment (`project_fields.set_id`), the
    /// set above or a set of people; none for group and scalar fields.
    pub stored_set: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i32,
    pub key: String,
    /// Unique ignoring case.
    pub name: String,
    pub issues: u32,
    pub fields: Vec<ProjectField>,
    /// User IDs, the most active first.
    pub team: Vec<i64>,
    /// The ID of the group of the project team, whose members are [`Project::team`].
    pub team_group: i64,
    pub team_weights: Weighted,
    /// Issues per sprint, in creation order.
    pub sprint_size: u32,
    /// The project's sprint board; the busiest projects have one.
    pub board: Option<BoardPlan>,
}

/// A sprint board over one project (BRD, step 1.8): its columns are the project's states and its
/// sprints the values of the project's sprint field, each holding the issues with that value.
#[derive(Debug, Clone, Copy)]
pub struct BoardPlan {
    pub id: i64,
    /// Sprint IDs are consecutive from here, one per value of `sprint_set`.
    pub first_sprint: i64,
    /// Index into [`Model::sets`].
    pub sprint_set: usize,
    /// Index into [`Model::fields`] and [`Model::sets`]: the column field and its set.
    pub state_field: usize,
    pub state_set: usize,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub plan: Plan,
    pub fields: Vec<Field>,
    pub sets: Vec<ValueSet>,
    pub projects: Vec<Project>,
    pub users: Vec<i64>,
    /// Some people work on many projects, most on a few.
    pub user_weights: Weighted,
    /// Project index of issue `id − 1`.
    pub issue_project: Vec<u32>,
    /// Number in its project of issue `id − 1`.
    pub issue_number: Vec<u32>,
    pub vocabulary: Vocabulary,
    /// Card ranks in order (`rank::sequence`), enough for the largest sprint.
    pub ranks: Vec<String>,
    /// The parent chain of every 20th issue (step 1.9 §3.14): the subtask link's parent of the
    /// issues that belong to a chain, so the parent-follows walk climbs three levels. An issue in
    /// the map takes this subtask link instead of the random one.
    pub chains: HashMap<u64, i64>,
    /// IDs the next value set, value and group get.
    next_set: i32,
    next_value: i64,
    next_group: i64,
}

const STATES_DEV: &[(&str, bool, f64)] = &[
    ("Open", false, 0.55),
    ("In progress", false, 0.3),
    ("In review", false, 0.15),
    ("Fixed", true, 0.6),
    ("Verified", true, 0.15),
    ("Won't fix", true, 0.15),
    ("Duplicate", true, 0.1),
];
const STATES_SUPPORT: &[(&str, bool, f64)] = &[
    ("Submitted", false, 0.5),
    ("Waiting for reply", false, 0.5),
    ("Answered", true, 0.6),
    ("Closed", true, 0.3),
    ("Rejected", true, 0.1),
];
const STATES_SIMPLE: &[(&str, bool, f64)] = &[
    ("To do", false, 0.6),
    ("Doing", false, 0.4),
    ("Done", true, 1.0),
];
const PRIORITIES: &[(&str, bool, f64)] = &[
    ("Blocker", false, 0.02),
    ("Critical", false, 0.08),
    ("Major", false, 0.25),
    ("Normal", false, 0.55),
    ("Minor", false, 0.1),
];
const TYPES_DEV: &[(&str, bool, f64)] = &[
    ("Bug", false, 0.4),
    ("Feature", false, 0.2),
    ("Task", false, 0.25),
    ("Improvement", false, 0.12),
    ("Epic", false, 0.03),
];
const TYPES_SUPPORT: &[(&str, bool, f64)] = &[
    ("Question", false, 0.5),
    ("Incident", false, 0.3),
    ("Request", false, 0.2),
];
const SEVERITIES: &[(&str, bool, f64)] = &[
    ("Cosmetic", false, 0.3),
    ("Minor", false, 0.4),
    ("Major", false, 0.2),
    ("Critical", false, 0.1),
];

/// The first twenty fields: what most trackers have.
fn core_fields() -> Vec<(&'static str, Kind, bool, Role, f64, f64, Sets)> {
    use Kind as K;
    use Role as R;
    vec![
        (
            names::STATE,
            K::State,
            false,
            R::State,
            1.0,
            1.0,
            Sets::Fixed(STATES_DEV),
        ),
        (
            names::PRIORITY,
            K::Enum,
            false,
            R::Other,
            1.0,
            1.0,
            Sets::Fixed(PRIORITIES),
        ),
        (
            names::TYPE,
            K::Enum,
            false,
            R::Other,
            1.0,
            1.0,
            Sets::Fixed(TYPES_DEV),
        ),
        (
            names::ASSIGNEE,
            K::User,
            false,
            R::Assignee,
            1.0,
            0.75,
            Sets::None,
        ),
        (
            "Subsystem",
            K::Owned,
            false,
            R::Other,
            0.7,
            0.6,
            Sets::PerProject,
        ),
        (
            names::FIX_VERSIONS,
            K::Version,
            true,
            R::FixVersions,
            0.6,
            0.5,
            Sets::PerProject,
        ),
        (
            "Affected versions",
            K::Version,
            true,
            R::AffectedVersions,
            0.4,
            0.3,
            Sets::PerProject,
        ),
        (
            "Fixed in build",
            K::Build,
            false,
            R::FixedInBuild,
            0.3,
            0.6,
            Sets::PerProject,
        ),
        (
            names::SPRINT,
            K::Version,
            true,
            R::Sprint,
            0.4,
            0.85,
            Sets::PerProject,
        ),
        (
            "Estimation",
            K::Duration,
            false,
            R::Other,
            0.4,
            0.3,
            Sets::None,
        ),
        (
            "Spent time",
            K::Duration,
            false,
            R::SpentTime,
            0.4,
            0.3,
            Sets::None,
        ),
        ("Due date", K::Date, false, R::Other, 0.3, 0.1, Sets::None),
        (
            "Story points",
            K::Float,
            false,
            R::Other,
            0.25,
            0.3,
            Sets::None,
        ),
        ("Reviewer", K::User, false, R::Other, 0.2, 0.3, Sets::None),
        (
            "Team",
            K::Group,
            false,
            R::Other,
            0.2,
            0.6,
            Sets::Shared(50),
        ),
        (
            "Severity",
            K::Enum,
            false,
            R::Other,
            0.2,
            0.7,
            Sets::Fixed(SEVERITIES),
        ),
        (
            "Customer",
            K::Enum,
            false,
            R::Other,
            0.1,
            0.5,
            Sets::Shared(300),
        ),
        (
            "Environment",
            K::String,
            false,
            R::Other,
            0.15,
            0.4,
            Sets::None,
        ),
        (
            "Steps to reproduce",
            K::Text,
            false,
            R::Other,
            0.15,
            0.25,
            Sets::None,
        ),
        (
            "Deadline",
            K::DateTime,
            false,
            R::Other,
            0.05,
            0.2,
            Sets::None,
        ),
    ]
}

/// Kinds of the fields that big instances collect over the years, used by a few projects each.
const EXTRA_KINDS: [(Kind, bool); 10] = [
    (Kind::Enum, false),
    (Kind::Enum, false),
    (Kind::Enum, true),
    (Kind::User, false),
    (Kind::Integer, false),
    (Kind::String, false),
    (Kind::Date, false),
    (Kind::Float, false),
    (Kind::Text, false),
    (Kind::Owned, false),
];

impl Model {
    /// The model for `plan`, with the IDs the users got in the database; groups it adds get IDs
    /// from `first_group` on.
    pub fn build(plan: Plan, users: Vec<i64>, first_group: i64) -> Self {
        let seed = plan.seed;
        let vocabulary = Vocabulary::new(seed);
        let fields = build_fields(seed);
        let project_count = plan.projects();
        let (issue_project, issue_number, sizes) = assign_issues(plan, project_count);
        let user_weights = Weighted::zipf(users.len(), 0.7);

        let mut model = Self {
            plan,
            fields,
            sets: Vec::new(),
            projects: Vec::new(),
            users,
            user_weights,
            issue_project,
            issue_number,
            vocabulary,
            ranks: taskmaster_domain::rank::sequence(sprint_size(u32::MAX) as usize),
            chains: HashMap::new(),
            next_set: 1,
            next_value: 1,
            next_group: first_group,
        };
        let shared = model.build_shared_sets();
        for (index, issues) in sizes.into_iter().enumerate() {
            let project = model.build_project(index, issues, &shared);
            model.projects.push(project);
        }
        model.plan_boards();
        model.plan_chains();
        model
    }

    /// Chains of depth 3 for every 20th issue (§3.14): parent, grandparent and great-grandparent,
    /// each an earlier issue of the same project, so no loop forms and each chain member keeps one
    /// parent. Issues already in a chain are left alone: one chain role per issue.
    fn plan_chains(&mut self) {
        const STEP: u64 = 20;
        let mut chains: HashMap<u64, i64> = HashMap::new();
        for bottom in (STEP..=self.issue_count()).step_by(STEP as usize) {
            let mut rng = Rng::for_item(self.plan.seed, "chain", bottom);
            let mut run: Vec<i64> = vec![bottom as i64];
            while run.len() < 4 {
                let earlier = chain_parent(self, *run.last().unwrap_or(&0), &mut rng);
                match earlier {
                    Some(parent) if !chains.contains_key(&(parent as u64)) => run.push(parent),
                    _ => break,
                }
            }
            if run.len() == 4 {
                for (child, parent) in run.iter().zip(&run[1..]) {
                    chains.insert(*child as u64, *parent);
                }
            }
        }
        self.chains = chains;
    }

    /// A sprint board for each of the busiest projects, which always run sprints.
    fn plan_boards(&mut self) {
        let (mut board, mut sprint) = (1, 1);
        for project in self.projects.iter_mut().take(SPRINT_PROJECTS) {
            let attached = |role: Role| {
                project.fields.iter().find_map(|f| {
                    let def = self.fields.get(f.field)?;
                    (def.role == role).then_some((f.field, f.set?))
                })
            };
            let (Some((_, sprint_set)), Some((state_field, state_set))) =
                (attached(Role::Sprint), attached(Role::State))
            else {
                continue;
            };
            project.board = Some(BoardPlan {
                id: board,
                first_sprint: sprint,
                sprint_set,
                state_field,
                state_set,
            });
            board += 1;
            sprint += self
                .sets
                .get(sprint_set)
                .map_or(0, |s| s.values.len() as i64);
        }
    }

    pub fn issue_count(&self) -> u64 {
        self.issue_project.len() as u64
    }

    /// Adds a set of `kind` with these values (name, resolved, weight); names that repeat ignoring
    /// case get a number, as the database requires. A list of groups takes group IDs.
    fn add_set(&mut self, kind: Kind, name: String, values: Vec<(String, bool, f64)>) -> usize {
        let groups = kind == Kind::Group;
        let (id, first) = if groups {
            (0, self.next_group)
        } else {
            (self.next_set, self.next_value)
        };
        let weights = Weighted::new(values.iter().map(|v| v.2));
        let resolved_weights = Weighted::new(values.iter().map(|v| if v.1 { v.2 } else { 0.0 }));
        let unresolved_weights = Weighted::new(values.iter().map(|v| if v.1 { 0.0 } else { v.2 }));
        let mut seen = HashSet::with_capacity(values.len());
        let values: Vec<Value> = values
            .into_iter()
            .zip(first..)
            .map(|((name, resolved, _), id)| Value {
                id,
                name: unique_name(&mut seen, name),
                resolved,
            })
            .collect();
        let next = first + values.len() as i64;
        if groups {
            self.next_group = next;
        } else {
            self.next_set += 1;
            self.next_value = next;
        }
        self.sets.push(ValueSet {
            id,
            name,
            kind,
            values,
            team: None,
            weights,
            resolved_weights,
            unresolved_weights,
        });
        self.sets.len() - 1
    }

    /// Sets shared by all projects, by field index; states and types get alternatives. Every
    /// field with a set kind also gets the set new attachments start from: its shared set, an
    /// empty one named after it when each project has its own, all members for user fields.
    fn build_shared_sets(&mut self) -> SharedSets {
        let mut shared = SharedSets::default();
        let mut rng = Rng::for_item(self.plan.seed, "sets", 0);
        for index in 0..self.fields.len() {
            let field = &self.fields[index];
            let (kind, name, sets) = (field.kind, field.name.clone(), field.sets);
            let set = match sets {
                Sets::Fixed(values) => Some(self.add_set(kind, name.clone(), fixed(values))),
                Sets::Shared(count) => {
                    let values = (0..count)
                        .map(|i| (self.word_name(&mut rng, 300 + i), false, 1.0))
                        .collect();
                    Some(self.add_set(kind, name.clone(), values))
                }
                Sets::None | Sets::PerProject => None,
            };
            shared.by_field.push(set);
            self.fields[index].set = match sets {
                _ if kind.set_kind().is_none() => None,
                Sets::Fixed(_) | Sets::Shared(_) => set,
                Sets::None | Sets::PerProject => Some(self.add_set(kind, name, Vec::new())),
            };
        }
        shared.support_states = self.add_set(
            Kind::State,
            "Support states".to_owned(),
            fixed(STATES_SUPPORT),
        );
        shared.simple_states = self.add_set(
            Kind::State,
            "Simple states".to_owned(),
            fixed(STATES_SIMPLE),
        );
        shared.support_types =
            self.add_set(Kind::Enum, "Support types".to_owned(), fixed(TYPES_SUPPORT));
        shared
    }

    /// A capitalized vocabulary word, for value and project names.
    fn word_name(&self, rng: &mut Rng, around: usize) -> String {
        let word = self.vocabulary.at(around + rng.index(around.max(1)));
        let mut chars = word.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    }

    fn build_project(&mut self, index: usize, issues: u32, shared: &SharedSets) -> Project {
        let mut rng = Rng::for_item(self.plan.seed, "project", index as u64);
        let id = i32::try_from(index + 1).unwrap_or(i32::MAX);
        let name = format!(
            "{} {}",
            self.word_name(&mut rng, 1000),
            self.word_name(&mut rng, 3000)
        );
        let key = project_key(&name, index);
        let mut seen: HashSet<String> = self
            .projects
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect();
        let name = unique_name(&mut seen, name);
        // Workflow flavour: most projects develop software, some run support, a few are simple.
        let flavour = rng.unit();
        let support = (0.75..0.9).contains(&flavour);
        let simple = flavour >= 0.9;

        let mut fields = Vec::new();
        for field in 0..self.fields.len() {
            let def = &self.fields[field];
            let (role, attach, sets) = (def.role, def.attach, def.sets);
            let (state, kind) = (def.name == names::STATE, def.name == names::TYPE);
            // Every field is used by at least one project.
            let owner = field % self.plan.projects() == index;
            let attached = owner
                || match role {
                    Role::Sprint => index < SPRINT_PROJECTS || rng.chance(attach),
                    _ => attach >= 1.0 || rng.chance(attach),
                };
            if !attached {
                continue;
            }
            let set = match sets {
                _ if state && support => Some(shared.support_states),
                _ if state && simple => Some(shared.simple_states),
                _ if kind && support => Some(shared.support_types),
                Sets::PerProject => Some(self.project_set(field, &name, issues, &mut rng)),
                _ => shared.by_field.get(field).copied().flatten(),
            };
            // Assignees come from the project team, held by a set of the project's own; other
            // people fields share the field's set of all members.
            let def = &self.fields[field];
            let stored_set = match def.kind {
                Kind::User if def.role == Role::Assignee => {
                    let people = format!("{name}: {}", def.name);
                    let people = self.add_set(Kind::User, people, Vec::new());
                    self.sets[people].team = Some(index);
                    Some(people)
                }
                Kind::User => def.set,
                Kind::Group => None,
                _ => set,
            };
            fields.push(ProjectField {
                field,
                set,
                stored_set,
            });
        }

        let team_size = (issues / 400).clamp(5, 150) as usize;
        let mut team: Vec<i64> = Vec::with_capacity(team_size);
        for _ in 0..team_size * 4 {
            if team.len() == team_size || self.users.is_empty() {
                break;
            }
            let user = self.users[self.user_weights.sample(&mut rng)];
            if !team.contains(&user) {
                team.push(user);
            }
        }
        let team_weights = Weighted::zipf(team.len(), 1.0);
        let team_group = self.next_group;
        self.next_group += 1;
        Project {
            id,
            key,
            name,
            issues,
            fields,
            team,
            team_group,
            team_weights,
            sprint_size: sprint_size(issues),
            board: None,
        }
    }

    /// A value set of a project's own, sized by its number of issues.
    fn project_set(&mut self, field: usize, project: &str, issues: u32, rng: &mut Rng) -> usize {
        let def = &self.fields[field];
        let (kind, name) = (def.kind, format!("{project}: {}", def.name));
        let values: Vec<(String, bool, f64)> = match def.role {
            Role::FixVersions | Role::AffectedVersions => (0..version_count(issues))
                .map(|k| (format!("{}.{}", 1 + k / 5, k % 5), false, 1.0))
                .collect(),
            Role::FixedInBuild => (0..build_count(issues))
                .map(|k| (format!("{}", 1000 + k * 7), false, 1.0))
                .collect(),
            Role::Sprint => (0..issues.div_ceil(sprint_size(issues)).max(1))
                .map(|k| (format!("Sprint {}", k + 1), false, 1.0))
                .collect(),
            _ => {
                let count = 4 + rng.index(17);
                (0..count)
                    .map(|k| {
                        let weight = 1.0 / (k as f64 + 1.0);
                        (
                            format!("{} {}", self.word_name(rng, 500), k + 1),
                            false,
                            weight,
                        )
                    })
                    .collect()
            }
        };
        self.add_set(kind, name, values)
    }
}

#[derive(Debug, Default)]
struct SharedSets {
    by_field: Vec<Option<usize>>,
    support_states: usize,
    simple_states: usize,
    support_types: usize,
}

/// `name`, or with a number if `seen` has it ignoring case; remembers the result.
fn unique_name(seen: &mut HashSet<String>, name: String) -> String {
    let mut unique = name.clone();
    let mut number = 2;
    while !seen.insert(unique.to_lowercase()) {
        unique = format!("{name} {number}");
        number += 1;
    }
    unique
}

fn fixed(values: &[(&str, bool, f64)]) -> Vec<(String, bool, f64)> {
    values
        .iter()
        .map(|&(name, resolved, weight)| (name.to_owned(), resolved, weight))
        .collect()
}

fn build_fields(seed: u64) -> Vec<Field> {
    let mut fields: Vec<Field> = core_fields()
        .into_iter()
        .map(|(name, kind, multi, role, attach, fill, sets)| Field {
            id: 0,
            name: name.to_owned(),
            kind,
            multi,
            role,
            attach,
            fill,
            set: None,
            sets,
        })
        .collect();
    let mut rng = Rng::for_item(seed, "fields", 0);
    while fields.len() < FIELDS {
        let number = fields.len() + 1;
        let (kind, multi) = EXTRA_KINDS[number % EXTRA_KINDS.len()];
        let sets = if kind == Kind::Owned {
            Sets::PerProject
        } else if kind.has_set() {
            Sets::Shared(3 + rng.index(28))
        } else {
            Sets::None
        };
        fields.push(Field {
            id: 0,
            name: format!("Custom {number} ({})", kind.name()),
            kind,
            multi,
            role: Role::Other,
            attach: 0.04 + rng.unit() * 0.08,
            fill: 0.2 + rng.unit() * 0.4,
            set: None,
            sets,
        });
    }
    for (field, id) in fields.iter_mut().zip(1..) {
        field.id = id;
    }
    fields
}

/// Spreads the issues over projects by Zipf's law and numbers them in creation order.
fn assign_issues(plan: Plan, projects: usize) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let weights = Weighted::zipf(projects, PROJECT_EXPONENT);
    let total = plan.issues();
    let mut project_of = Vec::with_capacity(total as usize);
    let mut number_of = Vec::with_capacity(total as usize);
    let mut sizes = vec![0_u32; projects];
    for issue in 1..=total {
        let mut rng = Rng::for_item(plan.seed, "issue-project", issue);
        let project = weights.sample(&mut rng);
        sizes[project] += 1;
        project_of.push(project as u32);
        number_of.push(sizes[project]);
    }
    (project_of, number_of, sizes)
}

/// Issues per sprint: big projects run sprints of about 500 cards (the board scenario).
pub fn sprint_size(issues: u32) -> u32 {
    (issues / 20).clamp(20, 590)
}

/// An earlier issue of `id`'s project to be its parent, like the links the issues themselves pick
/// (`issues::earlier_in_project`): within the links' lookback, at random.
fn chain_parent(model: &Model, id: i64, rng: &mut Rng) -> Option<i64> {
    let lookback = i64::try_from(super::issues::LINK_LOOKBACK).ok()?;
    let project = model.issue_project.get(id.checked_sub(1)? as usize)?;
    let mut found = [0_i64; 8];
    let mut count = 0;
    let mut at = id - 1;
    while at >= 1 && id - at <= lookback && count < found.len() {
        if model.issue_project.get((at - 1) as usize) == Some(project) {
            found[count] = at;
            count += 1;
        }
        at -= 1;
    }
    (count > 0).then(|| found[rng.index(count)])
}

fn version_count(issues: u32) -> u32 {
    (issues / 1500).clamp(3, 60)
}

fn build_count(issues: u32) -> u32 {
    (issues / 300).clamp(5, 400)
}

/// Uppercase letters of the name, made unique by the project's index.
fn project_key(name: &str, index: usize) -> String {
    let letters: String = name
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(3)
        .collect::<String>()
        .to_ascii_uppercase();
    let mut suffix = String::new();
    let mut n = index;
    loop {
        suffix.insert(0, char::from(b'A' + (n % 26) as u8));
        n /= 26;
        if n == 0 {
            break;
        }
    }
    format!("{letters}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        let plan = Plan::new(0.01, 1);
        let users = (1..=i64::try_from(plan.users()).unwrap()).collect();
        Model::build(plan, users, 3)
    }

    #[test]
    fn fields_cover_every_kind() {
        let model = model();
        assert_eq!(model.fields.len(), FIELDS);
        for kind in [
            Kind::Enum,
            Kind::State,
            Kind::User,
            Kind::Group,
            Kind::Version,
            Kind::Build,
            Kind::Owned,
            Kind::Integer,
            Kind::Duration,
            Kind::Float,
            Kind::Date,
            Kind::DateTime,
            Kind::String,
            Kind::Text,
        ] {
            assert!(model.fields.iter().any(|f| f.kind == kind), "{kind:?}");
        }
    }

    #[test]
    fn issues_spread_over_projects() {
        let model = model();
        assert_eq!(model.issue_count(), model.plan.issues());
        let sizes: Vec<u32> = model.projects.iter().map(|p| p.issues).collect();
        assert_eq!(u64::from(sizes.iter().sum::<u32>()), model.plan.issues());
        assert!(sizes[0] > 3 * sizes[sizes.len() - 1]);
        let keys: HashSet<&str> = model.projects.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys.len(), model.projects.len());
        let names: HashSet<String> = model
            .projects
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect();
        assert_eq!(names.len(), model.projects.len());
    }

    #[test]
    fn every_project_has_the_core_fields_and_a_team() {
        let model = model();
        for project in &model.projects {
            for name in [names::STATE, names::PRIORITY, names::TYPE, names::ASSIGNEE] {
                assert!(
                    project
                        .fields
                        .iter()
                        .any(|f| model.fields[f.field].name == name),
                    "{} has no {name}",
                    project.key
                );
            }
            assert!(!project.team.is_empty());
            for attached in &project.fields {
                let field = &model.fields[attached.field];
                assert_eq!(attached.set.is_some(), field.kind.has_set());
                assert_eq!(
                    attached.stored_set.is_some(),
                    field.kind.set_kind().is_some()
                );
                assert_eq!(field.set.is_some(), field.kind.set_kind().is_some());
                for set in [attached.stored_set, field.set].into_iter().flatten() {
                    assert_eq!(model.sets[set].kind, field.kind, "{}", field.name);
                }
            }
        }
    }

    #[test]
    fn ids_and_names_are_unique() {
        let model = model();
        let (groups, sets): (Vec<&ValueSet>, Vec<&ValueSet>) =
            model.sets.iter().partition(|s| s.kind == Kind::Group);
        let unique = |ids: Vec<i64>| {
            let count = ids.len();
            ids.into_iter().collect::<HashSet<i64>>().len() == count
        };
        assert!(unique(sets.iter().map(|s| i64::from(s.id)).collect()));
        assert!(unique(
            sets.iter()
                .flat_map(|s| s.values.iter().map(|v| v.id))
                .collect()
        ));
        // Groups: the lists of group fields, then the project teams.
        let mut group_ids: Vec<i64> = groups
            .iter()
            .flat_map(|s| s.values.iter().map(|v| v.id))
            .chain(model.projects.iter().map(|p| p.team_group))
            .collect();
        group_ids.sort_unstable();
        assert_eq!(group_ids.first(), Some(&3));
        assert!(group_ids.windows(2).all(|pair| pair[1] == pair[0] + 1));
        let names: HashSet<(&str, String)> = sets
            .iter()
            .map(|s| (s.kind.name(), s.name.to_lowercase()))
            .collect();
        assert_eq!(names.len(), sets.len());
        for set in &model.sets {
            let names: HashSet<String> = set.values.iter().map(|v| v.name.to_lowercase()).collect();
            assert_eq!(names.len(), set.values.len(), "{}", set.name);
        }
    }

    #[test]
    fn big_projects_run_sprints_of_about_500_cards() {
        assert_eq!(sprint_size(100_000), 590);
        assert_eq!(sprint_size(1_000), 50);
        assert_eq!(project_key("Brai Tou", 27), "BRABB");
    }
}
