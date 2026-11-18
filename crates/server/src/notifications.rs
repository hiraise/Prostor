//! `/users/me/notifications`, `/notifications/subscriptions`, `/notifications` (the inbox),
//! `/notifications/summary`, `/notifications/read` and `/issues/{id}/watchers` (NTF, plan step
//! 1.10 §3.1, §3.5). Rules live in `taskmaster_service::notifications`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use taskmaster_api::accounts::{UserRef, UserRefKind};
use taskmaster_api::error::{ErrorBody, FieldError};
use taskmaster_api::notifications::{
    Inbox, InboxNotification, InboxQuery, InboxSummary, InboxThread, IssueWatchers,
    NewNotificationSubscription, NotificationBody, NotificationChange, NotificationIssue,
    NotificationKind, NotificationLine, NotificationRead, NotificationReport, NotificationSettings,
    NotificationSubscription, NotificationSubscriptionPatch, NotificationSubscriptions,
    SubscriptionSourceKind, WatchersChange,
};
use taskmaster_domain::access::Access;
use taskmaster_domain::auth::Scope;
use taskmaster_domain::notifications::{Folder, Kind, Line, Settings, Source};
use taskmaster_domain::{IssueId, SubscriptionId};
use taskmaster_service::notifications as service;
use taskmaster_service::people::Names;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::{SessionCaller, require_scope};
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::user_ref;

fn kind(kind: Kind) -> NotificationKind {
    match kind {
        Kind::Created => NotificationKind::Created,
        Kind::Updated => NotificationKind::Updated,
        Kind::Resolved => NotificationKind::Resolved,
        Kind::Comment => NotificationKind::Comment,
        Kind::Vote => NotificationKind::Vote,
        Kind::TagAdded => NotificationKind::TagAdded,
        Kind::TagRemoved => NotificationKind::TagRemoved,
        Kind::System => NotificationKind::System,
    }
}

fn kinds(kinds: &[Kind]) -> Vec<NotificationKind> {
    kinds.iter().copied().map(kind).collect()
}

fn parse_kind(kind: NotificationKind) -> Kind {
    match kind {
        NotificationKind::Created => Kind::Created,
        NotificationKind::Updated => Kind::Updated,
        NotificationKind::Resolved => Kind::Resolved,
        NotificationKind::Comment => Kind::Comment,
        NotificationKind::Vote => Kind::Vote,
        NotificationKind::TagAdded => Kind::TagAdded,
        NotificationKind::TagRemoved => Kind::TagRemoved,
        NotificationKind::System => Kind::System,
    }
}

fn parse_kinds(kinds: &[NotificationKind]) -> Vec<Kind> {
    kinds.iter().copied().map(parse_kind).collect()
}

fn parse_source(source: SubscriptionSourceKind, id: i64) -> Source {
    match source {
        SubscriptionSourceKind::Tag => Source::Tag(id),
        SubscriptionSourceKind::Search => Source::Search(id),
    }
}

fn shown(
    row: &taskmaster_store::notifications::Subscription,
    name: &str,
) -> NotificationSubscription {
    let source = row.source();
    NotificationSubscription {
        id: row.id,
        source: match source {
            Source::Tag(_) => SubscriptionSourceKind::Tag,
            Source::Search(_) => SubscriptionSourceKind::Search,
        },
        source_id: match source {
            Source::Tag(tag) => tag,
            Source::Search(search) => search,
        },
        name: name.to_owned(),
        kinds: kinds(&row.kinds),
        created_at: row.created_at,
    }
}

/// The caller's notification settings (NTF-017): the defaults before their first change.
#[utoipa::path(
    get,
    path = "/users/me/notifications",
    operation_id = "get_my_notifications",
    tag = "notifications",
    security(("session" = [])),
    responses(
        (status = OK, description = "The caller's settings", body = NotificationSettings),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn settings(
    State(state): State<AppState>,
    caller: SessionCaller,
) -> Result<Response, ApiError> {
    let found = service::get_settings(&state.db, caller.principal.user).await?;
    Ok(json(StatusCode::OK, found))
}

/// Writes the caller's notification settings whole (NTF-030, NTF-034, NTF-066): the email
/// channel and its form, own changes, mentions, the kinds of the watch subscription.
#[utoipa::path(
    put,
    path = "/users/me/notifications",
    operation_id = "set_my_notifications",
    tag = "notifications",
    security(("session" = [])),
    request_body = NotificationSettings,
    responses(
        (status = OK, description = "The caller's settings as stored", body = NotificationSettings),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "Called with an API token", body = ErrorBody),
    ),
)]
pub async fn set_settings(
    State(state): State<AppState>,
    caller: SessionCaller,
    JsonBody(request): JsonBody<NotificationSettings>,
) -> Result<Response, ApiError> {
    let saved = service::set_settings(
        &state.db,
        caller.principal.user,
        Settings {
            email: request.email,
            email_html: request.email_html,
            own_changes: request.own_changes,
            mentions: request.mentions,
            watch_created: request.watch_created,
            watch_updated: request.watch_updated,
            watch_resolved: request.watch_resolved,
            watch_comment: request.watch_comment,
            watch_vote: request.watch_vote,
            watch_tag: request.watch_tag,
            autowatch_created: request.autowatch_created,
            autowatch_commented: request.autowatch_commented,
            autowatch_assigned: request.autowatch_assigned,
            autowatch_voted: request.autowatch_voted,
        },
    )
    .await?;
    Ok(json(StatusCode::OK, saved))
}

