//! Boards (BRD, step 1.8): boards with their projects, columns and sprints, each person's view of
//! a board, and the board view with its cards.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};

use crate::access::GroupRef;
use crate::accounts::UserRef;
use crate::fields::FieldRef;
use crate::issues::{HeldValue, TagRef};
use crate::projects::ProjectRef;
use crate::search::{QueryProblem, SortUsed};
use crate::sharing::{SharedWith, SharedWithInput};

/// What a new board starts from (BRD-003…BRD-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardPreset {
    Scrum,
    Kanban,
    Custom,
}

/// How issues join the board (BRD-025…BRD-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardMode {
    Manual,
    Auto,
    Query,
}

/// Where a column stands against its limits (BRD-046); limits warn, never block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WipState {
    Within,
    Under,
    Over,
}

/// What makes the lanes (BRD-049).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LaneMode {
    None,
    /// One lane per value of a field or attribute.
    Values,
    /// One lane per parent issue of the cards.
    Parents,
}

/// An issue attribute that makes value lanes (BRD-050).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LaneAttribute {
    Project,
    Tags,
    Created,
    Updated,
}

/// Where the lane of cards matching no lane goes (BRD-052); `off` hides those cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OtherLane {
    Off,
    Top,
    Bottom,
}

/// The lanes of a board (BRD-049…BRD-055). A lane key is `v:<value>`, `u:<account>`,
/// `g:<group>`, `p:<project>`, `t:<tag>`, `d:<range>` (`later`, `today`, `yesterday`,
/// `this_week`, `last_week`, `this_month`, `older`, `none`; in the instance time zone),
/// `i:<issue>` for a parent lane, or `other`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardLanes {
    pub mode: LaneMode,
    /// The field of value lanes, or the field a parent must hold a value of.
    #[schema(required = true)]
    pub field: Option<FieldRef>,
    #[schema(required = true)]
    pub attribute: Option<LaneAttribute>,
    /// Value lanes in order; for parent lanes the values a parent holds. Date lanes with none
    /// show every range.
    pub values: Vec<String>,
    pub other: OtherLane,
    /// The value new cards get on a parent-lane board (BRD-055).
    #[schema(required = true)]
    pub new_card_value: Option<i64>,
    /// The names of the value, person, group and tag keys in `values` (a tag only when the
    /// viewer knows it), so lanes without cards have titles too.
    pub labels: Vec<LaneLabel>,
}

/// The name of a lane key (BRD-053).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LaneLabel {
    pub key: String,
    pub name: String,
    /// The color of a value or a tag.
    #[schema(required = true)]
    pub color: Option<i16>,
}

/// New lane settings; they replace the old ones whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardLanesInput {
    pub mode: LaneMode,
    /// Value lanes: a state, enum, owned, version, build, user, group or date field (not the
    /// column field); parent lanes: an optional field with a list of values.
    #[serde(default)]
    pub field: Option<i64>,
    /// Value lanes without a field.
    #[serde(default)]
    pub attribute: Option<LaneAttribute>,
    /// Lane keys in order, of the field's or attribute's kind.
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default = "other_bottom")]
    pub other: OtherLane,
    #[serde(default)]
    pub new_card_value: Option<i64>,
}

fn other_bottom() -> OtherLane {
    OtherLane::Bottom
}

/// A lane's header (BRD-053).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Lane {
    pub key: String,
    /// Cards in the lane, the hidden ones included.
    pub count: i64,
    /// Their estimation (BRD-062) in the estimation field's unit (minutes for durations); `null`
    /// without an estimation field.
    #[schema(required = true)]
    pub estimate: Option<f64>,
    /// The parent issue of a parent lane; its card form is in `issues`.
    #[schema(required = true)]
    pub issue: Option<i64>,
}

/// A new parent lane (BRD-054): an issue with this summary in the first board project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewLane {
    pub title: String,
}

/// Where a parent lane goes among the lanes (BRD-053); neither neighbour: last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LaneMove {
    /// The parent lane it follows.
    #[serde(default)]
    pub after: Option<i64>,
    /// The parent lane it precedes.
    #[serde(default)]
    pub before: Option<i64>,
}

/// A parent lane just made, with its issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlacedLane {
    pub lane: Lane,
    pub issue: CardIssue,
    pub refs: CardRefs,
}

/// The issues a parent lane took back to the backlog (BRD-056).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LaneRemoved {
    pub issues: Vec<i64>,
}

