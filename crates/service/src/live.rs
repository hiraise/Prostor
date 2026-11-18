//! Live events of use cases (ISS-048, QRY-086; D-040, architecture.md §5). A use case that
//! changes an issue, comment, attachment or link records it in a [`Batch`] and calls
//! [`Batch::publish`] as its last statement before the commit. Facts for the guards are loaded
//! there, after every change, one query per kind; changes of drafts are dropped. Boards add their
//! own kinds (BRD-114, BRD-115): the board, its sprints and the card order.

use std::collections::BTreeMap;

use sqlx::PgConnection;
use taskmaster_domain::live::{Change, EntityKind, Guard, LINK_VERSION, NewEvent, Patch};
use taskmaster_domain::{AttachmentId, BoardId, CommentId, IssueId, LinkId, ProjectId, SprintId};
use taskmaster_store::boards as store_boards;
use taskmaster_store::live::{self as store, LinkFacts};

use crate::Result;

#[derive(Debug, Clone)]
struct IssueEntry {
    change: Change,
    /// The project it was moved from.
    from: Option<ProjectId>,
    /// Boards it joined, left or moved on (their sockets hear even when the projects differ).
    boards: Vec<BoardId>,
}

/// A card that moved inside a sprint's order.
#[derive(Debug, Clone, Copy)]
struct CardOrder {
    issue: IssueId,
    board: BoardId,
    sprint: SprintId,
    after: Option<IssueId>,
}

/// The changes of one transaction; one event per entity.
#[derive(Debug, Default)]
pub struct Batch {
    issues: BTreeMap<IssueId, IssueEntry>,
    comments: BTreeMap<CommentId, Change>,
    attachments: BTreeMap<AttachmentId, Change>,
    links: BTreeMap<LinkId, Change>,
    removed_links: Vec<LinkFacts>,
    boards: BTreeMap<BoardId, Change>,
    /// Events of deleted boards, built before the row went.
    removed_boards: Vec<NewEvent>,
    sprints: BTreeMap<SprintId, (BoardId, Change)>,
    card_orders: Vec<CardOrder>,
    backlog_orders: Vec<BacklogOrder>,
}

/// An issue that moved inside the manual order of a saved search (a backlog).
#[derive(Debug, Clone, Copy)]
struct BacklogOrder {
    issue: IssueId,
    search: i64,
    after: Option<IssueId>,
}

fn record<K: Ord>(entries: &mut BTreeMap<K, Change>, id: K, change: Change) {
    entries
        .entry(id)
        .and_modify(|earlier| *earlier = earlier.then(change))
        .or_insert(change);
}

