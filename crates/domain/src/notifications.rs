//! Notification rules (NTF, plan step 1.10): the kinds of events, personal settings and
//! subscriptions on sources. Watching is the built-in subscription of NTF-017: its kinds live in
//! [`Settings`] and are all on by default. Recipients, batching and the inbox are the server's
//! business (1.10a-2, 1.10a-3); this module only holds the rules both sides share.

use serde::{Deserialize, Serialize};

use crate::settings::Language;
use crate::{CommentId, FieldId, SavedSearchId, TagId};

/// One user-visible change a notification can be about (NTF-022, plan §3.2). `System` is what
/// the server itself tells a person (job reports, ADM-031); nobody can subscribe to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Created,
    Updated,
    Resolved,
    Comment,
    Vote,
    TagAdded,
    TagRemoved,
    System,
}

impl Kind {
    pub const ALL: [Kind; 8] = [
        Kind::Created,
        Kind::Updated,
        Kind::Resolved,
        Kind::Comment,
        Kind::Vote,
        Kind::TagAdded,
        Kind::TagRemoved,
        Kind::System,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Created => "created",
            Kind::Updated => "updated",
            Kind::Resolved => "resolved",
            Kind::Comment => "comment",
            Kind::Vote => "vote",
            Kind::TagAdded => "tag_added",
            Kind::TagRemoved => "tag_removed",
            Kind::System => "system",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Whether a subscription may ask for it.
    pub fn subscribable(self) -> bool {
        self != Kind::System
    }
}

/// What a subscription listens to (NTF-021): a tag or a saved search, both by ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Tag(TagId),
    Search(SavedSearchId),
}

/// Kinds a tag source accepts: all seven subscribable ones, the two tag kinds included.
const TAG_KINDS: [Kind; 7] = [
    Kind::Created,
    Kind::Updated,
    Kind::Resolved,
    Kind::Comment,
    Kind::Vote,
    Kind::TagAdded,
    Kind::TagRemoved,
];

/// Kinds a saved-search source accepts: everything but the two tag kinds (NTF-022) — a search
/// match says nothing about tags.
const SEARCH_KINDS: [Kind; 5] = [
    Kind::Created,
    Kind::Updated,
    Kind::Resolved,
    Kind::Comment,
    Kind::Vote,
];

impl Source {
    /// The kinds this source accepts (NTF-022).
    pub fn kinds(self) -> &'static [Kind] {
        match self {
            Source::Tag(_) => &TAG_KINDS,
            Source::Search(_) => &SEARCH_KINDS,
        }
    }

    /// Which of `kinds` the source does not take; empty when the set fits.
    pub fn rejects(self, kinds: &[Kind]) -> Vec<Kind> {
        kinds
            .iter()
            .copied()
            .filter(|kind| !self.kinds().contains(kind))
            .collect()
    }
}

/// A person's notification settings (NTF-017, NTF-030, NTF-034, NTF-066): the email channel and
/// its form, own changes, mentions, the kinds of the built-in watch subscription, and the
/// auto-watch switches (ISS-030). A person with no stored row sees [`Settings::default`]; the
/// inbox channel is always on (NTF-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub email: bool,
    pub email_html: bool,
    pub own_changes: bool,
    pub mentions: bool,
    pub watch_created: bool,
    pub watch_updated: bool,
    pub watch_resolved: bool,
    pub watch_comment: bool,
    pub watch_vote: bool,
    pub watch_tag: bool,
    pub autowatch_created: bool,
    pub autowatch_commented: bool,
    pub autowatch_assigned: bool,
    pub autowatch_voted: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            email: true,
            email_html: true,
            own_changes: false,
            mentions: true,
            watch_created: true,
            watch_updated: true,
            watch_resolved: true,
            watch_comment: true,
            watch_vote: true,
            watch_tag: true,
            autowatch_created: true,
            autowatch_commented: true,
            autowatch_assigned: true,
            autowatch_voted: true,
        }
    }
}

impl Settings {
    /// Whether the built-in watch subscription delivers this kind (NTF-017). `System` is never
    /// about watching.
    pub fn watch_allows(&self, kind: Kind) -> bool {
        match kind {
            Kind::Created => self.watch_created,
            Kind::Updated => self.watch_updated,
            Kind::Resolved => self.watch_resolved,
            Kind::Comment => self.watch_comment,
            Kind::Vote => self.watch_vote,
            Kind::TagAdded | Kind::TagRemoved => self.watch_tag,
            Kind::System => false,
        }
    }
}

/// The folders of the inbox (NTF-048, plan step 1.10 §3.5): everything, direct mentions,
/// tag and saved-search subscriptions, and what the server itself told the person (only people
/// who see the administration area may open that folder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    All,
    Direct,
    Subscriptions,
    System,
}

