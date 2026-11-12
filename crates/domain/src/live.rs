//! Live events (ISS-048, QRY-086, ADM-002; D-040, architecture.md §5): the changes the gateway
//! pushes to open tabs, who may receive each one, and the topics clients subscribe to.
//!
//! An event says only "this entity has version n now" (an issue's shape depends on the viewer), so
//! its guard needs only the visibility facts of the entity and of what contains it, taken after
//! the change. The gateway checks the guard per receiver against their current access. The one
//! patch is the card order of a board ([`Patch::CardOrder`]): IDs only, the same for everyone.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::access::Access;
use crate::access::visibility::{self, Audience, Item, Kind};
use crate::boards::access::BoardAccess;
use crate::issues::rights::{self, Facts as IssueFacts};
use crate::sharing::{Audience as Shared, Shared as Sharing};
use crate::{BoardId, GroupId, IssueId, ProjectId, SprintId, UserId, attachments, comments};

/// What an event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Issue,
    Comment,
    Attachment,
    IssueLink,
    /// A board's settings changed, or it was deleted (BRD-115).
    Board,
    /// A sprint of a board was created, changed or deleted.
    Sprint,
    /// A card moved inside the shared order of a sprint (BRD-069); the entity is the issue.
    CardOrder,
    /// A notification or a read change reached one person's inbox (NTF, plan step 1.10 §3.6);
    /// the entity is the reader.
    Inbox,
}

impl EntityKind {
    pub const ALL: [EntityKind; 8] = [
        EntityKind::Issue,
        EntityKind::Comment,
        EntityKind::Attachment,
        EntityKind::IssueLink,
        EntityKind::Board,
        EntityKind::Sprint,
        EntityKind::CardOrder,
        EntityKind::Inbox,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Issue => "issue",
            EntityKind::Comment => "comment",
            EntityKind::Attachment => "attachment",
            EntityKind::IssueLink => "issue_link",
            EntityKind::Board => "board",
            EntityKind::Sprint => "sprint",
            EntityKind::CardOrder => "card_order",
            EntityKind::Inbox => "inbox",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }
}

/// What happened to the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Change {
    Created,
    /// Changed, soft-deleted or restored: the entity is still readable.
    Updated,
    /// Gone for good, or an issue moved to the trash.
    Deleted,
}

impl Change {
    pub const ALL: [Change; 3] = [Change::Created, Change::Updated, Change::Deleted];

    pub fn as_str(self) -> &'static str {
        match self {
            Change::Created => "created",
            Change::Updated => "updated",
            Change::Deleted => "deleted",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|change| change.as_str() == raw)
    }

    /// One transaction changed an entity twice: what its single event says. A new entity changed
    /// again is still new; otherwise the later change counts.
    pub fn then(self, later: Change) -> Change {
        match (self, later) {
            (Change::Created, Change::Updated) => Change::Created,
            (_, later) => later,
        }
    }
}

/// The version link events carry: links are made and removed, never changed.
pub const LINK_VERSION: i64 = 1;

/// One condition of a guard: the visibility facts of one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardItem {
    pub kind: Kind,
    pub project: ProjectId,
    /// Reporter of an issue, author of a comment or file.
    pub author: Option<UserId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience_users: Vec<UserId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience_groups: Vec<GroupId>,
    /// An unpublished issue: only its reporter passes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
}

impl GuardItem {
    pub fn issue(issue: &IssueFacts<'_>) -> Self {
        Self::of(Kind::Issue, issue.project, issue.reporter, issue.audience).drafted(issue.draft)
    }

    pub fn comment(comment: &comments::Facts<'_>) -> Self {
        Self::of(
            Kind::Comment,
            comment.project,
            comment.author,
            comment.audience,
        )
    }

    pub fn attachment(file: &attachments::Facts<'_>) -> Self {
        Self::of(Kind::Attachment, file.project, file.author, file.audience)
    }

    fn of(kind: Kind, project: ProjectId, author: UserId, audience: Audience<'_>) -> Self {
        Self {
            kind,
            project,
            author: Some(author),
            audience_users: audience.users.to_vec(),
            audience_groups: audience.groups.to_vec(),
            draft: false,
        }
    }

