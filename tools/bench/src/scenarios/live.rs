//! Live delivery (architecture.md §7, step 1.6): the node is served on a local port and thousands
//! of WebSockets are opened to it, as the tabs of people who are not administrators, each with
//! their own API token. Every socket follows `instance`, its project's `list:` topic and three
//! `issue:` topics of that project. Field changes go over HTTP to issues that have subscribers at
//! a fixed rate, each started on time whether or not the previous one has answered (open loop,
//! the load model of D-032), in two phases:
//!
//! 1. team load: [`TEAM_CONNECTIONS`] sockets; the change and delivery budgets hold here;
//! 2. capacity: the rest of the sockets up to [`CONNECTIONS`]; every socket opens, no event is
//!    missed and delivery stays in budget; the change is reported without a budget.
//!
//! Under both, the team reads in the background, unmeasured: [`READ_RATE`] issue pages a second
//! (as the issue scenario opens them). Reads are most of a team's load, and without them the
//! gaps between changes let a developer's machine idle (power saving, the Docker VM), so every
//! change ran cold: p50 44 ms against 19 ms at ten times the rate (step 1.6, 1.6-5).
//!
//! Delivery runs from the start of the change's request to the event's arrival at each subscriber
//! of the issue's topic. Also the node's resident memory per connection.
//!
//! The clients run in a child process (this program with `--live-clients`, [`serve_clients`]), so
//! they share the machine's cores with the node but not its runtime, memory or descriptors. They
//! stamp each arrival with the wall clock, which both processes read.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use http::Method;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines};
use tokio::net::TcpStream;
use tokio::process::ChildStdout;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

use crate::catalog::Catalog;
use crate::http::{HOST, Node};
use crate::rng::Rng;
use crate::runner::Stats;
use crate::scenarios::issue;

/// Live connections a node holds (architecture.md §7).
pub const CONNECTIONS: usize = 5_000;
/// Change → event delivered, p95.
pub const DELIVERY_BUDGET: Duration = Duration::from_millis(150);
/// A field change commit with publishing, p95.
pub const CHANGE_BUDGET: Duration = Duration::from_millis(30);
/// Sockets of a team of 50: about six tabs and devices each (D-032).
pub const TEAM_CONNECTIONS: usize = 300;
/// Changes per second, about ten times a team's peak.
pub const RATE: u32 = 5;
/// Changes in each phase at most: 30 s at [`RATE`].
pub const PHASE_CHANGES: usize = 150;
/// Issue pages the team opens a second in the background: one every 5 s per person of 50.
pub const READ_RATE: u32 = 10;
/// Clients the node's pool is sized for: the changes in flight at [`RATE`] and the reader.
pub const CLIENTS: usize = 3;
/// The hidden option that runs this program as the scenario's clients.
pub const CLIENTS_FLAG: &str = "--live-clients";

const ISSUES_PER_CONNECTION: usize = 3;
/// Sockets opened at once while connecting.
const CONNECT_AT_ONCE: usize = 32;
/// Attempts to open one socket, each within the wait.
const CONNECT_ATTEMPTS: u64 = 5;
const CONNECT_WAIT: Duration = Duration::from_secs(2);
/// Issues the changes go to, in turn.
const TARGETS: usize = 100;
/// How long a change waits for all its deliveries before the missing ones count as missed.
const WAIT: Duration = Duration::from_secs(5);
/// Clients ping well within the server's idle close.
const PING_EVERY: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct LiveOptions {
    /// Sockets to open.
    pub connections: usize,
    /// Changes in each phase, at most [`PHASE_CHANGES`].
    pub changes: usize,
    pub seed: u64,
    /// The bench program, started with [`CLIENTS_FLAG`] as the clients' process.
    pub clients: PathBuf,
}

/// One phase: changes at [`RATE`] under its sockets.
#[derive(Debug, Clone)]
pub struct LivePhase {
    pub sockets: usize,
    /// Every delivery; rows: subscribers of the changed issue.
    pub delivery: Stats,
    /// The change requests.
    pub change: Stats,
    /// Deliveries that did not arrive within the wait.
    pub missed: usize,
}