/// Who views and who edits a board, and the two audiences over its projects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardSharing {
    pub view: SharedWith,
    pub edit: SharedWith,
    /// Everyone who reads issues in every board project views the board.
    pub view_readers: bool,
    /// Everyone who updates issues in a board project edits its settings.
    pub edit_updaters: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, ToSchema)]
pub struct BoardSharingInput {
    #[serde(default)]
    pub view: SharedWithInput,
    #[serde(default)]
    pub edit: SharedWithInput,
    #[serde(default)]
    pub view_readers: bool,
    #[serde(default)]
    pub edit_updaters: bool,
}

/// What the caller may do with a board.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardRights {
    /// Change the settings: name, projects, columns (BRD-013).
    pub edit: bool,
    /// Change the audiences and the two flags.
    pub share: bool,
    /// Give the board to another owner (BRD-011).
    pub transfer: bool,
    /// Delete it (BRD-016).
    pub delete: bool,
}

/// A project of a board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardProject {
    pub project: ProjectRef,
    /// Card color by project (BRD-060).
    #[schema(required = true)]
    pub color: Option<i16>,
}

/// A column: one value of the column field (BRD-042…BRD-046).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardColumn {
    pub id: i64,
    pub value: HeldValue,
    #[schema(required = true)]
    pub wip_min: Option<i32>,
    #[schema(required = true)]
    pub wip_max: Option<i32>,
}

/// How cards are colored (BRD-060).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CardColor {
    None,
    /// By the board project's color (`BoardProject.color`).
    Project,
    /// By the values of `color_field`.
    Field,
}

/// What cards show (BRD-059…BRD-062). Private fields the caller does not read are left out
/// (FLD-062); a change keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardCards {
    /// In order, at most 10.
    pub fields: Vec<FieldRef>,
    /// Of `fields`, those shown as a first letter only.
    pub initials: Vec<i64>,
    pub color: CardColor,
    #[schema(required = true)]
    pub color_field: Option<FieldRef>,
    /// Shown at the card's bottom and summed in the headers.
    #[schema(required = true)]
    pub estimation: Option<FieldRef>,
    #[schema(required = true)]
    pub original_estimation: Option<FieldRef>,
}

/// The card settings to keep; every part is replaced.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct BoardCardsInput {
    pub fields: Vec<i64>,
    #[serde(default)]
    pub initials: Vec<i64>,
    pub color: CardColor,
    #[serde(default)]
    pub color_field: Option<i64>,
    #[serde(default)]
    pub estimation: Option<i64>,
    #[serde(default)]
    pub original_estimation: Option<i64>,
    /// Card colors of board projects; those left out keep theirs.
    #[serde(default)]
    pub project_colors: Vec<ProjectColor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
pub struct ProjectColor {
    pub project: i64,
    /// One of the twelve color schemes (0…11); `null` for none.
    #[schema(required = true)]
    pub color: Option<i16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    /// The board shows no cards until it is fixed.
    Error,
    /// Part of the board does not work.
    Warning,
}

/// What is wrong with a board's settings (BRD-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthKind {
    NoProjects,
    NoColumnField,
    NoColumns,
    /// `project` does not have the column field.
    ColumnFieldMissing,
    /// The column of `value` is not in the set `project` uses.
    ColumnValueMissing,
    /// `project` is archived.
    ArchivedProject,
    /// The field of `slot` is in no board project.
    FieldMissing,
    /// The field of `slot` is of a kind the setting does not take.
    FieldWrongKind,
    /// The caller does not view the backlog search.
    BacklogHidden,
    /// New issues of `project` get a column value that is no column: a query board misses them.
    DefaultNotColumn,
}

/// The setting a field finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthSlot {
    Lane,
    Color,
    Estimation,
    OriginalEstimation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardHealth {
    pub severity: HealthSeverity,
    pub kind: HealthKind,
    #[schema(required = true)]
    pub project: Option<i64>,
    /// A column's value (`column_value_missing`).
    #[schema(required = true)]
    pub value: Option<i64>,
    #[schema(required = true)]
    pub slot: Option<HealthSlot>,
}

/// A sprint of a board (BRD-097…BRD-104).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Sprint {
    pub id: i64,
    pub name: String,
    pub goal: String,
    #[schema(required = true)]
    pub starts_on: Option<Date>,
    #[schema(required = true)]
    pub ends_on: Option<Date>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required = true)]
    pub finished_at: Option<OffsetDateTime>,
    pub archived: bool,
    /// Finished by hand, or its last day passed (BRD-100).
    pub finished: bool,
    /// The current sprint (BRD-099), never set by hand.
    pub current: bool,
    /// The hidden sprint of a sprintless board.
    pub internal: bool,
    pub version: i64,
}