    fn drafted(self, draft: bool) -> Self {
        Self { draft, ..self }
    }

    /// The rule of the item's kind: `rights::can_read` for issues (drafts included), the
    /// visibility rule for comments and files.
    fn passes(&self, viewer: &Access) -> bool {
        let audience = Audience {
            users: &self.audience_users,
            groups: &self.audience_groups,
        };
        match (self.kind, self.author) {
            (Kind::Issue, Some(reporter)) => rights::can_read(
                viewer,
                &IssueFacts {
                    project: self.project,
                    reporter,
                    draft: self.draft,
                    audience,
                },
            ),
            _ => {
                !self.draft
                    && visibility::can_see(
                        viewer,
                        self.kind,
                        Item {
                            project: self.project,
                            author: self.author,
                            audience,
                        },
                    )
            }
        }
    }
}

/// The view rule of a board (BRD-012): its owner, audiences, and whether readers of every board
/// project view it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardGuard {
    pub owner: UserId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_users: Vec<UserId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_groups: Vec<GroupId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edit_users: Vec<UserId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edit_groups: Vec<GroupId>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub view_readers: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<ProjectId>,
}

impl BoardGuard {
    pub fn of(access: &BoardAccess) -> Self {
        Self {
            owner: access.shared.owner,
            view_users: access.shared.view.users.clone(),
            view_groups: access.shared.view.groups.clone(),
            edit_users: access.shared.edit.users.clone(),
            edit_groups: access.shared.edit.groups.clone(),
            view_readers: access.view_readers,
            projects: access.projects.clone(),
        }
    }

    fn passes(&self, viewer: &Access) -> bool {
        BoardAccess {
            shared: Sharing {
                owner: self.owner,
                view: Shared {
                    users: self.view_users.clone(),
                    groups: self.view_groups.clone(),
                },
                use_: Shared::default(),
                edit: Shared {
                    users: self.edit_users.clone(),
                    groups: self.edit_groups.clone(),
                },
            },
            view_readers: self.view_readers,
            edit_updaters: false,
            projects: self.projects.clone(),
        }
        .can_view(viewer)
    }
}

/// Who may receive an event (D-040): a receiver passes when they pass every item, and the board's
/// view rule when there is one, and they are the named person when one is named. Nothing: everyone
/// signed in (notices).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "GuardForm", into = "GuardForm")]
pub struct Guard {
    pub items: Vec<GuardItem>,
    pub board: Option<BoardGuard>,
    /// Only this person may receive the event (an inbox change of theirs); never set together
    /// with items or a board.
    pub user: Option<UserId>,
}

/// The stored form: a list of items, or an object when a board's rule is part of it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum GuardForm {
    Items(Vec<GuardItem>),
    Board {
        #[serde(default)]
        items: Vec<GuardItem>,
        board: BoardGuard,
    },
    User {
        user: UserId,
    },
}

impl From<GuardForm> for Guard {
    fn from(form: GuardForm) -> Self {
        match form {
            GuardForm::Items(items) => Self {
                items,
                board: None,
                user: None,
            },
            GuardForm::Board { items, board } => Self {
                items,
                board: Some(board),
                user: None,
            },
            GuardForm::User { user } => Self {
                items: Vec::new(),
                board: None,
                user: Some(user),
            },
        }
    }
}

impl From<Guard> for GuardForm {
    fn from(guard: Guard) -> Self {
        match (guard.board, guard.user) {
            (None, None) => GuardForm::Items(guard.items),
            (Some(board), None) => GuardForm::Board {
                items: guard.items,
                board,
            },
            // Constructors never name a user together with items or a board.
            (_, Some(user)) => GuardForm::User { user },
        }
    }
}

impl Guard {
    fn items(items: Vec<GuardItem>) -> Self {
        Self {
            items,
            board: None,
            user: None,
        }
    }

    pub fn issue(issue: &IssueFacts<'_>) -> Self {
        Self::items(vec![GuardItem::issue(issue)])
    }

    pub fn comment(issue: &IssueFacts<'_>, comment: &comments::Facts<'_>) -> Self {
        Self::items(vec![GuardItem::issue(issue), GuardItem::comment(comment)])
    }

