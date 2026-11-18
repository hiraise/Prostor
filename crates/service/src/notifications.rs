//! Notifications (NTF, plan step 1.10): a person's settings, their subscriptions on sources they
//! can see, the watcher list of an issue (§3.1), and the deliveries of 1.10a-2 — event recording
//! in every writer (§3.2), the sweep with its batching window (§3.4) and the per-recipient render
//! into the inbox (§3.5).

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Value as Json, json};
use sqlx::PgConnection;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::issues::history::{ChangeKind, LoggedLink, Record};
use taskmaster_domain::issues::rights;
use taskmaster_domain::live::{Change, EntityKind, Guard, NewEvent, Patch};
use taskmaster_domain::notifications::{
    Content, Folder, IssueRef, Kind, Line, Payload, Reason, Settings, Source, SystemReport, Words,
    words,
};
use taskmaster_domain::search::Filter;
use taskmaster_domain::settings::{
    BASE_URL, DEFAULT_DATE_FORMAT, DEFAULT_TIME_ZONE, DateFormat, Language,
};
use taskmaster_domain::validation::{FieldProblem, Problem};
use taskmaster_domain::{
    CommentId, FieldId, IssueId, ProjectId, SavedSearchId, SubscriptionId, TagId, UserId,
};
use taskmaster_store::users::{self, User};
use taskmaster_store::visibility::Visibility;
use taskmaster_store::{
    PgPool, comments as comments_store, issues, links, live as live_store, notifications as store,
    projects, saved_searches, search as search_store, settings as settings_store, tags,
    unique_violation,
};
use time::OffsetDateTime;

use crate::{Error, Result};

/// A subscription as the settings list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub row: store::Subscription,
    /// The tag's or the saved search's current name.
    pub name: String,
}

/// The person's notification settings; the defaults before their first change (NTF-017).
pub async fn get_settings(pool: &PgPool, user: UserId) -> Result<Settings> {
    Ok(store::settings(pool, user).await?.unwrap_or_default())
}

/// Writes the person's settings whole (NTF-030, NTF-034, NTF-066); answers them as stored.
pub async fn set_settings(pool: &PgPool, user: UserId, settings: Settings) -> Result<Settings> {
    let mut tx = pool.begin().await?;
    let saved = store::save_settings(&mut tx, user, &settings).await?;
    tx.commit().await?;
    Ok(saved)
}

/// What happened that may make someone a watcher (ISS-030's auto-watch, plan step 1.12 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoWatch {
    /// They published the issue (a clone's publisher counts: they created it).
    Created,
    /// They wrote their first comment on the issue; later ones never re-add them.
    Commented,
    /// A user field of the issue now names them — the assignee or any other single place people
    /// are put.
    Assigned,
    /// They voted for the issue.
    Voted,
}

/// Makes the person a watcher when their own settings say so (ISS-030): idempotent, inside the
/// caller's transaction, never removing a marker that is there. Which switch decides is the
/// person's own; the comment switch fires on their first comment on the issue only.
pub(crate) async fn auto_watch(
    conn: &mut PgConnection,
    user: UserId,
    issue: IssueId,
    on: AutoWatch,
) -> Result<()> {
    let settings = store::settings(&mut *conn, user).await?.unwrap_or_default();
    let allowed = match on {
        AutoWatch::Created => settings.autowatch_created,
        AutoWatch::Commented => settings.autowatch_commented,
        AutoWatch::Assigned => settings.autowatch_assigned,
        AutoWatch::Voted => settings.autowatch_voted,
    };
    if allowed {
        issues::watch(conn, issue, &[user]).await?;
    }
    Ok(())
}

/// The person's subscriptions, oldest first (NTF-021).
pub async fn subscriptions(pool: &PgPool, user: UserId) -> Result<Vec<Subscription>> {
    Ok(store::subscriptions_of(pool, user)
        .await?
        .into_iter()
        .map(|(row, name)| Subscription { row, name })
        .collect())
}

/// Subscribes the viewer to a source (NTF-023): the source must be visible to them — a tag they
/// can view, a saved search they know, as everywhere else (D-038) — and the kinds must fit the
/// source (NTF-022).
pub async fn subscribe(
    pool: &PgPool,
    viewer: &Access,
    source: Source,
    kinds: &[Kind],
) -> Result<Subscription> {
    check_kinds(source, kinds)?;
    let mut tx = pool.begin().await?;
    check_source(&mut tx, viewer, source).await?;
    let id = store::subscribe(&mut tx, viewer.user(), source, &deduped(kinds))
        .await
        .map_err(taken)?;
    tx.commit().await?;
    one(pool, viewer.user(), id).await
}

/// Changes the kinds of the viewer's own subscription (NTF-024); none ticks leaves it stored but
/// empty, which delivers nothing. The source may have stopped being visible: my subscription is
/// still mine (NTF-025).
pub async fn set_kinds(
    pool: &PgPool,
    viewer: &Access,
    id: SubscriptionId,
    kinds: &[Kind],
) -> Result<Subscription> {
    let mut tx = pool.begin().await?;
    let row = own(&mut tx, viewer.user(), id).await?;
    check_kinds(row.source(), kinds)?;
    store::set_kinds(&mut tx, id, &deduped(kinds)).await?;
    tx.commit().await?;
    one(pool, viewer.user(), id).await
}

/// Removes the viewer's own subscription (NTF-024).
pub async fn remove(pool: &PgPool, viewer: &Access, id: SubscriptionId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let row = own(&mut tx, viewer.user(), id).await?;
    store::unsubscribe(&mut tx, row.id).await?;
    tx.commit().await?;
    Ok(())
}

/// The viewer's subscription on one source (NTF-025: the tag and saved-search dialogs answer
/// their subscriber state).
pub async fn subscription_on(
    pool: &PgPool,
    viewer: &Access,
    source: Source,
) -> Result<Option<Subscription>> {
    match store::subscription_on(pool, viewer.user(), source).await? {
        Some(row) => Ok(Some(Subscription {
            name: source_name(pool, source).await?,
            row,
        })),
        None => Ok(None),
    }
}

/// Who watches an issue (NTF-015), in the order they came — behind `issue.watchers.read`.
pub async fn watchers(pool: &PgPool, viewer: &Access, issue: IssueId) -> Result<Vec<User>> {
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, issue).await?;
    let (issue, _) = crate::issues::checked(&mut conn, viewer, found).await?;
    if !rights::may_see_watchers(viewer, issue.project_id) {
        return Err(Error::Refused(
            "you may not see who watches this issue".into(),
        ));
    }
    let ids = store::issue_watchers(&mut *conn, issue.id).await?;
    in_order(&mut conn, &ids).await
}

/// The accounts in the order the ids came.
async fn in_order(conn: &mut PgConnection, ids: &[UserId]) -> Result<Vec<User>> {
    let found = users::get_many(conn, ids).await?;
    Ok(ids
        .iter()
        .filter_map(|id| found.iter().find(|u| u.id == *id).cloned())
        .collect())
}