#[derive(Debug, Clone)]
pub struct LiveMeasurement {
    pub requested: usize,
    pub opened: usize,
    /// Time to open and subscribe every socket.
    pub connecting: Duration,
    pub team: LivePhase,
    /// Only when more sockets than [`TEAM_CONNECTIONS`] were asked for.
    pub capacity: Option<LivePhase>,
    /// Why a socket did not open (the last such reason).
    pub failure: Option<String>,
    /// Growth of the resident memory per connection, bytes; `None` where it cannot be read.
    pub memory_per_connection: Option<f64>,
    /// Wall time of the scenario.
    pub elapsed: Duration,
}

impl LiveMeasurement {
    /// The change's budget holds at the team's load only; capacity checks sockets and delivery.
    pub fn within_budget(&self) -> bool {
        let team = &self.team;
        let capacity_ok = self
            .capacity
            .as_ref()
            .is_none_or(|phase| phase.missed == 0 && phase.delivery.p95 <= DELIVERY_BUDGET);
        self.opened >= self.requested
            && team.missed == 0
            && team.delivery.p95 <= DELIVERY_BUDGET
            && team.change.p95 <= CHANGE_BUDGET
            && capacity_ok
    }
}

/// One socket's plan: whose it is and what it follows.
struct Subscriber {
    token: String,
    topics: Vec<String>,
    project: i32,
    issues: Vec<i64>,
}

/// A followed issue: its ID, its project and how many sockets follow it.
type Followed = (i64, i32, usize);

/// A change waiting for its deliveries.
struct Pending {
    /// When the change's request started, by [`wall_micros`].
    wall: u64,
    remaining: usize,
    times: Vec<Duration>,
    done: Option<oneshot::Sender<()>>,
}

/// Changes in flight by issue; arrivals from the clients are recorded here.
type Board = Arc<Mutex<HashMap<i64, Pending>>>;

/// One change's result.
#[derive(Debug)]
struct Sample {
    change: Duration,
    deliveries: Vec<Duration>,
    subscribers: usize,
    missed: usize,
}

pub async fn measure(
    catalog: &Catalog,
    node: &Arc<Node>,
    options: &LiveOptions,
) -> anyhow::Result<LiveMeasurement> {
    let started = Instant::now();
    // The node's end of every socket is a descriptor of this process.
    raise_open_files();
    let mut rng = Rng::for_item(options.seed, "live", 0);
    let subscribers = plan(catalog, options.connections, &mut rng)?;
    let team = options.connections.min(TEAM_CONNECTIONS);
    let changes = options.changes.min(PHASE_CHANGES);
    let address = node.serve().await?;
    let board: Board = Arc::default();

    let memory_before = resident_memory();
    let connect_started = Instant::now();
    let mut child = tokio::process::Command::new(&options.clients)
        .arg(CLIENTS_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| {
            format!(
                "cannot start the live clients: {}",
                options.clients.display()
            )
        })?;
    let mut input = child.stdin.take().context("no stdin of the live clients")?;
    let output = child
        .stdout
        .take()
        .context("no stdout of the live clients")?;
    let (answers, mut answered) = mpsc::unbounded_channel();
    let reading = tokio::spawn(record(
        BufReader::new(output).lines(),
        Arc::clone(&board),
        answers,
    ));
    let mut clients = Clients {
        input: &mut input,
        answered: &mut answered,
        address: Some(address),
        open: Vec::new(),
        failure: None,
    };

    let result = async {
        clients.open(&subscribers[..team]).await?;
        let mut connecting = connect_started.elapsed();
        let mut targets = targets(catalog, node, &clients.open, &mut rng).await?;
        let team_phase = phase(catalog, node, &board, &mut targets, changes, &mut rng).await?;
        let team_phase = summary(clients.open.len(), &team_phase);
        let mut capacity = None;
        if subscribers.len() > team {
            let more = Instant::now();
            clients.open(&subscribers[team..]).await?;
            connecting += more.elapsed();
            let followers = followed(&clients.open);
            for target in &mut targets {
                target.subscribers = (followers.iter())
                    .find(|f| f.0 == target.issue)
                    .map_or(target.subscribers, |f| f.2);
            }
            let samples = phase(catalog, node, &board, &mut targets, changes, &mut rng).await?;
            capacity = Some(summary(clients.open.len(), &samples));
        }
        Ok::<_, anyhow::Error>((connecting, team_phase, capacity))
    }
    .await;
    let opened = clients.open.len();
    let failure = clients.failure.take();
    let memory_per_connection = match (memory_before, resident_memory()) {
        (Some(before), Some(after)) if opened > 0 => {
            Some(after.saturating_sub(before) as f64 / opened as f64)
        }
        _ => None,
    };
    // Closing their input ends the clients; they close their sockets and exit.
    drop(input);
    if tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }
    reading.abort();
    let (connecting, team, capacity) = result?;
    Ok(LiveMeasurement {
        requested: options.connections,
        opened,
        connecting,
        team,
        capacity,
        failure,
        memory_per_connection,
        elapsed: started.elapsed(),
    })
}

