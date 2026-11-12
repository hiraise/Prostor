//! Records of the change history (HIST-002…HIST-005, HIST-012): what each kind of change stores in
//! `removed` and `added`. The same records feed the activity stream, notifications (1.10), live
//! events (1.6) and reports (P3), so their shapes are fixed here:
//!
//! - `created`, `resolved`, `reopened`, `deleted`: nothing;
//! - `summary`, `description`: the old and the new text, as JSON strings (HIST-003);
//! - `field`: arrays of the values that left and came, each value separately for multi-valued
//!   fields (HIST-004): strings for texts, dates (`YYYY-MM-DD`) and times (RFC 3339), numbers for
//!   numbers and durations (minutes), account and group IDs as numbers, values of sets as
//!   `{"id", "name"}` with the name they had;
//! - `project`: `{"project", "number", "id"}` before and after (HIST-005);
//! - `tags`: arrays of tag IDs (only their owners see personal tags, HIST-007);
//! - `visibility`: `{"users", "groups"}` before and after;
//! - `comment`, `comment_deleted`, `comment_restored`: nothing; the record names the comment
//!   (HIST-006);
//! - `comment_edited`: what changed, before and after: `{"text": …}` and/or
//!   `{"users": […], "groups": […]}` (CNT-004; the comment's versions, CNT-008, read these);
//! - `attachments`: arrays of the files that left and came, `{"id", "name"}` with the name they
//!   had (CNT-017, CNT-024);
//! - `links`: arrays of the links that left and came, `{"type", "side", "issue"}`: the link
//!   type, this issue's side and the other issue (HIST-005, LNK-003). Names are not kept: a
//!   renamed type is shown with its new names (LNK-020);
//! - `boards`: the board place that was left and the one that was joined,
//!   `{"board": {"id", "name"}, "sprint": {"id", "name"}}` (no `sprint` on a sprintless board),
//!   with the names they had, so a deleted board stays readable (HIST-002, BRD-025).

use serde_json::{Value as Json, json};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

use super::values::Value;
use crate::{BoardId, CommentId, FieldId, GroupId, ProjectId, SprintId, UserId};

/// Kinds of records (HIST-002). Links, attachments, boards and work items add theirs with their
/// steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Created,
    Summary,
    Description,
    Field,
    Project,
    Tags,
    Visibility,
    Resolved,
    Reopened,
    Deleted,
    /// A comment was posted: the stream shows the comment (HIST-006).
    Comment,
    CommentEdited,
    CommentDeleted,
    CommentRestored,
    /// Files attached or deleted (CNT-017, CNT-024).
    Attachments,
    /// Links added or removed (HIST-005).
    Links,
    /// Added to or removed from a board (HIST-002).
    Boards,
}

impl ChangeKind {
    pub const ALL: [ChangeKind; 17] = [
        ChangeKind::Created,
        ChangeKind::Summary,
        ChangeKind::Description,
        ChangeKind::Field,
        ChangeKind::Project,
        ChangeKind::Tags,
        ChangeKind::Visibility,
        ChangeKind::Resolved,
        ChangeKind::Reopened,
        ChangeKind::Deleted,
        ChangeKind::Comment,
        ChangeKind::CommentEdited,
        ChangeKind::CommentDeleted,
        ChangeKind::CommentRestored,
        ChangeKind::Attachments,
        ChangeKind::Links,
        ChangeKind::Boards,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ChangeKind::Created => "created",
            ChangeKind::Summary => "summary",
            ChangeKind::Description => "description",
            ChangeKind::Field => "field",
            ChangeKind::Project => "project",
            ChangeKind::Tags => "tags",
            ChangeKind::Visibility => "visibility",
            ChangeKind::Resolved => "resolved",
            ChangeKind::Reopened => "reopened",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Comment => "comment",
            ChangeKind::CommentEdited => "comment_edited",
            ChangeKind::CommentDeleted => "comment_deleted",
            ChangeKind::CommentRestored => "comment_restored",
            ChangeKind::Attachments => "attachments",
            ChangeKind::Links => "links",
            ChangeKind::Boards => "boards",
        }
    }

    /// Records about a comment, which name it.
    pub fn is_comment(self) -> bool {
        matches!(
            self,
            ChangeKind::Comment
                | ChangeKind::CommentEdited
                | ChangeKind::CommentDeleted
                | ChangeKind::CommentRestored
        )
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

/// Where a change came from (architecture.md §9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// The web app, with a session.
    #[default]
    Web,
    /// A personal API token.
    Api,
    /// Background work of the server, such as a job applying an administrator's change.
    System,
    /// A built-in behavior made the change on its trigger; the author is who triggered it
    /// (AUT-023, step 1.9).
    Rule,
}

