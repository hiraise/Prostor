//! The live event log on a real database (step 1.6): IDs in commit order, nothing from rolled-back
//! transactions, purging, and the events a batch builds from the facts after a change.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::Bytes;
use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::capability::CapabilitySet;
use trackflow_domain::access::role::PredefinedRole;
use trackflow_domain::access::visibility::Kind;
use trackflow_domain::access::{Access, Grant, Scope};
use trackflow_domain::issues::history::Origin;
use trackflow_domain::issues::values::Input;
use trackflow_domain::links::{BuiltinType, Side};
use trackflow_domain::live::{Change, EntityKind, LINK_VERSION, NewEvent};
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::{FieldId, IssueId, ProjectId, UploadId, ValueId};
use trackflow_files::FileStore;
use trackflow_service::attachments::{self as service_files, AttachmentChange};
use trackflow_service::comments::{self, CommentChange, NewComment};
use trackflow_service::issue_links::{self, NewLinks};
use trackflow_service::issues::jobs::FieldWork;
use trackflow_service::issues::{self, AudienceInput, FieldChange, IssueChange, NewIssue};
use trackflow_service::live::Batch;
use trackflow_service::projects::{self, NewProjectInput};
use trackflow_service::setup::{self, Setup};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Passwords, access, tags, uploads};
use trackflow_store::PgPool;
use trackflow_store::attachments::{self, NewAttachment};
use trackflow_store::links;
use trackflow_store::live::{self as log, Stored};
use trackflow_store::sessions::Origin as SessionOrigin;
use trackflow_testkit::TestDb;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

async fn admin(pool: &PgPool) -> Access {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    let signed_in = setup::run(
        pool,
        &Passwords::new(4),
        Setup {
            code: "TEST-CODE-2345".to_owned(),
            username: "admin".to_owned(),
            full_name: "Anna Admin".to_owned(),
            email: "anna@example.com".to_owned(),
            password: "correct horse".to_owned(),
            system_language: "en".to_owned(),
            base_url: None,
            time_zone: "UTC".to_owned(),
        },
        T0,
        &SessionOrigin::default(),
    )
    .await
    .unwrap();
    access::load(pool, signed_in.user.id).await.unwrap()
}

async fn project(pool: &PgPool, actor: &Access) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Blank,
            name: "Web".to_owned(),
            key: Some("WEB".to_owned()),
            first_number: None,
            description: String::new(),
        },
        Language::En,
    )
    .await
    .unwrap()
    .project
    .id
}

async fn issue(pool: &PgPool, actor: &Access, project: ProjectId, publish: bool) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: "Live".to_owned(),
            publish,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id
}