/// A board in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub owner: UserRef,
    pub preset: BoardPreset,
    pub sprints_on: bool,
    pub mode: BoardMode,
    /// The sprint new issues join in mode `auto` on a sprint board (BRD-026).
    #[schema(required = true)]
    pub auto_sprint: Option<i64>,
    /// The board query in mode `query` (BRD-029), by the caller's names; empty is every issue of
    /// the board projects.
    #[schema(required = true)]
    pub query: Option<String>,
    /// What the board query lost (a deleted value…), with spans in `query`: until it is fixed
    /// the board shows no cards.
    pub query_problems: Vec<QueryProblem>,
    /// The saved search used as the backlog (BRD-089); `null` for none.
    #[schema(required = true)]
    pub backlog: Option<i64>,
    /// The field whose values are the columns.
    #[schema(required = true)]
    pub column_field: Option<FieldRef>,
    /// In order.
    pub columns: Vec<BoardColumn>,
    pub lanes: BoardLanes,
    pub cards: BoardCards,
    pub projects: Vec<BoardProject>,
    /// What keeps the board from working, errors first (BRD-023).
    pub health: Vec<BoardHealth>,
    /// Only for those who may edit the board.
    #[schema(required = true)]
    pub sharing: Option<BoardSharing>,
    pub rights: BoardRights,
    pub favorite: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub version: i64,
}

/// A board in lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardSummary {
    pub id: i64,
    pub name: String,
    pub owner: UserRef,
    pub favorite: bool,
    pub projects: Vec<ProjectRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardList {
    pub items: Vec<BoardSummary>,
    #[schema(required = true)]
    pub next_cursor: Option<String>,
    /// The board the caller visited last and still views (BRD-001).
    #[schema(required = true)]
    pub last_visited: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BoardListQuery {
    /// Part of the name.
    pub q: Option<String>,
    /// Boards including this project.
    pub project: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewBoard {
    pub preset: BoardPreset,
    pub name: String,
    pub projects: Vec<i64>,
    #[serde(default)]
    pub sharing: BoardSharingInput,
}

/// Changes of a board; absent fields stay.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct BoardPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub projects: Option<Vec<i64>>,
    /// A new owner (BRD-011).
    #[serde(default)]
    pub owner: Option<i64>,
    #[serde(default)]
    pub sharing: Option<BoardSharingInput>,
    /// `null` removes the column field; any change drops the columns (BRD-042).
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i64>)]
    pub column_field: Option<Option<i64>>,
    /// Sprints on or off (BRD-024).
    #[serde(default)]
    pub sprints_on: Option<bool>,
    /// `manual`, `auto` or `query` (sprintless boards only). Leaving `query` keeps every issue it
    /// shows as a card (at most 20 000).
    #[serde(default)]
    pub mode: Option<BoardMode>,
    /// The board query (mode `query`, BRD-029); its problems answer like a search's.
    #[serde(default)]
    pub query: Option<String>,
    /// The sprint new issues join on a sprint board (BRD-026); `null` clears it.
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i64>)]
    pub auto_sprint: Option<Option<i64>>,
    /// The saved search used as the backlog (BRD-090): one the caller views that names only
    /// board projects; `null` leaves the board without.
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i64>)]
    pub backlog: Option<Option<i64>>,
    /// `true` makes a new backlog search owned by the caller, «<board> · бэклог», and uses it
    /// (BRD-089); wins over `backlog`.
    #[serde(default)]
    pub new_backlog: Option<bool>,
    /// The lanes (BRD-049): value lanes also reorder here.
    #[serde(default)]
    pub lanes: Option<BoardLanesInput>,
    /// The card settings (BRD-059…BRD-062).
    #[serde(default)]
    pub cards: Option<BoardCardsInput>,
    /// The version the change is based on.
    pub version: i64,
}

/// A new sprint (BRD-098).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewSprint {
    pub name: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub starts_on: Option<Date>,
    #[serde(default)]
    pub ends_on: Option<Date>,
    /// Carry the unresolved issues of this sprint to the new one (BRD-101).
    #[serde(default)]
    pub carry_from: Option<i64>,
    /// New issues join the new sprint; the board's mode becomes `auto` (BRD-026).
    #[serde(default)]
    pub auto_target: bool,
}