/// Replaces who watches an issue (NTF-016): the whole new list, the caller included. Changing
/// anyone but the caller needs `issue.watchers.update` — the rule a new issue's watchers follow —
/// so a caller without the right may still take their own marker off, as the star does. Taking
/// someone's marker off does not touch their other subscriptions. The answer is the new list; a
/// caller without the right has to name the others exactly as they were, so it tells them nothing
/// new.
pub async fn set_watchers(
    pool: &PgPool,
    viewer: &Access,
    id: IssueId,
    list: Vec<UserId>,
) -> Result<Vec<User>> {
    let mut tx = pool.begin().await?;
    let found = issues::get(&mut *tx, id).await?;
    let (issue, _) = crate::issues::checked(&mut tx, viewer, found).await?;
    let wanted = crate::issues::check_accounts(&mut tx, &list).await?;
    let old = store::issue_watchers(&mut *tx, issue.id).await?;
    let added: Vec<UserId> = wanted
        .iter()
        .copied()
        .filter(|u| !old.contains(u))
        .collect();
    let gone: Vec<UserId> = old
        .iter()
        .copied()
        .filter(|u| !wanted.contains(u))
        .collect();
    let changed = !added.is_empty() || !gone.is_empty();
    // Only the caller's own marker moves; anyone else's needs `issue.watchers.update`.
    let others =
        added.iter().any(|u| *u != viewer.user()) || gone.iter().any(|u| *u != viewer.user());
    if changed {
        if others {
            rights::may_manage_watchers(viewer, issue.project_id)?;
        }
        if !added.is_empty() {
            issues::watch(&mut tx, issue.id, &added).await?;
        }
        for user in &gone {
            issues::unwatch(&mut tx, issue.id, *user).await?;
        }
        crate::issues::bump(&mut tx, issue.id).await?;
    }
    tx.commit().await?;
    let mut conn = pool.acquire().await?;
    let ids = store::issue_watchers(&mut *conn, issue.id).await?;
    in_order(&mut conn, &ids).await
}

/// The creator's subscription on a new project's "unassigned" search (PRJ-019, NTF-027): new
/// items only. Runs in the project's creating transaction; the search is the creator's own, so
/// there is nothing to check.
pub(crate) async fn subscribe_unassigned(
    conn: &mut PgConnection,
    creator: UserId,
    search: taskmaster_domain::SavedSearchId,
) -> Result<()> {
    store::subscribe(conn, creator, Source::Search(search), &[Kind::Created]).await?;
    Ok(())
}

/// Loads the viewer's subscription; anyone else's is "not found".
async fn own(
    conn: &mut PgConnection,
    viewer: UserId,
    id: SubscriptionId,
) -> Result<store::Subscription> {
    let row = store::subscription(&mut *conn, id)
        .await?
        .filter(|row| row.user_id == viewer)
        .ok_or_else(|| Error::NotFound("no such subscription"))?;
    Ok(row)
}

/// One subscription of the viewer with its source's name.
async fn one(pool: &PgPool, viewer: UserId, id: SubscriptionId) -> Result<Subscription> {
    let row = store::subscription(pool, id)
        .await?
        .filter(|row| row.user_id == viewer)
        .ok_or_else(|| Error::NotFound("no such subscription"))?;
    Ok(Subscription {
        name: source_name(pool, row.source()).await?,
        row,
    })
}

/// The source's current name, for showing the subscription.
async fn source_name(pool: &PgPool, source: Source) -> Result<String> {
    let mut conn = pool.acquire().await?;
    let name = match source {
        Source::Tag(id) => tags::get(&mut *conn, id).await?.map(|tag| tag.name),
        Source::Search(id) => saved_searches::get(&mut *conn, id)
            .await?
            .map(|row| row.name),
    };
    Ok(name.unwrap_or_else(|| String::from("?")))
}

/// The kinds must fit the source (NTF-022): search sources take no tag kinds, nothing takes
/// `system`.
fn check_kinds(source: Source, kinds: &[Kind]) -> Result<()> {
    let rejected = source.rejects(kinds);
    if rejected.is_empty() {
        return Ok(());
    }
    Err(Error::Invalid(
        rejected
            .iter()
            .map(|kind| FieldProblem {
                path: "kinds".to_owned(),
                problem: Problem::Invalid,
                message: format!(
                    "a {} source does not take {}",
                    source_word(source),
                    kind.key()
                ),
            })
            .collect(),
    ))
}

fn source_word(source: Source) -> &'static str {
    match source {
        Source::Tag(_) => "tag",
        Source::Search(_) => "saved search",
    }
}

/// The source must be visible to its subscriber (NTF-023): a missing source is refused the same
/// way, without saying whether it exists.
async fn check_source(conn: &mut PgConnection, viewer: &Access, source: Source) -> Result<()> {
    match source {
        Source::Tag(id) => {
            let visible = tags::get(&mut *conn, id)
                .await?
                .is_some_and(|tag| tag.sharing().can_view(viewer));
            if !visible {
                return Err(Error::Refused("you cannot see this tag".into()));
            }
        }
        Source::Search(id) => {
            let found = saved_searches::get(&mut *conn, id).await?;
            let visible = found.is_some_and(|row| {
                let shared = row.sharing();
                shared.can_view(viewer) || shared.can_edit(viewer) || shared.may_delete(viewer)
            });
            if !visible {
                return Err(Error::Refused("you cannot see this saved search".into()));
            }
        }
    }
    Ok(())
}

/// Both unique keys of the table mean one subscription per person and source.
fn taken(error: sqlx::Error) -> Error {
    match unique_violation(&error) {
        Some(store::SOURCE_KEY) | Some(store::SEARCH_KEY) => Error::field(
            "source",
            Problem::Taken,
            "you already subscribe to this source",
        ),
        _ => error.into(),
    }
}

/// Kinds without repeats, in a stable order.
fn deduped(kinds: &[Kind]) -> Vec<Kind> {
    Kind::ALL
        .into_iter()
        .filter(|kind| kinds.contains(kind))
        .collect()
}

// ---------------------------------------------------------------------------
// Event recording (plan §3.2, 1.10a-2)
//

/// How long an `updated` batch waits for more updates by the same author (NTF-044).
pub const BATCH_WINDOW: Duration = Duration::from_secs(60);

/// How many due batches one sweep step fires. Steps run every 3 s; 500 lets one step eat a
/// burst the size of a whole e2e suite (board-perf alone writes ~500 changes), where 50 fell
/// behind and late events waited behind the backlog.
const SWEEP_BATCH: i64 = 500;

/// How long a rendered old/new value may be.
const LINE_MAX: usize = 80;
/// How long a rendered excerpt may be.
const EXCERPT_MAX: usize = 120;

/// One event of a change: its kind and the payload lines describing it.
struct Event {
    kind: Kind,
    lines: Vec<Payload>,
}

/// Records the events of one published change from its history records, in the writer's
/// transaction (plan §3.2). `mentions` are the accounts named by the change's new comment or
/// description edit. Drafts never come here: their writers are silent (NTF-041).
pub(crate) async fn record(
    conn: &mut PgConnection,
    issue: &issues::Issue,
    author: UserId,
    records: &[Record],
    mentions: &[UserId],
) -> Result<()> {
    let events = events_of(conn, issue.project_id, records).await?;
    write_events(conn, issue.id, author, &events, mentions).await
}

/// Records the one `created` event of a publication (plan §3.2) with everything it did at once:
/// its own records and the records of the rules the publication triggered.
pub(crate) async fn record_created(
    conn: &mut PgConnection,
    issue: &issues::Issue,
    author: UserId,
    records: &[Record],
) -> Result<()> {
    let names = Names::load(conn, issue.project_id, records).await?;
    let lines: Vec<Payload> = records.iter().filter_map(|r| names.line(r)).collect();
    let events = vec![Event {
        kind: Kind::Created,
        lines,
    }];
    write_events(conn, issue.id, author, &events, &[]).await
}

