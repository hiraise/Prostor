//! Boards (BRD, step 1.8, D-044): names, limits, presets, columns, sprints and who may do what.
//! The board holds issues of its projects as cards; every board has sprint rows, a sprintless
//! board one internal sprint (BRD-024).

pub mod access;
pub mod cards;
pub mod columns;
pub mod health;
pub mod lanes;
pub mod presets;
pub mod query_mode;
pub mod sprints;

use crate::ProjectId;
use crate::accounts::plain_text;
use crate::validation::{Problem, Rejection};

/// Board name length (BRD-009).
pub const NAME_MAX: usize = 100;
/// Sprint name length (BRD-098).
pub const SPRINT_NAME_MAX: usize = 100;
/// Sprint goal length (BRD-098).
pub const GOAL_MAX: usize = 2_000;
/// Projects per board: the live topic routes by them (BRD-114).
pub const PROJECTS_MAX: usize = 100;
/// Columns per board.
pub const COLUMNS_MAX: usize = 50;
/// Sprints per board, archived ones included.
pub const SPRINTS_MAX: usize = 1_000;
/// Fields shown on cards (BRD-049).
pub const CARD_FIELDS_MAX: usize = 10;

/// Validates a board name (BRD-009): one line without `/`, which separates the board from the
/// sprint in queries (`board/sprint`). Duplicates are allowed.
pub fn board_name(raw: &str) -> Result<String, Rejection> {
    let name = plain_text(raw, NAME_MAX, "name")?;
    if name.contains('/') {
        return Err(Rejection::new(Problem::Invalid, "name cannot contain /"));
    }
    Ok(name)
}

/// Validates a sprint name (BRD-098): one line; unique per board ignoring case (the store's
/// `sprints_name_key`).
pub fn sprint_name(raw: &str) -> Result<String, Rejection> {
    plain_text(raw, SPRINT_NAME_MAX, "name")
}

/// Validates a sprint goal: trimmed, may be empty, may hold line breaks.
pub fn goal(raw: &str) -> Result<String, Rejection> {
    let text = raw.trim();
    if text.chars().count() > GOAL_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("goal is longer than {GOAL_MAX} characters"),
        ));
    }
    Ok(text.to_owned())
}

/// The board's projects: sorted, without duplicates, 1…[`PROJECTS_MAX`].
pub fn projects(mut ids: Vec<ProjectId>) -> Result<Vec<ProjectId>, Rejection> {
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(Rejection::new(
            Problem::Required,
            "a board needs at least one project",
        ));
    }
    if ids.len() > PROJECTS_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("at most {PROJECTS_MAX} projects"),
        ));
    }
    Ok(ids)
}

/// Fields shown on cards: without duplicates, in the given order, at most [`CARD_FIELDS_MAX`].
pub fn card_fields(ids: Vec<crate::FieldId>) -> Result<Vec<crate::FieldId>, Rejection> {
    let mut out: Vec<crate::FieldId> = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.len() > CARD_FIELDS_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("at most {CARD_FIELDS_MAX} fields on cards"),
        ));
    }
    Ok(out)
}

/// What a new board starts from (BRD-003…BRD-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    Scrum,
    Kanban,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Scrum, Preset::Kanban, Preset::Custom];

    pub fn key(self) -> &'static str {
        match self {
            Preset::Scrum => "scrum",
            Preset::Kanban => "kanban",
            Preset::Custom => "custom",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// How issues join the board (BRD-025…BRD-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Only by hand.
    Manual,
    /// New issues of the board projects join (the auto-add target on sprint boards).
    Auto,
    /// The board query decides (1.8b).
    Query,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Manual, Mode::Auto, Mode::Query];

    pub fn key(self) -> &'static str {
        match self {
            Mode::Manual => "manual",
            Mode::Auto => "auto",
            Mode::Query => "query",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// Card colors (BRD-059, BRD-060).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorMode {
    None,
    Project,
    Field,
}

impl ColorMode {
    pub const ALL: [ColorMode; 3] = [ColorMode::None, ColorMode::Project, ColorMode::Field];

    pub fn key(self) -> &'static str {
        match self {
            ColorMode::None => "none",
            ColorMode::Project => "project",
            ColorMode::Field => "field",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// Card sizes of a person's view (BRD-058): 0 smallest … 3 largest.
pub const CARD_SIZE_MAX: i16 = 3;

pub fn card_size(size: i16) -> Result<i16, Rejection> {
    if !(0..=CARD_SIZE_MAX).contains(&size) {
        return Err(Rejection::new(
            Problem::OutOfRange,
            format!("card size is 0…{CARD_SIZE_MAX}"),
        ));
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brd_009_name_rules() {
        assert_eq!(board_name("  Команда  ").unwrap(), "Команда");
        assert_eq!(board_name("").unwrap_err().problem, Problem::Required);
        assert_eq!(board_name("a/b").unwrap_err().problem, Problem::Invalid);
        assert_eq!(board_name("a\nb").unwrap_err().problem, Problem::Invalid);
        assert!(board_name(&"x".repeat(NAME_MAX)).is_ok());
        assert_eq!(
            board_name(&"x".repeat(NAME_MAX + 1)).unwrap_err().problem,
            Problem::TooLong
        );
        assert_eq!(sprint_name("Sprint 1/2").unwrap(), "Sprint 1/2");
        assert_eq!(sprint_name(" ").unwrap_err().problem, Problem::Required);
        assert_eq!(goal(" line\nnext ").unwrap(), "line\nnext");
        assert_eq!(
            goal(&"x".repeat(GOAL_MAX + 1)).unwrap_err().problem,
            Problem::TooLong
        );
    }

    #[test]
    fn brd_009_limits() {
        assert_eq!(projects(vec![3, 1, 3]).unwrap(), vec![1, 3]);
        assert_eq!(projects(vec![]).unwrap_err().problem, Problem::Required);
        let many: Vec<ProjectId> = (0..=PROJECTS_MAX as i64).collect();
        assert_eq!(projects(many).unwrap_err().problem, Problem::TooLong);
        assert_eq!(card_fields(vec![5, 2, 5]).unwrap(), vec![5, 2]);
        assert!(card_fields((0..=CARD_FIELDS_MAX as i64).collect()).is_err());
        assert!(card_size(3).is_ok() && card_size(4).is_err() && card_size(-1).is_err());
        for p in Preset::ALL {
            assert_eq!(Preset::parse(p.key()), Some(p));
        }
        for m in Mode::ALL {
            assert_eq!(Mode::parse(m.key()), Some(m));
        }
        for c in ColorMode::ALL {
            assert_eq!(ColorMode::parse(c.key()), Some(c));
        }
    }
}
