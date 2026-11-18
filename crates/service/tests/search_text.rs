//! Full-text search and mentions (QRY-036…QRY-045, CNT-058, QRY-023) through the service on a real
//! database (plan step 1.5 §C4, D-037). Each test names the catalog rows it covers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::issues::values::Input;
use taskmaster_domain::projects::Template;
use taskmaster_domain::search::SortBy;
use taskmaster_domain::settings::Language;
use taskmaster_domain::{FieldId, IssueId, ProjectId, UserId};
use taskmaster_query::ProblemCode;
use taskmaster_service::fields::{self, NewFieldInput, SetSource};
use taskmaster_service::issues::{self, AudienceInput, FieldChange, NewIssue};
use taskmaster_service::project_fields::{self, AttachInput, SetUse};
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::search::{self, SearchInput, SearchPage};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::{Passwords, access, comments, roles};
use taskmaster_store::PgPool;
use taskmaster_store::access::Holder;
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

fn passwords() -> Passwords {
    Passwords::new(4)
}

async fn admin(pool: &PgPool) -> Access {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    let signed_in = setup::run(
        pool,
        &passwords(),
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

async fn create_user(pool: &PgPool, actor: &Access, username: &str) -> UserId {
    users::create(
        pool,
        &passwords(),
        actor,
        NewAccount {
            username: username.to_owned(),
            full_name: format!("{username} Person"),
            email: format!("{username}@example.com"),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id
}

async fn member(pool: &PgPool, actor: &Access, username: &str, of: ProjectId) -> Access {
    let user = create_user(pool, actor, username).await;
    let role = taskmaster_store::roles::builtin_id(pool, PredefinedRole::ProjectMember.key())
        .await
        .unwrap()
        .unwrap();
    roles::assign(pool, actor, role, Holder::User(user), &[Some(of)])
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

/// A person who reads the project's issues but not its private fields.
async fn reader(pool: &PgPool, actor: &Access, of: ProjectId) -> Access {
    let user = create_user(pool, actor, "rita").await;
    let role = roles::create(
        pool,
        actor,
        roles::NewRole {
            name: "Reader".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.read", "comment.read"]
                .map(str::to_owned)
                .to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(pool, actor, role.role.id, Holder::User(user), &[Some(of)])
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn project(pool: &PgPool, actor: &Access, name: &str, key: &str) -> ProjectId {
    projects::create(
        pool,
        actor,
        NewProjectInput {
            template: Template::Blank,
            name: name.to_owned(),
            key: Some(key.to_owned()),
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

async fn issue(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    summary: &str,
    description: &str,
) -> IssueId {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: summary.to_owned(),
            description: description.to_owned(),
            publish: true,
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

/// A comment; `only` limits its audience to these people.
async fn comment(pool: &PgPool, author: &Access, issue: IssueId, text: &str, only: &[UserId]) {
    comments::create(
        pool,
        author,
        WEB,
        issue,
        comments::NewComment {
            text: text.to_owned(),
            audience: (!only.is_empty()).then(|| AudienceInput {
                users: only.to_vec(),
                groups: Vec::new(),
            }),
            ..comments::NewComment::default()
        },
    )
    .await
    .unwrap();
}

/// A text field attached to the project.
async fn text_field(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    name: &str,
    private: bool,
) -> FieldId {
    let field = fields::create(
        pool,
        actor,
        NewFieldInput {
            name: name.to_owned(),
            kind: FieldKind::Text,
            multi: false,
            aliases: Vec::new(),
            set: SetSource::New { name: None },
            can_be_empty: true,
            empty_label: String::new(),
            private,
            default_values: Vec::new(),
            auto_attach: false,
        },
    )
    .await
    .unwrap()
    .field
    .id;
    project_fields::attach(
        pool,
        actor,
        project,
        AttachInput::Existing {
            field,
            set: SetUse::Shared,
        },
    )
    .await
    .unwrap();
    field
}

async fn set_text(pool: &PgPool, actor: &Access, issue: IssueId, field: FieldId, text: &str) {
    issues::set_field(
        pool,
        actor,
        WEB,
        issue,
        field,
        FieldChange::Set(Some(Input::Text(text.to_owned()))),
        false,
    )
    .await
    .unwrap();
}

async fn page(pool: &PgPool, viewer: &Access, text: &str) -> SearchPage {
    search::search(
        pool,
        viewer,
        SearchInput {
            text: text.to_owned(),
            limit: 100,
            ..SearchInput::default()
        },
    )
    .await
    .unwrap_or_else(|e| panic!("{text}: {e:?}"))
}

/// IDs found, in the answer's order.
async fn find(pool: &PgPool, viewer: &Access, text: &str) -> Vec<IssueId> {
    page(pool, viewer, text)
        .await
        .issues
        .issues
        .iter()
        .map(|v| v.issue.id)
        .collect()
}

/// IDs found, sorted.
async fn set(pool: &PgPool, viewer: &Access, text: &str) -> Vec<IssueId> {
    let mut out = find(pool, viewer, text).await;
    out.sort_unstable();
    out
}

fn sorted(mut ids: Vec<IssueId>) -> Vec<IssueId> {
    ids.sort_unstable();
    ids
}

struct World {
    admin: Access,
    web: ProjectId,
}

async fn world(pool: &PgPool) -> World {
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    World { admin, web }
}

#[tokio::test]
async fn qry_036_words_stems_and_or() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let en = issue(pool, a, w.web, "Crashes when loading", "").await;
    let ru = issue(pool, a, w.web, "Ошибки при загрузке", "").await;
    let note = issue(pool, a, w.web, "Slow start", "").await;
    comment(pool, a, note, "Seen after the update", &[]).await;
    // Other forms of the same word (English and Russian stems).
    assert_eq!(set(pool, a, "crash").await, [en]);
    assert_eq!(set(pool, a, "loaded crashing").await, [en]);
    assert_eq!(set(pool, a, "ошибка").await, [ru]);
    assert_eq!(set(pool, a, "загрузка").await, [ru]);
    // Words are AND, across summary and comments too; OR and NOT work.
    assert!(set(pool, a, "crash slow").await.is_empty());
    assert_eq!(set(pool, a, "slow updated").await, [note]);
    assert_eq!(set(pool, a, "crash or ошибки").await, sorted(vec![en, ru]));
    assert_eq!(set(pool, a, "not crash").await, sorted(vec![ru, note]));
}

#[tokio::test]
async fn qry_037_phrases_and_exact_forms() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let one = issue(pool, a, w.web, "Crashes when loading", "").await;
    let two = issue(pool, a, w.web, "Loading crash", "").await;
    // A phrase keeps the order; stop words inside count as gaps.
    assert_eq!(set(pool, a, "\"crashes when loading\"").await, [one]);
    assert_eq!(set(pool, a, "\"loading crash\"").await, [two]);
    assert_eq!(set(pool, a, "crash loading").await, sorted(vec![one, two]));
    // A quoted single word is that form only.
    assert_eq!(set(pool, a, "\"crashes\"").await, [one]);
    assert_eq!(set(pool, a, "\"crash\"").await, [two]);
    assert_eq!(set(pool, a, "\"CRASH\"").await, [two]);
}

#[tokio::test]
async fn qry_039_scopes_and_a_private_text_field() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let notes = text_field(pool, a, w.web, "Notes", false).await;
    let secret = text_field(pool, a, w.web, "Secret", true).await;
    let in_summary = issue(pool, a, w.web, "Kettle", "").await;
    let in_description = issue(pool, a, w.web, "Other", "kettle").await;
    let in_comment = issue(pool, a, w.web, "Third", "").await;
    comment(pool, a, in_comment, "kettle", &[]).await;
    let in_field = issue(pool, a, w.web, "Fourth", "").await;
    set_text(pool, a, in_field, notes, "kettle").await;
    let in_secret = issue(pool, a, w.web, "Fifth", "").await;
    set_text(pool, a, in_secret, secret, "kettle").await;
    assert_eq!(set(pool, a, "summary: kettle").await, [in_summary]);
    assert_eq!(set(pool, a, "description: kettle").await, [in_description]);
    assert_eq!(set(pool, a, "comments: kettle").await, [in_comment]);
    assert_eq!(set(pool, a, "notes: kettle").await, [in_field]);
    assert_eq!(set(pool, a, "secret: kettle").await, [in_secret]);
    let all = sorted(vec![
        in_summary,
        in_description,
        in_comment,
        in_field,
        in_secret,
    ]);
    assert_eq!(set(pool, a, "text: kettle").await, all);
    assert_eq!(set(pool, a, "kettle").await, all);
    // The private field is searched only for those who read it.
    let amy = member(pool, a, "amy", w.web).await;
    assert_eq!(set(pool, &amy, "kettle").await, all);
    let rita = reader(pool, a, w.web).await;
    let mut open = all.clone();
    open.retain(|id| *id != in_secret);
    assert_eq!(set(pool, &rita, "kettle").await, open);
    assert_eq!(set(pool, &rita, "text: kettle").await, open);
    assert!(set(pool, &rita, "secret: kettle").await.is_empty());
}

/// FLD-062: text in a private field matches only for its readers (the reader sees the issue).
#[tokio::test]
async fn fld_062_text_in_a_private_field() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let secret = text_field(pool, a, w.web, "Secret", true).await;
    let id = issue(pool, a, w.web, "Visible", "").await;
    set_text(pool, a, id, secret, "launch codes").await;
    let rita = reader(pool, a, w.web).await;
    assert_eq!(set(pool, &rita, "visible").await, [id]);
    assert!(set(pool, &rita, "codes").await.is_empty());
    assert!(set(pool, &rita, "\"launch codes\"").await.is_empty());
    assert_eq!(set(pool, a, "\"launch codes\"").await, [id]);
}

#[tokio::test]
async fn qry_041_tokens_and_common_words() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let under = issue(pool, a, w.web, "Bad user_name check", "").await;
    let hyphen = issue(pool, a, w.web, "Broken drag-drop", "").await;
    let dotted = issue(pool, a, w.web, "Panic in main.rs", "").await;
    let other = issue(pool, a, w.web, "The end of the line", "").await;
    // Underscores and hyphens split words; punctuation around words does not matter.
    assert_eq!(set(pool, a, "name").await, [under]);
    assert_eq!(set(pool, a, "user_name").await, [under]);
    assert_eq!(set(pool, a, "drop").await, [hyphen]);
    assert_eq!(set(pool, a, "drag-drop").await, [hyphen]);
    assert_eq!(set(pool, a, "\"broken,\"").await, [hyphen]);
    // PostgreSQL keeps dotted names (file names, hosts) whole (D-037).
    assert_eq!(set(pool, a, "main.rs").await, [dotted]);
    assert!(set(pool, a, "main").await.is_empty());
    // A common word alone is left out with a note; inside a phrase it counts as a gap.
    let the = page(pool, a, "the").await;
    assert_eq!(the.issues.issues.len(), 4);
    assert!(
        the.notes
            .iter()
            .any(|n| n.code == ProblemCode::IgnoredCommonWord)
    );
    assert_eq!(set(pool, a, "the line").await, [other]);
    assert_eq!(set(pool, a, "\"end of the line\"").await, [other]);
    assert!(set(pool, a, "\"end the line\"").await.is_empty());
    // Russian conjunctions are operators, other Russian common words are left out.
    assert_eq!(page(pool, a, "при").await.issues.issues.len(), 4);
}

#[tokio::test]
async fn qry_045_relevance() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let in_summary = issue(pool, a, w.web, "Parser crash", "").await;
    let in_description = issue(pool, a, w.web, "Unrelated thing", "Parser crash").await;
    let both = issue(pool, a, w.web, "Lexer crash", "Parser crash").await;
    let answer = page(pool, a, "parser").await;
    assert!(matches!(answer.sort[0].by, SortBy::Relevance(_)));
    let order: Vec<IssueId> = answer.issues.issues.iter().map(|v| v.issue.id).collect();
    let pos = |id| order.iter().position(|x| *x == id).unwrap();
    assert!(pos(in_summary) < pos(in_description));
    // OR keeps relevance: the issue with both words leads.
    assert_eq!(
        find(pool, a, "parser or lexer").await,
        [both, in_summary, in_description]
    );
    // An explicit sort replaces relevance.
    assert_eq!(
        find(pool, a, "parser sort: created asc").await,
        [in_summary, in_description, both]
    );
}

#[tokio::test]
async fn qry_016_bare_ids_are_the_issue_or_text() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let first = issue(pool, a, w.web, "First", "").await;
    let refers = issue(pool, a, w.web, "Follow-up", "Continues WEB-1").await;
    assert_eq!(set(pool, a, "WEB-1").await, sorted(vec![first, refers]));
    assert_eq!(set(pool, a, "web-1").await, sorted(vec![first, refers]));
    assert_eq!(set(pool, a, "\"WEB-1\"").await, [refers]);
}

/// CNT-058 and QRY-023: mentions of people and issues; hidden comments and text fields do not
/// count; former keys still name the issue.
#[tokio::test]
async fn cnt_058_qry_023_mentions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let a = &w.admin;
    let amy = member(pool, a, "amy", w.web).await;
    let notes = text_field(pool, a, w.web, "Notes", false).await;
    let target = issue(pool, a, w.web, "Target", "").await; // WEB-1
    let other = issue(pool, a, w.web, "Other target", "").await; // WEB-2
    let in_description = issue(pool, a, w.web, "One", "Ask @amy about WEB-1").await;
    let in_comment = issue(pool, a, w.web, "Two", "").await;
    comment(pool, a, in_comment, "@amy see WEB-1", &[]).await;
    let in_hidden = issue(pool, a, w.web, "Three", "").await;
    comment(pool, a, in_hidden, "@amy see WEB-2", &[a.user()]).await;
    let in_field = issue(pool, a, w.web, "Four", "").await;
    set_text(pool, a, in_field, notes, "@amy WEB-1 WEB-2").await;

    assert_eq!(
        set(pool, &amy, "mentions: me").await,
        sorted(vec![in_description, in_comment])
    );
    assert_eq!(
        set(pool, a, "mentions: amy").await,
        sorted(vec![in_description, in_comment, in_hidden])
    );
    assert_eq!(
        set(pool, a, "mentions: WEB-1").await,
        sorted(vec![in_description, in_comment])
    );
    assert_eq!(set(pool, a, "mentions: WEB-2").await, [in_hidden]);
    assert!(set(pool, &amy, "mentions: WEB-2").await.is_empty());
    assert_eq!(set(pool, a, "mentioned-in: WEB-3").await, [target]);
    assert_eq!(set(pool, a, "mentioned-in: WEB-5").await, [other]);
    assert!(set(pool, &amy, "mentioned-in: WEB-5").await.is_empty());
    assert!(set(pool, a, "mentioned-in: WEB-6").await.is_empty());

    // A new key: texts written with the old one still mention the issue.
    let row = projects::get(pool, a, w.web).await.unwrap();
    projects::update(
        pool,
        a,
        w.web,
        projects::ProjectChange {
            name: None,
            key: Some("FRONT".to_owned()),
            description: None,
            audience: None,
            version: row.project.version,
        },
    )
    .await
    .unwrap();
    for text in ["mentions: FRONT-1", "mentions: WEB-1"] {
        assert_eq!(
            set(pool, a, text).await,
            sorted(vec![in_description, in_comment]),
            "{text}"
        );
    }
    assert_eq!(set(pool, a, "mentioned-in: FRONT-3").await, [target]);
}