impl Origin {
    pub fn key(self) -> &'static str {
        match self {
            Origin::Web => "web",
            Origin::Api => "api",
            Origin::System => "system",
            Origin::Rule => "rule",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        [Origin::Web, Origin::Api, Origin::System, Origin::Rule]
            .into_iter()
            .find(|origin| origin.key() == key)
    }
}

/// A board, and the sprint of a sprint board, as a board record names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardPlace<'a> {
    pub board: (BoardId, &'a str),
    pub sprint: Option<(SprintId, &'a str)>,
}

impl BoardPlace<'_> {
    fn json(&self) -> Json {
        let mut place = json!({ "board": { "id": self.board.0, "name": self.board.1 } });
        if let Some((id, name)) = self.sprint {
            place["sprint"] = json!({ "id": id, "name": name });
        }
        place
    }
}

/// One record to write.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub kind: ChangeKind,
    pub field: Option<FieldId>,
    /// The comment of a comment record.
    pub comment: Option<CommentId>,
    pub removed: Option<Json>,
    pub added: Option<Json>,
}

impl Record {
    fn bare(kind: ChangeKind) -> Self {
        Self {
            kind,
            field: None,
            comment: None,
            removed: None,
            added: None,
        }
    }

    pub fn created() -> Self {
        Self::bare(ChangeKind::Created)
    }

    /// A creation by cloning another issue (AUT-066): the record names the original, which the
    /// stream renders as «created by cloning X».
    pub fn created_of(source: crate::IssueId) -> Self {
        Self {
            kind: ChangeKind::Created,
            field: None,
            comment: None,
            removed: None,
            added: Some(json!({ "clone_of": { "id": source } })),
        }
    }

    pub fn resolved(resolved: bool) -> Self {
        Self::bare(if resolved {
            ChangeKind::Resolved
        } else {
            ChangeKind::Reopened
        })
    }

    pub fn deleted() -> Self {
        Self::bare(ChangeKind::Deleted)
    }

    /// The issue left a board place and/or joined one (HIST-002); `None` when neither.
    pub fn boards(left: Option<&BoardPlace<'_>>, joined: Option<&BoardPlace<'_>>) -> Option<Self> {
        if left.is_none() && joined.is_none() || left == joined {
            return None;
        }
        Some(Self {
            kind: ChangeKind::Boards,
            field: None,
            comment: None,
            removed: left.map(BoardPlace::json),
            added: joined.map(BoardPlace::json),
        })
    }

    /// A summary or description change (HIST-003); `None` when the text did not change.
    pub fn text(kind: ChangeKind, old: &str, new: &str) -> Option<Self> {
        (old != new).then(|| Self {
            kind,
            field: None,
            comment: None,
            removed: Some(json!(old)),
            added: Some(json!(new)),
        })
    }

    /// A field change (HIST-004); `None` when nothing changed. `name_of` gives the names of set
    /// values, which the record keeps; it answers `None` for accounts and groups.
    pub fn field(
        field: FieldId,
        old: Option<&Value>,
        new: Option<&Value>,
        name_of: impl Fn(i64) -> Option<String>,
    ) -> Option<Self> {
        let (removed, added) = diff(old, new)?;
        Some(Self {
            kind: ChangeKind::Field,
            field: Some(field),
            comment: None,
            removed: Some(Json::Array(logged(removed.as_ref(), &name_of))),
            added: Some(Json::Array(logged(added.as_ref(), &name_of))),
        })
    }

    /// A move to another project (HIST-005).
    pub fn project(from: Place<'_>, to: Place<'_>) -> Self {
        Self {
            kind: ChangeKind::Project,
            field: None,
            comment: None,
            removed: Some(from.json()),
            added: Some(to.json()),
        }
    }

    /// Tags added and removed (HIST-005); `None` when there are none.
    pub fn tags(removed: &[i64], added: &[i64]) -> Option<Self> {
        (!removed.is_empty() || !added.is_empty()).then(|| Self {
            kind: ChangeKind::Tags,
            field: None,
            comment: None,
            removed: Some(json!(removed)),
            added: Some(json!(added)),
        })
    }