/// Records a vote (plan §3.2): no history record exists for one, so the writer tells the event.
pub(crate) async fn record_vote(
    conn: &mut PgConnection,
    issue: &issues::Issue,
    author: UserId,
    on: bool,
) -> Result<()> {
    let events = vec![Event {
        kind: Kind::Vote,
        lines: vec![Payload::Vote { on }],
    }];
    write_events(conn, issue.id, author, &events, &[]).await
}

/// Writes the events as pending batches (plan §3.4): `updated` waits out its window, everything
/// else is due at once.
async fn write_events(
    conn: &mut PgConnection,
    issue: IssueId,
    author: UserId,
    events: &[Event],
    mentions: &[UserId],
) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    for event in events {
        let due = if event.kind == Kind::Updated {
            now + BATCH_WINDOW
        } else {
            now
        };
        let payload = json!(event.lines);
        store::record_event(conn, issue, author, event.kind, &payload, mentions, due).await?;
    }
    Ok(())
}

/// Maps history records to events (plan §3.2): field values, texts, links, files, audience
/// changes, moves and board placements are `updated`; a resolution of its own is `resolved`; a
/// new comment is `comment`; tag changes are `tag_added` and `tag_removed`. The `updated` lines
/// of one change join one event; so does a reopen, with no line of its own.
async fn events_of(
    conn: &mut PgConnection,
    project: ProjectId,
    records: &[Record],
) -> Result<Vec<Event>> {
    let names = Names::load(conn, project, records).await?;
    let mut updated: Vec<Payload> = Vec::new();
    let mut reopened = false;
    let mut events = Vec::new();
    for record in records {
        match record.kind {
            ChangeKind::Resolved => events.push(Event {
                kind: Kind::Resolved,
                lines: Vec::new(),
            }),
            ChangeKind::Reopened => reopened = true,
            ChangeKind::Comment => {
                if let Some(id) = record.comment {
                    events.push(Event {
                        kind: Kind::Comment,
                        lines: vec![Payload::Comment { id }],
                    });
                }
            }
            ChangeKind::Tags => {
                let (removed, added) = names.tag_lists(record);
                if !added.is_empty() {
                    events.push(Event {
                        kind: Kind::TagAdded,
                        lines: vec![Payload::Tags {
                            added,
                            removed: Vec::new(),
                        }],
                    });
                }
                if !removed.is_empty() {
                    events.push(Event {
                        kind: Kind::TagRemoved,
                        lines: vec![Payload::Tags {
                            added: Vec::new(),
                            removed,
                        }],
                    });
                }
            }
            ChangeKind::CommentEdited
            | ChangeKind::CommentDeleted
            | ChangeKind::CommentRestored => {
                if let Some(id) = record.comment {
                    updated.push(Payload::Comment { id });
                }
            }
            ChangeKind::Created | ChangeKind::Deleted => {}
            _ => {
                if let Some(line) = names.line(record) {
                    updated.push(line);
                }
            }
        }
    }
    if !updated.is_empty() || reopened {
        events.push(Event {
            kind: Kind::Updated,
            lines: updated,
        });
    }
    Ok(events)
}

/// The names a payload line shows: fields of the project, tags and link ends of the change.
struct Names {
    fields: HashMap<FieldId, String>,
    tags: HashMap<TagId, String>,
    links: HashMap<LoggedLink, String>,
}

impl Names {
    async fn load(conn: &mut PgConnection, project: ProjectId, records: &[Record]) -> Result<Self> {
        let fields = crate::issues::values::of_project(&mut *conn, project)
            .await?
            .0
            .iter()
            .map(|f| (f.id(), f.field.name.clone()))
            .collect();
        let tag_ids: Vec<TagId> = records
            .iter()
            .filter(|r| r.kind == ChangeKind::Tags)
            .flat_map(tag_ids_of)
            .collect();
        let tags = if tag_ids.is_empty() {
            HashMap::new()
        } else {
            tags::get_many(&mut *conn, &tag_ids)
                .await?
                .into_iter()
                .map(|t| (t.id, t.name))
                .collect()
        };
        let link_ends: Vec<LoggedLink> = records
            .iter()
            .filter(|r| r.kind == ChangeKind::Links)
            .flat_map(link_list)
            .collect();
        let links = link_names(conn, &link_ends).await?;
        Ok(Names {
            fields,
            tags,
            links,
        })
    }

    /// The line a record contributes to an event: names resolved, values displayed.
    fn line(&self, record: &Record) -> Option<Payload> {
        match record.kind {
            ChangeKind::Field => {
                let id = record.field?;
                Some(Payload::Field {
                    id,
                    name: self.fields.get(&id).cloned().unwrap_or_default(),
                    old: short(&display(record.removed.as_ref())),
                    new: short(&display(record.added.as_ref())),
                })
            }
            ChangeKind::Summary => Some(Payload::Summary {
                old: short(&display(record.removed.as_ref())),
                new: short(&display(record.added.as_ref())),
            }),
            ChangeKind::Description => Some(Payload::Description {
                old: short(&display(record.removed.as_ref())),
                new: short(&display(record.added.as_ref())),
            }),
            ChangeKind::Comment | ChangeKind::CommentEdited => {
                record.comment.map(|id| Payload::Comment { id })
            }
            ChangeKind::Attachments => Some(Payload::Files {
                of: None,
                added: file_names(record.added.as_ref()),
                removed: file_names(record.removed.as_ref()),
            }),
            ChangeKind::Links => Some(Payload::Links {
                added: self.link_labels(record.added.as_ref()),
                removed: self.link_labels(record.removed.as_ref()),
            }),
            ChangeKind::Project => Some(Payload::Moved {
                from: place_id(record.removed.as_ref()),
                to: place_id(record.added.as_ref()),
            }),
            ChangeKind::Visibility => Some(Payload::Audience),
            ChangeKind::Boards => board_place(record.added.as_ref()),
            _ => None,
        }
    }

    /// The tag names a tags record added and removed.
    fn tag_lists(&self, record: &Record) -> (Vec<String>, Vec<String>) {
        let names = |json: Option<&Json>| -> Vec<String> {
            tag_ids_of_json(json)
                .iter()
                .filter_map(|id| self.tags.get(id).cloned())
                .collect()
        };
        (names(record.removed.as_ref()), names(record.added.as_ref()))
    }

    fn link_labels(&self, side: Option<&Json>) -> Vec<String> {
        link_list_json(side)
            .iter()
            .filter_map(|end| self.links.get(end).cloned())
            .collect()
    }
}

/// The tag ids of a tags record's two lists.
fn tag_ids_of(record: &Record) -> Vec<TagId> {
    let mut ids = tag_ids_of_json(record.removed.as_ref());
    ids.extend(tag_ids_of_json(record.added.as_ref()));
    ids
}

fn tag_ids_of_json(json: Option<&Json>) -> Vec<TagId> {
    json.as_ref()
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|i| i.as_i64()).collect())
        .unwrap_or_default()
}

/// The link ends of a links record's two lists.
fn link_list(record: &Record) -> Vec<LoggedLink> {
    let mut ends = link_list_json(record.removed.as_ref());
    ends.extend(link_list_json(record.added.as_ref()));
    ends
}

fn link_list_json(json: Option<&Json>) -> Vec<LoggedLink> {
    json.as_ref()
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(LoggedLink::read).collect())
        .unwrap_or_default()
}

