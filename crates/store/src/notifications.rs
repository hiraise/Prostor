//! Notification settings and subscriptions (NTF, plan step 1.10 §3.1), pending event batches
//! and the inbox (§3.4, §3.5). A person with no settings row has the defaults of
//! `domain::notifications`; the row appears on their first change.

use std::time::Duration;

use prostor_domain::notifications::{Folder, Kind, Settings, Source};
use prostor_domain::{IssueId, SavedSearchId, SubscriptionId, TagId, UserId};
use serde_json::Value as Json;
use sqlx::{PgConnection, PgExecutor};
use time::OffsetDateTime;

/// Both unique keys of `notification_subscriptions` mean the same thing: one subscription per
/// person and source.
pub const SOURCE_KEY: &str = "notification_subscriptions_user_id_tag_id_key";
pub const SEARCH_KEY: &str = "notification_subscriptions_user_id_saved_search_id_key";

/// A subscription row: one person, one source (a tag or a saved search), the kinds it delivers.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub user_id: UserId,
    pub tag_id: Option<TagId>,
    pub saved_search_id: Option<SavedSearchId>,
    pub kinds: Vec<Kind>,
    pub created_at: OffsetDateTime,
}

impl Subscription {
    pub fn source(&self) -> Source {
        match (self.tag_id, self.saved_search_id) {
            (Some(tag), _) => Source::Tag(tag),
            (_, Some(search)) => Source::Search(search),
            // The table's check constraint keeps exactly one of the two set.
            (None, None) => Source::Search(0),
        }
    }
}

/// The stored kinds of a subscription, as the column holds them.
fn stored(kinds: &[Kind]) -> Vec<String> {
    kinds.iter().map(|kind| kind.key().to_owned()).collect()
}

/// A person's settings; `None` before their first change (the defaults).
pub async fn settings(
    db: impl PgExecutor<'_>,
    user: UserId,
) -> Result<Option<Settings>, sqlx::Error> {
    let row = sqlx::query!(
        r#"select email, email_html, own_changes, mentions,
                  watch_created, watch_updated, watch_resolved, watch_comment,
                  watch_vote, watch_tag,
                  autowatch_created, autowatch_commented, autowatch_assigned, autowatch_voted
           from notification_settings where user_id = $1"#,
        user
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| Settings {
        email: r.email,
        email_html: r.email_html,
        own_changes: r.own_changes,
        mentions: r.mentions,
        watch_created: r.watch_created,
        watch_updated: r.watch_updated,
        watch_resolved: r.watch_resolved,
        watch_comment: r.watch_comment,
        watch_vote: r.watch_vote,
        watch_tag: r.watch_tag,
        autowatch_created: r.autowatch_created,
        autowatch_commented: r.autowatch_commented,
        autowatch_assigned: r.autowatch_assigned,
        autowatch_voted: r.autowatch_voted,
    }))
}

/// Writes a person's settings whole; answers them as stored.
pub async fn save_settings(
    conn: &mut PgConnection,
    user: UserId,
    settings: &Settings,
) -> Result<Settings, sqlx::Error> {
    sqlx::query!(
        r#"insert into notification_settings
             (user_id, email, email_html, own_changes, mentions,
              watch_created, watch_updated, watch_resolved, watch_comment, watch_vote, watch_tag,
              autowatch_created, autowatch_commented, autowatch_assigned, autowatch_voted)
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
           on conflict (user_id) do update
             set email = excluded.email, email_html = excluded.email_html,
                 own_changes = excluded.own_changes, mentions = excluded.mentions,
                 watch_created = excluded.watch_created,
                 watch_updated = excluded.watch_updated,
                 watch_resolved = excluded.watch_resolved,
                 watch_comment = excluded.watch_comment,
                 watch_vote = excluded.watch_vote, watch_tag = excluded.watch_tag,
                 autowatch_created = excluded.autowatch_created,
                 autowatch_commented = excluded.autowatch_commented,
                 autowatch_assigned = excluded.autowatch_assigned,
                 autowatch_voted = excluded.autowatch_voted
           returning email, email_html, own_changes, mentions,
                     watch_created, watch_updated, watch_resolved, watch_comment,
                     watch_vote, watch_tag,
                     autowatch_created, autowatch_commented, autowatch_assigned, autowatch_voted"#,
        user,
        settings.email,
        settings.email_html,
        settings.own_changes,
        settings.mentions,
        settings.watch_created,
        settings.watch_updated,
        settings.watch_resolved,
        settings.watch_comment,
        settings.watch_vote,
        settings.watch_tag,
        settings.autowatch_created,
        settings.autowatch_commented,
        settings.autowatch_assigned,
        settings.autowatch_voted,
    )
    .map(|r| Settings {
        email: r.email,
        email_html: r.email_html,
        own_changes: r.own_changes,
        mentions: r.mentions,
        watch_created: r.watch_created,
        watch_updated: r.watch_updated,
        watch_resolved: r.watch_resolved,
        watch_comment: r.watch_comment,
        watch_vote: r.watch_vote,
        watch_tag: r.watch_tag,
        autowatch_created: r.autowatch_created,
        autowatch_commented: r.autowatch_commented,
        autowatch_assigned: r.autowatch_assigned,
        autowatch_voted: r.autowatch_voted,
    })
    .fetch_one(conn)
    .await
}