    /// A board, or a sprint of it: those who view the board (BRD-115).
    pub fn board(access: &BoardAccess) -> Self {
        Self {
            items: Vec::new(),
            board: Some(BoardGuard::of(access)),
            user: None,
        }
    }

    /// A file of an issue, or of a comment: then the comment must be seen too.
    pub fn attachment(
        issue: &IssueFacts<'_>,
        comment: Option<&comments::Facts<'_>>,
        file: &attachments::Facts<'_>,
    ) -> Self {
        let mut items = vec![GuardItem::issue(issue)];
        items.extend(comment.map(GuardItem::comment));
        items.push(GuardItem::attachment(file));
        Self::items(items)
    }

    /// A link counts only for those who see both ends (D-034).
    pub fn link(source: &IssueFacts<'_>, target: &IssueFacts<'_>) -> Self {
        Self::items(vec![GuardItem::issue(source), GuardItem::issue(target)])
    }

    /// Only this person (their inbox; plan step 1.10 §3.6).
    pub fn user(user: UserId) -> Self {
        Self {
            items: Vec::new(),
            board: None,
            user: Some(user),
        }
    }

    pub fn allows(&self, viewer: &Access) -> bool {
        self.items.iter().all(|item| item.passes(viewer))
            && self.board.as_ref().is_none_or(|board| board.passes(viewer))
            && self.user.is_none_or(|user| user == viewer.user())
    }

    /// Whether it concerns an unpublished issue: such changes are not published.
    pub fn has_draft(&self) -> bool {
        self.items.iter().any(|item| item.draft)
    }
}

/// One change to publish: one row of the event log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    pub kind: EntityKind,
    pub entity: i64,
    /// After the change; for a deletion the last one.
    pub version: i64,
    pub change: Change,
    /// Issues whose topics get it: one, or both ends of a link.
    pub issues: Vec<IssueId>,
    /// Projects for list topics: the issue's, or the old and the new for a move.
    pub projects: Vec<ProjectId>,
    /// Boards whose topics get it besides those of the projects (BRD-114).
    pub boards: Vec<BoardId>,
    pub patch: Option<Patch>,
    pub guard: Guard,
}

/// What an event says beyond the version, the same for every receiver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Patch {
    /// The issue's card now follows `after` in the sprint (`None`: first) (BRD-115).
    CardOrder {
        issue: IssueId,
        after: Option<IssueId>,
        sprint: SprintId,
    },
    /// The issue now follows `after` in the manual order of the backlog `search` (BRD-093); the
    /// event's kind is still `card_order`.
    BacklogOrder {
        issue: IssueId,
        after: Option<IssueId>,
        search: i64,
    },
    /// A row reached the reader's inbox (plan step 1.10 §3.6): the event's kind is `inbox`, the
    /// entity the reader; the popup shows this without fetching. `event` is the row's kind key,
    /// `author` the author's name as this reader may see it.
    Notification {
        id: i64,
        event: String,
        /// The issue's readable ID, such as `WEB-12`; `system` rows carry none.
        issue: Option<String>,
        author: String,
        excerpt: String,
        reasons: Vec<String>,
    },
    /// The reader's read state changed (NTF-050): the new unread counts, the badge's numbers.
    Read {
        total: i64,
        direct: i64,
        subscriptions: i64,
        system: i64,
    },
}

/// Most projects a list topic names; wider lists get `list:all`.
pub const LIST_PROJECTS_MAX: usize = 100;

/// What a client subscribes to (architecture.md §5). A topic has one spelling: a string that does
/// not print back identically is malformed, so equal lists share one topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    /// `issue:<id>`: changes of the issue and of what is inside it.
    Issue(IssueId),
    /// `list:all` or `list:<ids>`: issue changes in these projects.
    List(ListScope),
    /// `instance`: settings and access changed.
    Instance,
    /// `board:<id>`: the board, its sprints and cards, and issue changes in its projects.
    Board(BoardId),
    /// `inbox:<id>`: notifications and read changes of this one person's inbox; only they may
    /// subscribe (plan step 1.10 §3.6).
    Inbox(UserId),
}