    /// An audience change (ISS-041, HIST-005); `None` when it is the same.
    pub fn visibility(old: (&[UserId], &[GroupId]), new: (&[UserId], &[GroupId])) -> Option<Self> {
        let same = |a: &[i64], b: &[i64]| a.len() == b.len() && a.iter().all(|x| b.contains(x));
        if same(old.0, new.0) && same(old.1, new.1) {
            return None;
        }
        Some(Self {
            kind: ChangeKind::Visibility,
            field: None,
            comment: None,
            removed: Some(json!({ "users": old.0, "groups": old.1 })),
            added: Some(json!({ "users": new.0, "groups": new.1 })),
        })
    }
}

impl Record {
    /// A comment posted, deleted or restored (HIST-006).
    pub fn comment(kind: CommentEvent, comment: CommentId) -> Self {
        Self {
            comment: Some(comment),
            ..Self::bare(match kind {
                CommentEvent::Posted => ChangeKind::Comment,
                CommentEvent::Deleted => ChangeKind::CommentDeleted,
                CommentEvent::Restored => ChangeKind::CommentRestored,
            })
        }
    }

    /// A comment's edit (CNT-004): what changed of its text and audience; `None` when nothing.
    pub fn comment_edited(
        comment: CommentId,
        text: Option<(&str, &str)>,
        audience: Option<(AudienceLists<'_>, AudienceLists<'_>)>,
    ) -> Option<Self> {
        let mut removed = serde_json::Map::new();
        let mut added = serde_json::Map::new();
        if let Some((old, new)) = text
            && old != new
        {
            removed.insert("text".to_owned(), json!(old));
            added.insert("text".to_owned(), json!(new));
        }
        if let Some((old, new)) = audience
            && Self::visibility(old, new).is_some()
        {
            removed.insert("users".to_owned(), json!(old.0));
            removed.insert("groups".to_owned(), json!(old.1));
            added.insert("users".to_owned(), json!(new.0));
            added.insert("groups".to_owned(), json!(new.1));
        }
        if added.is_empty() {
            return None;
        }
        Some(Self {
            comment: Some(comment),
            removed: Some(Json::Object(removed)),
            added: Some(Json::Object(added)),
            ..Self::bare(ChangeKind::CommentEdited)
        })
    }
}

impl Record {
    /// Files attached and deleted, each with the name it had; `None` when there are none.
    pub fn attachments(removed: &[(i64, &str)], added: &[(i64, &str)]) -> Option<Self> {
        if removed.is_empty() && added.is_empty() {
            return None;
        }
        let list = |files: &[(i64, &str)]| {
            Json::Array(
                files
                    .iter()
                    .map(|(id, name)| json!({ "id": id, "name": name }))
                    .collect(),
            )
        };
        Some(Self {
            removed: Some(list(removed)),
            added: Some(list(added)),
            ..Self::bare(ChangeKind::Attachments)
        })
    }
}

/// One end of a link as a record of one issue keeps it: the type, this issue's side, the other
/// issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoggedLink {
    pub link_type: i64,
    pub side: crate::links::Side,
    pub issue: crate::IssueId,
}

impl LoggedLink {
    fn json(self) -> Json {
        json!({ "type": self.link_type, "side": self.side.key(), "issue": self.issue })
    }

    /// Reads one entry of a `links` record.
    pub fn read(json: &Json) -> Option<Self> {
        Some(Self {
            link_type: json.get("type")?.as_i64()?,
            side: crate::links::Side::parse(json.get("side")?.as_str()?)?,
            issue: json.get("issue")?.as_i64()?,
        })
    }
}

impl Record {
    /// Links added and removed on one issue (HIST-005); `None` when there are none.
    pub fn links(removed: &[LoggedLink], added: &[LoggedLink]) -> Option<Self> {
        if removed.is_empty() && added.is_empty() {
            return None;
        }
        let list = |links: &[LoggedLink]| Json::Array(links.iter().map(|l| l.json()).collect());
        Some(Self {
            removed: Some(list(removed)),
            added: Some(list(added)),
            ..Self::bare(ChangeKind::Links)
        })
    }
}

/// An audience as two lists: accounts and groups.
pub type AudienceLists<'a> = (&'a [UserId], &'a [GroupId]);

/// The original a `created` record names, when the issue was made by cloning (AUT-066).
pub fn clone_of(json: &Json) -> Option<crate::IssueId> {
    json.get("clone_of")?.get("id")?.as_i64()
}

/// What happened to a comment, for its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentEvent {
    Posted,
    Deleted,
    Restored,
}

/// An issue's place: its project and number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place<'a> {
    pub project: ProjectId,
    pub number: i64,
    /// The readable ID at the time.
    pub id: &'a str,
}

impl Place<'_> {
    fn json(self) -> Json {
        json!({ "project": self.project, "number": self.number, "id": self.id })
    }
}

