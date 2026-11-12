//! The pump (architecture.md §5): one task that owns every connection's topics, reads the event
//! log in ID order and hands each event to the connections that subscribed to it and may see it.
//! Commands from connections are handled between events, so a replay never interleaves with live
//! events.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::Utf8Bytes;
use futures_util::StreamExt;
use time::OffsetDateTime;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use trackflow_api::live::{
    LiveChange, LiveEntity, LiveEvent, LiveResync, LiveServerMessage, LiveSubscribed, close,
};
use trackflow_domain::access::Access;
use trackflow_domain::live::{Change, EntityKind, ListScope, Topic};
use trackflow_domain::settings::session_policy;
use trackflow_domain::{BoardId, IssueId, ProjectId, UserId};
use trackflow_store::PgPool;
use trackflow_store::bus::{self, BusEvent};
use trackflow_store::live::Stored;

use super::{ACCESS_RELOAD, BUFFER_AGE, Config, Ending, PURGE_EVERY, READ_BATCH, RETENTION, TICK};
use crate::access::AccessCache;
use crate::settings::SettingsCache;

/// What a connection signed in with: the sweep closes it when that ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignedIn {
    Session(i64),
    Token(i64),
}

/// What connections tell the pump.
pub enum Command {
    Open {
        id: u64,
        user: UserId,
        signed_in: SignedIn,
        access: Arc<Access>,
        queue: mpsc::Sender<Utf8Bytes>,
        ending: Arc<Ending>,
    },
    /// Topics already checked against access, with the projects of their boards; `since` asks
    /// for a replay.
    Subscribe {
        id: u64,
        topics: Vec<Topic>,
        boards: Vec<(BoardId, Vec<ProjectId>)>,
        since: Option<i64>,
    },
    Unsubscribe {
        id: u64,
        topics: Vec<Topic>,
    },
    Close {
        id: u64,
    },
    /// The sweep found these sessions and tokens ended.
    Ended {
        sessions: Vec<i64>,
        tokens: Vec<i64>,
    },
}

struct Connection {
    user: UserId,
    signed_in: SignedIn,
    access: Arc<Access>,
    queue: mpsc::Sender<Utf8Bytes>,
    ending: Arc<Ending>,
    topics: HashSet<Topic>,
    /// The projects of each `board:` topic held (as checked when subscribing).
    boards: HashMap<BoardId, Vec<ProjectId>>,
}

/// Which connections hold which topics.
#[derive(Default)]
struct Index {
    by_issue: HashMap<IssueId, HashSet<u64>>,
    /// Holders of `list:<ids>` topics, by each listed project.
    by_project: HashMap<ProjectId, HashSet<u64>>,
    list_all: HashSet<u64>,
    instance: HashSet<u64>,
    by_board: HashMap<BoardId, HashSet<u64>>,
    /// Holders of `board:` topics, by each project of those boards.
    by_board_project: HashMap<ProjectId, HashSet<u64>>,
    /// Holders of `inbox:` topics, by the reader (only they may hold theirs).
    by_inbox: HashMap<UserId, HashSet<u64>>,
}

impl Index {
    fn add(&mut self, id: u64, topic: &Topic) {
        match topic {
            Topic::Issue(issue) => {
                self.by_issue.entry(*issue).or_default().insert(id);
            }
            Topic::List(ListScope::All) => {
                self.list_all.insert(id);
            }
            Topic::List(ListScope::Projects(projects)) => {
                for project in projects {
                    self.by_project.entry(*project).or_default().insert(id);
                }
            }
            Topic::Instance => {
                self.instance.insert(id);
            }
            Topic::Inbox(user) => {
                self.by_inbox.entry(*user).or_default().insert(id);
            }
            // Boards come with their projects: `add_board`.
            Topic::Board(_) => {}
        }
    }

    fn add_board(&mut self, id: u64, board: BoardId, projects: &[ProjectId]) {
        self.by_board.entry(board).or_default().insert(id);
        for project in projects {
            self.by_board_project
                .entry(*project)
                .or_default()
                .insert(id);
        }
    }

