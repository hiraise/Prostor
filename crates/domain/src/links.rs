//! Links between issues (LNK): link types and their names, the built-in types, who may link, and
//! why a link is refused. Design: decisions.md D-034.
//!
//! A link joins a source and a target. The source side name is how the source refers to the
//! target ("parent of"), the target side name how the target refers to the source ("child of");
//! an undirected type has one name for both. Hierarchy types give each target one source and allow
//! no cycles (D-013).

use crate::access::permission::Permission;
use crate::access::{Access, Denied, Scope};
use crate::accounts::plain_text;
use crate::fields::builtin::Names;
use crate::issues::rights::Facts;
use crate::validation::{Problem, Problems, Rejection};

/// Longest name of a type or side, in characters.
pub const NAME_MAX: usize = 50;
/// Most links on one issue: what its panel and its subtask counts load whole.
pub const LINKS_PER_ISSUE_MAX: usize = 2_000;
/// Most issues linked in one request (LNK-005).
pub const ADD_MAX: usize = 100;
/// Deepest hierarchy followed when looking for cycles; deeper chains are refused.
pub const DEPTH_MAX: i32 = 100;

/// How a type joins issues (LNK-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// One name on both sides.
    Undirected,
    /// Two side names.
    Directed,
    /// Two side names; each target has one source and there are no cycles (subtasks).
    Hierarchy,
}

impl Direction {
    pub const ALL: [Direction; 3] = [
        Direction::Undirected,
        Direction::Directed,
        Direction::Hierarchy,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Direction::Undirected => "undirected",
            Direction::Directed => "directed",
            Direction::Hierarchy => "hierarchy",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }

    pub fn has_target_name(self) -> bool {
        self != Direction::Undirected
    }
}

/// Which end of a link an issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    Source,
    Target,
}

impl Side {
    pub fn key(self) -> &'static str {
        match self {
            Side::Source => "source",
            Side::Target => "target",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        [Side::Source, Side::Target]
            .into_iter()
            .find(|s| s.key() == key)
    }

    pub fn other(self) -> Self {
        match self {
            Side::Source => Side::Target,
            Side::Target => Side::Source,
        }
    }

    /// The side a link shows under on an issue: undirected types have one.
    pub fn shown(self, direction: Direction) -> Self {
        if direction == Direction::Undirected {
            Side::Source
        } else {
            self
        }
    }
}

/// Validates the name of a type or a side (LNK-019): one line of plain text, without the
/// characters queries use.
pub fn name(raw: &str) -> Result<String, Rejection> {
    let text = plain_text(raw, NAME_MAX, "name")?;
    if text.contains(['<', '>', '/', ':']) {
        return Err(Rejection::new(
            Problem::Invalid,
            "name cannot contain <, >, / or :",
        ));
    }
    Ok(text)
}

/// The names of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeNames {
    pub name: String,
    pub source: String,
    /// `None` for undirected types.
    pub target: Option<String>,
}

impl TypeNames {
    /// The name of a side; undirected types answer their one name.
    pub fn side(&self, side: Side) -> &str {
        match (side, &self.target) {
            (Side::Target, Some(target)) => target,
            _ => &self.source,
        }
    }

    fn sides(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.source.as_str()).chain(self.target.as_deref())
    }
}

/// Checks a type's names against the other types that are not deleted (LNK-019): internal names
/// are unique among internal names, side names among all side names, ignoring case; a directed
/// type's two sides differ. Paths: `name`, `source_name`, `target_name`.
pub fn check_unique<'a>(
    names: &TypeNames,
    others: impl IntoIterator<Item = &'a TypeNames>,
) -> Problems {
    let mut problems = Problems::new();
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
    if let Some(target) = &names.target
        && same(target, &names.source)
    {
        problems.add(
            "target_name",
            Rejection::new(Problem::Invalid, "the two sides need different names"),
        );
    }
    let others: Vec<&TypeNames> = others.into_iter().collect();
    if others.iter().any(|o| same(&o.name, &names.name)) {
        problems.add(
            "name",
            Rejection::new(Problem::Taken, "another link type has this name"),
        );
    }
    let taken = |side: &str| others.iter().any(|o| o.sides().any(|s| same(s, side)));
    if taken(&names.source) {
        problems.add(
            "source_name",
            Rejection::new(Problem::Taken, "another link type uses this name"),
        );
    }
    if let Some(target) = &names.target
        && taken(target)
    {
        problems.add(
            "target_name",
            Rejection::new(Problem::Taken, "another link type uses this name"),
        );
    }
    problems
}