/// Adds a subscription; the caller has checked the source and the kinds.
pub async fn subscribe(
    conn: &mut PgConnection,
    user: UserId,
    source: Source,
    kinds: &[Kind],
) -> Result<SubscriptionId, sqlx::Error> {
    let (tag, search) = match source {
        Source::Tag(tag) => (Some(tag), None),
        Source::Search(id) => (None, Some(id)),
    };
    sqlx::query_scalar!(
        r#"insert into notification_subscriptions (user_id, tag_id, saved_search_id, kinds)
           values ($1, $2, $3, $4) returning id"#,
        user,
        tag,
        search,
        &stored(kinds) as &[String]
    )
    .fetch_one(conn)
    .await
}

/// One person's subscriptions, oldest first, with the source's current name (the row is gone
/// with its source, so the name is always there).
pub async fn subscriptions_of(
    db: impl PgExecutor<'_>,
    user: UserId,
) -> Result<Vec<(Subscription, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select s.id, s.user_id, s.tag_id, s.saved_search_id, s.kinds, s.created_at,
                  coalesce(t.name, q.name) as "name!"
           from notification_subscriptions s
           left join tags t on t.id = s.tag_id
           left join saved_searches q on q.id = s.saved_search_id
           where s.user_id = $1
           order by s.created_at, s.id"#,
        user
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                Subscription {
                    id: r.id,
                    user_id: r.user_id,
                    tag_id: r.tag_id,
                    saved_search_id: r.saved_search_id,
                    kinds: r.kinds.iter().filter_map(|k| Kind::parse(k)).collect(),
                    created_at: r.created_at,
                },
                r.name,
            )
        })
        .collect())
}

/// One subscription by ID.
pub async fn subscription(
    db: impl PgExecutor<'_>,
    id: SubscriptionId,
) -> Result<Option<Subscription>, sqlx::Error> {
    let row = sqlx::query!(
        r#"select id, user_id, tag_id, saved_search_id, kinds, created_at
           from notification_subscriptions where id = $1"#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| Subscription {
        id: r.id,
        user_id: r.user_id,
        tag_id: r.tag_id,
        saved_search_id: r.saved_search_id,
        kinds: r.kinds.iter().filter_map(|k| Kind::parse(k)).collect(),
        created_at: r.created_at,
    }))
}

/// The person's subscription on one source (NTF-025: the tag and search dialogs answer it).
pub async fn subscription_on(
    db: impl PgExecutor<'_>,
    user: UserId,
    source: Source,
) -> Result<Option<Subscription>, sqlx::Error> {
    let row = sqlx::query!(
        r#"select id, user_id, tag_id, saved_search_id, kinds, created_at
           from notification_subscriptions
           where user_id = $1 and (tag_id = $2 or saved_search_id = $3)"#,
        user,
        match source {
            Source::Tag(tag) => Some(tag),
            Source::Search(_) => None,
        },
        match source {
            Source::Search(id) => Some(id),
            Source::Tag(_) => None,
        },
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| Subscription {
        id: r.id,
        user_id: r.user_id,
        tag_id: r.tag_id,
        saved_search_id: r.saved_search_id,
        kinds: r.kinds.iter().filter_map(|k| Kind::parse(k)).collect(),
        created_at: r.created_at,
    }))
}