/// The projects of a list topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ListScope {
    All,
    /// Ascending, unique, 1 to [`LIST_PROJECTS_MAX`].
    Projects(Vec<ProjectId>),
}

impl Topic {
    pub fn parse(raw: &str) -> Option<Self> {
        let topic = Self::read(raw)?;
        (topic.to_string() == raw).then_some(topic)
    }

    fn read(raw: &str) -> Option<Self> {
        if raw == "instance" {
            return Some(Topic::Instance);
        }
        let (kind, rest) = raw.split_once(':')?;
        match kind {
            "issue" => positive(rest).map(Topic::Issue),
            "board" => positive(rest).map(Topic::Board),
            "inbox" => positive(rest).map(Topic::Inbox),
            "list" if rest == "all" => Some(Topic::List(ListScope::All)),
            "list" => {
                let ids: Vec<ProjectId> = rest.split(',').map(positive).collect::<Option<_>>()?;
                let valid = ids.len() <= LIST_PROJECTS_MAX && ids.windows(2).all(|w| w[0] < w[1]);
                valid.then_some(Topic::List(ListScope::Projects(ids)))
            }
            _ => None,
        }
    }
}

fn positive(raw: &str) -> Option<i64> {
    raw.parse().ok().filter(|id| *id > 0)
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Topic::Issue(id) => write!(f, "issue:{id}"),
            Topic::List(ListScope::All) => f.write_str("list:all"),
            Topic::List(ListScope::Projects(ids)) => {
                f.write_str("list:")?;
                for (n, id) in ids.iter().enumerate() {
                    if n > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{id}")?;
                }
                Ok(())
            }
            Topic::Instance => f.write_str("instance"),
            Topic::Board(id) => write!(f, "board:{id}"),
            Topic::Inbox(id) => write!(f, "inbox:{id}"),
        }
    }
}