/// Linking from an issue and removing its links (LNK-005, LNK-008): `issue.link` in its project,
/// or being its reporter with `issue.create` (ISS-022). A draft is its reporter's.
pub fn may_link(viewer: &Access, issue: &Facts<'_>) -> Result<(), Denied> {
    if issue.draft && issue.reporter == viewer.user() {
        return Ok(());
    }
    if viewer.has_in(Permission::IssueLink, issue.project) {
        return Ok(());
    }
    if issue.reporter == viewer.user() && viewer.has_in(Permission::IssueCreate, issue.project) {
        return Ok(());
    }
    viewer.require_at(Permission::IssueLink, Scope::Project(issue.project))
}

/// Managing link types (LNK-018).
pub fn may_manage_types(viewer: &Access) -> Result<(), Denied> {
    viewer.require(Permission::SystemWrite)
}

/// Why a link or a type change is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A built-in type the server relies on cannot change or go (LNK-002).
    Locked,
    /// The target already has a source of this hierarchy type (D-013).
    ParentTaken,
    /// The link would close a loop in a hierarchy (D-013).
    Cycle,
    /// A type cannot become a hierarchy while its links break the hierarchy rules.
    HierarchyConflict,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Refusal::Locked => "this link type is built in and cannot be changed or deleted",
            Refusal::ParentTaken => "the issue already has a parent of this kind",
            Refusal::Cycle => "the link would make an issue its own ancestor",
            Refusal::HierarchyConflict => {
                "some issues have several sources of this type or form a loop"
            }
        }
    }
}

/// Whether links `(source, target)` of a type keep the hierarchy rules (D-013): each target has
/// one source and following sources never comes back to where it started.
pub fn is_forest(pairs: &[(i64, i64)]) -> bool {
    use std::collections::HashMap;
    let mut parent: HashMap<i64, i64> = HashMap::with_capacity(pairs.len());
    for (source, target) in pairs {
        if parent.insert(*target, *source).is_some() {
            return false;
        }
    }
    // Walk up from every issue; a walk longer than the number of links went round a loop.
    let limit = parent.len();
    for start in parent.keys() {
        let mut at = *start;
        let mut steps = 0;
        while let Some(up) = parent.get(&at) {
            at = *up;
            steps += 1;
            if at == *start || steps > limit {
                return false;
            }
        }
    }
    true
}

/// The link types every instance starts with (LNK-002), in our own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinType {
    Related,
    Dependency,
    Duplicate,
    Subtask,
}

/// What a built-in type is created with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSeed {
    pub name: Names,
    pub source: Names,
    pub target: Option<Names>,
    pub direction: Direction,
    pub locked: bool,
}

impl BuiltinType {
    pub const ALL: [BuiltinType; 4] = [
        BuiltinType::Related,
        BuiltinType::Dependency,
        BuiltinType::Duplicate,
        BuiltinType::Subtask,
    ];