/// Link ends as language-neutral labels: the type's directional name and the other issue's
/// readable ID.
async fn link_names(
    conn: &mut PgConnection,
    ends: &[LoggedLink],
) -> Result<HashMap<LoggedLink, String>> {
    let mut type_ids: Vec<i64> = ends.iter().map(|e| e.link_type).collect();
    type_ids.sort_unstable();
    type_ids.dedup();
    let mut types = HashMap::new();
    for id in type_ids {
        if let Some(found) = links::get_type(&mut *conn, id).await? {
            types.insert(
                id,
                (
                    found.source_name,
                    found.target_name.unwrap_or_else(|| found.name.clone()),
                ),
            );
        }
    }
    let mut issue_ids: Vec<IssueId> = ends.iter().map(|e| e.issue).collect();
    issue_ids.sort_unstable();
    issue_ids.dedup();
    let readable = readable_refs(conn, &issue_ids).await?;
    Ok(ends
        .iter()
        .map(|end| {
            let word = match types.get(&end.link_type) {
                Some((source, target)) => match end.side {
                    taskmaster_domain::links::Side::Source => source.clone(),
                    taskmaster_domain::links::Side::Target => target.clone(),
                },
                None => String::new(),
            };
            let other = readable.get(&end.issue).cloned().unwrap_or_default();
            (
                *end,
                if word.is_empty() {
                    other
                } else {
                    format!("{word} {other}")
                },
            )
        })
        .collect())
}

/// The readable IDs (`KEY-12`) of issues, for lines that name them.
async fn readable_refs(
    conn: &mut PgConnection,
    ids: &[IssueId],
) -> Result<HashMap<IssueId, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query!(
        r#"select i.id, p.key as "key!", i.number as "number!"
           from issues i join projects p on p.id = i.project_id
           where i.id = any($1)"#,
        ids
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                taskmaster_domain::issues::readable_id(&r.key, r.number),
            )
        })
        .collect())
}

/// A history value as a line shows it: set values by name, scalars as written.
fn display(json: Option<&Json>) -> String {
    let one = |value: &Json| -> String {
        value
            .get("name")
            .and_then(|n| n.as_str())
            .map(str::to_owned)
            .or_else(|| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| value.to_string())
    };
    match json {
        None => String::new(),
        Some(Json::Array(items)) => items.iter().map(one).collect::<Vec<_>>().join(", "),
        Some(one_value) => one(one_value),
    }
}

fn file_names(json: Option<&Json>) -> Vec<String> {
    json.and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("name").and_then(|n| n.as_str()))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn place_id(json: Option<&Json>) -> String {
    json.and_then(|v| v.get("id"))
        .and_then(|i| i.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn board_place(json: Option<&Json>) -> Option<Payload> {
    let board = json?.get("board")?.get("name")?.as_str()?.to_owned();
    let sprint = json
        .and_then(|v| v.get("sprint"))
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_owned);
    Some(Payload::Boards { board, sprint })
}

/// Keeps a shown value to one line.
pub(crate) fn short(text: &str) -> String {
    if text.chars().count() <= LINE_MAX {
        return text.to_owned();
    }
    let cut: String = text.chars().take(LINE_MAX).collect();
    format!("{cut}…")
}

// ---------------------------------------------------------------------------
// The inbox API: folders, threads, read state, the badge (plan §3.5, 1.10a-3)
//

/// What one row says: the rendered change of an issue, or a job report of a `system` row.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Issue(Content),
    System(SystemReport),
}

/// One inbox row as the API shows it: the stored row with its content parsed and its author
/// resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct InboxRow {
    pub id: i64,
    pub issue_id: Option<IssueId>,
    pub kind: Kind,
    pub reasons: Vec<String>,
    pub body: Body,
    pub last: bool,
    pub read: bool,
    pub created_at: OffsetDateTime,
    /// The row's author; `None` cannot happen today (their account's deletion takes the row),
    /// the answer keeps its shape anyway.
    pub author: Option<User>,
}

/// One thread of the inbox list (NTF-049): the head row — the thread's newest — with the
/// thread's newest rows inline, the head first. A `system` row stands alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub head: InboxRow,
    pub latest: Vec<InboxRow>,
}

/// One page of the inbox: threads newest first, the cursor of the next page.
#[derive(Debug, Clone, PartialEq)]
pub struct Inbox {
    pub threads: Vec<Thread>,
    /// `before` of the next page; `None` on the last.
    pub next: Option<i64>,
}

/// The unread counts of the folders (the badge, plan §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub total: i64,
    pub direct: i64,
    pub subscriptions: i64,
    pub system: i64,
}

/// How many threads one inbox page holds at most; `limit` above this is cut.
pub const PAGE_MAX: i64 = 100;
/// The rows a thread head carries inline (NTF-049).
pub const THREAD_INLINE: i64 = 3;
/// The rows of one thread's full view, a safety cap above any real thread.
pub const THREAD_MAX: i64 = 500;

/// The caller's inbox (plan §3.5, NTF-048, NTF-049): without `issue`, one thread per issue —
/// its newest row of the folder, with the newest rows inline — newest first, paged by the
/// head's row id; with `issue`, that thread's rows. The `system` folder is answered only to
/// people who see the administration area.
pub async fn inbox(
    pool: &PgPool,
    viewer: &Access,
    folder: Folder,
    unread_only: bool,
    before: Option<i64>,
    issue: Option<IssueId>,
    limit: i64,
) -> Result<Inbox> {
    if folder == Folder::System && !viewer.has(Permission::SystemRead) {
        return Err(Error::Refused(
            "the system folder is part of the administration area".into(),
        ));
    }
    let user = viewer.user();
    if let Some(issue) = issue {
        let rows = store::notifications_of(pool, user, Some(issue), THREAD_MAX).await?;
        let authors = people(pool, rows.iter().map(|r| r.author_id)).await?;
        return Ok(Inbox {
            threads: rows
                .iter()
                .rev()
                .map(|r| Thread {
                    head: shown_row(r, &authors),
                    latest: Vec::new(),
                })
                .collect(),
            next: None,
        });
    }
    let limit = limit.clamp(1, PAGE_MAX);
    let heads = store::inbox_heads(pool, user, folder, unread_only, before, limit + 1).await?;
    let more = heads.len() as i64 > limit;
    let heads: Vec<store::Notification> = heads.into_iter().take(limit as usize).collect();
    let issues: Vec<IssueId> = heads.iter().filter_map(|head| head.issue_id).collect();
    let inline =
        store::inbox_latest(pool, user, &issues, folder, unread_only, THREAD_INLINE).await?;
    let authors = people(
        pool,
        heads
            .iter()
            .map(|h| h.author_id)
            .chain(inline.iter().map(|r| r.author_id)),
    )
    .await?;
    let threads: Vec<Thread> = heads
        .into_iter()
        .map(|head| {
            let latest: Vec<InboxRow> = inline
                .iter()
                .filter(|row| row.issue_id == head.issue_id)
                .map(|row| shown_row(row, &authors))
                .collect();
            let head = shown_row(&head, &authors);
            Thread {
                latest: if latest.is_empty() {
                    vec![head.clone()]
                } else {
                    latest
                },
                head,
            }
        })
        .collect();
    Ok(Inbox {
        next: more.then(|| threads.last().map(|t| t.head.id)).flatten(),
        threads,
    })
}