/// A change of a sprint (BRD-102); absent keeps a value, `null` clears a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SprintPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<Date>)]
    pub starts_on: Option<Option<Date>>,
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<Date>)]
    pub ends_on: Option<Option<Date>>,
    /// The version the change is based on.
    pub version: i64,
}

/// Where the unresolved issues of a sprint go (BRD-101).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SprintCarry {
    pub to: i64,
}

/// The sprints of a board, archived ones included, in creation order (none on a sprintless board).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SprintList {
    pub items: Vec<Sprint>,
}

/// What deleting a board takes with it (BRD-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardImpact {
    pub sprints: i64,
    pub cards: i64,
    /// Saved searches whose query names the board.
    pub searches: i64,
}

/// The caller's view of a board, synced across devices (BRD-001, BRD-047, BRD-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardProfile {
    pub favorite: bool,
    /// 0 smallest … 3 largest.
    pub card_size: i16,
    pub collapsed_columns: Vec<i64>,
    pub collapsed_lanes: Vec<String>,
}

/// Changes of the caller's view; absent fields stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, ToSchema)]
pub struct BoardProfilePatch {
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default)]
    pub card_size: Option<i16>,
    #[serde(default)]
    pub collapsed_columns: Option<Vec<i64>>,
    #[serde(default)]
    pub collapsed_lanes: Option<Vec<String>>,
}

/// A column to add: a value of the column field's set, or a new value by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewBoardColumn {
    #[serde(default)]
    pub value: Option<i64>,
    /// A new value of the set (FLD-031 rights; it shows in every project using the set).
    #[serde(default)]
    pub new_value: Option<String>,
    /// The column it follows; absent puts it last.
    #[serde(default)]
    pub after: Option<i64>,
}

/// Changes of a column; absent fields stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, ToSchema)]
pub struct BoardColumnPatch {
    /// The column it follows; `null` puts it first.
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i64>)]
    pub after: Option<Option<i64>>,
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i32>)]
    pub wip_min: Option<Option<i32>>,
    #[serde(default, deserialize_with = "crate::patch::nullable")]
    #[schema(value_type = Option<i32>)]
    pub wip_max: Option<Option<i32>>,
    /// Renames the value (BRD-044) in every project using the set.
    #[serde(default)]
    pub name: Option<String>,
}

/// Where an issue stands on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Card {
    pub issue: i64,
    pub column: i64,
    /// The lane key on a board with lanes.
    #[schema(required = true)]
    pub lane: Option<String>,
    #[schema(required = true)]
    pub rank: Option<String>,
}

/// A column's header.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ColumnCount {
    pub column: i64,
    /// Cards in the column, the hidden ones included.
    pub count: i64,
    /// Their estimation (BRD-062); `null` without an estimation field.
    #[schema(required = true)]
    pub estimate: Option<f64>,
    pub wip: WipState,
}

/// A cell showing fewer cards than it holds (BRD-057).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TruncatedCell {
    pub column: i64,
    #[schema(required = true)]
    pub lane: Option<String>,
    pub total: i64,
    /// Where the rest starts: the `cursor` of `GET /boards/{id}/cell`.
    #[schema(required = true)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BoardTotals {
    pub count: i64,
    /// The estimation of every card (BRD-062); `null` without an estimation field.
    #[schema(required = true)]
    pub estimate: Option<f64>,
}

/// The board with one sprint's cards (BRD-068).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BoardView {
    pub board: Board,
    /// The shown sprint; `null` on a sprint board without a current sprint.
    #[schema(required = true)]
    pub sprint: Option<Sprint>,
    /// Sprints that are not archived, in creation order (none on a sprintless board).
    pub sprints: Vec<Sprint>,
    pub columns: Vec<ColumnCount>,
    pub cards: Vec<Card>,
    /// Only the truncated cells.
    pub cells: Vec<TruncatedCell>,
    /// A page of lanes (at most 50) on a board with lanes; their cards are in `cards`.
    pub lanes: Vec<Lane>,
    /// The `lanes` cursor of the next page.
    #[schema(required = true)]
    pub lanes_next: Option<String>,
    /// Lanes on every page.
    pub lanes_total: i64,
    /// The issues of the cards and of the parent lanes, as cards show them.
    pub issues: Vec<CardIssue>,
    pub refs: CardRefs,
    pub totals: BoardTotals,
    /// The sort ordering each cell when a sort clause is active (BRD-070): the cards cannot be
    /// reordered by hand.
    #[schema(required = true)]
    pub sort: Option<Vec<SortUsed>>,
    /// Notes about the header filter.
    pub notes: Vec<QueryProblem>,
    pub live_topic: String,
    pub profile: BoardProfile,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BoardViewQuery {
    /// A sprint of the board; absent shows the current one (BRD-099).
    pub sprint: Option<i64>,
    /// The header filter (BRD-081): any issue-list query, resolved within the board projects.
    pub q: Option<String>,
    /// Where a page of lanes starts (`lanes_next`, BRD-057).
    pub lanes: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BoardCellQuery {
    pub sprint: Option<i64>,
    pub column: i64,
    /// The lane of the cell, on a board with lanes.
    pub lane: Option<String>,
    pub cursor: Option<String>,
    /// The header filter the view was asked with.
    pub q: Option<String>,
}

/// The rest of a truncated cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BoardCell {
    pub cards: Vec<Card>,
    pub issues: Vec<CardIssue>,
    pub refs: CardRefs,
    #[schema(required = true)]
    pub next_cursor: Option<String>,
}