/// The clients' process as the scenario talks to it.
struct Clients<'a> {
    input: &'a mut tokio::process::ChildStdin,
    answered: &'a mut mpsc::UnboundedReceiver<Value>,
    /// Sent with the first batch only.
    address: Option<SocketAddr>,
    /// The open sockets' projects and issues.
    open: Vec<(i32, Vec<i64>)>,
    failure: Option<String>,
}

impl Clients<'_> {
    /// Has the clients open these sockets and waits until they answer.
    async fn open(&mut self, batch: &[Subscriber]) -> anyhow::Result<()> {
        let wire: Vec<Value> = batch
            .iter()
            .map(|s| json!({ "token": s.token, "topics": s.topics }))
            .collect();
        let mut line = json!({ "subscribers": wire });
        if let Some(address) = self.address.take() {
            line["address"] = json!(address.to_string());
        }
        self.input.write_all(format!("{line}\n").as_bytes()).await?;
        self.input.flush().await?;
        let ready = self
            .answered
            .recv()
            .await
            .context("the live clients stopped before opening their sockets")?;
        let before = self.open.len();
        self.open.extend(
            ready["opened"]
                .as_array()
                .context("the live clients' answer has no `opened`")?
                .iter()
                .filter_map(|index| batch.get(usize::try_from(index.as_u64()?).ok()?))
                .map(|s| (s.project, s.issues.clone())),
        );
        if let Some(failure) = ready["failure"].as_str() {
            self.failure = Some(failure.to_owned());
        }
        if self.open.len() == before {
            bail!(
                "no live connection opened: {}",
                self.failure.as_deref().unwrap_or("no reason")
            );
        }
        Ok(())
    }
}

fn summary(sockets: usize, samples: &[Sample]) -> LivePhase {
    let deliveries: Vec<(Duration, usize)> = samples
        .iter()
        .flat_map(|s| s.deliveries.iter().map(|d| (*d, s.subscribers)))
        .collect();
    let changes: Vec<(Duration, usize)> =
        samples.iter().map(|s| (s.change, s.subscribers)).collect();
    LivePhase {
        sockets,
        delivery: Stats::of(&deliveries),
        change: Stats::of(&changes),
        missed: samples.iter().map(|s| s.missed).sum(),
    }
}

/// Reads the clients' arrivals (`{"issue": id, "at": µs}`) into the board and their other
/// answers into `answers` until they stop.
async fn record(
    mut lines: Lines<BufReader<ChildStdout>>,
    board: Board,
    answers: mpsc::UnboundedSender<Value>,
) {
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (Some(issue), Some(at)) = (message["issue"].as_i64(), message["at"].as_u64()) else {
            let _ = answers.send(message);
            continue;
        };
        let mut board = board.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(pending) = board.get_mut(&issue) {
            pending
                .times
                .push(Duration::from_micros(at.saturating_sub(pending.wall)));
            pending.remaining = pending.remaining.saturating_sub(1);
            if pending.remaining == 0
                && let Some(done) = pending.done.take()
            {
                let _ = done.send(());
            }
        }
    }
}

/// Microseconds since the Unix epoch: both processes read the same clock.
fn wall_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

fn raise_open_files() {
    if let Err(error) = rlimit::increase_nofile_limit(u64::MAX) {
        tracing::warn!(%error, "cannot raise the open-files limit");
    }
}