    /// Takes a board away; `kept` are the boards the connection keeps.
    fn remove_board(
        &mut self,
        id: u64,
        board: BoardId,
        projects: &[ProjectId],
        kept: &HashMap<BoardId, Vec<ProjectId>>,
    ) {
        drop_from(&mut self.by_board, board, id);
        for project in projects {
            if !kept.values().any(|listed| listed.contains(project)) {
                drop_from(&mut self.by_board_project, *project, id);
            }
        }
    }

    /// Takes a topic away; `others` are the topics the connection keeps (two lists may share a
    /// project).
    fn remove(&mut self, id: u64, topic: &Topic, others: &HashSet<Topic>) {
        match topic {
            Topic::Issue(issue) => drop_from(&mut self.by_issue, *issue, id),
            Topic::List(ListScope::All) => {
                self.list_all.remove(&id);
            }
            Topic::List(ListScope::Projects(projects)) => {
                for project in projects {
                    let kept = others.iter().any(|other| match other {
                        Topic::List(ListScope::Projects(listed)) => listed.contains(project),
                        _ => false,
                    });
                    if !kept {
                        drop_from(&mut self.by_project, *project, id);
                    }
                }
            }
            Topic::Instance => {
                self.instance.remove(&id);
            }
            Topic::Inbox(user) => drop_from(&mut self.by_inbox, *user, id),
            Topic::Board(_) => {}
        }
    }
}

fn drop_from<K: std::hash::Hash + Eq>(index: &mut HashMap<K, HashSet<u64>>, key: K, id: u64) {
    if let Some(ids) = index.get_mut(&key) {
        ids.remove(&id);
        if ids.is_empty() {
            index.remove(&key);
        }
    }
}

pub struct Pump {
    db: PgPool,
    access: AccessCache,
    settings: SettingsCache,
    config: Config,
    connections: HashMap<u64, Connection>,
    index: Index,
    /// Running sweep and purge, never two of a kind at once.
    sweeping: Option<JoinHandle<()>>,
    purging: Option<JoinHandle<()>>,
    /// When the last purge started; `None` before the first.
    purged_at: Option<Instant>,
    /// The last event dispatched.
    position: i64,
    /// Every event after this ID, up to `position`, is in the buffer.
    floor: i64,
    buffer: VecDeque<Stored>,
    /// Events up to this ID may be dispatched: their notification has arrived, and with it every
    /// access change committed before them.
    target: i64,
    /// The log's head at the previous tick: older than a tick, so its notifications have arrived
    /// unless the bus reconnects (which reloads everything anyway).
    last_head: i64,
}

