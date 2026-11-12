//! Messages of the live connection (D-007, `docs/architecture.md` §5).
//!
//! Each browser tab keeps one WebSocket to `/api/v1/live` and exchanges JSON text frames, one
//! message per frame, tagged by `type`. The client subscribes to topics; the server pushes events
//! for them. These types are the contract: the web app uses the TypeScript types generated from
//! them even before the server endpoint exists (step 1.6).
//!
//! Topics have one spelling each; a topic the server does not know, or one spelled differently, is
//! answered with `denied`:
//! - `issue:<id>`: changes of the issue and of its comments, files and links;
//! - `list:all`: changes of issues in every project; `list:<id>,<id>,…` (project IDs ascending,
//!   unique, at most 100): changes of issues in these projects. Search answers name the topic of
//!   their list;
//! - `instance`: notices with `entity.kind` `settings` or `access` (ID 0, version 0, `updated`):
//!   settings or permissions changed, reload what depends on them. Their `id` is the latest event
//!   ID; notices are never replayed;
//! - `board:<id>` (BRD-114, BRD-115): `issue` events of the board's projects and of issues that
//!   joined, left or moved on it; `board` (settings changed or deleted) and `sprint` (created,
//!   changed, deleted) events, which only its viewers get; `card_order` events (entity: the issue,
//!   version: the sprint's order version) with the patch `{ issue, after, sprint }`: the card now
//!   follows `after` (`null`: first) in that sprint. The topic is checked once: when the board's
//!   projects change, subscribe to it again.
//!
//! - `inbox:<id>` (plan step 1.10): the person's own inbox, no one else's — the check denies
//!   every other `inbox:` topic. `inbox` events (entity: the reader) come with a patch:
//!   `{ kind: "notification", id, event, issue, author, excerpt, reasons }` — a row arrived,
//!   the popup shows it without fetching — or `{ kind: "read", total, direct, subscriptions,
//!   system }` — the unread counts after a read change.
//!
//! Later features add `presence:<scope>` and `article:<id>`.
//!
//! Problems after the socket opens end it with one of the [`close`] codes.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Close codes the server ends a live connection with.
pub mod close {
    /// The server stops: reconnect (to another node).
    pub const RESTART: u16 = 1012;
    /// A malformed, oversized or unknown message; a correct client never gets it.
    pub const POLICY: u16 = 1008;
    /// The client did not read its messages fast enough: reconnect and replay with `since`.
    pub const TOO_SLOW: u16 = 4008;
    /// The session is over: do not reconnect, sign in again.
    pub const SESSION_ENDED: u16 = 4401;
}

/// A message from the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveClientMessage {
    Subscribe(LiveSubscribe),
    Unsubscribe(LiveUnsubscribe),
    /// Asks for a `pong`, to detect a dead connection.
    Ping,
}

/// Start receiving events of these topics. The server answers `subscribed` with the topics it
/// accepted and `denied` with the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LiveSubscribe {
    /// Topics to add; topics already subscribed are accepted again.
    pub topics: Vec<String>,
    /// ID of the last event the client received, sent when it subscribes again after a
    /// reconnect. The server first replays the later events of these topics; when it no longer
    /// has them it answers `resync` instead. `null` for a fresh subscription.
    #[serde(default)]
    #[schema(required = true)]
    pub since: Option<i64>,
}

/// Stop receiving events of these topics. Unknown topics are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LiveUnsubscribe {
    pub topics: Vec<String>,
}

/// A message from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveServerMessage {
    Subscribed(LiveSubscribed),
    Denied(LiveDenied),
    Event(LiveEvent),
    Resync(LiveResync),
    /// Answer to `ping`.
    Pong,
}

/// These topics are active: every later change of them is delivered (and, after `since`, the
/// missed ones were replayed before this message).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LiveSubscribed {
    pub topics: Vec<String>,
    /// ID of the latest event at this moment. Every earlier event of these topics is either
    /// already delivered or older than the data the client can load now, so the client keeps the
    /// larger of this and its last event ID as its `since` for the next reconnect.
    pub head: i64,
}

/// These topics cannot be subscribed: unknown, malformed, or about something the user cannot see.
/// The reasons are not told apart, so the answer never reveals that a hidden object exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LiveDenied {
    pub topics: Vec<String>,
}

/// Events of these topics were missed and cannot be replayed: the client reloads what it shows
/// for them. The topics stay subscribed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LiveResync {
    pub topics: Vec<String>,
}