impl Folder {
    pub fn key(self) -> &'static str {
        match self {
            Folder::All => "all",
            Folder::Direct => "direct",
            Folder::Subscriptions => "subscriptions",
            Folder::System => "system",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "all" => Some(Folder::All),
            "direct" => Some(Folder::Direct),
            "subscriptions" => Some(Folder::Subscriptions),
            "system" => Some(Folder::System),
            _ => None,
        }
    }
}

/// Why one notification reached its reader (NTF-042): every reason a person is a candidate,
/// kept together on one row. Stored as `watch`, `mention`, `tag:<id>`, `search:<id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    Watch,
    Mention,
    Tag(TagId),
    Search(SavedSearchId),
}

impl Reason {
    pub fn key(self) -> String {
        match self {
            Reason::Watch => "watch".to_owned(),
            Reason::Mention => "mention".to_owned(),
            Reason::Tag(id) => format!("tag:{id}"),
            Reason::Search(id) => format!("search:{id}"),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        if key == "watch" {
            return Some(Reason::Watch);
        }
        if key == "mention" {
            return Some(Reason::Mention);
        }
        let (kind, id) = key.split_once(':')?;
        let id = id.parse().ok()?;
        match kind {
            "tag" => Some(Reason::Tag(id)),
            "search" => Some(Reason::Search(id)),
            _ => None,
        }
    }
}

/// One fact of a pending batch's payload (plan §3.4), written by the change writer in its
/// transaction and rendered per recipient at fire time. Names are captured when the change
/// happens; field ids stay for the privacy filter of the render (FLD-062).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Payload {
    /// A field edit: display values, already shortened for a line.
    Field {
        id: FieldId,
        name: String,
        old: String,
        new: String,
    },
    /// A summary edit; the render names it in the instance language.
    Summary {
        old: String,
        new: String,
    },
    /// A description edit; the render names it in the instance language.
    Description {
        old: String,
        new: String,
    },
    /// A new comment; the excerpt is filled at fire time.
    Comment {
        id: CommentId,
    },
    /// Files of the change; `of` names the comment they belong to, whose audience may hide them
    /// (then the render shows a count).
    Files {
        of: Option<CommentId>,
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Tag names.
    Tags {
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Link ends as language-neutral labels («duplicate WEB-4»).
    Links {
        added: Vec<String>,
        removed: Vec<String>,
    },
    Vote {
        on: bool,
    },
    /// A board placement.
    Boards {
        board: String,
        sprint: Option<String>,
    },
    /// A move between projects, by keys.
    Moved {
        from: String,
        to: String,
    },
    /// An audience change.
    Audience,
}

/// One rendered line of a notification (plan §3.5): the one shape the inbox popup (1.10b) and
/// the emails (1.10c) show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Line {
    Text {
        text: String,
    },
    Field {
        name: String,
        old: String,
        new: String,
        /// A prose edit (summary, description): the email shows it as a word diff (NTF-060).
        /// The inbox shows the same line either way.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        text: bool,
    },
    Comment {
        id: CommentId,
        excerpt: String,
    },
}

/// What a fired notification holds (plan §3.5): the issue it is about and the lines this
/// recipient may see, rendered once at fire time in the instance language (NTF-067 is P2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Content {
    pub issue: IssueRef,
    pub lines: Vec<Line>,
    pub excerpt: String,
}

/// The issue a notification names: its readable ID and summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueRef {
    pub readable_id: String,
    pub summary: String,
}

/// What a `system` row says (plan §3.7): a job report to its initiator, the only `system` rows
/// of P1. `state` is a stable key (`finished`, `failed`), not a translated word.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemReport {
    pub job: String,
    pub state: String,
    pub message: Option<String>,
}

/// The words a rendered line needs, in the instance language. Proper nouns (tag names, board
/// names, readable IDs) come with the payload; these are the connectors around them.
pub struct Words {
    pub summary: &'static str,
    pub description: &'static str,
    pub vote_add: &'static str,
    pub vote_remove: &'static str,
    pub tags: &'static str,
    pub links: &'static str,
    pub files: &'static str,
    pub boards: &'static str,
    pub sprint: &'static str,
    pub moved: &'static str,
    pub audience: &'static str,
    pub stopped: &'static str,
    // The email words (plan step 1.10 §5.2–§5.3): subjects (NTF-062) and the footer (NTF-063).
    pub kind_created: &'static str,
    pub kind_updated: &'static str,
    pub kind_resolved: &'static str,
    pub kind_comment: &'static str,
    pub kind_vote: &'static str,
    pub kind_tag: &'static str,
    /// Who filed the issue, for a new-item email.
    pub by_new: &'static str,
    /// Who made the change, for a change-summary email.
    pub by_change: &'static str,
    /// The heading over a new item's first values.
    pub initial: &'static str,
    /// The heading over a change's lines.
    pub changes: &'static str,
    /// The button to the issue.
    pub open: &'static str,
    /// The footer's question, answered by the reasons (NTF-063).
    pub footer_why: &'static str,
    pub reason_watch: &'static str,
    pub reason_mention: &'static str,
    /// `{name}` is where the source's name goes.
    pub reason_tag: &'static str,
    /// `{name}` is where the source's name goes.
    pub reason_search: &'static str,
    /// The footer's link to the notifications settings (NTF-063).
    pub settings_link: &'static str,
}