/// Changes the kinds of a subscription; `false` when it is gone.
pub async fn set_kinds(
    conn: &mut PgConnection,
    id: SubscriptionId,
    kinds: &[Kind],
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "update notification_subscriptions set kinds = $2 where id = $1",
        id,
        &stored(kinds) as &[String]
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Removes a subscription; `false` when it is gone already.
pub async fn unsubscribe(conn: &mut PgConnection, id: SubscriptionId) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!("delete from notification_subscriptions where id = $1", id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Who watches an issue, in the order they came (NTF-015).
pub async fn issue_watchers(
    db: impl PgExecutor<'_>,
    issue: IssueId,
) -> Result<Vec<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select user_id from issue_watchers where issue_id = $1 order by created_at, user_id",
        issue
    )
    .fetch_all(db)
    .await
}

/// The settings of several people at once; those with no row are absent and take the defaults.
pub async fn settings_of(
    db: impl PgExecutor<'_>,
    users: &[UserId],
) -> Result<Vec<(UserId, Settings)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select user_id, email, email_html, own_changes, mentions,
                  watch_created, watch_updated, watch_resolved, watch_comment,
                  watch_vote, watch_tag,
                  autowatch_created, autowatch_commented, autowatch_assigned, autowatch_voted
           from notification_settings where user_id = any($1)"#,
        users
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.user_id,
                Settings {
                    email: r.email,
                    email_html: r.email_html,
                    own_changes: r.own_changes,
                    mentions: r.mentions,
                    watch_created: r.watch_created,
                    watch_updated: r.watch_updated,
                    watch_resolved: r.watch_resolved,
                    watch_comment: r.watch_comment,
                    watch_vote: r.watch_vote,
                    watch_tag: r.watch_tag,
                    autowatch_created: r.autowatch_created,
                    autowatch_commented: r.autowatch_commented,
                    autowatch_assigned: r.autowatch_assigned,
                    autowatch_voted: r.autowatch_voted,
                },
            )
        })
        .collect())
}

/// The tags of an issue, for tag-subscription candidates.
pub async fn issue_tags(
    db: impl PgExecutor<'_>,
    issue: IssueId,
) -> Result<Vec<TagId>, sqlx::Error> {
    sqlx::query_scalar!(
        "select tag_id from issue_tags where issue_id = $1 order by tag_id",
        issue
    )
    .fetch_all(db)
    .await
}

/// Subscriptions on these tags (plan §3.3); the caller checks the kinds.
pub async fn subscriptions_of_tags(
    db: impl PgExecutor<'_>,
    tags: &[TagId],
) -> Result<Vec<Subscription>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, user_id, tag_id, saved_search_id, kinds, created_at
           from notification_subscriptions where tag_id = any($1) order by id"#,
        tags
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            subscription_row(
                r.id,
                r.user_id,
                r.tag_id,
                r.saved_search_id,
                r.kinds,
                r.created_at,
            )
        })
        .collect())
}

/// Every saved-search subscription (plan §3.3): few, and each must be matched against the issue.
pub async fn search_subscriptions(
    db: impl PgExecutor<'_>,
) -> Result<Vec<Subscription>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, user_id, tag_id, saved_search_id, kinds, created_at
           from notification_subscriptions where saved_search_id is not null order by id"#
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            subscription_row(
                r.id,
                r.user_id,
                r.tag_id,
                r.saved_search_id,
                r.kinds,
                r.created_at,
            )
        })
        .collect())
}

/// One pending event batch (plan §3.4): an author's change of one kind on one issue.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: i64,
    pub issue_id: IssueId,
    pub author_id: UserId,
    pub kind: Kind,
    pub payload: Json,
    pub mentions: Vec<UserId>,
    pub created_at: OffsetDateTime,
    pub due_at: OffsetDateTime,
}

