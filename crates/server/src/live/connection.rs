//! One live connection: the upgrade with its sign-in checks, a reader that parses the client's
//! messages and checks topics, and a writer that drains the outgoing queue into the socket.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, header};
use axum::response::Response;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prostor_api::live::{
    LiveClientMessage, LiveDenied, LiveServerMessage, LiveSubscribe, LiveUnsubscribe, close,
};
use prostor_domain::UserId;
use prostor_domain::access::Access;
use prostor_domain::auth::{Credential, Scope};
use prostor_domain::live::Topic;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

use super::pump::{Command, SignedIn};
use super::{Ending, IDLE, MESSAGE_MAX, SUBSCRIBE_MAX, TOPICS_MAX, topics};
use crate::AppState;
use crate::auth::{Caller, RequestInfo, require_scope};
use crate::error::ApiError;

/// Closes a connection that stayed silent past [`IDLE`] ("going away").
const IDLE_CLOSE: u16 = 1001;
/// How long the writer waits to hand over its close frame.
const CLOSE_WAIT: Duration = Duration::from_secs(1);

/// `GET /api/v1/live`: opens a live connection (not in the API document; its messages are).
/// Signed in as every request; with the session cookie only from the instance's own pages, since
/// browsers send cookies with cross-site WebSocket handshakes too.
pub async fn upgrade(
    State(state): State<AppState>,
    Caller(principal): Caller,
    info: RequestInfo,
    headers: HeaderMap,
    socket: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    if !headers.contains_key(header::AUTHORIZATION) {
        info.require_same_origin()?;
    }
    require_scope(&principal, Scope::TrackerRead)?;
    if state.live.commands().is_none() {
        return Err(ApiError::unavailable("The server is starting"));
    }
    let access = state.access.get(&state.db, principal.user).await?;
    let user = principal.user;
    let signed_in = match principal.credential {
        Credential::Session { id, .. } => SignedIn::Session(id),
        Credential::Token { id, .. } => SignedIn::Token(id),
    };
    Ok(socket
        .max_message_size(MESSAGE_MAX)
        .max_frame_size(MESSAGE_MAX)
        .on_upgrade(move |socket| run(state, user, signed_in, access, socket)))
}

async fn run(
    state: AppState,
    user: UserId,
    signed_in: SignedIn,
    access: Arc<Access>,
    socket: WebSocket,
) {
    let Some(commands) = state.live.commands().cloned() else {
        return;
    };
    let id = state.live.next_connection();
    let ending = Arc::new(Ending::default());
    let (queue, outgoing) = mpsc::channel(state.live.config().queue);
    let (sink, stream) = socket.split();
    let open = Command::Open {
        id,
        user,
        signed_in,
        access,
        queue: queue.clone(),
        ending: Arc::clone(&ending),
    };
    if commands.send(open).await.is_err() {
        ending.close(close::RESTART);
    }
    let writer = tokio::spawn(write(sink, outgoing, Arc::clone(&ending)));
    let mut reader = Reader {
        state,
        user,
        id,
        commands,
        queue,
        ending: Arc::clone(&ending),
        topics: HashSet::new(),
    };
    reader.run(stream).await;
    ending.quit();
    let _ = reader.commands.send(Command::Close { id }).await;
    let _ = writer.await;
}

struct Reader {
    state: AppState,
    user: UserId,
    id: u64,
    commands: mpsc::Sender<Command>,
    queue: mpsc::Sender<Utf8Bytes>,
    ending: Arc<Ending>,
    /// What this connection holds, to keep within [`TOPICS_MAX`].
    topics: HashSet<Topic>,
}

impl Reader {
    async fn run(&mut self, mut stream: SplitStream<WebSocket>) {
        let closing = self.state.live.closing().clone();
        loop {
            let next = tokio::select! {
                biased;
                () = self.ending.token.cancelled() => return,
                () = closing.cancelled() => {
                    self.ending.close(close::RESTART);
                    return;
                }
                next = tokio::time::timeout(IDLE, stream.next()) => next,
            };
            match next {
                Err(_) => {
                    self.ending.close(IDLE_CLOSE);
                    return;
                }
                Ok(None) | Ok(Some(Ok(Message::Close(_)))) => return,
                // A broken frame or one over the size limit.
                Ok(Some(Err(_))) | Ok(Some(Ok(Message::Binary(_)))) => {
                    self.ending.close(close::POLICY);
                    return;
                }
                Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => {}
                Ok(Some(Ok(Message::Text(text)))) => self.handle(&text).await,
            }
        }
    }