/// The caller's subscriptions on tags and saved searches (NTF-021), oldest first.
#[utoipa::path(
    get,
    path = "/notifications/subscriptions",
    operation_id = "list_notification_subscriptions",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "The caller's subscriptions", body = NotificationSubscriptions),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn subscriptions(
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = service::subscriptions(&state.db, viewer.access.user()).await?;
    Ok(json(
        StatusCode::OK,
        NotificationSubscriptions {
            subscriptions: found.iter().map(|s| shown(&s.row, &s.name)).collect(),
        },
    ))
}

/// Subscribes the caller to a tag or a saved search they can see (NTF-023). A source the caller
/// cannot see answers `forbidden`; kinds the source does not take answer `invalid` (NTF-022).
#[utoipa::path(
    post,
    path = "/notifications/subscriptions",
    operation_id = "create_notification_subscription",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    request_body = NewNotificationSubscription,
    responses(
        (status = CREATED, description = "Created", body = NotificationSubscription),
        (status = BAD_REQUEST, description = "Kinds the source does not take, or already subscribed", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "The source is not visible to the caller", body = ErrorBody),
    ),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewNotificationSubscription>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let created = service::subscribe(
        &state.db,
        &viewer.access,
        parse_source(request.source, request.source_id),
        &parse_kinds(&request.kinds),
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        shown(&created.row, &created.name),
    ))
}

/// Changes the kinds of the caller's subscription (NTF-024): an empty list keeps it stored but
/// delivering nothing.
#[utoipa::path(
    put,
    path = "/notifications/subscriptions/{id}",
    operation_id = "update_notification_subscription",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Subscription ID")),
    request_body = NotificationSubscriptionPatch,
    responses(
        (status = OK, description = "The subscription now", body = NotificationSubscription),
        (status = BAD_REQUEST, description = "Kinds the source does not take", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = NOT_FOUND, description = "No such subscription of the caller", body = ErrorBody),
    ),
)]
pub async fn set_kinds(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<SubscriptionId>,
    JsonBody(request): JsonBody<NotificationSubscriptionPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let updated =
        service::set_kinds(&state.db, &viewer.access, id, &parse_kinds(&request.kinds)).await?;
    Ok(json(StatusCode::OK, shown(&updated.row, &updated.name)))
}