impl Words {
    /// The word a subject names the kind with (NTF-062): readable previews, stable order.
    pub fn kind_word(&self, kind: Kind) -> &'static str {
        match kind {
            Kind::Created => self.kind_created,
            Kind::Updated => self.kind_updated,
            Kind::Resolved => self.kind_resolved,
            Kind::Comment => self.kind_comment,
            Kind::Vote => self.kind_vote,
            Kind::TagAdded | Kind::TagRemoved => self.kind_tag,
            // `system` rows never email (plan §3.7); answer something stable anyway.
            Kind::System => self.kind_updated,
        }
    }
}

/// The words of the instance language (per-recipient language is NTF-067, P2).
pub fn words(language: Language) -> Words {
    match language {
        Language::En => Words {
            summary: "summary",
            description: "description",
            vote_add: "vote added",
            vote_remove: "vote withdrawn",
            tags: "tags",
            links: "links",
            files: "files",
            boards: "board",
            sprint: "sprint",
            moved: "moved",
            audience: "audience changed",
            stopped: "watching stopped",
            kind_created: "created",
            kind_updated: "changed",
            kind_resolved: "resolved",
            kind_comment: "comment",
            kind_vote: "vote",
            kind_tag: "tag",
            by_new: "Reported by",
            by_change: "Changed by",
            initial: "Initial values",
            changes: "Changes",
            open: "Open the issue",
            footer_why: "You received this email because:",
            reason_watch: "you watch this issue",
            reason_mention: "you were mentioned",
            reason_tag: "you subscribe to the tag {name}",
            reason_search: "you subscribe to the saved search {name}",
            settings_link: "Notification settings",
        },
        Language::Ru => Words {
            summary: "тема",
            description: "описание",
            vote_add: "голос отдан",
            vote_remove: "голос снят",
            tags: "теги",
            links: "связи",
            files: "файлы",
            boards: "доска",
            sprint: "спринт",
            moved: "перенос",
            audience: "аудитория изменена",
            stopped: "наблюдение прекращено",
            kind_created: "создана",
            kind_updated: "изменена",
            kind_resolved: "решена",
            kind_comment: "комментарий",
            kind_vote: "голос",
            kind_tag: "тег",
            by_new: "Автор",
            by_change: "Автор изменений",
            initial: "Начальные значения",
            changes: "Изменения",
            open: "Открыть задачу",
            footer_why: "Вы получили это письмо, потому что:",
            reason_watch: "вы наблюдаете эту задачу",
            reason_mention: "вас упомянули",
            reason_tag: "у вас подписка на тег {name}",
            reason_search: "у вас подписка на сохранённый поиск {name}",
            settings_link: "Настройки уведомлений",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.key()), Some(kind));
        }
        assert_eq!(Kind::parse("spent_time"), None);
    }

    #[test]
    fn system_is_not_subscribable() {
        assert!(!Kind::System.subscribable());
        assert!(Kind::Created.subscribable());
    }

    #[test]
    fn tag_sources_take_all_seven_kinds() {
        let source = Source::Tag(7);
        assert_eq!(source.kinds().len(), 7);
        assert!(
            source
                .rejects(&[Kind::Created, Kind::TagAdded, Kind::TagRemoved])
                .is_empty()
        );
        assert_eq!(source.rejects(&[Kind::System]), vec![Kind::System]);
    }

    #[test]
    fn search_sources_reject_the_tag_kinds() {
        let source = Source::Search(9);
        assert!(source.rejects(&SEARCH_KINDS).is_empty());
        assert_eq!(
            source.rejects(&[Kind::TagAdded, Kind::Updated, Kind::TagRemoved]),
            vec![Kind::TagAdded, Kind::TagRemoved]
        );
    }

    #[test]
    fn watch_kinds_cover_every_subscribable_kind() {
        let settings = Settings::default();
        for kind in Kind::ALL {
            assert_eq!(settings.watch_allows(kind), kind != Kind::System);
        }
        let quiet = Settings {
            watch_updated: false,
            watch_tag: false,
            ..Settings::default()
        };
        assert!(!quiet.watch_allows(Kind::Updated));
        assert!(!quiet.watch_allows(Kind::TagAdded));
        assert!(quiet.watch_allows(Kind::Comment));
    }

    #[test]
    fn defaults_match_the_owner() {
        // Own changes off, everything else on (NTF-030 against NTF-017, NTF-034, NTF-066).
        let defaults = Settings::default();
        assert!(!defaults.own_changes);
        assert!(defaults.email && defaults.email_html && defaults.mentions);
    }
}