/// The sockets to open, and the subscribed issues with the number of sockets following each.
fn plan(catalog: &Catalog, connections: usize, rng: &mut Rng) -> anyhow::Result<Vec<Subscriber>> {
    let mut by_project: HashMap<i32, Vec<i64>> = HashMap::new();
    for (issue, project) in &catalog.open_issues {
        by_project.entry(*project).or_default().push(*issue);
    }
    let mut subscribers = Vec::with_capacity(connections);
    for _ in 0..connections {
        let (first, project, member) = catalog.issue_and_member(rng);
        if first == 0 {
            bail!("the dataset has no issue a project member sees");
        }
        let mut issues = vec![first];
        let candidates = by_project.get(&project).map_or(&[][..], Vec::as_slice);
        for _ in 0..candidates.len().min(20) {
            if issues.len() == ISSUES_PER_CONNECTION {
                break;
            }
            let issue = Catalog::pick(candidates, rng);
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        }
        let mut topics = vec!["instance".to_owned(), format!("list:{project}")];
        for issue in &issues {
            topics.push(format!("issue:{issue}"));
        }
        subscribers.push(Subscriber {
            token: catalog.token(member),
            topics,
            project,
            issues,
        });
    }
    Ok(subscribers)
}

/// The issues the open sockets follow, with how many follow each.
fn followed(open: &[(i32, Vec<i64>)]) -> Vec<Followed> {
    let mut followers: HashMap<i64, (i32, usize)> = HashMap::new();
    for (project, issues) in open {
        for issue in issues {
            followers.entry(*issue).or_insert((*project, 0)).1 += 1;
        }
    }
    let mut followed: Vec<Followed> = followers
        .into_iter()
        .map(|(issue, (project, count))| (issue, project, count))
        .collect();
    followed.sort_unstable();
    followed
}

/// The clients' process (`prostor-bench --live-clients`). Each line of stdin is a batch of
/// sockets to open, `{"address"?, "subscribers": [{"token", "topics"}]}` (the address comes with
/// the first); each is answered with `{"opened": [indices in the batch], "failure"}`. Between and
/// after them it writes `{"issue", "at"}` for every issue event a socket gets on a topic it
/// follows. It ends when stdin closes.
pub async fn serve_clients() -> anyhow::Result<()> {
    raise_open_files();
    let mut input = BufReader::new(tokio::io::stdin()).lines();
    let (lines, mut written) = mpsc::unbounded_channel::<String>();
    let writing = tokio::spawn(async move {
        let mut out = BufWriter::new(tokio::io::stdout());
        while let Some(line) = written.recv().await {
            out.write_all(line.as_bytes()).await?;
            while let Ok(line) = written.try_recv() {
                out.write_all(line.as_bytes()).await?;
            }
            out.flush().await?;
        }
        Ok::<_, std::io::Error>(())
    });
    let stop = CancellationToken::new();
    let mut address: Option<SocketAddr> = None;
    while let Some(line) = input.next_line().await? {
        let batch: Value = serde_json::from_str(&line)?;
        if let Some(given) = batch["address"].as_str() {
            address = Some(given.parse()?);
        }
        let address = address.context("the first batch has no address")?;
        let plans: Vec<(String, Value)> = batch["subscribers"]
            .as_array()
            .context("the batch has no subscribers")?
            .iter()
            .map(|s| {
                let token = s["token"].as_str().unwrap_or_default().to_owned();
                (token, s["topics"].clone())
            })
            .collect();
        let (opened, failure) = open_all(address, plans, &lines, &stop).await?;
        let ready = json!({ "opened": opened, "failure": failure });
        let _ = lines.send(format!("{ready}\n"));
        if writing.is_finished() {
            break;
        }
    }
    stop.cancel();
    // Let the readers send their close frames.
    tokio::time::sleep(Duration::from_millis(200)).await;
    Ok(())
}