/// The accounts behind the rows' authors, by id.
async fn people(pool: &PgPool, ids: impl Iterator<Item = UserId>) -> Result<HashMap<UserId, User>> {
    let mut unique: Vec<UserId> = ids.collect();
    unique.sort_unstable();
    unique.dedup();
    Ok(users::get_many(pool, &unique)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect())
}

/// Parses a stored row into what the API shows.
fn shown_row(row: &store::Notification, authors: &HashMap<UserId, User>) -> InboxRow {
    let body = if row.kind == Kind::System {
        Body::System(
            serde_json::from_value(row.content.clone()).unwrap_or(SystemReport {
                job: String::new(),
                state: String::new(),
                message: None,
            }),
        )
    } else {
        serde_json::from_value(row.content.clone())
            .map(Body::Issue)
            .unwrap_or(Body::Issue(Content {
                issue: IssueRef {
                    readable_id: String::new(),
                    summary: String::new(),
                },
                lines: Vec::new(),
                excerpt: String::new(),
            }))
    };
    InboxRow {
        id: row.id,
        issue_id: row.issue_id,
        kind: row.kind,
        reasons: row.reasons.clone(),
        body,
        last: row.last,
        read: row.read_at.is_some(),
        created_at: row.created_at,
        author: authors.get(&row.author_id).cloned(),
    }
}

/// The unread counts of the folders (the badge).
pub async fn summary(pool: &PgPool, user: UserId) -> Result<Summary> {
    let (total, direct, subscriptions, system) = store::unread_summary(pool, user).await?;
    Ok(Summary {
        total,
        direct,
        subscriptions,
        system,
    })
}

/// Marks rows read or unread (NTF-050): by `ids`, by `issue` thread or `all`; exactly one
/// selector. Answers the new unread counts, and tells the reader's open tabs through the
/// `inbox:` topic in the same transaction (plan §3.6).
pub async fn mark_read(
    pool: &PgPool,
    viewer: &Access,
    ids: Option<Vec<i64>>,
    issue: Option<IssueId>,
    all: bool,
    unread: bool,
) -> Result<Summary> {
    if ids.is_none() && issue.is_none() && !all {
        return Err(Error::Refused("say what to mark: ids, issue or all".into()));
    }
    let user = viewer.user();
    let mut tx = pool.begin().await?;
    store::mark_read(&mut tx, user, ids.as_deref(), issue, all, unread).await?;
    let (total, direct, subscriptions, system) = store::unread_summary(&mut *tx, user).await?;
    let summary = Summary {
        total,
        direct,
        subscriptions,
        system,
    };
    live_store::publish(
        &mut tx,
        &[NewEvent {
            kind: EntityKind::Inbox,
            entity: user,
            version: 0,
            change: Change::Updated,
            issues: Vec::new(),
            projects: Vec::new(),
            boards: Vec::new(),
            patch: Some(Patch::Read {
                total: summary.total,
                direct: summary.direct,
                subscriptions: summary.subscriptions,
                system: summary.system,
            }),
            guard: Guard::user(user),
        }],
    )
    .await?;
    tx.commit().await?;
    Ok(summary)
}

/// Fires every due batch (plan §3.4): the ones whose window passed and, with each of them, the
/// other authors' pending `updated` batches of the same issue (NTF-044). One transaction per
/// step: the fired rows, the written inbox rows, the `inbox:` events of plan §3.6 and the queued
/// emails (§5.2) commit together. Answers how many rows the step delivered. `now` is a parameter
/// so tests can move inside the window; `outgoing` is the node's email state — with email off
/// nothing is queued (NTF-002).
pub async fn sweep(
    pool: &PgPool,
    now: OffsetDateTime,
    outgoing: &crate::mail::Outgoing,
) -> Result<u64> {
    let mut tx = pool.begin().await?;
    // Issues before batches (plan §3.4): a change writer locks its issue before it merges into
    // `notification_pending`, and the fired rows reference the issue too, so the sweep takes the
    // same order — key share first, then the batches. A busy issue waits one step.
    let mut issues = store::due_issues(&mut tx, now).await?;
    if issues.is_empty() {
        tx.commit().await?;
        return Ok(0);
    }
    issues = store::lock_issues(&mut tx, &issues).await?;
    let mut batches = store::due_batches(&mut tx, now, &issues, SWEEP_BATCH).await?;
    if batches.is_empty() {
        tx.commit().await?;
        return Ok(0);
    }
    let claimed: Vec<i64> = batches.iter().map(|b| b.id).collect();
    issues = batches.iter().map(|b| b.issue_id).collect();
    issues.sort_unstable();
    issues.dedup();
    batches.extend(store::flushed_updates(&mut tx, &issues, &claimed).await?);
    let language = crate::rules::instance_language(&mut tx).await?;
    let words = words(language);
    let names = crate::people::Names::load(pool).await?;
    let searches = prepared_searches(&mut tx, now).await?;
    // The email state of the step (§5.2): the switch and transport must let email out, and every
    // link an email carries comes from `base_url` (NTF-010), so without it nothing sends either.
    let emails = if outgoing.enabled {
        let base = settings_store::value(&mut tx, BASE_URL.key)
            .await?
            .and_then(|value| value.as_str().map(str::to_owned))
            .filter(|base| !base.is_empty());
        match base {
            Some(base) => {
                let zone = settings_store::value(&mut tx, DEFAULT_TIME_ZONE.key)
                    .await?
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "UTC".to_owned());
                let format = settings_store::value(&mut tx, DEFAULT_DATE_FORMAT.key)
                    .await?
                    .and_then(|value| serde_json::from_value::<DateFormat>(value).ok())
                    .unwrap_or(DateFormat::Locale);
                Some(Emails {
                    host: crate::mail::Letters::site(&base),
                    at: zoned(now, &zone, language, format),
                    base,
                })
            }
            None => None,
        }
    } else {
        None
    };
    let mut delivered = 0;
    let mut events: Vec<NewEvent> = Vec::new();
    let ids: Vec<i64> = batches.iter().map(|b| b.id).collect();
    for batch in &batches {
        delivered += fire(
            &mut tx,
            batch,
            &words,
            &names,
            &searches,
            emails.as_ref(),
            &mut events,
        )
        .await?;
    }
    store::drop_pending(&mut tx, &ids).await?;
    live_store::publish(&mut tx, &events).await?;
    tx.commit().await?;
    Ok(delivered)
}

/// The email side of one sweep step (plan §5.2): where links point, the host threading IDs use
/// (NTF-064) and the fired time in the instance time zone.
struct Emails {
    base: String,
    host: String,
    at: String,
}

/// A moment in words, in the instance default time zone (§5.2: per-recipient zones are NTF-067,
/// P2); UTC when the zone does not resolve. The date follows the instance's date format
/// (ADM-016): `Locale` keeps the language-neutral stamp with its offset, a pattern spells the
/// moment the way the web shows it, month names in the instance language.
fn zoned(now: OffsetDateTime, zone: &str, language: Language, format: DateFormat) -> String {
    const MONTHS_EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const MONTHS_RU: [&str; 12] = [
        "янв.",
        "февр.",
        "мар.",
        "апр.",
        "мая",
        "июн.",
        "июл.",
        "авг.",
        "сент.",
        "окт.",
        "нояб.",
        "дек.",
    ];
    let timestamp = jiff::Timestamp::new(now.unix_timestamp(), now.nanosecond() as i32)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    let zone = jiff::tz::TimeZone::get(zone).unwrap_or(jiff::tz::TimeZone::UTC);
    let at = timestamp.to_zoned(zone);
    let datetime = at.datetime();
    let (year, month, day) = (datetime.year(), datetime.month(), datetime.day());
    let (hour, minute) = (datetime.hour(), datetime.minute());
    match format {
        DateFormat::Locale => format!(
            "{}-{:02}-{:02} {:02}:{:02} {}",
            year,
            month,
            day,
            hour,
            minute,
            at.offset()
        ),
        DateFormat::DmyDot => format!("{day:02}.{month:02}.{year}, {hour:02}:{minute:02}"),
        DateFormat::DmyName => {
            let months = match language {
                Language::En => &MONTHS_EN,
                Language::Ru => &MONTHS_RU,
            };
            let name = months[(month - 1) as usize];
            format!("{day} {name} {year}, {hour:02}:{minute:02}")
        }
        DateFormat::Iso => format!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}"),
    }
}