/// Publishes one issue change in its own transaction.
async fn publish_one(pool: &PgPool, id: IssueId) {
    let mut tx = pool.begin().await.unwrap();
    let mut batch = Batch::new();
    batch.issue(id, Change::Updated);
    batch.publish(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
}

fn entities(events: &[Stored]) -> Vec<(EntityKind, i64, Change)> {
    events
        .iter()
        .map(|s| (s.event.kind, s.event.entity, s.event.change))
        .collect()
}

#[tokio::test]
async fn live_ids_follow_commit_order() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin).await;
    let first = issue(pool, &admin, web, true).await;
    let second = issue(pool, &admin, web, true).await;
    let start = log::head(pool).await.unwrap();

    // A publishes and holds the lock until it commits; B publishes meanwhile and must wait.
    let mut a = pool.begin().await.unwrap();
    let mut b = pool.begin().await.unwrap();
    let mut batch = Batch::new();
    batch.issue(first, Change::Updated);
    batch.publish(&mut a).await.unwrap();
    let a_committing = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&a_committing);
    let finish_a = async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        flag.store(true, Ordering::SeqCst);
        a.commit().await.unwrap();
    };
    let flag = Arc::clone(&a_committing);
    let run_b = async move {
        let mut batch = Batch::new();
        batch.issue(second, Change::Updated);
        batch.publish(&mut b).await.unwrap();
        assert!(
            flag.load(Ordering::SeqCst),
            "B published before A committed"
        );
        b.commit().await.unwrap();
    };
    tokio::join!(finish_a, run_b);

    let events = log::after(pool, start, 10).await.unwrap();
    assert_eq!(
        entities(&events),
        [
            (EntityKind::Issue, first, Change::Updated),
            (EntityKind::Issue, second, Change::Updated),
        ]
    );
    assert!(events[0].id < events[1].id);
    assert_eq!(log::head(pool).await.unwrap(), events[1].id);
    assert!(log::after(pool, events[1].id, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn live_rollback_publishes_nothing() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin).await;
    let id = issue(pool, &admin, web, true).await;
    let start = log::head(pool).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let mut batch = Batch::new();
    batch.issue(id, Change::Updated);
    batch.publish(&mut tx).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(log::head(pool).await.unwrap(), start);
    assert!(log::after(pool, start, 10).await.unwrap().is_empty());

    // The lock went with the transaction; an empty batch writes nothing.
    publish_one(pool, id).await;
    let mut tx = pool.begin().await.unwrap();
    Batch::new().publish(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let events = log::after(pool, start, 10).await.unwrap();
    assert_eq!(
        entities(&events),
        [(EntityKind::Issue, id, Change::Updated)]
    );
}

#[tokio::test]
async fn live_purge_keeps_the_newest() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin).await;
    let id = issue(pool, &admin, web, true).await;
    let start = log::head(pool).await.unwrap();
    for _ in 0..3 {
        publish_one(pool, id).await;
    }
    let head = log::head(pool).await.unwrap();

    assert_eq!(
        log::purge(pool, Duration::from_secs(3600)).await.unwrap(),
        0
    );
    let removed = log::purge(pool, Duration::ZERO).await.unwrap();
    assert!(removed >= 2, "removed {removed}");
    let left = log::after(pool, 0, 10).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, head);
    assert_eq!(log::head(pool).await.unwrap(), head);
    assert!(head > start);
    let recent = log::since_time(pool, OffsetDateTime::now_utc() - Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(recent.iter().map(|s| s.id).collect::<Vec<_>>(), [head]);
}