/// The topic of a list whose query covers these projects (QRY-086): its own projects when a
/// context or `project:` conditions narrowed it to at most [`LIST_PROJECTS_MAX`], else `list:all`.
pub fn list_topic(scope: &[ProjectId], narrowed: bool) -> Topic {
    let mut ids = scope.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if narrowed && !ids.is_empty() && ids.len() <= LIST_PROJECTS_MAX {
        Topic::List(ListScope::Projects(ids))
    } else {
        Topic::List(ListScope::All)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::capability::CapabilitySet;
    use crate::access::permission::{Permission, PermissionSet};
    use crate::access::role::PredefinedRole;
    use crate::access::{Grant, Scope};

    const OPEN: Audience<'static> = Audience {
        users: &[],
        groups: &[],
    };

    fn viewer(user: UserId, permissions: PermissionSet, groups: Vec<GroupId>) -> Access {
        Access::compile(
            user,
            [Grant {
                permissions,
                scope: Scope::Project(1),
            }],
            groups,
            CapabilitySet::default(),
        )
    }

    fn member(user: UserId) -> Access {
        viewer(user, PredefinedRole::ProjectMember.permissions(), vec![20])
    }

    fn issue(reporter: UserId, audience: Audience<'_>) -> IssueFacts<'_> {
        IssueFacts {
            project: 1,
            reporter,
            draft: false,
            audience,
        }
    }

    #[test]
    fn live_guard_issue_follows_the_read_rule() {
        let guard = Guard::issue(&issue(9, OPEN));
        assert!(guard.allows(&member(5)));
        assert!(!guard.allows(&viewer(5, PermissionSet::EMPTY, vec![])));
        let elsewhere = IssueFacts {
            project: 2,
            ..issue(9, OPEN)
        };
        assert!(!Guard::issue(&elsewhere).allows(&member(5)));
    }

    #[test]
    fn live_guard_audiences_narrow() {
        let only_7 = Audience {
            users: &[7],
            groups: &[],
        };
        let guard = Guard::issue(&issue(9, only_7));
        assert!(!guard.allows(&member(5)));
        assert!(guard.allows(&member(7)));
        // The reporter and those in a listed group pass.
        assert!(Guard::issue(&issue(5, only_7)).allows(&member(5)));
        let group_20 = Audience {
            users: &[],
            groups: &[20],
        };
        assert!(Guard::issue(&issue(9, group_20)).allows(&member(5)));
        let admin = viewer(8, PredefinedRole::SystemAdmin.permissions(), vec![]);
        assert!(guard.allows(&admin));
    }

    #[test]
    fn live_guard_drafts_reach_only_their_reporter() {
        let draft = IssueFacts {
            draft: true,
            ..issue(5, OPEN)
        };
        let guard = Guard::issue(&draft);
        assert!(guard.has_draft());
        assert!(guard.allows(&member(5)));
        assert!(!guard.allows(&member(6)));
        let admin = viewer(8, PredefinedRole::SystemAdmin.permissions(), vec![]);
        assert!(!guard.allows(&admin));
        assert!(!Guard::issue(&issue(5, OPEN)).has_draft());
    }

    #[test]
    fn live_guard_comment_needs_the_issue_and_the_comment() {
        let only_7 = Audience {
            users: &[7],
            groups: &[],
        };
        let comment = comments::Facts {
            project: 1,
            author: 6,
            audience: only_7,
        };
        let open_issue = issue(9, OPEN);
        let guard = Guard::comment(&open_issue, &comment);
        assert!(guard.allows(&member(7)));
        assert!(guard.allows(&member(6)));
        assert!(!guard.allows(&member(5)));
        // The comment is open but the issue is not.
        let hidden_issue = issue(9, only_7);
        let open_comment = comments::Facts {
            audience: OPEN,
            ..comment
        };
        assert!(!Guard::comment(&hidden_issue, &open_comment).allows(&member(5)));
        // Reading issues without reading comments.
        let issues_only: PermissionSet = [Permission::IssueRead].into_iter().collect();
        assert!(!Guard::comment(&open_issue, &open_comment).allows(&viewer(
            5,
            issues_only,
            vec![]
        )));
    }

    #[test]
    fn live_guard_attachment_needs_its_issue_and_comment() {
        let only_7 = Audience {
            users: &[7],
            groups: &[],
        };
        let open_issue = issue(9, OPEN);
        let file = attachments::Facts {
            project: 1,
            author: 6,
            audience: OPEN,
        };
        assert!(Guard::attachment(&open_issue, None, &file).allows(&member(5)));
        let hidden_file = attachments::Facts {
            audience: only_7,
            ..file
        };
        assert!(!Guard::attachment(&open_issue, None, &hidden_file).allows(&member(5)));
        assert!(Guard::attachment(&open_issue, None, &hidden_file).allows(&member(7)));
        // A comment's file: the comment's audience counts too.
        let comment = comments::Facts {
            project: 1,
            author: 6,
            audience: only_7,
        };
        let guard = Guard::attachment(&open_issue, Some(&comment), &file);
        assert_eq!(guard.items.len(), 3);
        assert!(!guard.allows(&member(5)));
        assert!(guard.allows(&member(7)));
    }

    #[test]
    fn live_guard_link_needs_both_ends() {
        let only_7 = Audience {
            users: &[7],
            groups: &[],
        };
        let open = issue(9, OPEN);
        let hidden = issue(9, only_7);
        assert!(Guard::link(&open, &open).allows(&member(5)));
        assert!(!Guard::link(&open, &hidden).allows(&member(5)));
        assert!(!Guard::link(&hidden, &open).allows(&member(5)));
        assert!(Guard::link(&open, &hidden).allows(&member(7)));
    }

    #[test]
    fn live_guard_empty_lets_everyone_signed_in() {
        let guard = Guard::default();
        assert!(guard.allows(&viewer(5, PermissionSet::EMPTY, vec![])));
        assert!(!guard.has_draft());
    }

    #[test]
    fn live_guard_survives_json() {
        let only_7 = Audience {
            users: &[7],
            groups: &[20],
        };
        let draft = IssueFacts {
            draft: true,
            ..issue(5, OPEN)
        };
        let comment = comments::Facts {
            project: 1,
            author: 6,
            audience: only_7,
        };
        for guard in [
            Guard::default(),
            Guard::issue(&draft),
            Guard::comment(&issue(9, only_7), &comment),
        ] {
            let json = serde_json::to_value(&guard).unwrap();
            assert_eq!(serde_json::from_value::<Guard>(json).unwrap(), guard);
        }
        // Empty audiences and `draft: false` are left out.
        let json = serde_json::to_value(Guard::issue(&issue(9, OPEN))).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{"kind": "issue", "project": 1, "author": 9}])
        );
    }

    #[test]
    fn live_kinds_and_changes_round_trip() {
        for kind in EntityKind::ALL {
            assert_eq!(EntityKind::parse(kind.as_str()), Some(kind));
        }
        for change in Change::ALL {
            assert_eq!(Change::parse(change.as_str()), Some(change));
        }
        assert_eq!(EntityKind::parse("link"), None);
        assert_eq!(Change::parse("moved"), None);
        assert_eq!(Change::Created.then(Change::Updated), Change::Created);
        assert_eq!(Change::Created.then(Change::Deleted), Change::Deleted);
        assert_eq!(Change::Updated.then(Change::Deleted), Change::Deleted);
        assert_eq!(Change::Deleted.then(Change::Updated), Change::Updated);
    }

    #[test]
    fn brd_115_board_guard_follows_the_view_rule() {
        let access = BoardAccess {
            shared: Sharing {
                owner: 9,
                view: Shared {
                    users: vec![7],
                    groups: Vec::new(),
                },
                ..Sharing::default()
            },
            ..BoardAccess::default()
        };
        let guard = Guard::board(&access);
        assert!(guard.allows(&member(9)));
        assert!(guard.allows(&member(7)));
        assert!(!guard.allows(&member(5)));
        let json = serde_json::to_value(&guard).unwrap();
        assert_eq!(serde_json::from_value::<Guard>(json).unwrap(), guard);
        let readers = Guard::board(&BoardAccess {
            view_readers: true,
            projects: vec![1],
            ..access
        });
        assert!(readers.allows(&member(5)));
    }

    #[test]
    fn brd_115_order_patch_shape() {
        let patch = Patch::CardOrder {
            issue: 3,
            after: None,
            sprint: 2,
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            serde_json::json!({"kind": "card_order", "issue": 3, "after": null, "sprint": 2})
        );
    }

    #[test]
    fn live_topic_round_trip() {
        for raw in [
            "issue:42",
            "list:all",
            "list:3",
            "list:1,2,30",
            "instance",
            "board:7",
            "inbox:9",
        ] {
            let topic = Topic::parse(raw).unwrap();
            assert_eq!(topic.to_string(), raw);
        }
        assert_eq!(Topic::parse("issue:42"), Some(Topic::Issue(42)));
        assert_eq!(
            Topic::parse("list:1,2"),
            Some(Topic::List(ListScope::Projects(vec![1, 2])))
        );
        let hundred = (1..=100)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert!(Topic::parse(&format!("list:{hundred}")).is_some());
    }

    #[test]
    fn live_topic_malformed() {
        let hundred_one = (1..=101)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        for raw in [
            "",
            "issue",
            "issue:",
            "issue:0",
            "issue:-3",
            "issue:+3",
            "issue:03",
            "issue:3:4",
            "issue: 3",
            "list:",
            "list:2,1",
            "list:1,1",
            "list:0,1",
            "list:1,,2",
            "list:1,",
            "list:ALL",
            "list:all,1",
            &format!("list:{hundred_one}"),
            "instance:1",
            "Instance",
            "board:1:2",
            "inbox",
            "article:5",
        ] {
            assert_eq!(Topic::parse(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn live_topic_of_a_list() {
        assert_eq!(list_topic(&[3, 1, 3], true).to_string(), "list:1,3");
        assert_eq!(list_topic(&[3, 1], false).to_string(), "list:all");
        assert_eq!(list_topic(&[], true).to_string(), "list:all");
        let wide: Vec<ProjectId> = (1..=101).collect();
        assert_eq!(list_topic(&wide, true).to_string(), "list:all");
        assert_eq!(list_topic(&wide[..100], true), {
            Topic::List(ListScope::Projects(wide[..100].to_vec()))
        });
    }
}