/// One committed change of one entity, checked against the receiver's permissions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LiveEvent {
    /// Position in the instance-wide event sequence, the same on every node. Increases in commit
    /// order; the client sends the last one it saw as `since` after a reconnect.
    pub id: i64,
    /// The receiver's subscribed topics this change belongs to; one message even when it matches
    /// several of them.
    pub topics: Vec<String>,
    pub entity: LiveEntity,
    pub change: LiveChange,
    /// New values of the changed fields (for `created`: all fields) in the entity's full API shape,
    /// when they are small and safe to send to this receiver; otherwise `null` and the client
    /// fetches the entity if it keeps it. Always `null` for `deleted`; for now set only on
    /// `card_order` events (`board:` topics).
    #[serde(default)]
    #[schema(required = true, value_type = Option<std::collections::HashMap<String, Value>>)]
    pub patch: Option<serde_json::Map<String, Value>>,
}

/// The entity an event is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LiveEntity {
    /// Entity type in snake_case, such as `issue`, `comment` or `article`; grows with features.
    pub kind: String,
    pub id: i64,
    /// The entity's version after the change (for `deleted`: its last version).
    pub version: i64,
}

/// What happened to the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveChange {
    Created,
    Updated,
    Deleted,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn client_messages_have_a_fixed_shape() {
        let subscribe = LiveClientMessage::Subscribe(LiveSubscribe {
            topics: vec!["issue:7".into(), "inbox".into()],
            since: Some(42),
        });
        assert_eq!(
            serde_json::to_value(&subscribe).unwrap(),
            json!({ "type": "subscribe", "topics": ["issue:7", "inbox"], "since": 42 })
        );
        assert_eq!(
            serde_json::to_value(LiveClientMessage::Ping).unwrap(),
            json!({ "type": "ping" })
        );
    }

    #[test]
    fn client_messages_are_read_from_json() {
        let fresh: LiveClientMessage =
            serde_json::from_value(json!({ "type": "subscribe", "topics": ["issue:7"] })).unwrap();
        assert_eq!(
            fresh,
            LiveClientMessage::Subscribe(LiveSubscribe {
                topics: vec!["issue:7".into()],
                since: None,
            })
        );
        let unsubscribe: LiveClientMessage =
            serde_json::from_value(json!({ "type": "unsubscribe", "topics": ["inbox"] })).unwrap();
        assert_eq!(
            unsubscribe,
            LiveClientMessage::Unsubscribe(LiveUnsubscribe {
                topics: vec!["inbox".into()],
            })
        );
        assert!(serde_json::from_value::<LiveClientMessage>(json!({ "type": "shout" })).is_err());
    }

    #[test]
    fn events_carry_the_entity_version_and_an_optional_patch() {
        let mut patch = serde_json::Map::new();
        patch.insert("summary".into(), json!("New title"));
        let event = LiveServerMessage::Event(LiveEvent {
            id: 1001,
            topics: vec!["issue:7".into()],
            entity: LiveEntity {
                kind: "issue".into(),
                id: 7,
                version: 3,
            },
            change: LiveChange::Updated,
            patch: Some(patch),
        });
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({
                "type": "event",
                "id": 1001,
                "topics": ["issue:7"],
                "entity": { "kind": "issue", "id": 7, "version": 3 },
                "change": "updated",
                "patch": { "summary": "New title" }
            })
        );

        let deleted = LiveServerMessage::Event(LiveEvent {
            id: 1002,
            topics: vec!["issue:7".into()],
            entity: LiveEntity {
                kind: "issue".into(),
                id: 7,
                version: 3,
            },
            change: LiveChange::Deleted,
            patch: None,
        });
        assert_eq!(
            serde_json::to_value(&deleted).unwrap()["patch"],
            Value::Null
        );
    }

    #[test]
    fn server_notices_have_a_fixed_shape() {
        let cases = [
            (
                LiveServerMessage::Subscribed(LiveSubscribed {
                    topics: vec!["inbox".into()],
                    head: 1000,
                }),
                json!({ "type": "subscribed", "topics": ["inbox"], "head": 1000 }),
            ),
            (
                LiveServerMessage::Denied(LiveDenied {
                    topics: vec!["issue:9".into()],
                }),
                json!({ "type": "denied", "topics": ["issue:9"] }),
            ),
            (
                LiveServerMessage::Resync(LiveResync {
                    topics: vec!["board:1:2".into()],
                }),
                json!({ "type": "resync", "topics": ["board:1:2"] }),
            ),
            (LiveServerMessage::Pong, json!({ "type": "pong" })),
        ];
        for (message, expected) in cases {
            assert_eq!(serde_json::to_value(&message).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<LiveServerMessage>(expected).unwrap(),
                message
            );
        }
    }
}