/// Opens a batch of sockets, a few at a time; answers the indices of those that opened and the
/// last reason one did not.
async fn open_all(
    address: SocketAddr,
    plans: Vec<(String, Value)>,
    lines: &mpsc::UnboundedSender<String>,
    stop: &CancellationToken,
) -> anyhow::Result<(Vec<usize>, Option<String>)> {
    let limit = Arc::new(Semaphore::new(CONNECT_AT_ONCE));
    let mut opening = tokio::task::JoinSet::new();
    for (index, (token, topics)) in plans.into_iter().enumerate() {
        let (limit, lines, stop) = (Arc::clone(&limit), lines.clone(), stop.clone());
        opening.spawn(async move {
            let socket = {
                let _turn = limit.acquire_owned().await?;
                open_retrying(address, &token, &topics).await?
            };
            tokio::spawn(read(socket, lines, stop));
            Ok::<_, anyhow::Error>(index)
        });
    }
    let mut opened = Vec::new();
    let mut failure = None;
    while let Some(done) = opening.join_next().await {
        match done? {
            Ok(index) => opened.push(index),
            Err(error) => failure = Some(format!("{error:#}")),
        }
    }
    opened.sort_unstable();
    Ok((opened, failure))
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Opens a socket, trying again as the web client does: a burst of connects can overflow the
/// listen queue of a developer's machine (128 on macOS), and the dropped ones time out.
async fn open_retrying(address: SocketAddr, token: &str, topics: &Value) -> anyhow::Result<Socket> {
    let mut attempt = 1;
    loop {
        let opened = tokio::time::timeout(CONNECT_WAIT, open(address, token, topics))
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("no answer within {CONNECT_WAIT:?}")));
        match opened {
            Ok(socket) => return Ok(socket),
            Err(error) if attempt >= CONNECT_ATTEMPTS => return Err(error),
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(200 * attempt)).await;
                attempt += 1;
            }
        }
    }
}

/// Opens a socket as a script with an API token does and waits until its topics are active.
async fn open(address: SocketAddr, token: &str, topics: &Value) -> anyhow::Result<Socket> {
    let mut request = format!("ws://{address}/api/v1/live").into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("host", HeaderValue::from_static(HOST));
    headers.insert("authorization", format!("Bearer {token}").parse()?);
    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .context("cannot open a live connection")?;
    let subscribe = json!({ "type": "subscribe", "topics": topics, "since": null });
    socket
        .send(Message::Text(subscribe.to_string().into()))
        .await?;
    loop {
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .context("no answer to subscribe")?;
        let Some(frame) = frame else {
            bail!("the live connection closed while subscribing");
        };
        if let Message::Text(text) = frame? {
            let message: Value = serde_json::from_str(&text)?;
            match message["type"].as_str() {
                Some("subscribed") => return Ok(socket),
                Some("denied") => bail!("topics denied: {}", message["topics"]),
                _ => {}
            }
        }
    }
}

/// Reads one socket until the scenario ends, sending the arrival of every issue event.
async fn read(socket: Socket, lines: mpsc::UnboundedSender<String>, stop: CancellationToken) {
    let (mut sink, mut stream) = socket.split();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            _ = ping.tick() => {
                let ping = json!({ "type": "ping" }).to_string();
                if sink.send(Message::Text(ping.into())).await.is_err() {
                    break;
                }
            }
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    let at = wall_micros();
                    if let Some(issue) = followed_issue(&text) {
                        let _ = lines.send(format!("{{\"issue\":{issue},\"at\":{at}}}\n"));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    let _ = sink.close().await;
}

/// The issue of an issue event that came on that issue's topic.
fn followed_issue(text: &str) -> Option<i64> {
    // Most events reach a socket for its list only; those need no parsing.
    if !text.contains("\"issue:") {
        return None;
    }
    let message = serde_json::from_str::<Value>(text).ok()?;
    if message["type"] != "event" || message["entity"]["kind"] != "issue" {
        return None;
    }
    let issue = message["entity"]["id"].as_i64()?;
    let topic = format!("issue:{issue}");
    message["topics"]
        .as_array()?
        .iter()
        .any(|t| t.as_str() == Some(topic.as_str()))
        .then_some(issue)
}

/// The issues the changes go to: those the open sockets follow most whose projects have two
/// priorities to alternate between, each set to its first.
async fn targets(
    catalog: &Catalog,
    node: &Node,
    open: &[(i32, Vec<i64>)],
    rng: &mut Rng,
) -> anyhow::Result<Vec<Target>> {
    let mut targets: Vec<Target> = followed(open)
        .into_iter()
        .filter_map(|(issue, project, subscribers)| {
            let values = catalog.project_priorities.get(&project)?;
            (values.len() >= 2).then(|| Target {
                issue,
                author: catalog.token(catalog.member(project, rng)),
                values: values.clone(),
                subscribers,
                current: 0,
            })
        })
        .collect();
    targets.sort_by_key(|t| std::cmp::Reverse(t.subscribers));
    targets.truncate(TARGETS);
    if targets.is_empty() {
        bail!("no followed issue has two priorities to switch between");
    }
    // Later changes always change the value. Events of these first changes are not waited for;
    // the pause lets them pass.
    for target in &targets {
        set(node, catalog.priority, target, target.values[0]).await?;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(targets)
}

/// `count` changes at [`RATE`], the targets in turn, each started on time as a task of its own,
/// while the team reads in the background.
async fn phase(
    catalog: &Catalog,
    node: &Arc<Node>,
    board: &Board,
    targets: &mut [Target],
    count: usize,
    rng: &mut Rng,
) -> anyhow::Result<Vec<Sample>> {
    let stop = CancellationToken::new();
    let (samples, ()) = tokio::join!(
        async {
            let samples = changes(node, catalog.priority, board, targets, count).await;
            stop.cancel();
            samples
        },
        read_in_background(catalog, node, rng, &stop),
    );
    samples
}

/// Opens issue pages at [`READ_RATE`] until stopped; unmeasured.
async fn read_in_background(
    catalog: &Catalog,
    node: &Node,
    rng: &mut Rng,
    stop: &CancellationToken,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(1) / READ_RATE);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            _ = tick.tick() => {}
        }
        if let Err(error) = issue::open(catalog, node, rng).await {
            tracing::warn!(%error, "a background read failed");
        }
    }
}