/// An issue to add to a sprint of the board (BRD-025); `column` also sets its column value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewCard {
    pub issue: i64,
    #[serde(default)]
    pub column: Option<i64>,
    /// The card it follows in the shared order; with neither neighbour it goes last.
    #[serde(default)]
    pub after: Option<i64>,
    /// The card it precedes.
    #[serde(default)]
    pub before: Option<i64>,
    /// A query board (BRD-028): answer the field changes that make the issue match and change
    /// nothing.
    #[serde(default)]
    pub preview: bool,
    /// The lane it is dropped into: sets the lane's value or parent.
    #[serde(default)]
    pub lane: Option<String>,
    /// A parent joining a parent-lane board becomes a lane (it holds a lane value, or has
    /// subtasks when no field limits the parents); `true` brings its subtasks as cards.
    #[serde(default)]
    pub subtasks: bool,
}

/// Where a card goes (BRD-068, BRD-069): its column, and its neighbours in the shared order
/// (neither: it keeps its place). `version` is the issue's version the caller saw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CardMove {
    pub column: i64,
    #[serde(default)]
    pub after: Option<i64>,
    #[serde(default)]
    pub before: Option<i64>,
    /// The lane it goes to (BRD-050, BRD-051): sets the lane's value, clears it for `other`, or
    /// changes the parent; absent keeps its lane. Project and date lanes refuse it.
    #[serde(default)]
    pub lane: Option<String>,
    pub version: i64,
    /// The header filter the caller sees: with a sort clause `after` and `before` are refused.
    #[serde(default)]
    pub q: Option<String>,
}

/// A field change that makes an issue match a query board (BRD-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CardChange {
    /// The field gets this value (added to a multi-valued field); names are in the card's issue.
    Set {
        field: i64,
        value: i64,
    },
    Clear {
        field: i64,
    },
    Tag {
        tag: i64,
    },
}

/// A card after a change, with its issue; `card` is `null` when it no longer shows on the board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlacedCard {
    #[schema(required = true)]
    pub card: Option<Card>,
    #[schema(required = true)]
    pub issue: Option<CardIssue>,
    pub refs: CardRefs,
    /// On a query board: the field changes made (or, for a preview, that would be made).
    pub changes: Vec<CardChange>,
    /// What the project's rules said (AUT-023, step 1.9).
    pub messages: Vec<crate::rules::Message>,
}

/// Issues the live board heard about (BRD-114): where they are now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CardBatch {
    /// The shown sprint; absent: the current one.
    #[serde(default)]
    pub sprint: Option<i64>,
    /// At most 100.
    pub issues: Vec<i64>,
    /// The header filter of the view.
    #[serde(default)]
    pub q: Option<String>,
}

/// Their cards in the order asked (`null`: not on the board for the caller), and their issues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CardBatchAnswer {
    pub cards: Vec<Option<Card>>,
    pub issues: Vec<CardIssue>,
    pub refs: CardRefs,
}

/// An issue as its card shows it (BRD-058, ISS-024): a board answers hundreds, so this is the
/// compact form, not the issue of lists (D-044): only fields that hold a value, with what they
/// name listed once per answer in `refs`. Markdown text fields are left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CardIssue {
    pub id: i64,
    #[schema(required = true)]
    pub readable_id: Option<String>,
    pub project: i64,
    pub summary: String,
    /// The start of the description (its first three lines, at most 300 characters), Markdown
    /// as written: the largest card size shows it.
    pub excerpt: String,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required = true)]
    pub resolved_at: Option<OffsetDateTime>,
    pub tags: Vec<TagRef>,
    pub fields: Vec<CardField>,
    /// Files of the issue itself the caller sees (medium cards and up, BRD-058).
    pub files: i64,
    /// Limited to an audience (ISS-042): the card shows a lock.
    pub restricted: bool,
    /// The caller watches the issue (NTF-014): the card menu offers to watch or unwatch.
    pub starred: bool,
    /// The caller may change the issue: move its card, edit it on the card.
    pub can_update: bool,
    /// Sent back with a move.
    pub version: i64,
}

