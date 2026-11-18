//! Issue search (QRY-*): the language through the service on a real database (plan step 1.5 §6
//! B5). Each test names the catalog rows it covers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::access::visibility::{Audience, Item, Kind, can_see};
use taskmaster_domain::boards::Preset;
use taskmaster_domain::fields::FieldKind;
use taskmaster_domain::issues::history::Origin;
use taskmaster_domain::issues::values::Input;
use taskmaster_domain::links::Side;
use taskmaster_domain::projects::Template;
use taskmaster_domain::search::SortBy;
use taskmaster_domain::settings::Language;
use taskmaster_domain::validation::Problem as InputProblem;
use taskmaster_domain::{BoardId, FieldId, IssueId, ProjectId, SprintId, UserId, ValueId};
use taskmaster_query::{Direction, ProblemCode};
use taskmaster_service::issues::{
    self, AudienceInput, FieldChange, FieldInput, IssueView, NewIssue,
};
use taskmaster_service::project_fields::{self, AttachInput, SettingsChange};
use taskmaster_service::projects::{self, NewProjectInput};
use taskmaster_service::search::{self, SearchInput};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users::{self, NewAccount};
use taskmaster_service::value_sets::{self, ValueInput};
use taskmaster_service::{Error, Passwords, access, comments, groups, issue_links, roles};
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