/// One `inbox:` event of a written row (plan §3.6): what the reader's popup shows without
/// fetching.
#[allow(clippy::too_many_arguments)]
fn inbox_event(
    reader: UserId,
    row: i64,
    kind: &str,
    issue: Option<String>,
    author: &str,
    excerpt: &str,
    reasons: &[String],
) -> NewEvent {
    NewEvent {
        kind: EntityKind::Inbox,
        entity: reader,
        version: row,
        change: Change::Created,
        issues: Vec::new(),
        projects: Vec::new(),
        boards: Vec::new(),
        patch: Some(Patch::Notification {
            id: row,
            event: kind.to_owned(),
            issue,
            author: author.to_owned(),
            excerpt: excerpt.to_owned(),
            reasons: reasons.to_vec(),
        }),
        guard: Guard::user(reader),
    }
}

/// Fires one batch: resolves its recipients (plan §3.3), renders for each and writes their rows;
/// with email on it also queues each recipient's email inside this transaction (§5.2). Answers
/// how many rows it wrote; every written row also queues its `inbox:` event.
async fn fire(
    conn: &mut PgConnection,
    batch: &store::Pending,
    words: &Words,
    names: &crate::people::Names,
    searches: &[PreparedSearch],
    emails: Option<&Emails>,
    events: &mut Vec<NewEvent>,
) -> Result<u64> {
    let facts = match live_store::issue_facts(&mut *conn, &[batch.issue_id])
        .await?
        .into_iter()
        .next()
    {
        Some(facts) => facts,
        // The issue is gone; its pending rows went with it (on delete cascade).
        None => return Ok(0),
    };
    let Some(issue) = issues::get(&mut *conn, batch.issue_id).await? else {
        return Ok(0);
    };
    let Some(project) = projects::get(&mut *conn, issue.project_id).await? else {
        return Ok(0);
    };
    // A draft takes no notifications (NTF-041); moves keep the issue's id, so the reference
    // always names the issue it became.
    if issue.is_draft() {
        return Ok(0);
    }
    let brief = IssueRef {
        readable_id: taskmaster_domain::issues::readable_id(
            &project.key,
            issue.number.unwrap_or(0),
        ),
        summary: issue.summary.clone(),
    };
    let candidates = candidates(conn, batch, &facts, searches).await?;
    if candidates.is_empty() {
        return Ok(0);
    }
    let people: Vec<UserId> = candidates.keys().copied().collect();
    let accounts: Vec<User> = users::get_many(&mut *conn, &people)
        .await?
        .into_iter()
        .filter(|account| account.deleted_at.is_none())
        .collect();
    let compiled = crate::access::compile_many(&mut *conn, &accounts).await?;
    let author = users::get_many(&mut *conn, &[batch.author_id])
        .await?
        .into_iter()
        .next();
    let payload: Vec<Payload> = serde_json::from_value(batch.payload.clone())
        .map_err(|e| Error::Internal(format!("a pending payload does not read: {e}")))?;
    // The email facts of the batch, resolved once (§5.2): each recipient's email settings, the
    // reasons in words (NTF-063) and, for a new item, its description and tags.
    let stored = store::settings_of(&mut *conn, &people).await?;
    let wants_email = |user: UserId| -> (bool, bool) {
        stored
            .iter()
            .find(|(id, _)| *id == user)
            .map(|(_, settings)| (settings.email, settings.email_html))
            .unwrap_or((true, true))
    };
    let reason_words = match emails {
        Some(_) => reason_texts(conn, &candidates, words).await?,
        None => HashMap::new(),
    };
    let new_item = match emails {
        Some(_) if batch.kind == Kind::Created => {
            let tag_ids = store::issue_tags(&mut *conn, facts.id).await?;
            let tags: Vec<String> = tags::get_many(&mut *conn, &tag_ids)
                .await?
                .into_iter()
                .map(|tag| tag.name)
                .collect();
            Some(crate::mail::NewItem {
                description_html: issue.description_html.as_str(),
                description_text: issue.description.as_str(),
                tags,
            })
        }
        _ => None,
    };
    let mut written = 0;
    for (account, access) in accounts.iter().zip(compiled) {
        let reasons = &candidates.get(&account.id).cloned().unwrap_or_default();
        if reasons.is_empty() {
            continue;
        }
        // The author's name as this reader may see it (ACC-017); the event carries it because
        // the popup shows the row without fetching.
        let author_name = author
            .as_ref()
            .map(|a| names.show(&access, a).full_name)
            .unwrap_or_default();
        let guard = Guard::issue(&facts.rule());
        if guard.allows(&access) {
            let content = render(
                conn,
                &payload,
                &brief,
                &facts,
                &access,
                issue.project_id,
                words,
            )
            .await?;
            let row = store::deliver(
                conn,
                account.id,
                Some(batch.issue_id),
                batch.author_id,
                batch.kind,
                &reason_keys(reasons),
                &serde_json::to_value(&content).map_err(|e| Error::Internal(e.to_string()))?,
                false,
            )
            .await?;
            store::clear_last(conn, account.id, batch.issue_id).await?;
            events.push(inbox_event(
                account.id,
                row,
                batch.kind.key(),
                Some(brief.readable_id.clone()),
                &author_name,
                &content.excerpt,
                &reason_keys(reasons),
            ));
            // The email of this row, when the person asks for the channel and can be reached
            // (§5.2); queued inside the sweep's transaction, delivered by the `mail.send` job.
            if let Some(emails) = emails {
                let (wants, html) = wants_email(account.id);
                let address = account.email.clone().unwrap_or_default();
                if wants && !address.is_empty() {
                    let first = store::first_id(&mut *conn, account.id, batch.issue_id).await?;
                    let threading = crate::mail::Threading {
                        message_id: format!("<notification-{row}@{}>", emails.host),
                        first: first
                            .filter(|id| *id != row)
                            .map(|id| format!("<notification-{id}@{}>", emails.host)),
                    };
                    let reason_list: Vec<String> = reasons
                        .iter()
                        .filter_map(|reason| reason_words.get(reason).cloned())
                        .collect();
                    let mail = crate::mail::notification(crate::mail::NotificationEmail {
                        to: &address,
                        kind: batch.kind,
                        content: &content,
                        author: &author_name,
                        at: &emails.at,
                        reasons: &reason_list,
                        new_item: new_item.clone(),
                        html,
                        base: &emails.base,
                        words,
                        threading,
                    });
                    crate::mail::queue(conn, &mail, taskmaster_domain::Actor::System).await?;
                }
            }
            written += 1;
        } else if batch.kind == Kind::Updated
            && !store::last_exists(&mut *conn, account.id, batch.issue_id).await?
        {
            // Access is gone and this update is the first the person hears of it: one final row,
            // then silence until access returns (NTF-043). No email: it would name an issue the
            // person can no longer read.
            let content = stopped_content(&brief, words);
            let row = store::deliver(
                conn,
                account.id,
                Some(batch.issue_id),
                batch.author_id,
                batch.kind,
                &reason_keys(reasons),
                &serde_json::to_value(&content).map_err(|e| Error::Internal(e.to_string()))?,
                true,
            )
            .await?;
            events.push(inbox_event(
                account.id,
                row,
                batch.kind.key(),
                Some(brief.readable_id.clone()),
                &author_name,
                &content.excerpt,
                &reason_keys(reasons),
            ));
            written += 1;
        }
    }
    Ok(written)
}