    async fn handle(&mut self, text: &str) {
        let Ok(message) = serde_json::from_str::<LiveClientMessage>(text) else {
            self.ending.close(close::POLICY);
            return;
        };
        match message {
            LiveClientMessage::Ping => self.reply(&LiveServerMessage::Pong),
            LiveClientMessage::Subscribe(subscribe) => self.subscribe(subscribe).await,
            LiveClientMessage::Unsubscribe(LiveUnsubscribe { topics }) => {
                let topics: Vec<Topic> = topics
                    .iter()
                    .filter_map(|raw| Topic::parse(raw))
                    .filter(|topic| self.topics.remove(topic))
                    .collect();
                if !topics.is_empty() {
                    self.command(Command::Unsubscribe {
                        id: self.id,
                        topics,
                    })
                    .await;
                }
            }
        }
    }

    async fn subscribe(&mut self, LiveSubscribe { mut topics, since }: LiveSubscribe) {
        let over = topics.split_off(topics.len().min(SUBSCRIBE_MAX));
        let db = &self.state.db;
        let checked = match self.state.access.get(db, self.user).await {
            Ok(access) => topics::check(db, &access, &topics).await.ok(),
            Err(_) => None,
        };
        let Some(checked) = checked else {
            // The database is unreachable: the client reconnects later.
            self.ending.close(close::RESTART);
            return;
        };
        let mut denied = checked.denied;
        let mut accepted = Vec::new();
        for topic in checked.accepted {
            if self.topics.contains(&topic) || self.topics.len() < TOPICS_MAX {
                self.topics.insert(topic.clone());
                accepted.push(topic);
            } else {
                denied.push(topic.to_string());
            }
        }
        denied.extend(over);
        if !denied.is_empty() {
            self.reply(&LiveServerMessage::Denied(LiveDenied { topics: denied }));
        }
        if !accepted.is_empty() {
            let boards = (checked.boards.into_iter())
                .filter(|(id, _)| accepted.contains(&Topic::Board(*id)))
                .collect();
            self.command(Command::Subscribe {
                id: self.id,
                topics: accepted,
                boards,
                since,
            })
            .await;
        }
    }

    async fn command(&self, command: Command) {
        if self.commands.send(command).await.is_err() {
            self.ending.close(close::RESTART);
        }
    }

    fn reply(&self, message: &LiveServerMessage) {
        let Some(text) = super::pump::encode(message) else {
            return;
        };
        match self.queue.try_send(text) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.ending.close(close::TOO_SLOW),
            Err(TrySendError::Closed(_)) => self.ending.quit(),
        }
    }
}

async fn write(
    mut sink: SplitSink<WebSocket, Message>,
    mut outgoing: mpsc::Receiver<Utf8Bytes>,
    ending: Arc<Ending>,
) {
    loop {
        let text = tokio::select! {
            biased;
            () = ending.token.cancelled() => break,
            text = outgoing.recv() => match text {
                Some(text) => text,
                None => break,
            },
        };
        // Whatever else is already queued goes out with it in one flush.
        let batch = async {
            sink.feed(Message::Text(text)).await?;
            while let Ok(text) = outgoing.try_recv() {
                sink.feed(Message::Text(text)).await?;
            }
            sink.flush().await
        };
        let sent = tokio::select! {
            biased;
            () = ending.token.cancelled() => break,
            sent = batch => sent,
        };
        if sent.is_err() {
            ending.quit();
            return;
        }
    }
    if let Some(code) = ending.code.get() {
        let frame = CloseFrame {
            code: *code,
            reason: Utf8Bytes::default(),
        };
        let _ = tokio::time::timeout(CLOSE_WAIT, sink.send(Message::Close(Some(frame)))).await;
    }
    let _ = tokio::time::timeout(CLOSE_WAIT, sink.close()).await;
}