async fn changes(
    node: &Arc<Node>,
    field: i32,
    board: &Board,
    targets: &mut [Target],
    count: usize,
) -> anyhow::Result<Vec<Sample>> {
    let mut tick = tokio::time::interval(Duration::from_secs(1) / RATE);
    let mut running = tokio::task::JoinSet::new();
    for n in 0..count {
        tick.tick().await;
        let total = targets.len();
        let target = &mut targets[n % total];
        target.current = (target.current + 1) % target.values.len();
        let job = Change {
            issue: target.issue,
            author: target.author.clone(),
            value: target.values[target.current],
            subscribers: target.subscribers,
        };
        running.spawn(change(Arc::clone(node), field, Arc::clone(board), job));
    }
    let mut samples = Vec::with_capacity(count);
    while let Some(done) = running.join_next().await {
        samples.push(done??);
    }
    Ok(samples)
}

struct Target {
    issue: i64,
    author: String,
    values: Vec<i64>,
    subscribers: usize,
    /// Index of the priority the issue has now.
    current: usize,
}

/// One change to send: the issue gets this priority.
struct Change {
    issue: i64,
    author: String,
    value: i64,
    subscribers: usize,
}

async fn set(node: &Node, field: i32, target: &Target, value: i64) -> anyhow::Result<()> {
    put(node, field, target.issue, &target.author, value).await
}

async fn put(node: &Node, field: i32, issue: i64, author: &str, value: i64) -> anyhow::Result<()> {
    node.send(
        Method::PUT,
        &format!("/issues/{issue}/fields/{field}"),
        author,
        Some(&json!({ "value": value, "silent": false })),
    )
    .await?;
    Ok(())
}

/// Sends the change and waits for the event at each subscriber of its issue.
async fn change(node: Arc<Node>, field: i32, board: Board, job: Change) -> anyhow::Result<Sample> {
    let (done, delivered) = oneshot::channel();
    let started = Instant::now();
    board.lock().unwrap_or_else(PoisonError::into_inner).insert(
        job.issue,
        Pending {
            wall: wall_micros(),
            remaining: job.subscribers,
            times: Vec::with_capacity(job.subscribers),
            done: Some(done),
        },
    );
    let sent = put(&node, field, job.issue, &job.author, job.value).await;
    let change = started.elapsed();
    if sent.is_ok() {
        let _ = tokio::time::timeout(WAIT, delivered).await;
    }
    let pending = board
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&job.issue);
    sent?;
    let pending = pending.context("a change lost its pending deliveries")?;
    Ok(Sample {
        change,
        deliveries: pending.times,
        subscribers: job.subscribers,
        missed: pending.remaining,
    })
}

/// This process's resident memory in bytes, from `ps` (macOS and Linux).
fn resident_memory() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let kib: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    Some(kib * 1024)
}