/// The reasons a batch carries as the email footer words them (NTF-063): every reason in words,
/// tag and search sources by name. Resolved once per batch, shown per recipient.
async fn reason_texts(
    conn: &mut PgConnection,
    candidates: &HashMap<UserId, Vec<Reason>>,
    words: &Words,
) -> Result<HashMap<Reason, String>> {
    let mut tag_ids: Vec<TagId> = Vec::new();
    let mut search_ids: Vec<SavedSearchId> = Vec::new();
    for reasons in candidates.values() {
        for reason in reasons {
            match reason {
                Reason::Tag(id) => tag_ids.push(*id),
                Reason::Search(id) => search_ids.push(*id),
                _ => {}
            }
        }
    }
    tag_ids.sort_unstable();
    tag_ids.dedup();
    search_ids.sort_unstable();
    search_ids.dedup();
    let tags: HashMap<TagId, String> = tags::get_many(&mut *conn, &tag_ids)
        .await?
        .into_iter()
        .map(|tag| (tag.id, tag.name))
        .collect();
    let searches: HashMap<SavedSearchId, String> =
        saved_searches::get_many(&mut *conn, &search_ids)
            .await?
            .into_iter()
            .map(|search| (search.id, search.name))
            .collect();
    let mut out = HashMap::new();
    out.insert(Reason::Watch, words.reason_watch.to_owned());
    out.insert(Reason::Mention, words.reason_mention.to_owned());
    for id in tag_ids {
        let name = tags.get(&id).cloned().unwrap_or_default();
        out.insert(Reason::Tag(id), words.reason_tag.replace("{name}", &name));
    }
    for id in search_ids {
        let name = searches.get(&id).cloned().unwrap_or_default();
        out.insert(
            Reason::Search(id),
            words.reason_search.replace("{name}", &name),
        );
    }
    Ok(out)
}

/// Who may hear about this batch and why (plan §3.3): watchers by their watch kinds, tag and
/// saved-search subscribers by their kinds, mentions by the mentions setting — the author only
/// with `own_changes`, everyone only while they can still be reached.
async fn candidates(
    conn: &mut PgConnection,
    batch: &store::Pending,
    facts: &live_store::IssueFacts,
    searches: &[PreparedSearch],
) -> Result<HashMap<UserId, Vec<Reason>>> {
    let mut found: HashMap<UserId, Vec<Reason>> = HashMap::new();
    let push = |map: &mut HashMap<UserId, Vec<Reason>>, user: UserId, reason: Reason| {
        let list = map.entry(user).or_default();
        if !list.contains(&reason) {
            list.push(reason);
        }
    };
    // Watchers (their settings filter the kind).
    let watchers = store::issue_watchers(&mut *conn, facts.id).await?;
    // Tag subscribers of the issue's tags.
    let tags = store::issue_tags(&mut *conn, facts.id).await?;
    let tag_subs = store::subscriptions_of_tags(&mut *conn, &tags).await?;
    // Saved-search subscribers whose search matches the issue: the searches arrive resolved for
    // the whole sweep step, so only the issue's match is per batch.
    let mut search_subs: Vec<(UserId, Reason)> = Vec::new();
    for prepared in searches {
        if !prepared.kinds.contains(&batch.kind) {
            continue;
        }
        let holds = search_store::holds_each(
            &mut *conn,
            batch.issue_id,
            &[&prepared.filter],
            &prepared.vis,
        )
        .await?
        .first()
        .copied()
        .unwrap_or(false);
        if holds {
            search_subs.push((prepared.user, Reason::Search(prepared.search)));
        }
    }
    // Everyone whose settings must be checked, with the author's own-changes flag among them.
    let mut everyone: Vec<UserId> = Vec::new();
    everyone.extend(watchers.iter().copied());
    everyone.extend(tag_subs.iter().map(|s| s.user_id));
    everyone.extend(search_subs.iter().map(|(user, _)| *user));
    everyone.extend(batch.mentions.iter().copied());
    everyone.push(batch.author_id);
    everyone.sort_unstable();
    everyone.dedup();
    let stored = store::settings_of(&mut *conn, &everyone).await?;
    let settings = |user: UserId| -> Settings {
        stored
            .iter()
            .find(|(id, _)| *id == user)
            .map(|(_, s)| *s)
            .unwrap_or_default()
    };
    for user in watchers {
        if settings(user).watch_allows(batch.kind) {
            push(&mut found, user, Reason::Watch);
        }
    }
    for sub in tag_subs {
        if sub.kinds.contains(&batch.kind)
            && let Some(tag) = sub.tag_id
        {
            push(&mut found, sub.user_id, Reason::Tag(tag));
        }
    }
    for (user, reason) in search_subs {
        push(&mut found, user, reason);
    }
    for user in batch.mentions.iter().copied() {
        if settings(user).mentions {
            push(&mut found, user, Reason::Mention);
        }
    }
    // The author hears their own change only with `own_changes` on (NTF-030).
    if !settings(batch.author_id).own_changes {
        found.remove(&batch.author_id);
    }
    Ok(found)
}

/// A saved-search subscription resolved for one sweep step (plan §3.3): the subscriber, the
/// kinds it delivers and the search's filter under the subscriber's access. Resolving loads the
/// search catalog, which answers the same for every batch of the step — per batch it made a step
/// with many batches and many searches crawl.
struct PreparedSearch {
    user: UserId,
    kinds: Vec<Kind>,
    search: SavedSearchId,
    filter: Filter,
    vis: Visibility,
}

/// Resolves every search subscription once for the step. A deleted subscriber, a search that no
/// longer reads or one that no longer resolves skips its subscription, as before.
async fn prepared_searches(
    conn: &mut PgConnection,
    now: OffsetDateTime,
) -> Result<Vec<PreparedSearch>> {
    let mut out = Vec::new();
    for sub in store::search_subscriptions(&mut *conn).await? {
        let Some(account) = users::get(&mut *conn, sub.user_id).await? else {
            continue;
        };
        if account.deleted_at.is_some() {
            continue;
        }
        let Some(row) =
            saved_searches::get(&mut *conn, sub.saved_search_id.unwrap_or_default()).await?
        else {
            continue;
        };
        let Ok(stored) = crate::saved_searches::decode(&row.query) else {
            continue;
        };
        let access = crate::access::compile(&mut *conn, &account).await?;
        let input = crate::search::SearchInput {
            text: String::new(),
            project: None,
            tag: None,
            saved_search: None,
            hide_resolved: false,
            time_zone: None,
            cursor: None,
            limit: 1,
            now: Some(now),
            manual_order: None,
        };
        let Ok(prepared) = crate::search::resolve_parsed(
            &mut *conn,
            &access,
            &input,
            &stored.query,
            false,
            None,
            crate::search::QueryBoards::Expand,
        )
        .await
        else {
            continue;
        };
        out.push(PreparedSearch {
            user: sub.user_id,
            kinds: sub.kinds.clone(),
            search: sub.saved_search_id.unwrap_or_default(),
            filter: prepared.filter,
            vis: prepared.vis,
        });
    }
    Ok(out)
}