/// What left and what came between two values of a field; `None` when they are the same. For
/// references each one counts separately; other values are replaced whole.
pub fn diff(old: Option<&Value>, new: Option<&Value>) -> Option<(Option<Value>, Option<Value>)> {
    match (old, new) {
        (None, None) => None,
        (Some(Value::Refs(before)), Some(Value::Refs(after))) => {
            let gone: Vec<i64> = before
                .iter()
                .filter(|id| !after.contains(id))
                .copied()
                .collect();
            let came: Vec<i64> = after
                .iter()
                .filter(|id| !before.contains(id))
                .copied()
                .collect();
            if gone.is_empty() && came.is_empty() {
                return None;
            }
            let wrap = |ids: Vec<i64>| (!ids.is_empty()).then_some(Value::Refs(ids));
            Some((wrap(gone), wrap(came)))
        }
        (old, new) if old == new => None,
        (old, new) => Some((old.cloned(), new.cloned())),
    }
}

/// A value as the history stores it.
pub fn logged(value: Option<&Value>, name_of: &impl Fn(i64) -> Option<String>) -> Vec<Json> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        Value::String(text) | Value::Text(text) => vec![json!(text)],
        Value::Integer(number) | Value::Duration(number) => vec![json!(number)],
        Value::Float(number) => vec![json!(number)],
        Value::Date(date) => date
            .format(format_description!("[year]-[month]-[day]"))
            .map(|text| vec![json!(text)])
            .unwrap_or_default(),
        Value::DateTime(at) => at
            .format(&Rfc3339)
            .map(|text| vec![json!(text)])
            .unwrap_or_default(),
        Value::Refs(ids) => ids
            .iter()
            .map(|id| match name_of(*id) {
                Some(name) => json!({ "id": id, "name": name }),
                None => json!(id),
            })
            .collect(),
    }
}