impl Batch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
            && self.comments.is_empty()
            && self.attachments.is_empty()
            && self.links.is_empty()
            && self.removed_links.is_empty()
            && self.boards.is_empty()
            && self.removed_boards.is_empty()
            && self.sprints.is_empty()
            && self.card_orders.is_empty()
            && self.backlog_orders.is_empty()
    }

    /// An issue was published (`Created`), changed (`Updated`) or moved to the trash (`Deleted`).
    pub fn issue(&mut self, id: IssueId, change: Change) {
        self.issues
            .entry(id)
            .and_modify(|earlier| earlier.change = earlier.change.then(change))
            .or_insert(IssueEntry {
                change,
                from: None,
                boards: Vec::new(),
            });
    }

    /// An issue changed on a board: joined, left or moved to another column (BRD-114).
    pub fn issue_on_board(&mut self, id: IssueId, change: Change, board: BoardId) {
        self.issue(id, change);
        if let Some(entry) = self.issues.get_mut(&id)
            && !entry.boards.contains(&board)
        {
            entry.boards.push(board);
        }
    }

    /// A board's settings changed (BRD-115).
    pub fn board(&mut self, id: BoardId, change: Change) {
        record(&mut self.boards, id, change);
    }

    /// A board is deleted: its event is built from the row before it goes.
    pub fn board_deleted(&mut self, board: &store_boards::Board) {
        self.boards.remove(&board.id);
        self.removed_boards.push(NewEvent {
            kind: EntityKind::Board,
            entity: board.id,
            version: board.version,
            change: Change::Deleted,
            issues: Vec::new(),
            projects: Vec::new(),
            boards: vec![board.id],
            patch: None,
            guard: Guard::board(&board.access()),
        });
    }

    /// A sprint was created, changed or deleted (its board stays).
    pub fn sprint(&mut self, board: BoardId, id: SprintId, change: Change) {
        self.sprints
            .entry(id)
            .and_modify(|earlier| earlier.1 = earlier.1.then(change))
            .or_insert((board, change));
    }

    /// A card now follows `after` (`None`: first) in the sprint's order, nothing else changed.
    pub fn card_order(
        &mut self,
        issue: IssueId,
        board: BoardId,
        sprint: SprintId,
        after: Option<IssueId>,
    ) {
        self.card_orders
            .retain(|c| c.issue != issue || c.sprint != sprint);
        self.card_orders.push(CardOrder {
            issue,
            board,
            sprint,
            after,
        });
    }

    /// An issue now follows `after` (`None`: first) in the manual order of a saved search: the
    /// boards whose backlog it is hear of it (BRD-116).
    pub fn backlog_order(&mut self, issue: IssueId, search: i64, after: Option<IssueId>) {
        self.backlog_orders
            .retain(|c| c.issue != issue || c.search != search);
        self.backlog_orders.push(BacklogOrder {
            issue,
            search,
            after,
        });
    }

    /// An issue moved to another project: lists of both projects hear of it.
    pub fn issue_moved(&mut self, id: IssueId, from: ProjectId) {
        self.issue(id, Change::Updated);
        if let Some(entry) = self.issues.get_mut(&id) {
            entry.from.get_or_insert(from);
        }
    }

    /// A comment was added, changed, deleted or restored (the last two are `Updated`).
    pub fn comment(&mut self, id: CommentId, change: Change) {
        record(&mut self.comments, id, change);
    }

    /// A file was added, changed, deleted or restored (the last two are `Updated`).
    pub fn attachment(&mut self, id: AttachmentId, change: Change) {
        record(&mut self.attachments, id, change);
    }

    /// A link was made.
    pub fn link(&mut self, id: LinkId, change: Change) {
        record(&mut self.links, id, change);
    }

    /// A link is removed: its facts are loaded before the row goes.
    pub fn link_removed(&mut self, facts: LinkFacts) {
        self.links.remove(&facts.id);
        self.removed_links.push(facts);
    }

    /// Writes the events; the last statement before the commit (`store::live::publish`).
    pub async fn publish(self, conn: &mut PgConnection) -> Result<()> {
        let mut events = Vec::new();
        self.issue_events(conn, &mut events).await?;
        self.comment_events(conn, &mut events).await?;
        self.attachment_events(conn, &mut events).await?;
        let made = self.link_ids();
        if !made.is_empty() {
            let facts = store::link_facts(&mut *conn, &made).await?;
            events.extend(sorted(facts).iter().filter_map(|facts| {
                let change = *self.links.get(&facts.id)?;
                link_event(facts, change)
            }));
        }
        events.extend(
            self.removed_links
                .iter()
                .filter_map(|facts| link_event(facts, Change::Deleted)),
        );
        self.board_events(conn, &mut events).await?;
        events.extend(self.removed_boards);
        store::publish(conn, &events).await?;
        Ok(())
    }

    async fn issue_events(
        &self,
        conn: &mut PgConnection,
        events: &mut Vec<NewEvent>,
    ) -> Result<()> {
        if self.issues.is_empty() {
            return Ok(());
        }
        let ids: Vec<IssueId> = self.issues.keys().copied().collect();
        let mut facts = store::issue_facts(&mut *conn, &ids).await?;
        facts.sort_by_key(|f| f.id);
        for issue in facts.iter().filter(|f| !f.draft) {
            let Some(entry) = self.issues.get(&issue.id) else {
                continue;
            };
            let mut projects = vec![issue.project];
            projects.extend(entry.from.filter(|from| *from != issue.project));
            projects.sort_unstable();
            events.push(NewEvent {
                kind: EntityKind::Issue,
                entity: issue.id,
                version: issue.version,
                change: entry.change,
                issues: vec![issue.id],
                projects,
                boards: entry.boards.clone(),
                patch: None,
                guard: Guard::issue(&issue.rule()),
            });
        }
        Ok(())
    }

    async fn comment_events(
        &self,
        conn: &mut PgConnection,
        events: &mut Vec<NewEvent>,
    ) -> Result<()> {
        if self.comments.is_empty() {
            return Ok(());
        }
        let ids: Vec<CommentId> = self.comments.keys().copied().collect();
        let mut facts = store::comment_facts(&mut *conn, &ids).await?;
        facts.sort_by_key(|f| f.id);
        for comment in facts.iter().filter(|f| !f.issue.draft) {
            let Some(change) = self.comments.get(&comment.id) else {
                continue;
            };
            events.push(NewEvent {
                kind: EntityKind::Comment,
                entity: comment.id,
                version: comment.version,
                change: *change,
                issues: vec![comment.issue.id],
                projects: vec![comment.issue.project],
                boards: Vec::new(),
                patch: None,
                guard: Guard::comment(&comment.issue.rule(), &comment.rule()),
            });
        }
        Ok(())
    }

    async fn attachment_events(
        &self,
        conn: &mut PgConnection,
        events: &mut Vec<NewEvent>,
    ) -> Result<()> {
        if self.attachments.is_empty() {
            return Ok(());
        }
        let ids: Vec<AttachmentId> = self.attachments.keys().copied().collect();
        let mut facts = store::attachment_facts(&mut *conn, &ids).await?;
        facts.sort_by_key(|f| f.id);
        for file in facts.iter().filter(|f| !f.issue.draft) {
            let Some(change) = self.attachments.get(&file.id) else {
                continue;
            };
            let comment = file.comment_rule();
            events.push(NewEvent {
                kind: EntityKind::Attachment,
                entity: file.id,
                version: file.version,
                change: *change,
                issues: vec![file.issue.id],
                projects: vec![file.issue.project],
                boards: Vec::new(),
                patch: None,
                guard: Guard::attachment(&file.issue.rule(), comment.as_ref(), &file.rule()),
            });
        }
        Ok(())
    }

    /// Board, sprint and card-order events: the boards in one query, their sprints and the
    /// moved issues in one each.
    async fn board_events(
        &self,
        conn: &mut PgConnection,
        events: &mut Vec<NewEvent>,
    ) -> Result<()> {
        let mut ids: Vec<BoardId> = self.boards.keys().copied().collect();
        ids.extend(self.sprints.values().map(|(board, _)| *board));
        ids.extend(self.card_orders.iter().map(|c| c.board));
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() && self.backlog_orders.is_empty() {
            return Ok(());
        }
        let boards = store_boards::get_many(&mut *conn, &ids).await?;
        let board = |id: BoardId| boards.iter().find(|b| b.id == id);
        for (id, change) in &self.boards {
            let Some(b) = board(*id) else { continue };
            events.push(NewEvent {
                kind: EntityKind::Board,
                entity: b.id,
                version: b.version,
                change: *change,
                issues: Vec::new(),
                projects: Vec::new(),
                boards: vec![b.id],
                patch: None,
                guard: Guard::board(&b.access()),
            });
        }
        let mut sprint_boards: Vec<BoardId> = self.sprints.values().map(|(b, _)| *b).collect();
        sprint_boards.extend(self.card_orders.iter().map(|c| c.board));
        sprint_boards.sort_unstable();
        sprint_boards.dedup();
        let sprints = store_boards::sprints::of_boards(&mut *conn, &sprint_boards).await?;
        let sprint = |id: SprintId| sprints.iter().find(|s| s.id == id);
        for (id, (board_id, change)) in &self.sprints {
            let Some(b) = board(*board_id) else { continue };
            let version = sprint(*id).map_or(0, |s| s.version);
            events.push(NewEvent {
                kind: EntityKind::Sprint,
                entity: *id,
                version,
                change: *change,
                issues: Vec::new(),
                projects: Vec::new(),
                boards: vec![b.id],
                patch: None,
                guard: Guard::board(&b.access()),
            });
        }
        if self.card_orders.is_empty() && self.backlog_orders.is_empty() {
            return Ok(());
        }
        let mut issue_ids: Vec<IssueId> = self.card_orders.iter().map(|c| c.issue).collect();
        issue_ids.extend(self.backlog_orders.iter().map(|c| c.issue));
        let facts = store::issue_facts(&mut *conn, &issue_ids).await?;
        for order in &self.card_orders {
            let Some(issue) = facts.iter().find(|f| f.id == order.issue && !f.draft) else {
                continue;
            };
            let Some(s) = sprint(order.sprint) else {
                continue;
            };
            events.push(NewEvent {
                kind: EntityKind::CardOrder,
                entity: issue.id,
                version: s.order_version,
                change: Change::Updated,
                issues: Vec::new(),
                projects: Vec::new(),
                boards: vec![order.board],
                patch: Some(Patch::CardOrder {
                    issue: issue.id,
                    after: order.after,
                    sprint: order.sprint,
                }),
                guard: Guard::issue(&issue.rule()),
            });
        }
        for order in &self.backlog_orders {
            let Some(issue) = facts.iter().find(|f| f.id == order.issue && !f.draft) else {
                continue;
            };
            let boards =
                taskmaster_store::saved_searches::ranks::boards_of(&mut *conn, order.search)
                    .await?;
            if boards.is_empty() {
                continue;
            }
            events.push(NewEvent {
                kind: EntityKind::CardOrder,
                entity: issue.id,
                version: 0,
                change: Change::Updated,
                issues: Vec::new(),
                projects: Vec::new(),
                boards,
                patch: Some(Patch::BacklogOrder {
                    issue: issue.id,
                    after: order.after,
                    search: order.search,
                }),
                guard: Guard::issue(&issue.rule()),
            });
        }
        Ok(())
    }

    fn link_ids(&self) -> Vec<LinkId> {
        self.links.keys().copied().collect()
    }
}

fn sorted(mut facts: Vec<LinkFacts>) -> Vec<LinkFacts> {
    facts.sort_by_key(|f| f.id);
    facts
}

/// A link event; none while either end is a draft.
fn link_event(facts: &LinkFacts, change: Change) -> Option<NewEvent> {
    if facts.source.draft || facts.target.draft {
        return None;
    }
    let mut projects = vec![facts.source.project, facts.target.project];
    projects.sort_unstable();
    projects.dedup();
    Some(NewEvent {
        kind: EntityKind::IssueLink,
        entity: facts.id,
        version: LINK_VERSION,
        change,
        issues: vec![facts.source.id, facts.target.id],
        projects,
        boards: Vec::new(),
        patch: None,
        guard: Guard::link(&facts.source.rule(), &facts.target.rule()),
    })
}