impl Pump {
    /// Starts at the log's head with the last minutes in the replay buffer.
    pub async fn load(
        db: PgPool,
        access: AccessCache,
        settings: SettingsCache,
        config: Config,
    ) -> Result<Self, sqlx::Error> {
        let position = trackflow_store::live::head(&db).await?;
        let recent =
            trackflow_store::live::since_time(&db, OffsetDateTime::now_utc() - BUFFER_AGE).await?;
        let mut floor = position;
        let mut buffer = VecDeque::new();
        if let Some(first) = recent.first() {
            // Rows carry their transaction's start time, so a later ID may be older: read by ID.
            floor = first.id - 1;
            let mut after = floor;
            'read: loop {
                let rows = trackflow_store::live::after(&db, after, READ_BATCH).await?;
                if rows.is_empty() {
                    break;
                }
                for row in rows {
                    if row.id > position {
                        break 'read;
                    }
                    after = row.id;
                    buffer.push_back(row);
                }
            }
        }
        let mut pump = Self {
            db,
            access,
            settings,
            config,
            connections: HashMap::new(),
            index: Index::default(),
            sweeping: None,
            purging: None,
            purged_at: None,
            position,
            floor,
            buffer,
            target: position,
            last_head: position,
        };
        pump.evict();
        Ok(pump)
    }

    pub async fn run(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        to_self: mpsc::WeakSender<Command>,
        mut bus: broadcast::Receiver<BusEvent>,
        stop: CancellationToken,
    ) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sweep = tokio::time::interval_at(
            tokio::time::Instant::now() + self.config.sweep,
            self.config.sweep,
        );
        sweep.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                event = bus.recv() => {
                    if !self.on_bus(event).await {
                        break;
                    }
                }
                command = commands.recv() => match command {
                    Some(command) => self.on_command(command),
                    None => break,
                },
                _ = tick.tick() => self.on_tick().await,
                _ = sweep.tick() => self.sweep(&to_self),
            }
        }
        for task in [self.sweeping.take(), self.purging.take()]
            .into_iter()
            .flatten()
        {
            task.abort();
        }
    }

    /// Checks the sessions and tokens of every connection on a task of its own; the ended ones
    /// come back as [`Command::Ended`].
    fn sweep(&mut self, to_self: &mpsc::WeakSender<Command>) {
        if self
            .sweeping
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return;
        }
        let mut sessions = HashSet::new();
        let mut tokens = HashSet::new();
        for connection in self.connections.values() {
            match connection.signed_in {
                SignedIn::Session(id) => sessions.insert(id),
                SignedIn::Token(id) => tokens.insert(id),
            };
        }
        if sessions.is_empty() && tokens.is_empty() {
            return;
        }
        let (Some(to_self), db) = (to_self.upgrade(), self.db.clone()) else {
            return;
        };
        let policy = session_policy(&self.settings.get());
        self.sweeping = Some(tokio::spawn(async move {
            let sessions: Vec<i64> = sessions.into_iter().collect();
            let tokens: Vec<i64> = tokens.into_iter().collect();
            let now = OffsetDateTime::now_utc();
            match trackflow_service::auth::alive(&db, &sessions, &tokens, policy, now).await {
                Ok((live_sessions, live_tokens)) => {
                    let ended = Command::Ended {
                        sessions: (sessions.into_iter())
                            .filter(|id| !live_sessions.contains(id))
                            .collect(),
                        tokens: (tokens.into_iter())
                            .filter(|id| !live_tokens.contains(id))
                            .collect(),
                    };
                    let _ = to_self.send(ended).await;
                }
                Err(error) => tracing::warn!(?error, "cannot check live connections' sessions"),
            }
        }));
    }

    /// Starts removing old events from the log, and empty drafts, every [`PURGE_EVERY`].
    fn purge(&mut self) {
        let due = self.purged_at.is_none_or(|at| at.elapsed() >= PURGE_EVERY);
        if !due
            || self
                .purging
                .as_ref()
                .is_some_and(|task| !task.is_finished())
        {
            return;
        }
        self.purged_at = Some(Instant::now());
        let db = self.db.clone();
        self.purging = Some(tokio::spawn(async move {
            if let Err(error) = trackflow_store::live::purge(&db, RETENTION).await {
                tracing::warn!(%error, "cannot purge old live events");
            }
            // Inbox rows older than the retention go on the same schedule (plan §3.5).
            if let Err(error) =
                trackflow_store::notifications::purge(&db, super::INBOX_RETENTION).await
            {
                tracing::warn!(%error, "cannot purge old inbox rows");
            }
            // Empty drafts a closed form could not delete (ISS-010) go on the same schedule.
            let idle = trackflow_service::issues::EMPTY_DRAFT_IDLE;
            if let Err(error) = trackflow_service::issues::sweep_drafts(&db, idle).await {
                tracing::warn!(%error, "cannot sweep empty drafts");
            }
        }));
    }

    /// Answers whether to go on.
    async fn on_bus(&mut self, event: Result<BusEvent, broadcast::error::RecvError>) -> bool {
        match event {
            Ok(BusEvent::Notified { channel, payload }) => match channel.as_str() {
                bus::LIVE => {
                    if let Ok(id) = payload.parse::<i64>() {
                        self.target = self.target.max(id);
                    }
                    self.catch_up().await;
                }
                bus::ACCESS => {
                    self.reload_access().await;
                    self.notice("access");
                }
                bus::SETTINGS => self.notice("settings"),
                _ => {}
            },
            Ok(BusEvent::Resync) | Err(broadcast::error::RecvError::Lagged(_)) => {
                // Anything may have been missed: access first, then every event there is.
                self.reload_access().await;
                self.notice("settings");
                self.notice("access");
                match trackflow_store::live::head(&self.db).await {
                    Ok(head) => self.target = self.target.max(head),
                    Err(error) => tracing::warn!(%error, "cannot read the live event head"),
                }
                self.catch_up().await;
            }
            Err(broadcast::error::RecvError::Closed) => return false,
        }
        true
    }

    async fn on_tick(&mut self) {
        self.evict();
        self.purge();
        match trackflow_store::live::head(&self.db).await {
            Ok(head) => {
                self.target = self.target.max(self.last_head);
                self.last_head = head;
            }
            Err(error) => tracing::warn!(%error, "cannot read the live event head"),
        }
        self.catch_up().await;
    }

    /// Dispatches the events up to `target`.
    async fn catch_up(&mut self) {
        while self.position < self.target {
            let rows = match trackflow_store::live::after(&self.db, self.position, READ_BATCH).await
            {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(%error, "cannot read live events");
                    return;
                }
            };
            if rows.is_empty() {
                // The target was rolled back or purged.
                self.position = self.target;
                return;
            }
            for row in rows {
                if row.id > self.target {
                    return;
                }
                self.dispatch(&row);
                self.position = row.id;
                self.buffer.push_back(row);
            }
            self.evict();
        }
    }

    fn dispatch(&mut self, row: &Stored) {
        let mut receivers: HashMap<u64, Vec<Topic>> = HashMap::new();
        for issue in &row.event.issues {
            for id in self.index.by_issue.get(issue).into_iter().flatten() {
                receivers.entry(*id).or_default().push(Topic::Issue(*issue));
            }
        }
        // An inbox event names its one reader (plan step 1.10 §3.6); the guard agrees.
        if row.event.kind == EntityKind::Inbox {
            for id in self
                .index
                .by_inbox
                .get(&row.event.entity)
                .into_iter()
                .flatten()
            {
                receivers
                    .entry(*id)
                    .or_default()
                    .push(Topic::Inbox(row.event.entity));
            }
        }
        // Lists hear only of issues: comments, files and links do not change their rows.
        if row.event.kind == EntityKind::Issue {
            let listeners: HashSet<u64> = (row.event.projects.iter())
                .filter_map(|project| self.index.by_project.get(project))
                .flatten()
                .chain(&self.index.list_all)
                .copied()
                .collect();
            for id in listeners {
                let Some(connection) = self.connections.get(&id) else {
                    continue;
                };
                let lists = (connection.topics.iter())
                    .filter(|topic| {
                        matches!(topic, Topic::List(_)) && matches(row, topic, &connection.boards)
                    })
                    .cloned();
                receivers.entry(id).or_default().extend(lists);
            }
        }
        // Boards hear of what names them, and of issues in their projects (BRD-114).
        let mut board_listeners: HashSet<u64> = (row.event.boards.iter())
            .filter_map(|board| self.index.by_board.get(board))
            .flatten()
            .copied()
            .collect();
        if row.event.kind == EntityKind::Issue {
            board_listeners.extend(
                (row.event.projects.iter())
                    .filter_map(|project| self.index.by_board_project.get(project))
                    .flatten(),
            );
        }
        for id in board_listeners {
            let Some(connection) = self.connections.get(&id) else {
                continue;
            };
            let boards = (connection.boards.keys())
                .map(|board| Topic::Board(*board))
                .filter(|topic| matches(row, topic, &connection.boards));
            receivers.entry(id).or_default().extend(boards);
        }
        // Most receivers share a topic set (one list, one issue): encode each set once.
        let mut encoded: HashMap<Vec<Topic>, Option<Utf8Bytes>> = HashMap::new();
        let mut gone = Vec::new();
        for (id, topics) in receivers {
            let Some(connection) = self.connections.get(&id) else {
                continue;
            };
            if !row.event.guard.allows(&connection.access) {
                continue;
            }
            let text = (encoded.entry(topics))
                .or_insert_with_key(|topics| encode(&event_message(row, topics)));
            if let Some(text) = text
                && !send(connection, text.clone())
            {
                gone.push(id);
            }
        }
        for id in gone {
            self.remove(id);
        }
    }

    fn on_command(&mut self, command: Command) {
        match command {
            Command::Open {
                id,
                user,
                signed_in,
                access,
                queue,
                ending,
            } => {
                self.connections.insert(
                    id,
                    Connection {
                        user,
                        signed_in,
                        access,
                        queue,
                        ending,
                        topics: HashSet::new(),
                        boards: HashMap::new(),
                    },
                );
            }
            Command::Subscribe {
                id,
                topics,
                boards,
                since,
            } => self.subscribe(id, topics, boards, since),
            Command::Unsubscribe { id, topics } => {
                if let Some(connection) = self.connections.get_mut(&id) {
                    for topic in topics {
                        if connection.topics.remove(&topic) {
                            self.index.remove(id, &topic, &connection.topics);
                        }
                        if let Topic::Board(board) = topic
                            && let Some(projects) = connection.boards.remove(&board)
                        {
                            (self.index).remove_board(id, board, &projects, &connection.boards);
                        }
                    }
                }
            }
            Command::Close { id } => self.remove(id),
            Command::Ended { sessions, tokens } => {
                let ended: Vec<u64> = (self.connections.iter())
                    .filter(|(_, c)| match c.signed_in {
                        SignedIn::Session(session) => sessions.contains(&session),
                        SignedIn::Token(token) => tokens.contains(&token),
                    })
                    .map(|(id, _)| *id)
                    .collect();
                for id in ended {
                    if let Some(connection) = self.connections.get(&id) {
                        connection.ending.close(close::SESSION_ENDED);
                    }
                    self.remove(id);
                }
            }
        }
    }

    fn subscribe(
        &mut self,
        id: u64,
        topics: Vec<Topic>,
        boards: Vec<(BoardId, Vec<ProjectId>)>,
        since: Option<i64>,
    ) {
        let Some(connection) = self.connections.get_mut(&id) else {
            return;
        };
        for topic in &topics {
            if connection.topics.insert(topic.clone()) {
                self.index.add(id, topic);
            }
        }
        // A board subscribed again takes its projects as they are now.
        for (board, projects) in boards {
            if let Some(old) = connection.boards.remove(&board) {
                (self.index).remove_board(id, board, &old, &connection.boards);
            }
            self.index.add_board(id, board, &projects);
            connection.boards.insert(board, projects);
        }
        // Notices are not replayed: the client reloads what `instance` covers anyway.
        let replayed: Vec<&Topic> = topics.iter().filter(|t| **t != Topic::Instance).collect();
        let mut alive = true;
        if let Some(since) = since
            && !replayed.is_empty()
        {
            if since >= self.floor {
                for row in self.buffer.iter().filter(|row| row.id > since) {
                    let matching: Vec<Topic> = replayed
                        .iter()
                        .filter(|topic| matches(row, topic, &connection.boards))
                        .map(|topic| (*topic).clone())
                        .collect();
                    if matching.is_empty() || !row.event.guard.allows(&connection.access) {
                        continue;
                    }
                    alive = deliver(connection, &event_message(row, &matching));
                    if !alive {
                        break;
                    }
                }
            } else {
                let resync = LiveServerMessage::Resync(LiveResync {
                    topics: replayed.iter().map(ToString::to_string).collect(),
                });
                alive = deliver(connection, &resync);
            }
        }
        if alive {
            let subscribed = LiveServerMessage::Subscribed(LiveSubscribed {
                topics: topics.iter().map(ToString::to_string).collect(),
                head: self.position,
            });
            alive = deliver(connection, &subscribed);
        }
        if !alive {
            self.remove(id);
        }
    }

    fn remove(&mut self, id: u64) {
        if let Some(connection) = self.connections.remove(&id) {
            for topic in &connection.topics {
                self.index.remove(id, topic, &HashSet::new());
            }
            for (board, projects) in &connection.boards {
                (self.index).remove_board(id, *board, projects, &HashMap::new());
            }
        }
    }

    /// Reloads the access of everyone connected, a few at a time, before any later event.
    async fn reload_access(&mut self) {
        self.access.clear();
        let users: HashSet<UserId> = self.connections.values().map(|c| c.user).collect();
        let loaded: Vec<_> = futures_util::stream::iter(users)
            .map(|user| {
                let cache = self.access.clone();
                let db = self.db.clone();
                async move { (user, cache.get(&db, user).await) }
            })
            .buffer_unordered(ACCESS_RELOAD)
            .collect()
            .await;
        let mut failed = Vec::new();
        for (user, result) in loaded {
            match result {
                Ok(access) => {
                    for connection in self.connections.values_mut().filter(|c| c.user == user) {
                        connection.access = Arc::clone(&access);
                    }
                }
                Err(error) => {
                    // Stale access must not let anything through: they reconnect.
                    tracing::warn!(?error, user, "cannot reload access for live connections");
                    failed.extend(
                        self.connections
                            .iter()
                            .filter(|(_, c)| c.user == user)
                            .map(|(id, _)| *id),
                    );
                }
            }
        }
        for id in failed {
            if let Some(connection) = self.connections.get(&id) {
                connection.ending.close(close::RESTART);
            }
            self.remove(id);
        }
    }

    /// Tells every `instance` subscriber that settings or access changed.
    fn notice(&mut self, kind: &str) {
        let message = LiveServerMessage::Event(LiveEvent {
            id: self.position,
            topics: vec![Topic::Instance.to_string()],
            entity: LiveEntity {
                kind: kind.to_owned(),
                id: 0,
                version: 0,
            },
            change: LiveChange::Updated,
            patch: None,
        });
        let Some(text) = encode(&message) else {
            return;
        };
        let gone: Vec<u64> = self
            .index
            .instance
            .iter()
            .filter(|id| {
                self.connections
                    .get(id)
                    .is_some_and(|connection| !send(connection, text.clone()))
            })
            .copied()
            .collect();
        for id in gone {
            self.remove(id);
        }
    }

    /// Drops what is older than the buffer keeps.
    fn evict(&mut self) {
        let oldest = OffsetDateTime::now_utc() - BUFFER_AGE;
        while let Some(front) = self.buffer.front() {
            if self.buffer.len() <= self.config.buffer_max && front.at >= oldest {
                break;
            }
            self.floor = front.id;
            self.buffer.pop_front();
        }
    }
}