/// Records one event (plan §3.2) in the writer's transaction. An `updated` event merges into the
/// author's pending batch on the same issue: payload appended, mentions joined, the window
/// restarted (NTF-044); every other kind is its own row, due at once.
pub async fn record_event(
    conn: &mut PgConnection,
    issue: IssueId,
    author: UserId,
    kind: Kind,
    payload: &Json,
    mentions: &[UserId],
    due_at: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    if kind == Kind::Updated {
        let merged = sqlx::query_scalar!(
            r#"update notification_pending
                  set payload = payload || $3::jsonb,
                      mentions = (select array(select distinct unnest(mentions || $4))),
                      due_at = $5
                where issue_id = $1 and author_id = $2 and kind = 'updated'
                returning id"#,
            issue,
            author,
            payload,
            mentions,
            due_at
        )
        .fetch_optional(&mut *conn)
        .await?;
        if merged.is_some() {
            return Ok(());
        }
    }
    sqlx::query!(
        r#"insert into notification_pending (issue_id, author_id, kind, payload, mentions, due_at)
           values ($1, $2, $3, $4, $5, $6)"#,
        issue,
        author,
        kind.key(),
        payload,
        mentions,
        due_at
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The issues that have due batches (plan §3.4), read without locking anything.
pub async fn due_issues(
    conn: &mut PgConnection,
    now: OffsetDateTime,
) -> Result<Vec<IssueId>, sqlx::Error> {
    let ids = sqlx::query_scalar!(
        "select distinct issue_id from notification_pending where due_at <= $1",
        now
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// Key-share locks on these issues, in id order, skipping the busy ones. Every change writer
/// locks its issue before it touches `notification_pending`, and the sweep's inbox rows carry the
/// issue as a foreign key, so the sweep locks the issues first: the other order lets a fired
/// batch and a committing change wait for each other (seen as a deadlock in the bench, 1.10c).
/// `for key share` leaves room for a concurrent reader; a writer's `for update` makes the issue
/// skip — its batches stay due for the next step.
pub async fn lock_issues(
    conn: &mut PgConnection,
    issues: &[IssueId],
) -> Result<Vec<IssueId>, sqlx::Error> {
    let ids = sqlx::query_scalar!(
        "select id from issues where id = any($1) order by id for key share skip locked",
        issues
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// The batches whose window has passed, oldest first (plan §3.4), among `issues` — the issues
/// the step has already locked with [`lock_issues`].
pub async fn due_batches(
    conn: &mut PgConnection,
    now: OffsetDateTime,
    issues: &[IssueId],
    limit: i64,
) -> Result<Vec<Pending>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, issue_id, author_id, kind, payload, mentions, created_at, due_at
           from notification_pending where due_at <= $1 and issue_id = any($2)
           order by due_at, id limit $3
           for update skip locked"#,
        now,
        issues,
        limit
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            pending_row(
                r.id,
                r.issue_id,
                r.author_id,
                r.kind,
                r.payload,
                r.mentions,
                r.created_at,
                r.due_at,
            )
        })
        .collect())
}

/// The other authors' pending `updated` batches of these issues, flushed with a fired batch
/// (NTF-044), each as its own notification.
pub async fn flushed_updates(
    conn: &mut PgConnection,
    issues: &[IssueId],
    skip: &[i64],
) -> Result<Vec<Pending>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, issue_id, author_id, kind, payload, mentions, created_at, due_at
           from notification_pending
           where kind = 'updated' and issue_id = any($1) and not (id = any($2))
           order by id
           for update"#,
        issues,
        skip
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            pending_row(
                r.id,
                r.issue_id,
                r.author_id,
                r.kind,
                r.payload,
                r.mentions,
                r.created_at,
                r.due_at,
            )
        })
        .collect())
}

/// Drops the fired batches, in the sweep's transaction (plan §3.4).
pub async fn drop_pending(conn: &mut PgConnection, ids: &[i64]) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from notification_pending where id = any($1)", ids)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// One delivered row of the inbox (plan §3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: i64,
    pub user_id: UserId,
    pub issue_id: Option<IssueId>,
    pub author_id: UserId,
    pub kind: Kind,
    pub reasons: Vec<String>,
    pub content: Json,
    pub last: bool,
    pub read_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

/// Writes one rendered row, in the sweep's transaction. `issue` is empty only for `system` rows.
#[allow(clippy::too_many_arguments)]
pub async fn deliver(
    conn: &mut PgConnection,
    user: UserId,
    issue: Option<IssueId>,
    author: UserId,
    kind: Kind,
    reasons: &[String],
    content: &Json,
    last: bool,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"insert into notifications
             (user_id, issue_id, author_id, kind, reasons, content, last)
           values ($1, $2, $3, $4, $5, $6, $7) returning id"#,
        user,
        issue,
        author,
        kind.key(),
        reasons,
        content,
        last
    )
    .fetch_one(&mut *conn)
    .await
}