/// Renders the batch for one recipient (plan §3.5): what they may see of the change. Private
/// fields are filtered as issue views filter them (FLD-062), comments they cannot see are left
/// out with their files shown as a count, and every line speaks the instance language.
async fn render(
    conn: &mut PgConnection,
    payload: &[Payload],
    brief: &IssueRef,
    facts: &live_store::IssueFacts,
    access: &Access,
    project: ProjectId,
    words: &Words,
) -> Result<Content> {
    let fields = crate::issues::values::of_project(&mut *conn, project).await?;
    let private = |id: FieldId| fields.0.iter().any(|f| f.id() == id && f.private());
    let private_read = rights::may_read_private(access, project);
    let comment_ids: Vec<CommentId> = payload
        .iter()
        .filter_map(|line| match line {
            Payload::Comment { id } => Some(*id),
            Payload::Files { of: Some(id), .. } => Some(*id),
            _ => None,
        })
        .collect();
    let comment_facts = live_store::comment_facts(&mut *conn, &comment_ids).await?;
    let seen = |id: CommentId| -> bool {
        comment_facts
            .iter()
            .find(|c| c.id == id)
            .is_some_and(|c| Guard::comment(&facts.rule(), &c.rule()).allows(access))
    };
    let texts = comment_texts(conn, &comment_ids).await?;
    let excerpt_of = |id: CommentId| -> String {
        texts
            .iter()
            .find(|(cid, _)| *cid == id)
            .map(|(_, text)| short(text))
            .unwrap_or_default()
    };
    let mut lines = Vec::new();
    for line in payload {
        let rendered = match line {
            Payload::Field { id, name, old, new } => {
                if private(*id) && !private_read {
                    None
                } else {
                    Some(Line::Field {
                        name: name.clone(),
                        old: old.clone(),
                        new: new.clone(),
                        text: false,
                    })
                }
            }
            Payload::Summary { old, new } => Some(Line::Field {
                name: words.summary.to_owned(),
                old: old.clone(),
                new: new.clone(),
                text: true,
            }),
            Payload::Description { old, new } => Some(Line::Field {
                name: words.description.to_owned(),
                old: old.clone(),
                new: new.clone(),
                text: true,
            }),
            Payload::Comment { id } => seen(*id).then(|| Line::Comment {
                id: *id,
                excerpt: excerpt_of(*id),
            }),
            Payload::Files { of, added, removed } => match of {
                Some(id) if !seen(*id) => {
                    let count = added.len() + removed.len();
                    Some(Line::Text {
                        text: format!("{}: {count}", words.files),
                    })
                }
                _ => Some(Line::Text {
                    text: list_line(words.files, added, removed),
                }),
            },
            Payload::Tags { added, removed } => Some(Line::Text {
                text: list_line(words.tags, added, removed),
            }),
            Payload::Links { added, removed } => Some(Line::Text {
                text: list_line(words.links, added, removed),
            }),
            Payload::Vote { on } => Some(Line::Text {
                text: if *on {
                    words.vote_add.to_owned()
                } else {
                    words.vote_remove.to_owned()
                },
            }),
            Payload::Boards { board, sprint } => Some(Line::Text {
                text: match sprint {
                    Some(sprint) => {
                        format!("{}: {board}, {}: {sprint}", words.boards, words.sprint)
                    }
                    None => format!("{}: {board}", words.boards),
                },
            }),
            Payload::Moved { from, to } => Some(Line::Text {
                text: format!("{}: {from} → {to}", words.moved),
            }),
            Payload::Audience => Some(Line::Text {
                text: words.audience.to_owned(),
            }),
        };
        if let Some(rendered) = rendered {
            lines.push(rendered);
        }
    }
    let excerpt = short(&format!(
        "{}: {}",
        brief.readable_id,
        lines
            .first()
            .map(line_text)
            .unwrap_or_else(|| brief.summary.clone())
    ));
    if excerpt.chars().count() > EXCERPT_MAX {
        // `short` already trimmed to LINE_MAX; keep the excerpt tighter still.
        let cut: String = excerpt.chars().take(EXCERPT_MAX).collect();
        return Ok(Content {
            issue: brief.clone(),
            lines,
            excerpt: format!("{cut}…"),
        });
    }
    Ok(Content {
        issue: brief.clone(),
        lines,
        excerpt,
    })
}

/// The minimal content of a final row: the issue and the stop words (NTF-043).
fn stopped_content(brief: &IssueRef, words: &Words) -> Content {
    let text = words.stopped.to_owned();
    Content {
        issue: brief.clone(),
        lines: vec![Line::Text { text: text.clone() }],
        excerpt: short(&format!("{}: {text}", brief.readable_id)),
    }
}

/// The text of a line, for excerpts.
fn line_text(line: &Line) -> String {
    match line {
        Line::Text { text } => text.clone(),
        Line::Field { name, old, new, .. } => format!("{name}: {old} → {new}"),
        Line::Comment { excerpt, .. } => excerpt.clone(),
    }
}

/// A names list line: «теги: +a, b −c».
fn list_line(word: &str, added: &[String], removed: &[String]) -> String {
    let mut parts = Vec::new();
    if !added.is_empty() {
        parts.push(format!("+{}", added.join(", ")));
    }
    if !removed.is_empty() {
        parts.push(format!("−{}", removed.join(", ")));
    }
    if parts.is_empty() {
        word.to_owned()
    } else {
        format!("{word}: {}", parts.join(" "))
    }
}

/// The texts of comments, for excerpts.
async fn comment_texts(
    conn: &mut PgConnection,
    ids: &[CommentId],
) -> Result<Vec<(CommentId, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(comments_store::get_many(&mut *conn, ids)
        .await?
        .into_iter()
        .map(|comment| (comment.id, comment.text))
        .collect())
}

/// The reason keys of a row (NTF-042): every reason on one notification.
fn reason_keys(reasons: &[Reason]) -> Vec<String> {
    reasons.iter().map(|reason| reason.key()).collect()
}

#[cfg(test)]
mod tests {
    use taskmaster_domain::settings::{DateFormat, Language};
    use time::OffsetDateTime;

    use super::zoned;

    #[test]
    fn email_stamps_follow_the_instance_date_format() {
        // 31 Dec 2000, 23:59 in Berlin.
        let moment = OffsetDateTime::from_unix_timestamp(978_303_540).unwrap();
        let zone = "Europe/Berlin";

        // The language-driven default keeps its offset.
        assert_eq!(
            zoned(moment, zone, Language::En, DateFormat::Locale),
            "2000-12-31 23:59 +01"
        );
        assert_eq!(
            zoned(moment, zone, Language::En, DateFormat::DmyDot),
            "31.12.2000, 23:59"
        );
        // Month names follow the instance language.
        assert_eq!(
            zoned(moment, zone, Language::Ru, DateFormat::DmyName),
            "31 дек. 2000, 23:59"
        );
        assert_eq!(
            zoned(moment, zone, Language::Ru, DateFormat::Iso),
            "2000-12-31 23:59"
        );
    }
}