    pub fn key(self) -> &'static str {
        match self {
            BuiltinType::Related => "related",
            BuiltinType::Dependency => "dependency",
            BuiltinType::Duplicate => "duplicate",
            BuiltinType::Subtask => "subtask",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// The parent is the source of a subtask link; the original the source of a duplicate link.
    pub fn seed(self) -> TypeSeed {
        match self {
            BuiltinType::Related => TypeSeed {
                name: Names::new("Related", "Связь"),
                source: Names::new("related to", "связана с"),
                target: None,
                direction: Direction::Undirected,
                locked: false,
            },
            BuiltinType::Dependency => TypeSeed {
                name: Names::new("Dependency", "Зависимость"),
                source: Names::new("blocks", "блокирует"),
                target: Some(Names::new("blocked by", "заблокирована")),
                direction: Direction::Directed,
                locked: false,
            },
            BuiltinType::Duplicate => TypeSeed {
                name: Names::new("Duplicate", "Дубликат"),
                source: Names::new("has duplicate", "имеет дубликат"),
                target: Some(Names::new("duplicate of", "дубликат")),
                direction: Direction::Hierarchy,
                locked: true,
            },
            BuiltinType::Subtask => TypeSeed {
                name: Names::new("Subtask", "Подзадача"),
                source: Names::new("parent of", "родитель для"),
                target: Some(Names::new("child of", "подзадача для")),
                direction: Direction::Hierarchy,
                locked: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Grant;
    use crate::access::capability::CapabilitySet;
    use crate::access::permission::PermissionSet;
    use crate::access::visibility::Audience;
    use crate::settings::Language;

    fn names(name: &str, source: &str, target: Option<&str>) -> TypeNames {
        TypeNames {
            name: name.to_owned(),
            source: source.to_owned(),
            target: target.map(str::to_owned),
        }
    }

    fn paths(problems: Problems) -> Vec<(String, &'static str)> {
        problems
            .into_vec()
            .into_iter()
            .map(|p| (p.path, p.problem.code()))
            .collect()
    }

    #[test]
    fn lnk_019_names_are_one_line_without_query_characters() {
        assert_eq!(name("  blocks  ").unwrap(), "blocks");
        assert_eq!(name("").unwrap_err().problem, Problem::Required);
        assert_eq!(name("a\nb").unwrap_err().problem, Problem::Invalid);
        assert_eq!(name("part: of").unwrap_err().problem, Problem::Invalid);
        assert_eq!(name(&"x".repeat(51)).unwrap_err().problem, Problem::TooLong);
        assert!(name(&"я".repeat(50)).is_ok());
    }

    #[test]
    fn lnk_019_side_names_are_unique_across_all_types() {
        let existing = [
            names("Dependency", "blocks", Some("blocked by")),
            names("Related", "related to", None),
        ];
        let fine = names("Causes", "causes", Some("caused by"));
        assert!(check_unique(&fine, &existing).is_empty());
        // Ignoring case, against both sides and the internal names apart.
        let clash = names("dependency", "Blocked By", Some("RELATED TO"));
        assert_eq!(
            paths(check_unique(&clash, &existing)),
            vec![
                ("name".to_owned(), "taken"),
                ("source_name".to_owned(), "taken"),
                ("target_name".to_owned(), "taken"),
            ]
        );
        // An internal name may equal a side name: they live in different places.
        let crossed = names("blocks", "precedes", Some("follows"));
        assert!(check_unique(&crossed, &existing).is_empty());
        let same_sides = names("Mirror", "mirrors", Some("Mirrors"));
        assert_eq!(
            paths(check_unique(&same_sides, &[])),
            vec![("target_name".to_owned(), "invalid")]
        );
    }

    #[test]
    fn lnk_001_side_names_and_directions() {
        let directed = names("Dependency", "blocks", Some("blocked by"));
        assert_eq!(directed.side(Side::Source), "blocks");
        assert_eq!(directed.side(Side::Target), "blocked by");
        let undirected = names("Related", "related to", None);
        assert_eq!(undirected.side(Side::Target), "related to");
        assert_eq!(Side::Target.shown(Direction::Undirected), Side::Source);
        assert_eq!(Side::Target.shown(Direction::Hierarchy), Side::Target);
        for d in Direction::ALL {
            assert_eq!(Direction::parse(d.key()), Some(d));
        }
        assert!(!Direction::Undirected.has_target_name());
    }

    #[test]
    fn lnk_002_builtin_types_are_valid_and_distinct() {
        for language in [Language::En, Language::Ru] {
            let all: Vec<TypeNames> = BuiltinType::ALL
                .iter()
                .map(|t| {
                    let seed = t.seed();
                    assert_eq!(seed.target.is_some(), seed.direction.has_target_name());
                    names(
                        seed.name.get(language),
                        seed.source.get(language),
                        seed.target.map(|n| n.get(language)),
                    )
                })
                .collect();
            for (i, one) in all.iter().enumerate() {
                assert!(name(&one.name).is_ok());
                let others: Vec<&TypeNames> = all
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, o)| o)
                    .collect();
                assert!(check_unique(one, others).is_empty(), "{one:?}");
            }
        }
        assert!(BuiltinType::Subtask.seed().locked);
        assert!(BuiltinType::Duplicate.seed().locked);
        assert!(!BuiltinType::Related.seed().locked);
        assert_eq!(BuiltinType::parse("subtask"), Some(BuiltinType::Subtask));
    }

    #[test]
    fn lnk_013_hierarchies_have_one_source_and_no_loops() {
        assert!(is_forest(&[]));
        assert!(is_forest(&[(1, 2), (1, 3), (2, 4), (5, 6)]));
        // Two parents of 3.
        assert!(!is_forest(&[(1, 3), (2, 3)]));
        // 1 → 2 → 3 → 1.
        assert!(!is_forest(&[(1, 2), (2, 3), (3, 1)]));
    }

    fn person(user: i64, permissions: PermissionSet) -> Access {
        Access::compile(
            user,
            [Grant {
                permissions,
                scope: Scope::Project(1),
            }],
            vec![],
            CapabilitySet::default(),
        )
    }

    fn issue(reporter: i64, draft: bool) -> Facts<'static> {
        Facts {
            project: 1,
            reporter,
            draft,
            audience: Audience::default(),
        }
    }

    #[test]
    fn lnk_005_linking_needs_the_link_permission_or_the_reporter_rule() {
        let linker = person(1, PermissionSet::from_iter([Permission::IssueLink]));
        let reporter = person(2, PermissionSet::from_iter([Permission::IssueCreate]));
        let reader = person(3, PermissionSet::from_iter([Permission::IssueRead]));
        assert!(may_link(&linker, &issue(9, false)).is_ok());
        assert!(may_link(&reporter, &issue(2, false)).is_ok());
        assert!(may_link(&reporter, &issue(9, false)).is_err());
        assert!(may_link(&reader, &issue(9, false)).is_err());
        // A draft is its reporter's.
        assert!(may_link(&reader, &issue(3, true)).is_ok());
        let denied = may_link(&reader, &issue(9, false)).unwrap_err();
        assert!(denied.missing.contains(Permission::IssueLink));
    }
}