/// Whether this person already has the final «наблюдение прекращено» row for the issue (NTF-043).
pub async fn last_exists(
    db: impl PgExecutor<'_>,
    user: UserId,
    issue: IssueId,
) -> Result<bool, sqlx::Error> {
    let found = sqlx::query_scalar!(
        "select exists(select 1 from notifications where user_id = $1 and issue_id = $2 and last)",
        user,
        issue
    )
    .fetch_one(db)
    .await?;
    Ok(found.unwrap_or(false))
}

/// The first notification this person has about the issue (NTF-064): what an email's
/// `In-Reply-To` points at, and the flag that this is the thread's first message.
pub async fn first_id(
    conn: &mut PgConnection,
    user: UserId,
    issue: IssueId,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "select min(id) from notifications where user_id = $1 and issue_id = $2",
        user,
        issue
    )
    .fetch_one(&mut *conn)
    .await
}

/// Access is back: the next ordinary row clears the final-notice flag (NTF-043).
pub async fn clear_last(
    conn: &mut PgConnection,
    user: UserId,
    issue: IssueId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update notifications set last = false where user_id = $1 and issue_id = $2 and last",
        user,
        issue
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A person's rows, newest first; one issue's thread when `issue` is given. The read state and
/// folders of the API (1.10a-3) build on this.
pub async fn notifications_of(
    db: impl PgExecutor<'_>,
    user: UserId,
    issue: Option<IssueId>,
    limit: i64,
) -> Result<Vec<Notification>, sqlx::Error> {
    let rows = sqlx::query_as!(
        NotificationRow,
        r#"select id, user_id, issue_id, author_id, kind, reasons, content, last, read_at, created_at
           from notifications
           where user_id = $1 and ($2::bigint is null or issue_id = $2)
           order by created_at desc, id desc limit $3"#,
        user,
        issue,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(inbox_row).collect())
}

/// Reads one inbox query row; an unknown kind skips it (forward compatibility).
fn inbox_row(r: NotificationRow) -> Option<Notification> {
    Some(Notification {
        id: r.id,
        user_id: r.user_id,
        issue_id: r.issue_id,
        author_id: r.author_id,
        kind: Kind::parse(&r.kind)?,
        reasons: r.reasons,
        content: r.content,
        last: r.last,
        read_at: r.read_at,
        created_at: r.created_at,
    })
}

/// One row of a `notifications` query (the columns every inbox query selects).
#[derive(Debug)]
struct NotificationRow {
    id: i64,
    user_id: UserId,
    issue_id: Option<IssueId>,
    author_id: UserId,
    kind: String,
    reasons: Vec<String>,
    content: Json,
    last: bool,
    read_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
}

/// The inbox's thread heads (NTF-049, plan §3.5): one row per issue — its newest row of the
/// folder — newest first; a `system` row stands alone, its own head. `before` pages by the
/// head's row id.
pub async fn inbox_heads(
    db: impl PgExecutor<'_>,
    user: UserId,
    folder: Folder,
    unread_only: bool,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Notification>, sqlx::Error> {
    let rows = sqlx::query_as!(
        NotificationRow,
        r#"select * from (
               select distinct on (coalesce(issue_id, -id))
                      id, user_id, issue_id, author_id, kind, reasons, content, last, read_at,
                      created_at
               from notifications
               where user_id = $1
                 and ($2::text = 'all'
                      or ($2 = 'direct' and 'mention' = any(reasons))
                      or ($2 = 'subscriptions' and exists(
                              select 1 from unnest(reasons) as r
                              where r like 'tag:%' or r like 'search:%'))
                      or ($2 = 'system' and kind = 'system'))
                 and (not $3::boolean or read_at is null)
                 and ($4::bigint is null or id < $4)
               order by coalesce(issue_id, -id), created_at desc, id desc
               limit $5
           ) heads
           order by created_at desc, id desc"#,
        user,
        folder.key(),
        unread_only,
        before,
        limit
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(inbox_row).collect())
}

/// The `per_issue` newest rows of these threads, newest first (the heads' inline rows, NTF-049).
pub async fn inbox_latest(
    db: impl PgExecutor<'_>,
    user: UserId,
    issues: &[IssueId],
    folder: Folder,
    unread_only: bool,
    per_issue: i64,
) -> Result<Vec<Notification>, sqlx::Error> {
    let rows = sqlx::query_as!(
        NotificationRow,
        r#"select id, user_id, issue_id, author_id, kind, reasons, content, last, read_at,
                  created_at
           from (
               select id, user_id, issue_id, author_id, kind, reasons, content, last, read_at,
                      created_at, row_number() over (
                          partition by issue_id
                          order by created_at desc, id desc) as n
               from notifications
               where user_id = $1
                 and issue_id = any($2)
                 and ($3::text = 'all'
                      or ($3 = 'direct' and 'mention' = any(reasons))
                      or ($3 = 'subscriptions' and exists(
                              select 1 from unnest(reasons) as r
                              where r like 'tag:%' or r like 'search:%'))
                      or ($3 = 'system' and kind = 'system'))
                 and (not $4::boolean or read_at is null)
           ) rows
           where n <= $5
           order by created_at desc, id desc"#,
        user,
        issues,
        folder.key(),
        unread_only,
        per_issue
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(inbox_row).collect())
}

/// The unread counts of the folders (the badge, plan §3.5): total, direct, subscriptions, system.
pub async fn unread_summary(
    db: impl PgExecutor<'_>,
    user: UserId,
) -> Result<(i64, i64, i64, i64), sqlx::Error> {
    let row = sqlx::query!(
        r#"select count(*) filter (where read_at is null) as "total!",
                  count(*) filter (
                      where read_at is null and 'mention' = any(reasons)) as "direct!",
                  count(*) filter (
                      where read_at is null and exists(
                          select 1 from unnest(reasons) as r
                          where r like 'tag:%' or r like 'search:%')) as "subscriptions!",
                  count(*) filter (
                      where read_at is null and kind = 'system') as "system!"
           from notifications
           where user_id = $1"#,
        user
    )
    .fetch_one(db)
    .await?;
    Ok((row.total, row.direct, row.subscriptions, row.system))
}

/// Marks rows read or unread (NTF-050): by ids, by thread (`issue`) or everything (`all`);
/// exactly one selector is passed. Answers how many rows changed.
pub async fn mark_read(
    conn: &mut PgConnection,
    user: UserId,
    ids: Option<&[i64]>,
    issue: Option<IssueId>,
    all: bool,
    unread: bool,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"update notifications
           set read_at = case when $5::boolean then null else now() end
           where user_id = $1
             and ($2::boolean
                  or ($3::bigint[] is not null and id = any($3))
                  or ($4::bigint is not null and issue_id = $4))"#,
        user,
        all,
        ids,
        issue,
        unread
    )
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected())
}