fn matches(row: &Stored, topic: &Topic, boards: &HashMap<BoardId, Vec<ProjectId>>) -> bool {
    match topic {
        Topic::Board(id) => {
            row.event.boards.contains(id)
                || (row.event.kind == EntityKind::Issue
                    && boards.get(id).is_some_and(|projects| {
                        row.event.projects.iter().any(|p| projects.contains(p))
                    }))
        }
        Topic::Issue(id) => row.event.issues.contains(id),
        Topic::List(scope) => {
            row.event.kind == EntityKind::Issue
                && match scope {
                    ListScope::All => true,
                    ListScope::Projects(ids) => row.event.projects.iter().any(|p| ids.contains(p)),
                }
        }
        Topic::Instance => false,
        Topic::Inbox(user) => row.event.kind == EntityKind::Inbox && row.event.entity == *user,
    }
}

fn event_message(row: &Stored, topics: &[Topic]) -> LiveServerMessage {
    LiveServerMessage::Event(LiveEvent {
        id: row.id,
        topics: topics.iter().map(ToString::to_string).collect(),
        entity: LiveEntity {
            kind: row.event.kind.as_str().to_owned(),
            id: row.event.entity,
            version: row.event.version,
        },
        change: match row.event.change {
            Change::Created => LiveChange::Created,
            Change::Updated => LiveChange::Updated,
            Change::Deleted => LiveChange::Deleted,
        },
        patch: row.event.patch.as_ref().and_then(|patch| {
            serde_json::to_value(patch)
                .ok()
                .and_then(|value| value.as_object().cloned())
                .map(|mut fields| {
                    // The entity kind already says which patch it is — except `inbox` events,
                    // whose two patch shapes keep their `kind` to tell each other apart.
                    if row.event.kind != EntityKind::Inbox {
                        fields.remove("kind");
                    }
                    fields
                })
        }),
    })
}

/// Queues a message; answers `false` when the connection is gone or too slow (then it closes).
fn deliver(connection: &Connection, message: &LiveServerMessage) -> bool {
    encode(message).is_none_or(|text| send(connection, text))
}

/// The message as the text a socket sends; shared by every receiver without a copy.
pub(super) fn encode(message: &LiveServerMessage) -> Option<Utf8Bytes> {
    serde_json::to_string(message).ok().map(Utf8Bytes::from)
}

/// Queues an encoded message, as [`deliver`].
fn send(connection: &Connection, text: Utf8Bytes) -> bool {
    match connection.queue.try_send(text) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            connection.ending.close(close::TOO_SLOW);
            false
        }
        Err(TrySendError::Closed(_)) => false,
    }
}