/// A field value on a card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CardField {
    pub field: i64,
    /// Values of a set, accounts or groups by ID, as the field's kind says (`refs`).
    pub ids: Vec<i64>,
    /// Strings, dates (`YYYY-MM-DD`) and times (RFC 3339).
    #[schema(required = true)]
    pub text: Option<String>,
    /// Integers, floats and durations (minutes).
    #[schema(required = true)]
    pub number: Option<f64>,
}

/// What the cards of an answer name, each once.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CardRefs {
    pub fields: Vec<FieldRef>,
    pub values: Vec<HeldValue>,
    pub users: Vec<UserRef>,
    pub groups: Vec<GroupRef>,
}

/// Why a board shows no backlog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BacklogMissing {
    /// The board has none: never chosen, or its search was deleted.
    None,
    /// The caller does not view the board's backlog search.
    Hidden,
}

/// The saved search a backlog is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BacklogSearch {
    pub id: i64,
    pub name: String,
    /// The caller may change its order: they edit the search (BRD-093).
    pub may_order: bool,
}

/// A page of the board's backlog (BRD-089…BRD-093): issues the search orders by hand first, the
/// rest in its sort. Follow `page.live_topic` like a list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BoardBacklog {
    #[schema(required = true)]
    pub search: Option<BacklogSearch>,
    /// Set when there is no page.
    #[schema(required = true)]
    pub missing: Option<BacklogMissing>,
    #[schema(required = true)]
    pub page: Option<crate::search::IssueSearchPage>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BoardBacklogQuery {
    /// The panel's filter, ANDed with the backlog search.
    pub q: Option<String>,
    pub cursor: Option<String>,
    /// 1–100; default 50.
    pub limit: Option<u32>,
    /// IANA name for dates in `q`; the instance's by default.
    pub time_zone: Option<String>,
}

/// A new place of an issue in the backlog's order (BRD-093): after `after`, else before
/// `before`, else first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BacklogOrder {
    pub issue: i64,
    #[serde(default)]
    pub after: Option<i64>,
    #[serde(default)]
    pub before: Option<i64>,
}

/// A board and sprint holding an issue (BRD-033, ISS-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueBoard {
    pub board: BoardName,
    pub sprints_on: bool,
    /// `null` on a sprintless board.
    #[schema(required = true)]
    pub sprint: Option<SprintName>,
    /// A draft joins this board on its own when published (ISS-007).
    pub auto: bool,
    /// The draft is kept off it: it will not join when published.
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BoardName {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SprintName {
    pub id: i64,
    pub name: String,
}

/// The boards of an issue the caller views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IssueBoards {
    pub items: Vec<IssueBoard>,
}

/// Puts an issue on a board: into `sprint`, else the board's current sprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewIssueBoard {
    pub board: i64,
    #[serde(default)]
    pub sprint: Option<i64>,
}

/// The cell whose «+» opens the new-issue form (BRD-077).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct NewCellDraft {
    pub sprint: i64,
    pub column: i64,
    /// The lane of the cell, on a board with lanes.
    #[serde(default)]
    pub lane: Option<String>,
    /// The project of a new draft; absent takes the lane's project, else the first board project
    /// the caller creates issues in.
    #[serde(default)]
    pub project: Option<i64>,
}

/// The caller's draft of a cell (BRD-077): kept until it is published or deleted. Open it like
/// any draft (`GET /issues/{id}`); it is a card of the sprint already and shows once published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CellDraft {
    pub issue: i64,
    /// Made now, rather than the kept one.
    pub created: bool,
}

/// How a board uses a field (BRD-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldBoardUse {
    pub board: BoardName,
    pub column: bool,
    pub lane: bool,
    pub color: bool,
    pub estimation: bool,
    pub original_estimation: bool,
    pub cards: bool,
}

/// The boards using a field (BRD-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldBoards {
    /// The boards the caller views, by name.
    pub items: Vec<FieldBoardUse>,
    /// Boards using it that the caller does not view.
    pub others: i64,
}