/// Drops rows older than the retention (plan §3.5: 180 days), on the live purge's schedule.
pub async fn purge(db: impl PgExecutor<'_>, older_than: Duration) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "delete from notifications where created_at < now() - make_interval(secs => $1)",
        older_than.as_secs_f64()
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

/// A `system` row (plan §3.7): a job report to its initiator (ADM-031). The content says the
/// job, its state and the message; there is no issue and no author but the reader.
pub async fn system(
    db: impl PgExecutor<'_>,
    user: UserId,
    content: &Json,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"insert into notifications (user_id, issue_id, author_id, kind, reasons, content)
           values ($1, null, $1, 'system', '{}', $2) returning id"#,
        user,
        content
    )
    .fetch_one(db)
    .await
}

/// Reads a subscription row from its columns.
fn subscription_row(
    id: SubscriptionId,
    user_id: UserId,
    tag_id: Option<TagId>,
    saved_search_id: Option<SavedSearchId>,
    kinds: Vec<String>,
    created_at: OffsetDateTime,
) -> Subscription {
    Subscription {
        id,
        user_id,
        tag_id,
        saved_search_id,
        kinds: kinds.iter().filter_map(|k| Kind::parse(k)).collect(),
        created_at,
    }
}

/// Reads a pending row; an unknown kind skips it (forward compatibility).
#[allow(clippy::too_many_arguments)]
fn pending_row(
    id: i64,
    issue_id: IssueId,
    author_id: UserId,
    kind: String,
    payload: Json,
    mentions: Vec<UserId>,
    created_at: OffsetDateTime,
    due_at: OffsetDateTime,
) -> Option<Pending> {
    Some(Pending {
        id,
        issue_id,
        author_id,
        kind: Kind::parse(&kind)?,
        payload,
        mentions,
        created_at,
        due_at,
    })
}