#[tokio::test]
async fn live_batch_builds_events_from_facts() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin).await;
    let a = issue(pool, &admin, web, true).await;
    let b = issue(pool, &admin, web, true).await;
    let draft = issue(pool, &admin, web, false).await;
    let (comment, _) = comments::create(
        pool,
        &admin,
        WEB,
        a,
        NewComment {
            text: "Seen".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    let comment = comment.comment.id;
    let mut tx = pool.begin().await.unwrap();
    let related = links::type_by_builtin(&mut *tx, BuiltinType::Related.key())
        .await
        .unwrap()
        .unwrap();
    let link = links::insert_many(&mut tx, related.id, &[(a, b)], admin.user())
        .await
        .unwrap()[0]
        .id;
    let file = attachments::insert(
        &mut tx,
        &NewAttachment {
            issue: a,
            comment: Some(comment),
            author: admin.user(),
            name: "notes.txt",
            size: 5,
            content_type: "text/plain",
            sha256: &[0; 32],
            storage_key: "live/notes",
            secret: &[1; 32],
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let start = log::head(pool).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let mut batch = Batch::new();
    batch.issue(a, Change::Created);
    batch.issue(a, Change::Updated);
    batch.issue_moved(b, 999_999);
    batch.issue(draft, Change::Updated);
    batch.comment(comment, Change::Created);
    batch.attachment(file, Change::Created);
    batch.link(link, Change::Created);
    assert!(!batch.is_empty());
    batch.publish(&mut tx).await.unwrap();
    tx.commit().await.unwrap();

    let events = log::after(pool, start, 10).await.unwrap();
    assert_eq!(
        entities(&events),
        [
            (EntityKind::Issue, a, Change::Created),
            (EntityKind::Issue, b, Change::Updated),
            (EntityKind::Comment, comment, Change::Created),
            (EntityKind::Attachment, file, Change::Created),
            (EntityKind::IssueLink, link, Change::Created),
        ],
        "the draft publishes nothing"
    );
    let facts = log::issue_facts(pool, &[a]).await.unwrap();
    assert_eq!(events[0].event.version, facts[0].version);
    assert_eq!(events[0].event.issues, [a]);
    assert_eq!(events[0].event.projects, [web]);
    assert_eq!(events[1].event.projects, [web, 999_999]);
    assert_eq!(events[2].event.issues, [a]);
    let kinds =
        |n: usize| -> Vec<Kind> { events[n].event.guard.items.iter().map(|i| i.kind).collect() };
    assert_eq!(kinds(2), [Kind::Issue, Kind::Comment]);
    assert_eq!(kinds(3), [Kind::Issue, Kind::Comment, Kind::Attachment]);
    assert_eq!(kinds(4), [Kind::Issue, Kind::Issue]);
    assert_eq!(events[4].event.issues, [a, b]);
    assert_eq!(events[4].event.version, LINK_VERSION);
    let stranger = Access::none(admin.user() + 1000);
    for stored in &events {
        assert!(stored.event.guard.allows(&admin));
        assert!(!stored.event.guard.allows(&stranger));
    }

    // A removed link: facts loaded before the row goes.
    let start = log::head(pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let facts = log::link_facts(&mut *tx, &[link]).await.unwrap();
    links::delete(&mut tx, link).await.unwrap();
    let mut batch = Batch::new();
    for facts in facts {
        batch.link_removed(facts);
    }
    batch.publish(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let events = log::after(pool, start, 10).await.unwrap();
    assert_eq!(
        entities(&events),
        [(EntityKind::IssueLink, link, Change::Deleted)]
    );
    assert_eq!(events[0].event.issues, [a, b]);
}

// Events from every writer (L2): each use case of the plan's table, checked against the rule that
// every change of a version (or a new or removed entity) has exactly one event with that version.

const MAX: u64 = 10 * 1024 * 1024;

type Snapshot = BTreeMap<(&'static str, i64), i64>;

/// `(kind, id) → version` of every published issue and what hangs on one: one query per table.
async fn snapshot(pool: &PgPool) -> Snapshot {
    let tables: [(&'static str, &str); 4] = [
        (
            "issue",
            "select id, version from issues where number is not null",
        ),
        (
            "comment",
            "select c.id, c.version from comments c join issues i on i.id = c.issue_id
             where i.number is not null",
        ),
        (
            "attachment",
            "select a.id, a.version from attachments a join issues i on i.id = a.issue_id
             where i.number is not null",
        ),
        (
            "issue_link",
            "select l.id, 1::bigint from issue_links l
                 join issues s on s.id = l.source_id join issues t on t.id = l.target_id
             where s.number is not null and t.number is not null",
        ),
    ];
    let mut all = Snapshot::new();
    for (kind, sql) in tables {
        let rows: Vec<(i64, i64)> = sqlx::query_as(sql).fetch_all(pool).await.unwrap();
        all.extend(rows.into_iter().map(|(id, v)| ((kind, id), v)));
    }
    all
}

/// Watches the log and the versions between use cases.
struct Watch {
    before: Snapshot,
    head: i64,
}

impl Watch {
    async fn new(pool: &PgPool) -> Self {
        Self {
            before: snapshot(pool).await,
            head: log::head(pool).await.unwrap(),
        }
    }

    /// The events written since the last call; asserts they are exactly the changed versions.
    async fn check(&mut self, pool: &PgPool, what: &str) -> Vec<NewEvent> {
        let after = snapshot(pool).await;
        let mut changed: BTreeSet<(&str, i64, i64)> = after
            .iter()
            .filter(|(key, v)| self.before.get(*key) != Some(*v))
            .map(|((kind, id), v)| (*kind, *id, *v))
            .collect();
        changed.extend(
            self.before
                .iter()
                .filter(|(key, _)| !after.contains_key(*key))
                .map(|((kind, id), v)| (*kind, *id, *v)),
        );
        let events: Vec<NewEvent> = log::after(pool, self.head, 1000)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.event)
            .collect();
        let published: BTreeSet<(&str, i64, i64)> = events
            .iter()
            .map(|e| (e.kind.as_str(), e.entity, e.version))
            .collect();
        assert_eq!(
            published.len(),
            events.len(),
            "{what}: one event per entity"
        );
        assert_eq!(published, changed, "{what}: events and changed versions");
        self.before = after;
        self.head = log::head(pool).await.unwrap();
        events
    }
}

fn shape(events: &[NewEvent]) -> Vec<(EntityKind, i64, Change)> {
    events
        .iter()
        .map(|e| (e.kind, e.entity, e.change))
        .collect()
}

struct Scene {
    db: TestDb,
    admin: Access,
    member: Access,
    web: ProjectId,
    app: ProjectId,
    _dir: tempfile::TempDir,
    files: FileStore,
}

impl Scene {
    async fn new() -> Self {
        let db = TestDb::new().await;
        let pool = db.pool();
        let admin = admin(pool).await;
        let web = project(pool, &admin).await;
        let app = projects::create(
            pool,
            &admin,
            NewProjectInput {
                template: Template::Blank,
                name: "App".to_owned(),
                key: Some("APP".to_owned()),
                first_number: None,
                description: String::new(),
            },
            Language::En,
        )
        .await
        .unwrap()
        .project
        .id;
        let id = users::create(
            pool,
            &Passwords::new(4),
            &admin,
            NewAccount {
                username: "mia".to_owned(),
                full_name: "Mia Member".to_owned(),
                email: "mia@example.com".to_owned(),
                password: "correct horse".to_owned(),
                must_change_password: false,
            },
        )
        .await
        .unwrap()
        .id;
        let member = Access::compile(
            id,
            [web, app].map(|p| Grant {
                permissions: PredefinedRole::ProjectMember.permissions(),
                scope: Scope::Project(p),
            }),
            vec![],
            CapabilitySet::default(),
        );
        let dir = tempfile::tempdir().unwrap();
        let files = FileStore::new(dir.path());
        files.prepare().await.unwrap();
        Self {
            db,
            admin,
            member,
            web,
            app,
            _dir: dir,
            files,
        }
    }

    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    async fn upload(&self, name: &str) -> UploadId {
        let chunks: Vec<Result<Bytes, Infallible>> = vec![Ok(Bytes::from_static(b"hello"))];
        uploads::receive(
            self.pool(),
            &self.files,
            &self.admin,
            name,
            futures_util::stream::iter(chunks),
            MAX,
        )
        .await
        .unwrap()
        .id
    }

    async fn view(&self, id: IssueId) -> issues::IssueView {
        issues::get(self.pool(), &self.admin, id).await.unwrap().0
    }

    /// The State field of an issue and a value of it, resolved or not.
    async fn state(&self, id: IssueId, resolved: bool) -> (FieldId, ValueId) {
        let view = self.view(id).await;
        let state = view
            .fields
            .iter()
            .find(|f| f.field.field.name == "State")
            .unwrap();
        let set = state.field.attached.set_id.unwrap();
        let value = trackflow_store::value_sets::values_of(self.pool(), &[set])
            .await
            .unwrap()
            .into_iter()
            .find(|v| v.resolved == resolved)
            .unwrap();
        (state.field.id(), value.id)
    }
}

async fn issue_use_cases(s: &Scene, w: &mut Watch) {
    let pool = s.pool();
    let a = issue(pool, &s.admin, s.web, true).await;
    let events = w.check(pool, "issues::create").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Created)]);
    assert_eq!(events[0].issues, [a]);
    assert_eq!(events[0].projects, [s.web]);

    let draft = issue(pool, &s.admin, s.web, false).await;
    assert!(w.check(pool, "a draft").await.is_empty());
    issues::publish(pool, &s.admin, WEB, draft).await.unwrap();
    let events = w.check(pool, "issues::publish").await;
    assert_eq!(
        shape(&events),
        [(EntityKind::Issue, draft, Change::Created)]
    );

    let text_version = s.view(a).await.issue.text_version;
    issues::update(
        pool,
        &s.admin,
        WEB,
        a,
        IssueChange {
            summary: Some("Live again".to_owned()),
            text_version: Some(text_version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let events = w.check(pool, "issues::update").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Updated)]);

    let (state, fixed) = s.state(a, true).await;
    issues::set_field(
        pool,
        &s.admin,
        WEB,
        a,
        state,
        FieldChange::Set(Some(Input::Integer(fixed))),
        false,
    )
    .await
    .unwrap();
    let events = w.check(pool, "issues::set_field").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Updated)]);

    let updated_at = s.view(a).await.issue.updated_at;
    issues::set_vote(pool, &s.member, a, true).await.unwrap();
    let events = w.check(pool, "issues::set_vote").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Updated)]);
    assert_eq!(
        s.view(a).await.issue.updated_at,
        updated_at,
        "a vote is no edit"
    );
    issues::set_vote(pool, &s.member, a, true).await.unwrap();
    assert!(w.check(pool, "a second vote").await.is_empty());
    // The voter already watches from the vote (ISS-030's auto-watch), so their star adds
    // nothing — and taking it off still announces the watcher change.
    issues::set_star(pool, &s.member, a, true).await.unwrap();
    assert!(w.check(pool, "a star the vote had put").await.is_empty());
    issues::set_star(pool, &s.member, a, false).await.unwrap();
    let events = w.check(pool, "issues::set_star off").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Updated)]);

    let tag = tags::create(pool, &s.admin, "live", None).await.unwrap();
    tags::set_on_issue(pool, &s.admin, WEB, a, tag.id, true)
        .await
        .unwrap();
    let events = w.check(pool, "tags::set_on_issue").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Updated)]);

    issues::moves::move_issue(pool, &s.admin, WEB, a, s.app, false)
        .await
        .unwrap();
    let events = w.check(pool, "issues::moves::move_issue").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, a, Change::Updated)]);
    let mut both = vec![s.web, s.app];
    both.sort_unstable();
    assert_eq!(events[0].projects, both);

    // A job replacing a deleted value: one event per touched issue.
    let b = issue(pool, &s.admin, s.web, true).await;
    let (state, open) = s.state(b, false).await;
    issues::set_field(
        pool,
        &s.admin,
        WEB,
        b,
        state,
        FieldChange::Set(Some(Input::Integer(open))),
        false,
    )
    .await
    .unwrap();
    w.check(pool, "set_field before the job").await;
    let (_, other) = s.state(b, true).await;
    issues::jobs::run(
        pool,
        FieldWork::Replace {
            value: open,
            name: "Open".to_owned(),
            replacement: Some(other),
            actor: s.admin.user(),
        },
    )
    .await
    .unwrap();
    let events = w.check(pool, "issues::jobs").await;
    assert!(shape(&events).contains(&(EntityKind::Issue, b, Change::Updated)));

    issues::delete(pool, &s.admin, WEB, b).await.unwrap();
    let events = w.check(pool, "issues::delete").await;
    assert_eq!(shape(&events), [(EntityKind::Issue, b, Change::Deleted)]);
}