/// A reference read back from a record: its ID and, for values of sets, the name it had.
pub fn read_ref(item: &Json) -> Option<(i64, Option<String>)> {
    match item {
        Json::Number(number) => number.as_i64().map(|id| (id, None)),
        Json::Object(map) => {
            let id = map.get("id")?.as_i64()?;
            let name = map.get("name").and_then(Json::as_str).map(str::to_owned);
            Some((id, name))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn hist_002_kinds_and_origins() {
        for kind in ChangeKind::ALL {
            assert_eq!(ChangeKind::parse(kind.key()), Some(kind));
        }
        assert_eq!(Origin::parse("api"), Some(Origin::Api));
        assert_eq!(Origin::parse("x"), None);
    }

    #[test]
    fn hist_003_texts_keep_both_versions() {
        let record = Record::text(ChangeKind::Summary, "old", "new").unwrap();
        assert_eq!(record.removed, Some(json!("old")));
        assert_eq!(record.added, Some(json!("new")));
        assert_eq!(Record::text(ChangeKind::Description, "a", "a"), None);
    }

    #[test]
    fn hist_004_each_reference_separately() {
        let names = |id: i64| (id < 100).then(|| format!("v{id}"));
        let record = Record::field(
            3,
            Some(&Value::Refs(vec![1, 2])),
            Some(&Value::Refs(vec![2, 5, 200])),
            names,
        )
        .unwrap();
        assert_eq!(record.field, Some(3));
        assert_eq!(record.removed, Some(json!([{ "id": 1, "name": "v1" }])));
        assert_eq!(record.added, Some(json!([{ "id": 5, "name": "v5" }, 200])));
        assert_eq!(
            Record::field(
                3,
                Some(&Value::Refs(vec![2, 1])),
                Some(&Value::Refs(vec![1, 2])),
                names
            ),
            None
        );
        let cleared =
            Record::field(4, Some(&Value::Date(date!(2026 - 10 - 22))), None, names).unwrap();
        assert_eq!(cleared.removed, Some(json!(["2026-10-22"])));
        assert_eq!(cleared.added, Some(json!([])));
        assert_eq!(
            read_ref(&json!({ "id": 1, "name": "v1" })),
            Some((1, Some("v1".to_owned())))
        );
        assert_eq!(read_ref(&json!(7)), Some((7, None)));
        assert_eq!(read_ref(&json!("x")), None);
    }

    #[test]
    fn hist_005_tags_visibility_and_moves() {
        assert_eq!(Record::tags(&[], &[]), None);
        let tags = Record::tags(&[1], &[2, 3]).unwrap();
        assert_eq!(tags.added, Some(json!([2, 3])));
        assert_eq!(Record::visibility((&[1, 2], &[]), (&[2, 1], &[])), None);
        let audience = Record::visibility((&[], &[]), (&[4], &[9])).unwrap();
        assert_eq!(audience.added, Some(json!({ "users": [4], "groups": [9] })));
        let moved = Record::project(
            Place {
                project: 1,
                number: 5,
                id: "A-5",
            },
            Place {
                project: 2,
                number: 9,
                id: "B-9",
            },
        );
        assert_eq!(
            moved.removed,
            Some(json!({ "project": 1, "number": 5, "id": "A-5" }))
        );
    }

    #[test]
    fn hist_006_cnt_004_comment_records() {
        let posted = Record::comment(CommentEvent::Posted, 7);
        assert_eq!(
            (posted.kind, posted.comment),
            (ChangeKind::Comment, Some(7))
        );
        assert!(posted.kind.is_comment() && !ChangeKind::Field.is_comment());
        assert_eq!(Record::comment_edited(7, Some(("a", "a")), None), None);
        let edited = Record::comment_edited(7, Some(("a", "b")), None).unwrap();
        assert_eq!(edited.removed, Some(json!({ "text": "a" })));
        assert_eq!(edited.added, Some(json!({ "text": "b" })));
        let both =
            Record::comment_edited(7, Some(("a", "b")), Some(((&[], &[]), (&[3], &[])))).unwrap();
        assert_eq!(
            both.added,
            Some(json!({ "text": "b", "users": [3], "groups": [] }))
        );
    }

    #[test]
    fn cnt_017_attachment_records() {
        assert_eq!(Record::attachments(&[], &[]), None);
        let record = Record::attachments(&[], &[(4, "shot.png")]).unwrap();
        assert_eq!(record.kind, ChangeKind::Attachments);
        assert_eq!(record.kind.key(), "attachments");
        assert_eq!(record.removed, Some(json!([])));
        assert_eq!(record.added, Some(json!([{ "id": 4, "name": "shot.png" }])));
        assert_eq!(
            read_ref(&json!({ "id": 4, "name": "shot.png" })),
            Some((4, Some("shot.png".to_owned())))
        );
    }

    #[test]
    fn hist_002_board_records() {
        let a = BoardPlace {
            board: (4, "Team"),
            sprint: Some((8, "Sprint 1")),
        };
        let b = BoardPlace {
            sprint: Some((9, "Sprint 2")),
            ..a
        };
        let moved = Record::boards(Some(&a), Some(&b)).unwrap();
        assert_eq!(moved.kind, ChangeKind::Boards);
        assert_eq!(
            moved.removed,
            Some(
                json!({"board": {"id": 4, "name": "Team"}, "sprint": {"id": 8, "name": "Sprint 1"}})
            )
        );
        let kanban = BoardPlace {
            board: (5, "Flow"),
            sprint: None,
        };
        let added = Record::boards(None, Some(&kanban)).unwrap();
        assert_eq!(added.removed, None);
        assert_eq!(
            added.added,
            Some(json!({"board": {"id": 5, "name": "Flow"}}))
        );
        assert_eq!(Record::boards(Some(&a), Some(&a)), None);
        assert_eq!(Record::boards(None, None), None);
    }

    #[test]
    fn hist_005_links_keep_type_side_and_other_issue() {
        use crate::links::Side;
        let link = LoggedLink {
            link_type: 4,
            side: Side::Target,
            issue: 77,
        };
        let record = Record::links(&[], &[link]).unwrap();
        assert_eq!(record.kind, ChangeKind::Links);
        assert_eq!(record.removed, Some(json!([])));
        assert_eq!(
            record.added,
            Some(json!([{ "type": 4, "side": "target", "issue": 77 }]))
        );
        assert_eq!(
            LoggedLink::read(&json!({ "type": 4, "side": "target", "issue": 77 })),
            Some(link)
        );
        assert_eq!(
            LoggedLink::read(&json!({ "type": 4, "side": "up", "issue": 77 })),
            None
        );
        assert_eq!(Record::links(&[], &[]), None);
    }

    #[test]
    fn aut_066_created_names_the_original() {
        let record = Record::created_of(77);
        assert_eq!(record.kind, ChangeKind::Created);
        assert_eq!(record.added, Some(json!({ "clone_of": { "id": 77 } })));
        assert_eq!(record.added.as_ref().and_then(clone_of), Some(77));
        // An ordinary creation names nothing.
        assert_eq!(Record::created().added.and_then(|j| clone_of(&j)), None);
    }
}