async fn create_user(pool: &PgPool, actor: &Access, username: &str, full_name: &str) -> UserId {
    users::create(
        pool,
        &passwords(),
        actor,
        NewAccount {
            username: username.to_owned(),
            full_name: full_name.to_owned(),
            email: format!("{username}@example.com"),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id
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

async fn grant(
    pool: &PgPool,
    actor: &Access,
    user: UserId,
    role: PredefinedRole,
    of: &[ProjectId],
) {
    let role = taskmaster_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().copied().map(Some).collect();
    roles::assign(pool, actor, role, Holder::User(user), &scopes)
        .await
        .unwrap();
}

async fn person(
    pool: &PgPool,
    actor: &Access,
    username: &str,
    role: PredefinedRole,
    of: &[ProjectId],
) -> Access {
    let full_name = format!("{} Person", username[..1].to_uppercase() + &username[1..]);
    let user = create_user(pool, actor, username, &full_name).await;
    grant(pool, actor, user, role, of).await;
    access::load(pool, user).await.unwrap()
}

/// Publishes an issue with these values (`None` clears the field).
async fn make(
    pool: &PgPool,
    actor: &Access,
    project: ProjectId,
    summary: &str,
    values: &[(FieldId, Option<Input>)],
) -> IssueView {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: summary.to_owned(),
            publish: true,
            fields: values
                .iter()
                .map(|(field, value)| FieldInput {
                    field: *field,
                    value: value.clone(),
                })
                .collect(),
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
}

fn ids(values: &[ValueId]) -> Option<Input> {
    Some(Input::Ids(values.to_vec()))
}

fn field_id(view: &IssueView, name: &str) -> FieldId {
    view.fields
        .iter()
        .find(|f| f.field.field.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
        .field
        .id()
}

/// The value named `name` of the set a field uses in the issue's project.
async fn value(pool: &PgPool, view: &IssueView, field: &str, name: &str) -> ValueId {
    let set = view
        .fields
        .iter()
        .find(|f| f.field.field.name == field)
        .unwrap()
        .field
        .attached
        .set_id
        .unwrap();
    taskmaster_store::value_sets::values_of(pool, &[set])
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("no value {name}"))
        .id
}

fn input(text: &str) -> SearchInput {
    SearchInput {
        text: text.to_owned(),
        limit: 100,
        ..SearchInput::default()
    }
}

/// IDs found, in the answer's order.
async fn find(pool: &PgPool, viewer: &Access, text: &str) -> Vec<IssueId> {
    found(pool, viewer, input(text)).await
}

async fn found(pool: &PgPool, viewer: &Access, input: SearchInput) -> Vec<IssueId> {
    let text = input.text.clone();
    search::search(pool, viewer, input)
        .await
        .unwrap_or_else(|e| panic!("{text}: {e:?}"))
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

/// The error codes of a refused query.
async fn refused(pool: &PgPool, viewer: &Access, text: &str) -> Vec<ProblemCode> {
    match search::search(pool, viewer, input(text)).await {
        Err(Error::InvalidQuery(problems)) => problems.into_iter().map(|p| p.code).collect(),
        other => panic!("{text}: expected an invalid query, got {other:?}"),
    }
}

/// Two blank projects, WEB and API, and five issues in WEB.
struct World {
    admin: Access,
    web: ProjectId,
    api: ProjectId,
    priority: FieldId,
    kind: FieldId,
    state: FieldId,
    assignee: FieldId,
    /// In WEB: high bug, normal task, low bug (open), high feature (fixed), blocking task.
    issues: Vec<IssueId>,
    /// In API: one normal task.
    api_issue: IssueId,
}

async fn world(pool: &PgPool) -> World {
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let api = project(pool, &admin, "Api", "API").await;
    let first = make(pool, &admin, web, "First", &[]).await;
    let priority = field_id(&first, "Priority");
    let kind = field_id(&first, "Type");
    let state = field_id(&first, "State");
    let assignee = field_id(&first, "Assignee");
    let v = |f: &'static str, n: &'static str| {
        let first = &first;
        async move { value(pool, first, f, n).await }
    };
    let (high, normal, low, blocking) = (
        v("Priority", "High").await,
        v("Priority", "Normal").await,
        v("Priority", "Low").await,
        v("Priority", "Blocking").await,
    );
    let (bug, task, feature) = (
        v("Type", "Bug").await,
        v("Type", "Task").await,
        v("Type", "Feature").await,
    );
    let (open, fixed) = (v("State", "Open").await, v("State", "Fixed").await);
    issues::set_field(
        pool,
        &admin,
        WEB,
        first.issue.id,
        priority,
        FieldChange::Set(ids(&[high])),
        false,
    )
    .await
    .unwrap();
    issues::set_field(
        pool,
        &admin,
        WEB,
        first.issue.id,
        kind,
        FieldChange::Set(ids(&[bug])),
        false,
    )
    .await
    .unwrap();
    let mut list = vec![first.issue.id];
    for (summary, values) in [
        (
            "Second",
            vec![(priority, ids(&[normal])), (kind, ids(&[task]))],
        ),
        (
            "Third",
            vec![
                (priority, ids(&[low])),
                (kind, ids(&[bug])),
                (state, ids(&[open])),
            ],
        ),
        (
            "Fourth",
            vec![
                (priority, ids(&[high])),
                (kind, ids(&[feature])),
                (state, ids(&[fixed])),
            ],
        ),
        (
            "Fifth",
            vec![(priority, ids(&[blocking])), (kind, ids(&[task]))],
        ),
    ] {
        list.push(make(pool, &admin, web, summary, &values).await.issue.id);
    }
    let api_issue = make(pool, &admin, api, "Api one", &[]).await.issue.id;
    World {
        admin,
        web,
        api,
        priority,
        kind,
        state,
        assignee,
        issues: list,
        api_issue,
    }
}

#[tokio::test]
async fn qry_001_empty_query_lists_every_readable_issue() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let all = find(pool, &w.admin, "").await;
    // Newest change first (the default `updated desc`); the first issue changed after creation.
    let mut expected: Vec<IssueId> = w.issues[1..].iter().rev().copied().collect();
    expected.insert(0, w.api_issue);
    expected.push(w.issues[0]);
    assert_eq!(all[..1], [w.api_issue]);
    assert_eq!(sorted(all.clone()), sorted(expected));
    // A member of WEB sees WEB's issues only.
    let mia = person(
        pool,
        &w.admin,
        "mia",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    assert_eq!(set(pool, &mia, "").await, sorted(w.issues.clone()));
    // Keywords in any case; the canonical text runs the same.
    let page = search::search(
        pool,
        &w.admin,
        input("  PRIORITY:   high   sort:  SUMMARY  "),
    )
    .await
    .unwrap();
    assert_eq!(page.canonical, "PRIORITY: high sort: SUMMARY");
    let first: Vec<IssueId> = page.issues.issues.iter().map(|v| v.issue.id).collect();
    assert_eq!(first, [w.issues[0], w.issues[3]]);
    assert_eq!(find(pool, &w.admin, &page.canonical).await, first);
    assert_eq!(
        page.sort
            .iter()
            .map(|s| (s.by.clone(), s.direction))
            .collect::<Vec<_>>(),
        [
            (SortBy::Summary, Direction::Asc),
            (SortBy::Id, Direction::Desc)
        ]
    );
}

#[tokio::test]
async fn qry_002_names_aliases_and_unknown_attributes() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let row = project_fields::attach(
        pool,
        &w.admin,
        w.web,
        AttachInput::New {
            name: "Story points".to_owned(),
            kind: FieldKind::Integer,
            multi: false,
            aliases: vec!["sp".to_owned()],
        },
    )
    .await
    .unwrap();
    let points = row.field.id;
    for (issue, n) in [(w.issues[0], 3), (w.issues[1], 8)] {
        issues::set_field(
            pool,
            &w.admin,
            WEB,
            issue,
            points,
            FieldChange::Set(Some(Input::Integer(n))),
            false,
        )
        .await
        .unwrap();
    }
    assert_eq!(set(pool, &w.admin, "story-points: 3").await, [w.issues[0]]);
    assert_eq!(
        set(pool, &w.admin, "Story_Points: 3..8").await,
        sorted(w.issues[..2].to_vec())
    );
    assert_eq!(set(pool, &w.admin, "SP > 3").await, [w.issues[1]]);
    assert_eq!(
        set(pool, &w.admin, "\"story points\": 8").await,
        [w.issues[1]]
    );
    assert_eq!(
        set(pool, &w.admin, "type: BUG").await,
        sorted(vec![w.issues[0], w.issues[2]])
    );
    // Unknown names and values are errors, never text (QRY-002).
    assert_eq!(
        refused(pool, &w.admin, "colour: red").await,
        [ProblemCode::UnknownAttribute]
    );
    assert_eq!(
        refused(pool, &w.admin, "type: gadget").await,
        [ProblemCode::UnknownValue]
    );
    // Outside the scope of `project:` a field is unknown.
    assert_eq!(
        refused(pool, &w.admin, "project: API sp: 3").await,
        [ProblemCode::UnknownAttribute]
    );
}

#[tokio::test]
async fn qry_003_lists_are_any_of_and_repetition_is_and() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    assert_eq!(
        set(pool, &w.admin, "priority: high, low").await,
        sorted(vec![w.issues[0], w.issues[2], w.issues[3]])
    );
    // Repeating an attribute is AND: on a single-valued field nothing matches, with a note.
    let page = search::search(pool, &w.admin, input("priority: high priority: low"))
        .await
        .unwrap();
    assert!(page.issues.issues.is_empty());
    assert_eq!(
        page.notes.iter().map(|n| n.code).collect::<Vec<_>>(),
        [ProblemCode::SameFieldTwice]
    );
    assert_eq!(
        set(pool, &w.admin, "type: bug priority: high, low").await,
        sorted(vec![w.issues[0], w.issues[2]])
    );
}

#[tokio::test]
async fn qry_004_exclusions_and_negation() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let web = sorted(w.issues.clone());
    let without = |skip: &[usize]| -> Vec<IssueId> {
        sorted(
            w.issues
                .iter()
                .enumerate()
                .filter(|(i, _)| !skip.contains(i))
                .map(|(_, id)| *id)
                .collect(),
        )
    };
    assert_eq!(
        set(pool, &w.admin, "project: web priority: !high").await,
        without(&[0, 3])
    );
    assert_eq!(
        set(pool, &w.admin, "project: web priority: high, !high").await,
        Vec::<IssueId>::new()
    );
    assert_eq!(
        set(pool, &w.admin, "project: web !type: bug").await,
        without(&[0, 2])
    );
    assert_eq!(
        set(pool, &w.admin, "project: web not type: bug").await,
        without(&[0, 2])
    );
    assert_eq!(
        set(pool, &w.admin, "project: web priority: !high, !low").await,
        without(&[0, 2, 3])
    );
    assert_eq!(
        set(pool, &w.admin, "project: web !!type: bug").await,
        without(&[1, 3, 4])
    );
    assert_eq!(set(pool, &w.admin, "project: web").await, web);
}

#[tokio::test]
async fn qry_005_is_unions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    // A second field with a value named like a priority.
    let row = project_fields::attach(
        pool,
        &w.admin,
        w.web,
        AttachInput::New {
            name: "Impact".to_owned(),
            kind: FieldKind::Enum,
            multi: false,
            aliases: Vec::new(),
        },
    )
    .await
    .unwrap();
    let set_id = row.attached.set_id.unwrap();
    let added = value_sets::add_value(
        pool,
        &w.admin,
        set_id,
        ValueInput {
            name: Some("High".to_owned()),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
    let impact_high = added.values.iter().find(|v| v.name == "High").unwrap().id;
    issues::set_field(
        pool,
        &w.admin,
        WEB,
        w.issues[1],
        row.field.id,
        FieldChange::Set(ids(&[impact_high])),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        set(pool, &w.admin, "is: high").await,
        sorted(vec![w.issues[0], w.issues[1], w.issues[3]])
    );
    // A project key, and a username (any role).
    assert_eq!(set(pool, &w.admin, "is: api").await, [w.api_issue]);
    let mia = person(
        pool,
        &w.admin,
        "mia",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let hers = make(pool, &mia, w.web, "Hers", &[]).await.issue.id;
    let view = issues::get(pool, &w.admin, hers).await.unwrap().0;
    let assignees = view
        .fields
        .iter()
        .find(|f| f.field.id() == w.assignee)
        .unwrap()
        .field
        .attached
        .set_id
        .unwrap();
    value_sets::add_members(pool, &w.admin, assignees, &[mia.user()], &[])
        .await
        .unwrap();
    issues::set_field(
        pool,
        &w.admin,
        WEB,
        w.issues[4],
        w.assignee,
        FieldChange::Set(Some(Input::Ids(vec![mia.user()]))),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        set(pool, &w.admin, "is: mia").await,
        sorted(vec![w.issues[4], hers])
    );
    assert_eq!(
        set(pool, &mia, "is: me").await,
        sorted(vec![w.issues[4], hers])
    );
    assert_eq!(
        refused(pool, &w.admin, "is: nothing-like-it").await,
        [ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn qry_006_quoting() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let progress = value(
        pool,
        &issues::get(pool, &w.admin, w.issues[1]).await.unwrap().0,
        "State",
        "In progress",
    )
    .await;
    issues::set_field(
        pool,
        &w.admin,
        WEB,
        w.issues[1],
        w.state,
        FieldChange::Set(ids(&[progress])),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        set(pool, &w.admin, "state: \"in progress\"").await,
        [w.issues[1]]
    );
    assert_eq!(
        set(pool, &w.admin, "\"State\": \"IN PROGRESS\"").await,
        [w.issues[1]]
    );
    assert_eq!(
        refused(pool, &w.admin, "state: \"in progress").await,
        [ProblemCode::UnclosedQuote]
    );
    // Unquoted, the words are a value and then text.
    assert_eq!(
        refused(pool, &w.admin, "state: in progress").await,
        [ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn qry_007_precedence_and_groups() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    // NOT, then AND, then OR.
    assert_eq!(
        set(pool, &w.admin, "priority: high or priority: low type: bug").await,
        sorted(vec![w.issues[0], w.issues[2], w.issues[3]])
    );
    assert_eq!(
        set(
            pool,
            &w.admin,
            "(priority: high or priority: low) type: bug"
        )
        .await,
        sorted(vec![w.issues[0], w.issues[2]])
    );
    assert_eq!(
        set(
            pool,
            &w.admin,
            "type: bug (priority: high or priority: low)"
        )
        .await,
        sorted(vec![w.issues[0], w.issues[2]])
    );
    assert_eq!(
        set(
            pool,
            &w.admin,
            "not type: bug and not type: task or priority: low"
        )
        .await,
        sorted(vec![w.issues[2], w.issues[3]])
    );
    assert_eq!(
        set(pool, &w.admin, "(((type: task)))").await,
        sorted(vec![w.issues[1], w.issues[4], w.api_issue])
    );
    assert_eq!(
        refused(pool, &w.admin, "(type: task").await,
        [ProblemCode::UnclosedGroup]
    );
}

#[tokio::test]
async fn qry_008_ranges_follow_each_projects_order() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    // API gets its own priorities with Normal moved up: Blocking, Normal, High, Elevated, Low.
    let forked = project_fields::fork_set(pool, &w.admin, w.api, w.priority)
        .await
        .unwrap();
    let own = forked.attached.set_id.unwrap();
    let values = taskmaster_store::value_sets::values_of(pool, &[own])
        .await
        .unwrap();
    let named = |n: &str| values.iter().find(|v| v.name == n).unwrap().id;
    value_sets::move_value(
        pool,
        &w.admin,
        own,
        named("Normal"),
        Some(named("Blocking")),
    )
    .await
    .unwrap();
    let api_normal = make(
        pool,
        &w.admin,
        w.api,
        "Api normal",
        &[(w.priority, ids(&[named("Normal")]))],
    )
    .await
    .issue
    .id;
    let api_low = make(
        pool,
        &w.admin,
        w.api,
        "Api low",
        &[(w.priority, ids(&[named("Low")]))],
    )
    .await
    .issue
    .id;
    // High..Low: in WEB Normal is inside, in API it is not.
    assert_eq!(
        set(pool, &w.admin, "priority: high..low").await,
        sorted(vec![
            w.issues[0],
            w.issues[1],
            w.issues[2],
            w.issues[3],
            api_low
        ])
    );
    assert_eq!(
        set(pool, &w.admin, "priority: ..normal").await,
        sorted(vec![
            w.issues[0],
            w.issues[1],
            w.issues[3],
            w.issues[4],
            api_normal,
            w.api_issue
        ])
    );
    assert_eq!(
        set(pool, &w.admin, "priority > elevated").await,
        sorted(vec![w.issues[1], w.issues[2], api_low])
    );
}

#[tokio::test]
async fn qry_010_none_any_and_the_empty_label() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    issues::set_field(
        pool,
        &w.admin,
        WEB,
        w.issues[2],
        w.assignee,
        FieldChange::Set(Some(Input::Ids(vec![w.admin.user()]))),
        false,
    )
    .await
    .unwrap();
    let mut unassigned = w.issues.clone();
    unassigned.retain(|id| *id != w.issues[2]);
    unassigned.push(w.api_issue);
    assert_eq!(
        set(pool, &w.admin, "assignee: none").await,
        sorted(unassigned.clone())
    );
    assert_eq!(set(pool, &w.admin, "assignee: any").await, [w.issues[2]]);
    assert_eq!(set(pool, &w.admin, "assignee: me").await, [w.issues[2]]);
    // FLD-056: WEB's label for an empty assignee means `none` there only.
    let row = project_fields::list(pool, &w.admin, w.web)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.field.id == w.assignee)
        .unwrap();
    project_fields::update(
        pool,
        &w.admin,
        w.web,
        w.assignee,
        SettingsChange {
            can_be_empty: None,
            empty_label: Some("Nobody yet".to_owned()),
            default_values: None,
            private: None,
            version: row.attached.version,
        },
    )
    .await
    .unwrap();
    unassigned.retain(|id| *id != w.api_issue);
    assert_eq!(
        set(pool, &w.admin, "assignee: \"nobody yet\"").await,
        sorted(unassigned)
    );
    // Collections.
    comments::create(
        pool,
        &w.admin,
        WEB,
        w.issues[1],
        comments::NewComment {
            text: "A note".to_owned(),
            ..comments::NewComment::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(set(pool, &w.admin, "comments: any").await, [w.issues[1]]);
    assert_eq!(set(pool, &w.admin, "comments: none").await.len(), 5);
    assert_eq!(set(pool, &w.admin, "votes: none").await.len(), 6);
}

#[tokio::test]
async fn qry_011_resolved_only_when_every_state_field_is() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let row = project_fields::attach(
        pool,
        &w.admin,
        w.web,
        AttachInput::New {
            name: "Review".to_owned(),
            kind: FieldKind::State,
            multi: false,
            aliases: Vec::new(),
        },
    )
    .await
    .unwrap();
    let review = row.field.id;
    let set_id = row.attached.set_id.unwrap();
    for (name, resolved) in [("Pending", false), ("Approved", true)] {
        value_sets::add_value(
            pool,
            &w.admin,
            set_id,
            ValueInput {
                name: Some(name.to_owned()),
                resolved: Some(resolved),
                ..ValueInput::default()
            },
            None,
        )
        .await
        .unwrap();
    }
    let values = taskmaster_store::value_sets::values_of(pool, &[set_id])
        .await
        .unwrap();
    let named = |n: &str| values.iter().find(|v| v.name == n).unwrap().id;
    let fixed = value(
        pool,
        &issues::get(pool, &w.admin, w.issues[3]).await.unwrap().0,
        "State",
        "Fixed",
    )
    .await;
    let half = make(
        pool,
        &w.admin,
        w.web,
        "Half",
        &[(w.state, ids(&[fixed])), (review, ids(&[named("Pending")]))],
    )
    .await
    .issue
    .id;
    let whole = make(
        pool,
        &w.admin,
        w.web,
        "Whole",
        &[
            (w.state, ids(&[fixed])),
            (review, ids(&[named("Approved")])),
        ],
    )
    .await
    .issue
    .id;
    let resolved = set(pool, &w.admin, "is: resolved").await;
    assert!(
        resolved.contains(&whole) && !resolved.contains(&half),
        "{resolved:?}"
    );
    assert!(set(pool, &w.admin, "is: unresolved").await.contains(&half));
    // A state field's `resolved` names that field's resolved values.
    let fixed_states = set(pool, &w.admin, "project: web state: resolved").await;
    assert!(fixed_states.contains(&half) && fixed_states.contains(&whole));
    assert_eq!(set(pool, &w.admin, "review: resolved").await, [whole]);
    assert_eq!(set(pool, &w.admin, "review: unresolved").await, [half]);
}

#[tokio::test]
async fn qry_044_sort_keys_directions_and_pages() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let in_web = |text: &str| format!("project: web {text}");
    // Priority by position: Blocking, High (two, newest first), Normal, Low.
    let by_priority = find(pool, &w.admin, &in_web("sort: priority")).await;
    assert_eq!(
        by_priority,
        [
            w.issues[4],
            w.issues[3],
            w.issues[0],
            w.issues[1],
            w.issues[2]
        ]
    );
    let desc = find(pool, &w.admin, &in_web("sort: priority desc")).await;
    assert_eq!(
        desc,
        [
            w.issues[2],
            w.issues[1],
            w.issues[3],
            w.issues[0],
            w.issues[4]
        ]
    );
    assert_eq!(
        find(pool, &w.admin, &in_web("sort: summary")).await,
        [
            w.issues[4],
            w.issues[0],
            w.issues[3],
            w.issues[1],
            w.issues[2]
        ]
    );
    assert_eq!(
        find(pool, &w.admin, &in_web("sort: id asc")).await,
        w.issues
    );
    assert_eq!(
        find(pool, &w.admin, &in_web("sort: created")).await.len(),
        5
    );
    // Empty values last in both directions.
    issues::set_field(
        pool,
        &w.admin,
        WEB,
        w.issues[1],
        w.assignee,
        FieldChange::Set(Some(Input::Ids(vec![w.admin.user()]))),
        false,
    )
    .await
    .unwrap();
    for text in [
        "sort: assignee",
        "sort: assignee desc",
        "sort: resolved",
        "sort: resolved asc",
    ] {
        let list = find(pool, &w.admin, &in_web(text)).await;
        let first = if text.contains("assignee") {
            w.issues[1]
        } else {
            w.issues[3]
        };
        assert_eq!(list[0], first, "{text}");
    }
    // Two keys: type, then priority.
    let two = find(pool, &w.admin, &in_web("sort: type, priority")).await;
    assert_eq!(two[..2], [w.issues[0], w.issues[2]]);
    // Pages over a sort with ties: no gaps, no repeats.
    for text in [
        "sort: priority",
        "sort: type desc, summary",
        "sort: assignee",
        "sort: resolved",
        "",
    ] {
        let whole = find(pool, &w.admin, &in_web(text)).await;
        let mut paged = Vec::new();
        let mut cursor = None;
        loop {
            let page = search::search(
                pool,
                &w.admin,
                SearchInput {
                    limit: 2,
                    cursor: cursor.take(),
                    ..input(&in_web(text))
                },
            )
            .await
            .unwrap();
            paged.extend(page.issues.issues.iter().map(|v| v.issue.id));
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(paged, whole, "{text}");
    }
    assert_eq!(
        refused(pool, &w.admin, "sort: priority, type, state, summary").await,
        [ProblemCode::TooManySortKeys]
    );
    let _ = w.kind;
}

/// The union by position (D-024): without a project scope the sort runs as one branch per
/// position, the no-value issues come last in both directions, a condition on the sort field
/// selects branches, and cursors page through exactly what one page holds.
#[tokio::test]
async fn d_024_union_by_position_pages_and_selects_branches() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    // Across both projects (Api one holds Normal of its own project's set): Blocking, High
    // (newest first), Normal (Api one newer than Second), Low.
    let whole = find(pool, &w.admin, "sort: priority").await;
    assert_eq!(
        whole,
        [
            w.issues[4],
            w.issues[3],
            w.issues[0],
            w.api_issue,
            w.issues[1],
            w.issues[2]
        ]
    );
    assert_eq!(
        find(pool, &w.admin, "sort: priority desc").await,
        [
            w.issues[2],
            w.api_issue,
            w.issues[1],
            w.issues[3],
            w.issues[0],
            w.issues[4]
        ]
    );
    // A condition on the sort field: only its branches run.
    assert_eq!(
        find(pool, &w.admin, "priority: high sort: priority").await,
        [w.issues[3], w.issues[0]]
    );
    // The no-value tail: state is optional, and First, Second and Fifth hold none — they walk
    // the union's last branch. The union must order Web's issues exactly as the project-scoped
    // scan does, no-value issues last both ways.
    let by_state = find(pool, &w.admin, "sort: state").await;
    let scan = find(pool, &w.admin, "project: web sort: state").await;
    let web_only: Vec<IssueId> = by_state
        .iter()
        .copied()
        .filter(|id| *id != w.api_issue)
        .collect();
    assert_eq!(web_only, scan);
    // Pages of two walk the union: every issue once, in order, over the branch cursors.
    for text in ["sort: priority", "sort: priority desc", "sort: state"] {
        let mut paged = Vec::new();
        let mut cursor = None;
        loop {
            let page = search::search(
                pool,
                &w.admin,
                SearchInput {
                    limit: 2,
                    cursor: cursor.take(),
                    ..input(text)
                },
            )
            .await
            .unwrap();
            paged.extend(page.issues.issues.iter().map(|v| v.issue.id));
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(paged, find(pool, &w.admin, text).await, "{text}");
    }
}

#[tokio::test]
async fn qry_092_counts_and_cursors() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    assert_eq!(
        search::count(pool, &w.admin, input("")).await.unwrap(),
        (6, 5)
    );
    assert_eq!(
        search::count(pool, &w.admin, input("type: bug"))
            .await
            .unwrap(),
        (2, 2)
    );
    let answers = search::counts(
        pool,
        &w.admin,
        vec![
            input("type: task"),
            input("colour: red"),
            input("is: resolved"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(answers[0], Ok((3, 3)));
    assert_eq!(
        answers[1]
            .as_ref()
            .unwrap_err()
            .iter()
            .map(|p| p.code)
            .collect::<Vec<_>>(),
        [ProblemCode::UnknownAttribute]
    );
    assert_eq!(answers[2], Ok((1, 0)));
    let too_many = search::counts(pool, &w.admin, vec![input(""); 51]).await;
    assert!(matches!(too_many, Err(Error::Invalid(_))));
    // A cursor belongs to its query.
    let page = search::search(
        pool,
        &w.admin,
        SearchInput {
            limit: 2,
            ..input("")
        },
    )
    .await
    .unwrap();
    let cursor = page.next_cursor.unwrap();
    for (text, cursor) in [("type: bug", cursor.clone()), ("", "garbage".to_owned())] {
        let error = search::search(
            pool,
            &w.admin,
            SearchInput {
                cursor: Some(cursor),
                ..input(text)
            },
        )
        .await
        .unwrap_err();
        match error {
            Error::Invalid(problems) => {
                assert_eq!(problems[0].path, "cursor");
                assert_eq!(problems[0].problem, InputProblem::Invalid);
            }
            other => panic!("{other:?}"),
        }
    }
    let next = search::search(
        pool,
        &w.admin,
        SearchInput {
            cursor: Some(cursor),
            ..input("")
        },
    )
    .await
    .unwrap();
    assert_eq!(next.issues.issues.len(), 4);
    // Contexts the viewer may not use are not found.
    let mia = person(
        pool,
        &w.admin,
        "mia",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let error = search::count(
        pool,
        &mia,
        SearchInput {
            project: Some(w.api),
            ..input("")
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::NotFound(_)));
}

#[tokio::test]
async fn acc_049_search_and_the_rust_rule_agree_across_projects() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let api = project(pool, &admin, "Api", "API").await;
    let ops = project(pool, &admin, "Ops", "OPS").await;
    let group = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Insiders".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let a = person(
        pool,
        &admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[web, api],
    )
    .await;
    let b = person(pool, &admin, "bob", PredefinedRole::ProjectMember, &[web]).await;
    groups::add_members(pool, &admin, group.id, &[b.user()])
        .await
        .unwrap();
    let b = access::load(pool, b.user()).await.unwrap();
    let c = person(
        pool,
        &admin,
        "cid",
        PredefinedRole::ProjectAdmin,
        &[api, ops],
    )
    .await;
    // A submitter in OPS: creates, does not read (ACC-047).
    let sam = create_user(pool, &admin, "sam", "Sam Person").await;
    let role = roles::create(
        pool,
        &admin,
        roles::NewRole {
            name: "Submitter".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.create", "comment.create"]
                .map(str::to_owned)
                .to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(pool, &admin, role.role.id, Holder::User(sam), &[Some(ops)])
        .await
        .unwrap();
    let sam = access::load(pool, sam).await.unwrap();

    let audiences = [
        AudienceInput::default(),
        AudienceInput {
            users: vec![a.user()],
            groups: vec![],
        },
        AudienceInput {
            users: vec![],
            groups: vec![group.id],
        },
        AudienceInput {
            users: vec![c.user()],
            groups: vec![group.id],
        },
    ];
    let mut created = Vec::new();
    for (project, reporters) in [
        (web, vec![&admin, &a, &b]),
        (api, vec![&admin, &a, &c]),
        (ops, vec![&admin, &c, &sam]),
    ] {
        for (i, audience) in audiences.iter().enumerate() {
            for reporter in &reporters {
                let view = issues::create(
                    pool,
                    reporter,
                    WEB,
                    NewIssue {
                        project,
                        summary: format!("case {i}"),
                        publish: true,
                        audience: Some(audience.clone()),
                        ..NewIssue::default()
                    },
                )
                .await
                .unwrap()
                .done
                .0;
                created.push(view);
            }
        }
    }
    // Comments on the unrestricted issues, by their reporters, with each audience.
    let mut comments_made: Vec<(IssueId, ProjectId, UserId, AudienceInput)> = Vec::new();
    for (n, view) in created
        .iter()
        .filter(|v| v.issue.audience_users.is_empty() && v.issue.audience_groups.is_empty())
        .enumerate()
    {
        let author = [&admin, &a, &b, &c, &sam]
            .into_iter()
            .find(|p| p.user() == view.issue.reporter_id)
            .unwrap();
        let audience = audiences[n % audiences.len()].clone();
        comments::create(
            pool,
            author,
            WEB,
            view.issue.id,
            comments::NewComment {
                text: format!("comment {n}"),
                audience: Some(audience.clone()),
                ..comments::NewComment::default()
            },
        )
        .await
        .unwrap();
        comments_made.push((
            view.issue.id,
            view.issue.project_id,
            author.user(),
            audience,
        ));
    }
    for viewer in [&admin, &a, &b, &c, &sam] {
        let mut ruled = Vec::new();
        for view in &created {
            let project = taskmaster_store::projects::get(pool, view.project.id)
                .await
                .unwrap()
                .unwrap();
            if issues::can_read(viewer, &view.issue, &project) {
                ruled.push(view.issue.id);
            }
        }
        ruled.sort_unstable();
        assert_eq!(
            set(pool, viewer, "").await,
            ruled,
            "viewer {}",
            viewer.user()
        );
        assert_eq!(
            search::count(pool, viewer, input("")).await.unwrap().0,
            i64::try_from(ruled.len()).unwrap()
        );
        // Within one project, the same rule.
        for project in [web, api, ops] {
            let mut within: Vec<IssueId> = created
                .iter()
                .filter(|v| v.project.id == project && ruled.contains(&v.issue.id))
                .map(|v| v.issue.id)
                .collect();
            within.sort_unstable();
            let result = search::search(
                pool,
                viewer,
                SearchInput {
                    project: Some(project),
                    ..input("")
                },
            )
            .await;
            match result {
                Ok(page) => assert_eq!(
                    sorted(page.issues.issues.iter().map(|v| v.issue.id).collect()),
                    within,
                    "viewer {} in {project}",
                    viewer.user()
                ),
                Err(Error::NotFound(_)) => assert!(within.is_empty()),
                Err(other) => panic!("{other:?}"),
            }
        }
        // Comments: an issue matches `comments: any` when a visible comment is on it.
        let mut with_comments: Vec<IssueId> = comments_made
            .iter()
            .filter(|(issue, project, author, audience)| {
                ruled.contains(issue)
                    && can_see(
                        viewer,
                        Kind::Comment,
                        Item {
                            project: *project,
                            author: Some(*author),
                            audience: Audience {
                                users: &audience.users,
                                groups: &audience.groups,
                            },
                        },
                    )
            })
            .map(|(issue, ..)| *issue)
            .collect();
        with_comments.sort_unstable();
        with_comments.dedup();
        assert_eq!(
            set(pool, viewer, "comments: any").await,
            with_comments,
            "comments of viewer {}",
            viewer.user()
        );
    }
}

/// Moves an issue's timestamp (`created_at`, `updated_at` or `resolved_at`).
async fn stamp(pool: &PgPool, issue: IssueId, column: &str, at: OffsetDateTime) {
    let sql = match column {
        "created_at" => "update issues set created_at = $1 where id = $2",
        "updated_at" => "update issues set updated_at = $1 where id = $2",
        "resolved_at" => "update issues set resolved_at = $1 where id = $2",
        other => panic!("no column {other}"),
    };
    sqlx::query(sql)
        .bind(at)
        .bind(issue)
        .execute(pool)
        .await
        .unwrap();
}

fn at(text: &str, now: OffsetDateTime, zone: Option<&str>) -> SearchInput {
    SearchInput {
        now: Some(now),
        time_zone: zone.map(str::to_owned),
        ..input(text)
    }
}

/// Assigns an issue of WEB, first adding the person to WEB's team (assignees follow the team).
async fn assign(pool: &PgPool, w: &World, issue: IssueId, user: UserId) {
    taskmaster_service::teams::add(pool, &w.admin, w.web, &[user], &[])
        .await
        .unwrap();
    issues::set_field(
        pool,
        &w.admin,
        WEB,
        issue,
        w.assignee,
        FieldChange::Set(Some(Input::Ids(vec![user]))),
        false,
    )
    .await
    .unwrap();
}

async fn comment(pool: &PgPool, author: &Access, issue: IssueId, audience: Option<AudienceInput>) {
    comments::create(
        pool,
        author,
        WEB,
        issue,
        comments::NewComment {
            text: "A note".to_owned(),
            audience,
            ..comments::NewComment::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn qry_013_me_in_user_fields_and_involves() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    assign(pool, &w, w.issues[2], amy.user()).await;
    let own = make(pool, &amy, w.web, "Amy's", &[]).await.issue.id;
    comment(pool, &amy, w.issues[0], None).await;
    assert_eq!(set(pool, &amy, "assignee: me").await, [w.issues[2]]);
    assert_eq!(set(pool, &amy, "reporter: me").await, [own]);
    assert_eq!(set(pool, &amy, "commenter: me").await, [w.issues[0]]);
    let all = sorted(vec![w.issues[0], w.issues[2], own]);
    assert_eq!(set(pool, &amy, "involves: me").await, all);
    assert_eq!(set(pool, &amy, "is: me").await, all);
    assert_eq!(set(pool, &w.admin, "involves: amy").await, all);
    assert_eq!(set(pool, &w.admin, "is: amy").await, all);
    // A full name works too, folded.
    assert_eq!(
        set(pool, &w.admin, "assignee: \"amy person\"").await,
        [w.issues[2]]
    );
    assert!(set(pool, &w.admin, "updater: amy").await.contains(&own));
    // A full name two accounts share is ambiguous; the login is not.
    create_user(pool, &w.admin, "amy2", "Amy Person").await;
    assert_eq!(
        refused(pool, &w.admin, "assignee: \"amy person\"").await,
        [ProblemCode::AmbiguousUser]
    );
    assert_eq!(set(pool, &w.admin, "assignee: amy").await, [w.issues[2]]);
}

#[tokio::test]
async fn qry_014_groups_mean_their_members() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let bob = person(
        pool,
        &w.admin,
        "bob",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let backend = groups::create(
        pool,
        &w.admin,
        groups::NewGroup {
            name: "Backend".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    groups::add_members(pool, &w.admin, backend.id, &[amy.user(), bob.user()])
        .await
        .unwrap();
    assign(pool, &w, w.issues[1], amy.user()).await;
    assign(pool, &w, w.issues[3], bob.user()).await;
    assert_eq!(
        set(pool, &w.admin, "assignee: backend").await,
        sorted(vec![w.issues[1], w.issues[3]])
    );
    assert_eq!(
        set(pool, &w.admin, "assignee: backend, !bob").await,
        [w.issues[1]]
    );
    assert!(set(pool, &w.admin, "reporter: backend").await.is_empty());
}

#[tokio::test]
async fn qry_015_former_project_keys_and_names() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let row = projects::get(pool, &w.admin, w.web).await.unwrap();
    projects::update(
        pool,
        &w.admin,
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
    let web = sorted(w.issues.clone());
    assert_eq!(set(pool, &w.admin, "project: front").await, web);
    assert_eq!(set(pool, &w.admin, "project: web").await, web);
    assert_eq!(set(pool, &w.admin, "project: Web").await, web);
    assert_eq!(
        set(pool, &w.admin, "project: front, api").await.len(),
        web.len() + 1
    );
    assert_eq!(
        refused(pool, &w.admin, "project: nowhere").await,
        [ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn qry_016_issue_ids_old_and_new() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let issue = w.issues[1];
    let old = issues::get(pool, &w.admin, issue)
        .await
        .unwrap()
        .0
        .readable_id()
        .unwrap();
    taskmaster_service::issues::moves::move_issue(pool, &w.admin, WEB, issue, w.api, false)
        .await
        .unwrap();
    let new = issues::get(pool, &w.admin, issue)
        .await
        .unwrap()
        .0
        .readable_id()
        .unwrap();
    assert_ne!(old, new);
    for text in [
        format!("id: {old}"),
        format!("id: {new}"),
        format!("id: {}", issue),
    ] {
        let page = search::search(pool, &w.admin, input(&text)).await.unwrap();
        assert_eq!(page.single_id, Some(issue), "{text}");
        assert_eq!(page.issues.issues.len(), 1, "{text}");
    }
    let both = format!("id: {new}, {}", w.issues[0]);
    let page = search::search(pool, &w.admin, input(&both)).await.unwrap();
    assert_eq!(page.single_id, None);
    assert_eq!(page.issues.issues.len(), 2);
    // An ID the viewer cannot read is unknown, like one that does not exist.
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    assert_eq!(
        refused(pool, &amy, &format!("id: {new}")).await,
        [ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn qry_017_commenters_through_visible_comments_only() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let bob = person(
        pool,
        &w.admin,
        "bob",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    // Bob's comment for the admin only; the admin's for everybody.
    comment(
        pool,
        &bob,
        w.issues[0],
        Some(AudienceInput {
            users: vec![w.admin.user(), bob.user()],
            groups: Vec::new(),
        }),
    )
    .await;
    comment(pool, &w.admin, w.issues[1], None).await;
    assert_eq!(set(pool, &w.admin, "commenter: bob").await, [w.issues[0]]);
    assert!(set(pool, &amy, "commenter: bob").await.is_empty());
    assert_eq!(set(pool, &amy, "commented-by: admin").await, [w.issues[1]]);
    assert_eq!(set(pool, &amy, "comments: any").await, [w.issues[1]]);
    assert!(
        !set(pool, &amy, "involves: bob")
            .await
            .contains(&w.issues[0])
    );
}

#[tokio::test]
async fn qry_018_dates_follow_the_time_zone() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let now = datetime!(2026-10-22 12:00 UTC);
    let late = datetime!(2026-10-20 22:30 UTC); // 2026-10-21 01:30 in Moscow
    for issue in &w.issues {
        stamp(pool, *issue, "created_at", datetime!(2026-09-01 12:00 UTC)).await;
        stamp(pool, *issue, "updated_at", datetime!(2026-09-02 12:00 UTC)).await;
    }
    stamp(
        pool,
        w.api_issue,
        "created_at",
        datetime!(2026-09-01 12:00 UTC),
    )
    .await;
    stamp(
        pool,
        w.api_issue,
        "updated_at",
        datetime!(2026-09-02 12:00 UTC),
    )
    .await;
    stamp(pool, w.issues[0], "created_at", late).await;
    stamp(pool, w.issues[1], "updated_at", late).await;
    stamp(pool, w.issues[3], "resolved_at", late).await;
    let get = |text: &str, zone: Option<&str>| found(pool, &w.admin, at(text, now, zone));
    assert_eq!(get("created: 2026-10-20", None).await, [w.issues[0]]);
    assert!(get("created: 2026-10-21", None).await.is_empty());
    assert_eq!(
        get("created: 2026-10-21", Some("Europe/Moscow")).await,
        [w.issues[0]]
    );
    assert_eq!(get("updated: 2026-10-20", None).await, [w.issues[1]]);
    assert_eq!(get("resolved: 2026-10-20", None).await, [w.issues[3]]);
    assert_eq!(get("created: 2026-10-01..", None).await, [w.issues[0]]);
    assert_eq!(get("created > 2026-10-01", None).await, [w.issues[0]]);
    assert_eq!(get("created: ..2026-09-30", None).await.len(), 5);
    assert_eq!(get("created < 2026-10-20", None).await.len(), 5);
    assert_eq!(get("created <= 2026-10-20", None).await.len(), 6);
    assert_eq!(get("resolved: none", None).await.len(), 5);
    // Comments: the time of a visible comment.
    comment(pool, &w.admin, w.issues[2], None).await;
    sqlx::query("update comments set created_at = $1 where issue_id = $2")
        .bind(late)
        .bind(w.issues[2])
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(get("commented: 2026-10-20", None).await, [w.issues[2]]);
    assert_eq!(
        get("commented: 2026-10-21", Some("Europe/Moscow")).await,
        [w.issues[2]]
    );
    assert_eq!(
        refused(pool, &w.admin, "created: 2026-13-01").await,
        [ProblemCode::BadDate]
    );
}

#[tokio::test]
async fn qry_019_own_tags() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let urgent = taskmaster_service::tags::create(pool, &w.admin, "Urgent", None)
        .await
        .unwrap();
    taskmaster_service::tags::set_on_issue(pool, &w.admin, WEB, w.issues[0], urgent.id, true)
        .await
        .unwrap();
    assert_eq!(set(pool, &w.admin, "tag: urgent").await, [w.issues[0]]);
    assert_eq!(set(pool, &w.admin, "tag: any").await, [w.issues[0]]);
    assert_eq!(set(pool, &w.admin, "tag: none").await.len(), 5);
    assert_eq!(set(pool, &w.admin, "is: urgent").await, [w.issues[0]]);
    // Another person's tag is unknown to Amy, and her `tag: none` ignores it.
    assert_eq!(
        refused(pool, &amy, "tag: urgent").await,
        [ProblemCode::UnknownValue]
    );
    assert_eq!(set(pool, &amy, "tag: none").await.len(), 5);
}

#[tokio::test]
async fn qry_020_starred() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    issues::set_star(pool, &amy, w.issues[2], true)
        .await
        .unwrap();
    assert_eq!(set(pool, &amy, "is: starred").await, [w.issues[2]]);
    // The star is per person: amy's marker on its own does not star the issue for the admin —
    // the admin's own starred set is the creator's auto-watch, every issue they made (ISS-030).
    let starred = set(pool, &w.admin, "is: starred").await;
    assert!(w.issues.iter().all(|issue| starred.contains(issue)));
    let first = find(pool, &amy, "sort: starred").await;
    assert_eq!(first.first(), Some(&w.issues[2]));
    assert_eq!(set(pool, &amy, "not is: starred").await.len(), 4);
}

#[tokio::test]
async fn qry_030_link_side_names_both_ways() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let ops = project(pool, &w.admin, "Ops", "OPS").await;
    let hidden = make(pool, &w.admin, ops, "Hidden", &[]).await.issue.id;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let dependency = issue_links::list_types(pool, &w.admin)
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.link_type.name == "Dependency")
        .unwrap()
        .link_type
        .id;
    let link = |from: IssueId, to: IssueId| {
        issue_links::add(
            pool,
            &w.admin,
            WEB,
            from,
            issue_links::NewLinks {
                link_type: dependency,
                side: Side::Source,
                issues: vec![to],
            },
            false,
        )
    };
    // First blocks Second; Third blocks the hidden OPS issue.
    link(w.issues[0], w.issues[1]).await.unwrap();
    link(w.issues[2], hidden).await.unwrap();
    let id = |i: usize| format!("{}", w.issues[i]);
    assert_eq!(
        set(pool, &w.admin, &format!("blocks: {}", id(1))).await,
        [w.issues[0]]
    );
    assert_eq!(
        set(pool, &w.admin, &format!("\"blocked by\": {}", id(0))).await,
        [w.issues[1]]
    );
    assert!(
        set(pool, &w.admin, &format!("blocks: {}", id(0)))
            .await
            .is_empty()
    );
    assert_eq!(
        set(pool, &w.admin, &format!("linked: {}", id(0))).await,
        [w.issues[1]]
    );
    assert_eq!(
        set(pool, &w.admin, "blocks: any").await,
        sorted(vec![w.issues[0], w.issues[2]])
    );
    assert_eq!(set(pool, &w.admin, "links: any").await.len(), 4);
    // Amy cannot read the OPS issue: Third has no link she sees.
    assert_eq!(set(pool, &amy, "blocks: any").await, [w.issues[0]]);
    assert_eq!(
        set(pool, &amy, "links: any").await,
        sorted(vec![w.issues[0], w.issues[1]])
    );
    assert_eq!(set(pool, &amy, "links: none").await.len(), 3);
}

#[tokio::test]
async fn qry_033_absolute_dates_and_times() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let now = datetime!(2026-10-22 12:00 UTC);
    let stamps = [
        datetime!(2026-10-20 14:30 UTC),
        datetime!(2026-10-20 15:10 UTC),
        datetime!(2026-09-05 08:00 UTC),
        datetime!(2025-10-20 08:00 UTC),
        datetime!(2026-10-22 08:00 UTC),
    ];
    for (issue, t) in w.issues.iter().zip(stamps) {
        stamp(pool, *issue, "created_at", t).await;
    }
    stamp(
        pool,
        w.api_issue,
        "created_at",
        datetime!(2024-01-01 0:00 UTC),
    )
    .await;
    let admin = &w.admin;
    let get = |text: &str| {
        let text = text.to_owned();
        async move { sorted(found(pool, admin, at(&text, now, None)).await) }
    };
    let i = |n: usize| w.issues[n];
    assert_eq!(
        get("created: 2026-10").await,
        sorted(vec![i(0), i(1), i(4)])
    );
    assert_eq!(get("created: 10-20").await, sorted(vec![i(0), i(1)]));
    assert_eq!(get("created: 2026-10-20T14:30").await, [i(0)]);
    assert_eq!(
        get("created: 2026-10-20T14:00..2026-10-20T15:00").await,
        [i(0)]
    );
    assert_eq!(
        get("created > 2026-10-20T15:00").await,
        sorted(vec![i(1), i(4)])
    );
    assert_eq!(
        get("created: 2026-09, 2025-10-20").await,
        sorted(vec![i(2), i(3)])
    );
    assert_eq!(get("created: 2026-09..2026-10-20").await.len(), 3);
}

#[tokio::test]
async fn qry_034_relative_date_words() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let dates = project(pool, &admin, "Dates", "DAT").await;
    // Thursday.
    let now = datetime!(2026-10-22 12:00 UTC);
    let mut named = Vec::new();
    for (name, t) in [
        ("today", datetime!(2026-10-22 08:00 UTC)),
        ("yesterday", datetime!(2026-10-21 10:00 UTC)),
        ("tomorrow", datetime!(2026-10-23 10:00 UTC)),
        ("monday", datetime!(2026-10-19 10:00 UTC)),
        ("last week", datetime!(2026-10-14 10:00 UTC)),
        ("two weeks ago", datetime!(2026-10-07 10:00 UTC)),
        ("three weeks ago", datetime!(2026-09-30 10:00 UTC)),
        ("next week", datetime!(2026-10-27 10:00 UTC)),
        ("next month", datetime!(2026-11-05 10:00 UTC)),
        ("older", datetime!(2026-08-15 10:00 UTC)),
        ("older still", datetime!(2025-01-15 10:00 UTC)),
    ] {
        let id = make(pool, &admin, dates, name, &[]).await.issue.id;
        stamp(pool, id, "created_at", t).await;
        named.push((name, id));
    }
    let n = |name: &str| named.iter().find(|(n, _)| *n == name).unwrap().1;
    let get = |text: &str, now: OffsetDateTime| {
        let text = format!("project: dat created: {text}");
        let admin = &admin;
        async move { sorted(found(pool, admin, at(&text, now, None)).await) }
    };
    let many = |names: &[&str]| sorted(names.iter().map(|x| n(x)).collect());
    assert_eq!(get("today", now).await, many(&["today"]));
    assert_eq!(get("yesterday", now).await, many(&["yesterday"]));
    assert_eq!(get("tomorrow", now).await, many(&["tomorrow"]));
    assert_eq!(get("monday", now).await, many(&["monday"]));
    assert_eq!(get("wednesday", now).await, many(&["yesterday"]));
    assert_eq!(get("friday", now).await, many(&["tomorrow"]));
    assert_eq!(get("sunday", now).await, many(&[]));
    assert_eq!(
        get("this-week", now).await,
        many(&["today", "yesterday", "tomorrow", "monday"])
    );
    assert_eq!(get("last-week", now).await, many(&["last week"]));
    assert_eq!(get("next-week", now).await, many(&["next week"]));
    assert_eq!(get("two-weeks-ago", now).await, many(&["two weeks ago"]));
    assert_eq!(
        get("three-weeks-ago", now).await,
        many(&["three weeks ago"])
    );
    assert_eq!(
        get("this-month", now).await,
        many(&[
            "today",
            "yesterday",
            "tomorrow",
            "monday",
            "last week",
            "two weeks ago",
            "next week"
        ])
    );
    assert_eq!(get("last-month", now).await, many(&["three weeks ago"]));
    assert_eq!(get("next-month", now).await, many(&["next month"]));
    assert_eq!(get("older", now).await, many(&["older", "older still"]));
    assert_eq!(get("last-working-day", now).await, many(&["yesterday"]));
    // On a Monday the last working day is Friday.
    let monday = datetime!(2026-10-26 09:00 UTC);
    assert_eq!(get("last-working-day", monday).await, many(&["tomorrow"]));
    assert_eq!(get("..now", now).await.len(), 8);
    assert_eq!(get("today..", now).await.len(), 4);
    // Russian words mean the same.
    assert_eq!(get("вчера", now).await, many(&["yesterday"]));
    assert_eq!(get("прошлая-неделя", now).await, many(&["last week"]));
}

#[tokio::test]
async fn qry_053_project_and_tag_contexts() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let in_api = SearchInput {
        project: Some(w.api),
        ..input("")
    };
    assert_eq!(found(pool, &w.admin, in_api.clone()).await, [w.api_issue]);
    // The context is ANDed: a project condition cannot widen it.
    let widened = SearchInput {
        text: "project: web".to_owned(),
        ..in_api.clone()
    };
    assert!(found(pool, &w.admin, widened).await.is_empty());
    // Names resolve in the context's projects: WEB's empty label is unknown in API.
    assert!(matches!(
        search::search(pool, &amy, in_api).await,
        Err(Error::NotFound(_))
    ));
    let mine = taskmaster_service::tags::create(pool, &w.admin, "Mine", None)
        .await
        .unwrap();
    taskmaster_service::tags::set_on_issue(pool, &w.admin, WEB, w.issues[3], mine.id, true)
        .await
        .unwrap();
    let in_tag = SearchInput {
        tag: Some(mine.id),
        ..input("")
    };
    assert_eq!(found(pool, &w.admin, in_tag.clone()).await, [w.issues[3]]);
    assert!(matches!(
        search::search(pool, &amy, in_tag).await,
        Err(Error::NotFound(_))
    ));
    // Counting follows the context too.
    let (all, open) = search::count(
        pool,
        &w.admin,
        SearchInput {
            project: Some(w.web),
            ..input("")
        },
    )
    .await
    .unwrap();
    assert_eq!((all, open), (5, 4));
}

#[tokio::test]
async fn qry_054_hide_resolved_and_its_override() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let hiding = |text: &str| SearchInput {
        hide_resolved: true,
        ..input(text)
    };
    let shown = found(pool, &w.admin, hiding("project: web")).await;
    assert_eq!(shown.len(), 4);
    assert!(!shown.contains(&w.issues[3]));
    // A query about resolution wins, with a note.
    let page = search::search(pool, &w.admin, hiding("is: resolved"))
        .await
        .unwrap();
    assert_eq!(
        page.issues
            .issues
            .iter()
            .map(|v| v.issue.id)
            .collect::<Vec<_>>(),
        [w.issues[3]]
    );
    assert!(
        page.notes
            .iter()
            .any(|p| p.code == ProblemCode::HideResolvedIgnored)
    );
    assert_eq!(
        found(pool, &w.admin, hiding("state: fixed")).await,
        [w.issues[3]]
    );
    // One issue by ID is shown whatever its state.
    let page = search::search(pool, &w.admin, hiding(&format!("id: {}", w.issues[3])))
        .await
        .unwrap();
    assert_eq!(page.single_id, Some(w.issues[3]));
    assert_eq!(page.issues.issues.len(), 1);
}

#[tokio::test]
async fn qry_062_russian_queries_find_the_same() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    assign(pool, &w, w.issues[1], amy.user()).await;
    for (en, ru) in [
        ("project: web", "проект: web"),
        ("is: resolved", "это: решена"),
        ("is: unresolved", "это: открыта"),
        ("reporter: me", "автор: я"),
        ("assignee: none", "assignee: нет"),
        ("assignee: any", "assignee: есть"),
        (
            "project: web and not priority: high",
            "проект: web и не priority: high",
        ),
        (
            "priority: high or priority: low",
            "priority: high или priority: low",
        ),
        ("resolved: none", "решена: пусто"),
        ("comments: none", "комментарии: нет"),
    ] {
        assert_eq!(
            set(pool, &amy, en).await,
            set(pool, &amy, ru).await,
            "{en} / {ru}"
        );
    }
    assert_eq!(
        find(pool, &w.admin, "project: web sort: summary desc").await,
        find(pool, &w.admin, "проект: web сортировка: заголовок убыв").await
    );
    assert_eq!(set(pool, &amy, "assignee: мне").await, [w.issues[1]]);
}

#[tokio::test]
async fn fld_056_empty_labels_per_project() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    for (project, label) in [(w.web, "Nobody yet"), (w.api, "Unowned")] {
        let row = project_fields::list(pool, &w.admin, project)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.field.id == w.assignee)
            .unwrap();
        project_fields::update(
            pool,
            &w.admin,
            project,
            w.assignee,
            SettingsChange {
                can_be_empty: None,
                empty_label: Some(label.to_owned()),
                default_values: None,
                private: None,
                version: row.attached.version,
            },
        )
        .await
        .unwrap();
    }
    let web = sorted(w.issues.clone());
    assert_eq!(set(pool, &w.admin, "assignee: \"nobody yet\"").await, web);
    assert_eq!(
        set(pool, &w.admin, "assignee: unowned").await,
        [w.api_issue]
    );
    assert_eq!(set(pool, &w.admin, "is: unowned").await, [w.api_issue]);
    assert_eq!(set(pool, &w.admin, "assignee: none").await.len(), 6);
    assert_eq!(
        refused(pool, &w.admin, "project: api assignee: \"nobody yet\"").await,
        [ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn fld_062_private_values_match_only_for_readers() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let row = project_fields::list(pool, &w.admin, w.web)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.field.id == w.priority)
        .unwrap();
    project_fields::update(
        pool,
        &w.admin,
        w.web,
        w.priority,
        SettingsChange {
            can_be_empty: None,
            empty_label: None,
            default_values: None,
            private: Some(true),
            version: row.attached.version,
        },
    )
    .await
    .unwrap();
    // Reader: a custom role that reads issues but not private fields.
    let reader = create_user(pool, &w.admin, "rita", "Rita Reader").await;
    let role = roles::create(
        pool,
        &w.admin,
        roles::NewRole {
            name: "Reader".to_owned(),
            description: String::new(),
            permissions: ["project.basic", "issue.read"].map(str::to_owned).to_vec(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        &w.admin,
        role.role.id,
        Holder::User(reader),
        &[Some(w.web), Some(w.api)],
    )
    .await
    .unwrap();
    let rita = access::load(pool, reader).await.unwrap();
    let amy = person(
        pool,
        &w.admin,
        "amy",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    let high = sorted(vec![w.issues[0], w.issues[3]]);
    assert_eq!(set(pool, &amy, "priority: high").await, high);
    assert!(set(pool, &rita, "priority: high").await.is_empty());
    // The negation is uniformly true where the field is hidden: it tells nothing.
    assert_eq!(set(pool, &rita, "not priority: high").await.len(), 6);
    assert_eq!(set(pool, &rita, "priority: !high").await.len(), 6);
    assert!(
        set(pool, &rita, "project: web priority: none")
            .await
            .is_empty()
    );
    assert!(set(pool, &rita, "project: web is: high").await.is_empty());
    // Sorting puts hidden values among the empty ones: newest first by the tie-breaker.
    let by_priority = find(pool, &rita, "project: web sort: priority").await;
    let mut newest = w.issues.clone();
    newest.reverse();
    assert_eq!(by_priority, newest);
}

fn taken_username<T>(result: &Result<T, Error>) -> bool {
    matches!(result, Err(Error::Invalid(problems))
        if problems.iter().any(|p| p.path == "username" && p.problem == InputProblem::Taken))
}

#[tokio::test]
async fn acc_017_query_words_are_reserved_usernames() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    for name in ["none", "Open", "me", "or"] {
        let result = users::create(
            pool,
            &passwords(),
            &admin,
            NewAccount {
                username: name.to_owned(),
                full_name: "Some One".to_owned(),
                email: format!("{}@example.com", name.to_lowercase()),
                password: "correct horse".to_owned(),
                must_change_password: false,
            },
        )
        .await;
        assert!(taken_username(&result), "{name}: {result:?}");
    }
    let ann = create_user(pool, &admin, "ann", "Ann Person").await;
    let rename = |to: &str| users::IdentityInput {
        username: Some(to.to_owned()),
        full_name: None,
        email: None,
        current_password: None,
        version: 0,
    };
    let letters = taskmaster_service::mail::Letters {
        enabled: false,
        base_url: None,
        language: Language::En,
    };
    let result = users::update_identity(
        pool,
        &passwords(),
        &admin,
        &letters,
        ann,
        rename("starred"),
        T0,
    )
    .await;
    assert!(taken_username(&result), "{result:?}");
    // An account that took such a name earlier keeps it.
    sqlx::query("update users set username = 'any' where id = $1")
        .bind(ann)
        .execute(pool)
        .await
        .unwrap();
    let version: i64 = sqlx::query_scalar("select version from users where id = $1")
        .bind(ann)
        .fetch_one(pool)
        .await
        .unwrap();
    let kept = users::update_identity(
        pool,
        &passwords(),
        &admin,
        &letters,
        ann,
        users::IdentityInput {
            full_name: Some("Ann Other".to_owned()),
            version,
            ..rename("any")
        },
        T0,
    )
    .await
    .unwrap();
    assert_eq!(kept.username.as_str(), "any");
}

/// Every kind of text and mention condition runs (the full checks are the `qry_036`… tests).
#[tokio::test]
async fn text_and_mention_queries_run() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let first = w.issues[0];
    assert_eq!(set(pool, &w.admin, "first").await, [first]);
    assert_eq!(set(pool, &w.admin, "seconds").await, [w.issues[1]]);
    assert_eq!(set(pool, &w.admin, "\"first\"").await, [first]);
    assert!(set(pool, &w.admin, "\"firsts\"").await.is_empty());
    assert_eq!(
        set(pool, &w.admin, "summary: \"api one\"").await,
        [w.api_issue]
    );
    assert_eq!(set(pool, &w.admin, "the").await.len(), 6);
    assert_eq!(set(pool, &w.admin, "WEB-1").await, [first]);
    for text in [
        "description: first",
        "comments: first",
        "text: first, !second",
        "mentions: me",
        "mentions: WEB-2",
        "mentioned-in: WEB-1",
    ] {
        assert!(set(pool, &w.admin, text).await.len() <= 1, "{text}");
    }
    // Relevance pages with a cursor.
    let mut page = SearchInput {
        limit: 1,
        ..input("first or second")
    };
    let one = search::search(pool, &w.admin, page.clone()).await.unwrap();
    assert_eq!(one.issues.issues.len(), 1);
    page.cursor = one.next_cursor;
    let two = search::search(pool, &w.admin, page).await.unwrap();
    assert_eq!(two.issues.issues.len(), 1);
    assert_ne!(one.issues.issues[0].issue.id, two.issues.issues[0].issue.id);
    assert!(two.next_cursor.is_none());
}

#[tokio::test]
async fn qry_086_the_answer_names_its_topic() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let admin = &w.admin;
    let topic = |input: SearchInput| async move {
        search::search(pool, admin, input).await.unwrap().live_topic
    };
    assert_eq!(topic(input("priority: high")).await, "list:all");
    // `or` does not narrow: the list may hold issues of any project.
    assert_eq!(
        topic(input("project: web or priority: high")).await,
        "list:all"
    );
    assert_eq!(
        topic(input("project: web priority: high")).await,
        format!("list:{}", w.web)
    );
    let (low, high) = (w.web.min(w.api), w.web.max(w.api));
    assert_eq!(
        topic(input("project: web, api")).await,
        format!("list:{low},{high}")
    );
    let within = SearchInput {
        project: Some(w.api),
        ..input("")
    };
    assert_eq!(topic(within).await, format!("list:{}", w.api));
    // More than a list topic holds: every project.
    let mut names = vec!["Web".to_owned(), "Api".to_owned()];
    for n in 0..99 {
        let name = format!("P{n}");
        project(pool, admin, &name, &format!("P{n}X")).await;
        names.push(name);
    }
    let all = format!("project: {}", names.join(", "));
    assert_eq!(topic(input(&all)).await, "list:all");
}

/// A scrum board "Team" over WEB with sprints "Sprint 1" (the first, ended), "Sprint 2" (runs
/// today), "Old" (archived); the issues 0 and 1 in Sprint 1, 2 in Sprint 2, 3 in Old.
async fn team_board(pool: &PgPool, w: &World) -> (BoardId, [SprintId; 3]) {
    use taskmaster_service::boards::{self, NewBoard, Sharing};
    let board = boards::create(
        pool,
        &w.admin,
        Language::En,
        NewBoard {
            preset: Preset::Scrum,
            name: "Team".to_owned(),
            projects: vec![w.web],
            sharing: Sharing::default(),
        },
    )
    .await
    .unwrap();
    let id = board.full.board.id;
    let first = board.full.sprints.iter().find(|s| !s.internal).unwrap().id;
    sqlx::query(
        "update sprints set name = 'Sprint 1', starts_on = current_date - 20,
                ends_on = current_date - 10 where id = $1",
    )
    .bind(first)
    .execute(pool)
    .await
    .unwrap();
    let insert = |sql: &'static str| async move {
        sqlx::query_scalar::<_, SprintId>(sql)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    };
    let second = insert(
        "insert into sprints (board_id, name, starts_on, ends_on)
         values ($1, 'Sprint 2', current_date - 3, current_date + 7) returning id",
    )
    .await;
    let old = insert(
        "insert into sprints (board_id, name, archived_at) values ($1, 'Old', now()) returning id",
    )
    .await;
    for (sprint, issue) in [(first, 0), (first, 1), (second, 2), (old, 3)] {
        sqlx::query("insert into sprint_issues (sprint_id, issue_id, rank) values ($1, $2, 'm')")
            .bind(sprint)
            .bind(w.issues[issue])
            .execute(pool)
            .await
            .unwrap();
    }
    (id, [first, second, old])
}

#[tokio::test]
async fn qry_049_board_and_sprint_conditions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    team_board(pool, &w).await;
    let i = &w.issues;
    let a = &w.admin;
    let on_board = sorted(vec![i[0], i[1], i[2], i[3]]);
    assert_eq!(set(pool, a, "board: team").await, on_board);
    assert_eq!(set(pool, a, "доска: Team").await, on_board);
    assert_eq!(set(pool, a, "board: any").await, on_board);
    assert_eq!(
        set(pool, a, "board: none").await,
        sorted(vec![i[4], w.api_issue])
    );
    assert_eq!(
        set(pool, a, "!board: Team").await,
        sorted(vec![i[4], w.api_issue])
    );
    assert_eq!(
        set(pool, a, "sprint: \"Sprint 1\"").await,
        sorted(vec![i[0], i[1]])
    );
    assert_eq!(
        set(pool, a, "sprint: \"Team/Sprint 1\"").await,
        sorted(vec![i[0], i[1]])
    );
    assert_eq!(set(pool, a, "sprint: current").await, vec![i[2]]);
    assert_eq!(set(pool, a, "спринт: \"team/текущий\"").await, vec![i[2]]);
    // Archived sprints resolve.
    assert_eq!(set(pool, a, "sprint: Old").await, vec![i[3]]);
    assert_eq!(
        set(pool, a, "board: Team Priority: High").await,
        vec![i[0], i[3]]
    );
    assert_eq!(
        refused(pool, a, "sprint: \"Team/Nope\"").await,
        vec![ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn qry_049_invisible_boards_are_unknown() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let (board, _) = team_board(pool, &w).await;
    let reader = person(
        pool,
        &w.admin,
        "reader",
        PredefinedRole::ProjectMember,
        &[w.web],
    )
    .await;
    // A private board of the admin: the reader does not know it.
    assert_eq!(
        refused(pool, &reader, "board: Team").await,
        vec![ProblemCode::UnknownValue]
    );
    assert_eq!(
        refused(pool, &reader, "sprint: \"Sprint 1\"").await,
        vec![ProblemCode::UnknownValue]
    );
    assert!(set(pool, &reader, "board: any").await.is_empty());
    assert_eq!(set(pool, &reader, "board: none").await.len(), 5);
    // A query board counts by its query (here none stored: nothing matches); its card rows,
    // sprints and `any` do not count.
    sqlx::query("update boards set mode = 'query' where id = $1")
        .bind(board)
        .execute(pool)
        .await
        .unwrap();
    assert!(set(pool, &w.admin, "board: Team").await.is_empty());
    assert!(set(pool, &w.admin, "board: any").await.is_empty());
    assert_eq!(
        refused(pool, &w.admin, "sprint: \"Team/Sprint 1\"").await,
        vec![ProblemCode::UnknownValue]
    );
}

#[tokio::test]
async fn qry_049_saved_search_follows_renames() {
    use taskmaster_service::saved_searches;
    let db = TestDb::new().await;
    let pool = db.pool();
    let w = world(pool).await;
    let (board, [first, _, _]) = team_board(pool, &w).await;
    let saved = saved_searches::create(
        pool,
        &w.admin,
        "Mine",
        "board: Team sprint: \"Team/Sprint 1\" or sprint: \"Team/current\"",
    )
    .await
    .unwrap();
    sqlx::query("update boards set name = 'Core' where id = $1")
        .bind(board)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("update sprints set name = 'Start' where id = $1")
        .bind(first)
        .execute(pool)
        .await
        .unwrap();
    let again = saved_searches::get(pool, &w.admin, saved.row.id)
        .await
        .unwrap();
    assert_eq!(
        again.text,
        "board: Core sprint: Core/Start or sprint: Core/current"
    );
    assert!(again.problems.is_empty());
    assert_eq!(
        set(pool, &w.admin, &again.text).await,
        sorted(vec![w.issues[0], w.issues[1], w.issues[2]])
    );
    // A deleted board keeps its words.
    sqlx::query("delete from boards where id = $1")
        .bind(board)
        .execute(pool)
        .await
        .unwrap();
    let gone = saved_searches::get(pool, &w.admin, saved.row.id)
        .await
        .unwrap();
    assert_eq!(gone.problems.len(), 3);
    assert!(
        gone.problems
            .iter()
            .all(|p| p.code == ProblemCode::DeletedReference)
    );
}

// ---- 2.1b: the query bar's assist (plan §5, QRY-056…QRY-058) ----

use taskmaster_query::completion::Group;
use taskmaster_service::assist;

/// A value in a project's own set, as the commands tests make theirs.
async fn value_of(pool: &PgPool, actor: &Access, project: ProjectId, field: &str, name: &str) {
    let probe = issues::create(
        pool,
        actor,
        WEB,
        issues::NewIssue {
            project,
            summary: "Probe".to_owned(),
            publish: true,
            ..issues::NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    let view = issues::get(pool, actor, probe).await.unwrap();
    let set = view
        .0
        .fields
        .iter()
        .find(|f| f.field.field.name == field)
        .unwrap()
        .field
        .attached
        .set_id
        .unwrap();
    value_sets::add_value(
        pool,
        actor,
        set,
        ValueInput {
            name: Some(name.to_owned()),
            ..ValueInput::default()
        },
        None,
    )
    .await
    .unwrap();
    issues::delete(pool, actor, WEB, probe).await.unwrap();
}

/// Caret-aware suggestions (QRY-056): an empty bar shows attributes and keywords, an
/// attribute's values follow it, a partial word matches attributes and values, and the sort
/// spec offers its keys and directions.
#[tokio::test]
async fn qry_056_completion_by_caret() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    value_of(pool, &admin, web, "Fix versions", "Static").await;

    // An empty bar: the built-in attributes, the fields, the structural keywords.
    let answer = assist::query(pool, &admin, "", 0, None, None)
        .await
        .unwrap();
    let texts = |answer: &assist::QueryAssistAnswer, group: Group| {
        answer
            .suggestions
            .iter()
            .filter(|s| s.group == group)
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
    };
    let attributes = texts(&answer, Group::Attribute);
    assert!(attributes.contains(&"project".to_owned()), "{attributes:?}");
    assert!(attributes.contains(&"State".to_owned()), "{attributes:?}");
    let keywords = texts(&answer, Group::Keyword);
    assert!(keywords.contains(&"sort".to_owned()), "{keywords:?}");
    assert!(
        answer
            .suggestions
            .iter()
            .all(|s| !matches!(s.group, Group::Value))
    );

    // After an attribute: its values, labelled with it.
    let answer = assist::query(pool, &admin, "state: ", 7, None, None)
        .await
        .unwrap();
    let found = answer
        .suggestions
        .iter()
        .find(|s| s.text == "\"In progress\"")
        .unwrap();
    assert_eq!(found.description, "State");

    // A partial word matches attributes and values both.
    let answer = assist::query(pool, &admin, "sta", 3, None, None)
        .await
        .unwrap();
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::Attribute && s.text == "State")
    );
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::Value && s.text == "Static")
    );

    // The sort spec: its keys, then the directions after one.
    let answer = assist::query(pool, &admin, "sort: ", 6, None, None)
        .await
        .unwrap();
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::SortKey && s.text == "updated")
    );
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::SortKey && s.text == "State")
    );
    let answer = assist::query(pool, &admin, "sort: updated ", 14, None, None)
        .await
        .unwrap();
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::SortDirection && s.text == "desc")
    );
}