async fn comment_use_cases(s: &Scene, w: &mut Watch) {
    let pool = s.pool();
    let a = issue(pool, &s.admin, s.web, true).await;
    w.check(pool, "an issue").await;
    let upload = s.upload("first.txt").await;
    let (created, _) = comments::create(
        pool,
        &s.admin,
        WEB,
        a,
        NewComment {
            text: "Hello".to_owned(),
            uploads: vec![upload],
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    let comment = created.comment.id;
    let events = w.check(pool, "comments::create").await;
    let kinds: Vec<(EntityKind, Change)> = events.iter().map(|e| (e.kind, e.change)).collect();
    assert_eq!(
        kinds,
        [
            (EntityKind::Issue, Change::Updated),
            (EntityKind::Comment, Change::Created),
            (EntityKind::Attachment, Change::Created),
        ]
    );
    assert_eq!(events[1].entity, comment);
    assert_eq!(events[1].issues, [a]);

    comments::update(
        pool,
        &s.admin,
        WEB,
        comment,
        CommentChange {
            text: Some("Hello again".to_owned()),
            version: created.comment.version,
            ..CommentChange::default()
        },
    )
    .await
    .unwrap();
    let events = w.check(pool, "comments::update").await;
    assert!(shape(&events).contains(&(EntityKind::Comment, comment, Change::Updated)));
    comments::delete(pool, &s.admin, WEB, comment)
        .await
        .unwrap();
    let events = w.check(pool, "comments::delete").await;
    assert!(shape(&events).contains(&(EntityKind::Comment, comment, Change::Updated)));
    comments::restore(pool, &s.admin, WEB, comment)
        .await
        .unwrap();
    let events = w.check(pool, "comments::restore").await;
    assert!(shape(&events).contains(&(EntityKind::Comment, comment, Change::Updated)));
}

async fn attachment_use_cases(s: &Scene, w: &mut Watch) {
    let pool = s.pool();
    let a = issue(pool, &s.admin, s.web, true).await;
    w.check(pool, "an issue").await;
    let upload = s.upload("notes.txt").await;
    let listed = service_files::attach(pool, &s.admin, WEB, a, &[upload], false)
        .await
        .unwrap();
    let file = listed.files[0].file.clone();
    let events = w.check(pool, "attachments::attach").await;
    assert_eq!(
        shape(&events),
        [
            (EntityKind::Issue, a, Change::Updated),
            (EntityKind::Attachment, file.id, Change::Created),
        ]
    );
    service_files::update(
        pool,
        &s.admin,
        file.id,
        AttachmentChange {
            name: Some("renamed.txt".to_owned()),
            audience: None,
            version: file.version,
        },
    )
    .await
    .unwrap();
    let events = w.check(pool, "attachments::update").await;
    assert_eq!(
        shape(&events),
        [(EntityKind::Attachment, file.id, Change::Updated)]
    );
    service_files::delete(pool, &s.admin, WEB, file.id)
        .await
        .unwrap();
    let events = w.check(pool, "attachments::delete").await;
    assert!(shape(&events).contains(&(EntityKind::Attachment, file.id, Change::Updated)));
}

async fn link_use_cases(s: &Scene, w: &mut Watch) {
    let pool = s.pool();
    let a = issue(pool, &s.admin, s.web, true).await;
    let b = issue(pool, &s.admin, s.app, true).await;
    w.check(pool, "two issues").await;
    let related = links::type_by_builtin(pool, BuiltinType::Related.key())
        .await
        .unwrap()
        .unwrap();
    issue_links::add(
        pool,
        &s.admin,
        WEB,
        a,
        NewLinks {
            link_type: related.id,
            side: Side::Source,
            issues: vec![b],
        },
        false,
    )
    .await
    .unwrap();
    let link = links::of_issue(pool, a).await.unwrap()[0].link;
    let events = w.check(pool, "issue_links::add").await;
    assert_eq!(
        shape(&events),
        [
            (EntityKind::Issue, a, Change::Updated),
            (EntityKind::Issue, b, Change::Updated),
            (EntityKind::IssueLink, link, Change::Created),
        ]
    );
    let mut both = vec![s.web, s.app];
    both.sort_unstable();
    assert_eq!(events[2].issues, [a, b]);
    assert_eq!(events[2].projects, both);
    issue_links::remove(pool, &s.admin, WEB, a, link, false)
        .await
        .unwrap();
    let events = w.check(pool, "issue_links::remove").await;
    assert_eq!(
        shape(&events),
        [
            (EntityKind::Issue, a, Change::Updated),
            (EntityKind::Issue, b, Change::Updated),
            (EntityKind::IssueLink, link, Change::Deleted),
        ]
    );
    assert_eq!(events[2].issues, [a, b]);
}

#[tokio::test]
async fn iss_048_issue_events() {
    let s = Scene::new().await;
    let mut w = Watch::new(s.pool()).await;
    issue_use_cases(&s, &mut w).await;
}

#[tokio::test]
async fn iss_048_comment_events() {
    let s = Scene::new().await;
    let mut w = Watch::new(s.pool()).await;
    comment_use_cases(&s, &mut w).await;
}

#[tokio::test]
async fn iss_048_attachment_events() {
    let s = Scene::new().await;
    let mut w = Watch::new(s.pool()).await;
    attachment_use_cases(&s, &mut w).await;
}

#[tokio::test]
async fn iss_048_link_events() {
    let s = Scene::new().await;
    let mut w = Watch::new(s.pool()).await;
    link_use_cases(&s, &mut w).await;
}

#[tokio::test]
async fn iss_048_a_draft_publishes_nothing_until_published() {
    let s = Scene::new().await;
    let pool = s.pool();
    let target = issue(pool, &s.admin, s.web, true).await;
    let mut w = Watch::new(pool).await;
    let related = links::type_by_builtin(pool, BuiltinType::Related.key())
        .await
        .unwrap()
        .unwrap();
    let upload = s.upload("draft.txt").await;
    let draft = issues::create(
        pool,
        &s.admin,
        WEB,
        NewIssue {
            project: s.web,
            summary: "Draft".to_owned(),
            uploads: vec![upload],
            links: vec![NewLinks {
                link_type: related.id,
                side: Side::Source,
                issues: vec![target],
            }],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    let text_version = s.view(draft).await.issue.text_version;
    issues::update(
        pool,
        &s.admin,
        WEB,
        draft,
        IssueChange {
            summary: Some("Draft, edited".to_owned()),
            text_version: Some(text_version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    assert!(w.check(pool, "changes of a draft").await.is_empty());
    issues::publish(pool, &s.admin, WEB, draft).await.unwrap();
    let events = w.check(pool, "publishing").await;
    assert_eq!(
        shape(&events),
        [
            (EntityKind::Issue, target, Change::Updated),
            (EntityKind::Issue, draft, Change::Created),
            (EntityKind::Attachment, events[2].entity, Change::Created),
            (EntityKind::IssueLink, events[3].entity, Change::Created),
        ],
        "the issue, the one its link reaches, its file and link"
    );
}

#[tokio::test]
async fn acc_049_guard_of_a_limited_comment() {
    let s = Scene::new().await;
    let pool = s.pool();
    let a = issue(pool, &s.admin, s.web, true).await;
    let mut w = Watch::new(pool).await;
    comments::create(
        pool,
        &s.admin,
        WEB,
        a,
        NewComment {
            text: "For Mia only".to_owned(),
            audience: Some(AudienceInput {
                users: vec![s.member.user()],
                groups: vec![],
            }),
            ..NewComment::default()
        },
    )
    .await
    .unwrap();
    let events = w.check(pool, "a limited comment").await;
    let outsider = Access::compile(
        s.member.user() + 1000,
        [Grant {
            permissions: PredefinedRole::ProjectMember.permissions(),
            scope: Scope::Project(s.web),
        }],
        vec![],
        CapabilitySet::default(),
    );
    let issue_event = events.iter().find(|e| e.kind == EntityKind::Issue).unwrap();
    let comment = events
        .iter()
        .find(|e| e.kind == EntityKind::Comment)
        .unwrap();
    assert!(issue_event.guard.allows(&outsider), "the issue is open");
    assert!(comment.guard.allows(&s.member), "the audience reads it");
    assert!(!comment.guard.allows(&outsider), "others do not");
}

#[tokio::test]
async fn live_every_version_change_has_an_event() {
    let s = Scene::new().await;
    let mut w = Watch::new(s.pool()).await;
    issue_use_cases(&s, &mut w).await;
    comment_use_cases(&s, &mut w).await;
    attachment_use_cases(&s, &mut w).await;
    link_use_cases(&s, &mut w).await;
}