/// Removes the caller's subscription (NTF-024).
#[utoipa::path(
    delete,
    path = "/notifications/subscriptions/{id}",
    operation_id = "delete_notification_subscription",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Subscription ID")),
    responses(
        (status = NO_CONTENT, description = "Removed"),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = NOT_FOUND, description = "No such subscription of the caller", body = ErrorBody),
    ),
)]
pub async fn remove(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<SubscriptionId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service::remove(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Who watches an issue (NTF-015): people, as the audience candidates endpoint answers them.
/// Needs `issue.watchers.read` in the issue's project.
#[utoipa::path(
    get,
    path = "/issues/{id}/watchers",
    operation_id = "issue_watchers",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    responses(
        (status = OK, description = "The watchers, in the order they came", body = IssueWatchers),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "No `issue.watchers.read`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn watchers(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = service::watchers(&state.db, &viewer.access, id).await?;
    let names: Names = state.names();
    Ok(json(
        StatusCode::OK,
        IssueWatchers {
            users: found
                .iter()
                .map(|u| user_ref(&names, &viewer.access, u))
                .collect(),
        },
    ))
}

/// Replaces the watcher list (NTF-016): the whole new list, the caller included. Moving anyone
/// but the caller needs `issue.watchers.update` in the issue's project.
#[utoipa::path(
    put,
    path = "/issues/{id}/watchers",
    operation_id = "set_issue_watchers",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = WatchersChange,
    responses(
        (status = OK, description = "The watchers now, in the order they came", body = IssueWatchers),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = BAD_REQUEST, description = "No such account in the list", body = ErrorBody),
        (status = FORBIDDEN, description = "Moving others needs `issue.watchers.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn set_watchers(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<IssueId>,
    JsonBody(change): JsonBody<WatchersChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let found = service::set_watchers(&state.db, &viewer.access, id, change.users).await?;
    let names: Names = state.names();
    Ok(json(
        StatusCode::OK,
        IssueWatchers {
            users: found
                .iter()
                .map(|u| user_ref(&names, &viewer.access, u))
                .collect(),
        },
    ))
}

/// One inbox row on the wire: the stored row with its author as the caller sees them.
fn inbox_row(names: &Names, viewer: &Access, row: &service::InboxRow) -> InboxNotification {
    InboxNotification {
        id: row.id,
        kind: kind(row.kind),
        issue_id: row.issue_id,
        content: match &row.body {
            service::Body::Issue(content) => NotificationBody::Change(NotificationChange {
                issue: NotificationIssue {
                    readable_id: content.issue.readable_id.clone(),
                    summary: content.issue.summary.clone(),
                },
                lines: content.lines.iter().map(line).collect(),
                excerpt: content.excerpt.clone(),
            }),
            service::Body::System(report) => NotificationBody::Report(NotificationReport {
                job: report.job.clone(),
                state: report.state.clone(),
                message: report.message.clone(),
            }),
        },
        reasons: row.reasons.clone(),
        last: row.last,
        read: row.read,
        author: row
            .author
            .as_ref()
            .map(|author| user_ref(names, viewer, author))
            .unwrap_or_else(|| UserRef {
                id: 0,
                username: String::new(),
                full_name: String::new(),
                avatar_color: 0,
                avatar_url: None,
                kind: UserRefKind::Deleted,
                pseudonym: None,
                disabled: true,
            }),
        created_at: row.created_at,
    }
}

fn line(line: &Line) -> NotificationLine {
    match line {
        Line::Text { text } => NotificationLine::Text { text: text.clone() },
        Line::Field { name, old, new, .. } => NotificationLine::Field {
            name: name.clone(),
            old: old.clone(),
            new: new.clone(),
        },
        Line::Comment { id, excerpt } => NotificationLine::Comment {
            id: *id,
            excerpt: excerpt.clone(),
        },
    }
}

/// The caller's inbox (NTF-048, NTF-049, plan §3.5): without `issue`, thread heads — one per
/// issue, its newest row — with the thread's newest rows inline, newest first, paged by the
/// cursor; with `issue`, that thread's rows. The `system` folder is served only to people who
/// see the administration area.
#[utoipa::path(
    get,
    path = "/notifications",
    operation_id = "list_notifications",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    params(InboxQuery),
    responses(
        (status = OK, description = "A page of the inbox", body = Inbox),
        (status = BAD_REQUEST, description = "An unknown folder or unread value", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
        (status = FORBIDDEN, description = "The system folder without the administration area", body = ErrorBody),
    ),
)]
pub async fn inbox(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<InboxQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let folder = match query.folder.as_deref() {
        None | Some("all") => Folder::All,
        Some("direct") => Folder::Direct,
        Some("subscriptions") => Folder::Subscriptions,
        Some("system") => Folder::System,
        Some(_) => {
            return Err(ApiError::invalid(vec![FieldError::new(
                "folder",
                "unknown",
                "The folder is one of all, direct, subscriptions, system",
            )]));
        }
    };
    let unread_only = match query.unread.as_deref() {
        None => false,
        Some("only") => true,
        Some(_) => {
            return Err(ApiError::invalid(vec![FieldError::new(
                "unread",
                "unknown",
                "unread is only, when read rows are left out",
            )]));
        }
    };
    let page = service::inbox(
        &state.db,
        &viewer.access,
        folder,
        unread_only,
        query.before,
        query.issue,
        query.limit.unwrap_or(25),
    )
    .await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        Inbox {
            threads: page
                .threads
                .iter()
                .map(|thread| InboxThread {
                    head: inbox_row(&names, &viewer.access, &thread.head),
                    latest: thread
                        .latest
                        .iter()
                        .map(|row| inbox_row(&names, &viewer.access, row))
                        .collect(),
                })
                .collect(),
            next: page.next,
        },
    ))
}

/// The unread counts of the inbox folders (the badge, NTF-048).
#[utoipa::path(
    get,
    path = "/notifications/summary",
    operation_id = "notification_summary",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    responses(
        (status = OK, description = "The unread counts", body = InboxSummary),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn summary(State(state): State<AppState>, viewer: Viewer) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = service::summary(&state.db, viewer.access.user()).await?;
    Ok(json(
        StatusCode::OK,
        InboxSummary {
            total: found.total,
            direct: found.direct,
            subscriptions: found.subscriptions,
            system: found.system,
        },
    ))
}

/// Marks rows read or unread (NTF-050): these `ids`, one `issue` thread or `all`; answers the
/// new unread counts and tells the caller's open tabs over the `inbox:` topic.
#[utoipa::path(
    post,
    path = "/notifications/read",
    operation_id = "mark_notifications_read",
    tag = "notifications",
    security(("session" = []), ("token" = [])),
    request_body = NotificationRead,
    responses(
        (status = OK, description = "The new unread counts", body = InboxSummary),
        (status = BAD_REQUEST, description = "No selector given", body = ErrorBody),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorBody),
    ),
)]
pub async fn read(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NotificationRead>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    if request.ids.is_none() && request.issue.is_none() && !request.all.unwrap_or(false) {
        return Err(ApiError::invalid(vec![FieldError::new(
            "ids",
            "missing",
            "Say what to mark: ids, issue or all",
        )]));
    }
    let marked = service::mark_read(
        &state.db,
        &viewer.access,
        request.ids,
        request.issue,
        request.all.unwrap_or(false),
        request.unread,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        InboxSummary {
            total: marked.total,
            direct: marked.direct,
            subscriptions: marked.subscriptions,
            system: marked.system,
        },
    ))
}