/// Context- and permission-aware suggestions (QRY-057): values of the projects the query
/// implies rank above the rest, users follow the directory right, and relative dates answer
/// their concrete value.
#[tokio::test]
async fn qry_057_context_and_permission_aware_suggestions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    project(pool, &admin, "Web", "WEB").await;
    let mars = project(pool, &admin, "Mars", "MARS").await;
    value_of(pool, &admin, mars, "State", "In mars").await;
    let bob = users::create(
        pool,
        &passwords(),
        &admin,
        users::NewAccount {
            username: "bob".to_owned(),
            full_name: "Bob Person".to_owned(),
            email: "bob@example.com".to_owned(),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let bob = access::load(pool, bob).await.unwrap();

    // The query names one project: its values rank above the other project's.
    let answer = assist::query(pool, &admin, "project: Web state: ", 21, None, None)
        .await
        .unwrap();
    let values: Vec<&str> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::Value)
        .map(|s| s.text.as_str())
        .collect();
    // The named project's values rank above the other project's (QRY-057).
    let web_at = values
        .iter()
        .position(|value| *value == "\"In progress\"")
        .unwrap();
    let mars_at = values
        .iter()
        .position(|value| *value == "\"In mars\"")
        .unwrap();
    assert!(mars_at > web_at, "{values:?}");

    // Users: everyone holds the directory right through All members (ACC-021); without it,
    // only the account itself.
    let answer = assist::query(pool, &bob, "reporter: ", 10, None, None)
        .await
        .unwrap();
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.group == Group::User && s.text == "admin")
    );
    let all_members = taskmaster_store::groups::builtin_ids(pool).await.unwrap().1;
    let assignment = roles::assignments_of_holder(
        pool,
        &admin,
        taskmaster_store::access::Holder::Group(all_members),
    )
    .await
    .unwrap()[0]
        .id;
    roles::revoke(pool, &admin, assignment).await.unwrap();
    let bob = access::load(pool, bob.user()).await.unwrap();
    let answer = assist::query(pool, &bob, "reporter: ", 10, None, None)
        .await
        .unwrap();
    let users: Vec<&str> = answer
        .suggestions
        .iter()
        .filter(|s| s.group == Group::User)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(users, ["bob"]);

    // A relative date answers its concrete value in the caller's zone.
    let answer = assist::query(
        pool,
        &admin,
        "created: ",
        9,
        None,
        Some("Europe/Moscow".to_owned()),
    )
    .await
    .unwrap();
    let today = answer
        .suggestions
        .iter()
        .find(|s| s.text == "today")
        .unwrap();
    assert_eq!(
        today.resolved.as_deref(),
        Some(time::OffsetDateTime::now_utc().date().to_string().as_str())
    );
}

/// The assist answers the check (QRY-058): the tokens for coloring and the problems for the
/// markers ride along with the suggestions.
#[tokio::test]
async fn qry_058_the_assist_answers_the_check() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    project(pool, &admin, "Web", "WEB").await;

    let answer = assist::query(pool, &admin, "state: Nope", 11, None, None)
        .await
        .unwrap();

    assert!(
        answer
            .check
            .problems
            .iter()
            .any(|problem| problem.code == ProblemCode::UnknownValue)
    );
    assert!(
        answer
            .check
            .tokens
            .iter()
            .any(|token| token.1 == taskmaster_query::Class::Attribute)
    );
    // A value that matches still completes while the problem marks the unknown one.
    let answer = assist::query(pool, &admin, "state: In Pro", 13, None, None)
        .await
        .unwrap();
    assert!(
        answer
            .suggestions
            .iter()
            .any(|s| s.text == "\"In progress\"")
    );
}
