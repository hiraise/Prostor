//! Comments, rich text and the activity stream on a real database (step 1.4a): posting, rights
//! and audiences, edits with conflicts, placeholders and restoring, the stream with its filters,
//! grouping and the page around a comment, mentions and issue IDs, project descriptions, the
//! re-rendering job, and what deleting accounts and groups does to comments.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prostor_domain::access::capability::CapabilitySet;
use prostor_domain::access::role::PredefinedRole;
use prostor_domain::access::{Access, Grant, Scope};
use prostor_domain::issues::history::Origin;
use prostor_domain::projects::Template;
use prostor_domain::settings::Language;
use prostor_domain::validation::Problem;
use prostor_domain::{CommentId, IssueId, ProjectId, UserId};
use prostor_service::auth::SignedIn;
use prostor_service::comments::{self, CommentChange, NewComment};
use prostor_service::issues::activity::{self, Item, Query};
use prostor_service::issues::{self, AudienceInput, IssueChange, IssueView, NewIssue};
use prostor_service::projects::{self, NewProjectInput, ProjectChange};
use prostor_service::setup::{self, Setup};
use prostor_service::users::{self, NewAccount};
use prostor_service::{Error, Passwords, access, groups, roles, text};
use prostor_store::PgPool;
use prostor_store::access::Holder;
use prostor_store::issue_history::Direction;
use prostor_store::sessions::Origin as SessionOrigin;
use prostor_testkit::TestDb;
use serde_json::json;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;

fn passwords() -> Passwords {
    Passwords::new(4)
}

async fn set_up(pool: &PgPool) -> SignedIn {
    setup::prepare(pool, Some("TEST-CODE-2345")).await.unwrap();
    setup::run(
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
    .unwrap()
}

async fn admin(pool: &PgPool) -> Access {
    let signed_in = set_up(pool).await;
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

async fn person(
    pool: &PgPool,
    actor: &Access,
    username: &str,
    role: PredefinedRole,
    of: &[ProjectId],
) -> Access {
    let user = create_user(pool, actor, username).await;
    let role = prostor_store::roles::builtin_id(pool, role.key())
        .await
        .unwrap()
        .unwrap();
    let scopes: Vec<Option<ProjectId>> = of.iter().copied().map(Some).collect();
    roles::assign(pool, actor, role, Holder::User(user), &scopes)
        .await
        .unwrap();
    access::load(pool, user).await.unwrap()
}

/// A person holding a custom role of these permissions in a project.
async fn custom(
    pool: &PgPool,
    admin: &Access,
    username: &str,
    permissions: &[&str],
    project: ProjectId,
) -> Access {
    let user = create_user(pool, admin, username).await;
    let role = roles::create(
        pool,
        admin,
        roles::NewRole {
            name: format!("Role of {username}"),
            description: String::new(),
            permissions: permissions.iter().map(|p| (*p).to_owned()).collect(),
        },
    )
    .await
    .unwrap();
    roles::assign(
        pool,
        admin,
        role.role.id,
        Holder::User(user),
        &[Some(project)],
    )
    .await
    .unwrap();
    access::load(pool, user).await.unwrap()
}

async fn create(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueView {
    issues::create(
        pool,
        actor,
        WEB,
        NewIssue {
            project,
            summary: summary.to_owned(),
            publish: true,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
}

async fn comment(pool: &PgPool, actor: &Access, issue: IssueId, text: &str) -> CommentId {
    comments::create(
        pool,
        actor,
        WEB,
        issue,
        NewComment {
            text: text.to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .comment
    .id
}

fn query() -> Query {
    Query {
        comments: true,
        changes: true,
        from: None,
        direction: Direction::Older,
        limit: 50,
        around_comment: None,
    }
}

/// The comments a viewer sees in the stream, oldest first, `None` for placeholders.
async fn seen(pool: &PgPool, viewer: &Access, issue: IssueId) -> Vec<Option<String>> {
    let page = activity::page(
        pool,
        viewer,
        issue,
        Query {
            changes: false,
            ..query()
        },
    )
    .await
    .unwrap();
    page.items
        .iter()
        .rev()
        .filter_map(|item| match item {
            Item::Comment { comment, .. } => {
                Some((!comment.is_deleted()).then(|| comment.comment.text.clone()))
            }
            Item::Change { .. } => None,
        })
        .collect()
}

fn problems(error: Error) -> Vec<(String, Problem)> {
    match error {
        Error::Invalid(problems) => problems.into_iter().map(|p| (p.path, p.problem)).collect(),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

#[tokio::test]
async fn cnt_001_hist_006_comments_are_posted_on_published_issues() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Crash").await;
    let before = issue.issue.updated_at;

    let (posted, _) = comments::create(
        pool,
        &admin,
        WEB,
        issue.issue.id,
        NewComment {
            text: "Seen **twice** \n\n".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(posted.comment.text, "Seen **twice**");
    assert_eq!(posted.comment.html, "<p>Seen <strong>twice</strong></p>\n");
    assert_eq!(posted.comment.updated_at, None);
    assert!(posted.can.update && posted.can.delete && !posted.can.restore);
    // ISS-001: commenting counts as an update of the issue.
    let after = issues::get(pool, &admin, issue.issue.id).await.unwrap().0;
    assert!(after.issue.updated_at >= before);
    assert!(after.issue.version > issue.issue.version);
    // HIST-006: the comment is a stream item at its posting record.
    let page = activity::page(pool, &admin, issue.issue.id, query())
        .await
        .unwrap();
    assert!(matches!(&page.items[0], Item::Comment { comment, entry }
        if comment.comment.id == posted.comment.id && entry.kind == "comment"));
    assert!(matches!(&page.items[1], Item::Change { entry, .. } if entry.kind == "created"));

    // Empty texts, drafts and archived projects are refused.
    let empty = comments::create(
        pool,
        &admin,
        WEB,
        issue.issue.id,
        NewComment {
            text: "  \n ".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(problems(empty), [("text".to_owned(), Problem::Required)]);
    let draft = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            project: web,
            publish: false,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0;
    let on_draft = comments::create(
        pool,
        &admin,
        WEB,
        draft.issue.id,
        NewComment {
            text: "x".to_owned(),
            ..NewComment::default()
        },
    )
    .await;
    assert!(matches!(on_draft, Err(Error::Conflict(_))));
    projects::set_archived(pool, &admin, web, true)
        .await
        .unwrap();
    let archived = comments::create(
        pool,
        &admin,
        WEB,
        issue.issue.id,
        NewComment {
            text: "x".to_owned(),
            ..NewComment::default()
        },
    )
    .await;
    assert!(matches!(archived, Err(Error::Conflict(_))));
    // Nothing may be done to comments of an archived project.
    let shown = comments::get(pool, &admin, posted.comment.id)
        .await
        .unwrap()
        .0;
    assert_eq!(shown.can, comments::Abilities::default());
}

#[tokio::test]
async fn cnt_002_each_permission_on_its_own() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Crash").await.issue.id;
    let first = comment(pool, &admin, issue, "From the admin").await;

    // Reading the issue does not mean reading its comments.
    let reader = custom(pool, &admin, "rita", &["project.basic", "issue.read"], web).await;
    assert!(issues::get(pool, &reader, issue).await.is_ok());
    assert!(seen(pool, &reader, issue).await.is_empty());
    assert!(matches!(
        comments::get(pool, &reader, first).await,
        Err(Error::NotFound(_))
    ));
    let commenter = custom(
        pool,
        &admin,
        "carl",
        &["project.basic", "issue.read", "comment.read"],
        web,
    )
    .await;
    assert_eq!(seen(pool, &commenter, issue).await.len(), 1);
    // Reading is not writing.
    let refused = comments::create(
        pool,
        &commenter,
        WEB,
        issue,
        NewComment {
            text: "x".to_owned(),
            ..NewComment::default()
        },
    )
    .await;
    assert!(matches!(refused, Err(Error::Forbidden(_))));

    // Own and others' comments are separate rights.
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let own = comment(pool, &member, issue, "Mine").await;
    let mine = comments::get(pool, &member, own).await.unwrap().0;
    assert!(mine.can.update && mine.can.delete);
    let theirs = comments::get(pool, &member, first).await.unwrap().0;
    assert!(!theirs.can.update && !theirs.can.delete);
    let change = CommentChange {
        text: Some("Edited".to_owned()),
        version: theirs.comment.version,
        ..CommentChange::default()
    };
    assert!(matches!(
        comments::update(pool, &member, WEB, first, change).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        comments::delete(pool, &member, WEB, first).await,
        Err(Error::Forbidden(_))
    ));
    // An administrator of the project changes and deletes others' comments.
    let manager = person(pool, &admin, "pam", PredefinedRole::ProjectAdmin, &[web]).await;
    comments::update(
        pool,
        &manager,
        WEB,
        own,
        CommentChange {
            text: Some("Fixed a typo".to_owned()),
            version: mine.comment.version,
            ..CommentChange::default()
        },
    )
    .await
    .unwrap();
    comments::delete(pool, &manager, WEB, own).await.unwrap();
}

#[tokio::test]
async fn cnt_003_hist_007_acc_049_audiences_only_narrow() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let insider = person(pool, &admin, "ivy", PredefinedRole::ProjectMember, &[web]).await;
    let other = person(pool, &admin, "otto", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Crash").await.issue.id;
    comment(pool, &admin, issue, "For everyone").await;
    let (restricted, _) = comments::create(
        pool,
        &member,
        WEB,
        issue,
        NewComment {
            text: "For Ivy".to_owned(),
            audience: Some(AudienceInput {
                users: vec![insider.user()],
                groups: vec![],
            }),
            silent: false,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .done;
    // The author, the audience and those who bypass audiences see it; others do not.
    assert_eq!(seen(pool, &member, issue).await.len(), 2);
    assert_eq!(seen(pool, &insider, issue).await.len(), 2);
    assert_eq!(seen(pool, &admin, issue).await.len(), 2);
    assert_eq!(
        seen(pool, &other, issue).await,
        [Some("For everyone".to_owned())]
    );
    assert!(matches!(
        comments::get(pool, &other, restricted.comment.id).await,
        Err(Error::NotFound(_))
    ));

    // A comment's audience cannot widen the issue's: someone in the comment's audience who may
    // not see the issue sees nothing.
    let private = issues::create(
        pool,
        &member,
        WEB,
        NewIssue {
            project: web,
            summary: "Private".to_owned(),
            publish: true,
            audience: Some(AudienceInput {
                users: vec![member.user()],
                groups: vec![],
            }),
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    let (widened, _) = comments::create(
        pool,
        &member,
        WEB,
        private,
        NewComment {
            text: "Hi Ivy".to_owned(),
            audience: Some(AudienceInput {
                users: vec![insider.user()],
                groups: vec![],
            }),
            silent: false,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert!(matches!(
        comments::get(pool, &insider, widened.comment.id).await,
        Err(Error::NotFound(_))
    ));
    assert!(
        activity::page(pool, &insider, private, query())
            .await
            .is_err()
    );

    // Silent comments need `issue.silent`.
    let silent = comments::create(
        pool,
        &member,
        WEB,
        issue,
        NewComment {
            text: "x".to_owned(),
            silent: true,
            ..NewComment::default()
        },
    )
    .await;
    assert!(matches!(silent, Err(Error::Forbidden(_))));
}

#[tokio::test]
async fn cnt_004_edits_keep_both_versions() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Crash").await.issue.id;
    let id = comment(pool, &member, issue, "Frist").await;
    let original = comments::get(pool, &member, id).await.unwrap().0;

    let (edited, _) = comments::update(
        pool,
        &member,
        WEB,
        id,
        CommentChange {
            text: Some("First".to_owned()),
            audience: Some(AudienceInput {
                users: vec![member.user()],
                groups: vec![],
            }),
            version: original.comment.version,
            silent: false,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(edited.comment.text, "First");
    assert_eq!(edited.comment.html, "<p>First</p>\n");
    assert_eq!(edited.comment.audience_users, [member.user()]);
    assert_eq!(edited.comment.updated_by, Some(member.user()));
    assert!(edited.comment.updated_at.is_some());
    // The edit is recorded with both versions, but it is no stream item.
    let records = prostor_store::issue_history::all_of(pool, issue)
        .await
        .unwrap();
    let record = records.iter().find(|e| e.kind == "comment_edited").unwrap();
    assert_eq!(record.comment_id, Some(id));
    assert_eq!(
        record.removed,
        Some(json!({ "text": "Frist", "users": [], "groups": [] }))
    );
    assert_eq!(
        record.added,
        Some(json!({ "text": "First", "users": [member.user()], "groups": [] }))
    );
    let page = activity::page(pool, &admin, issue, query()).await.unwrap();
    assert!(
        page.items
            .iter()
            .all(|i| i.entry().kind != "comment_edited")
    );
    // An edit based on an older version is refused.
    let stale = comments::update(
        pool,
        &member,
        WEB,
        id,
        CommentChange {
            text: Some("Third".to_owned()),
            version: original.comment.version,
            ..CommentChange::default()
        },
    )
    .await;
    assert!(matches!(stale, Err(Error::Conflict(_))));
    // An edit that changes nothing writes nothing.
    comments::update(
        pool,
        &member,
        WEB,
        id,
        CommentChange {
            text: Some("First".to_owned()),
            version: edited.comment.version,
            ..CommentChange::default()
        },
    )
    .await
    .unwrap();
    let again = prostor_store::issue_history::all_of(pool, issue)
        .await
        .unwrap();
    assert_eq!(again.len(), records.len());
}

#[tokio::test]
async fn cnt_005_deleted_comments_are_placeholders_until_restored() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let other = person(pool, &admin, "otto", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Crash").await.issue.id;
    let id = comment(pool, &member, issue, "Oops").await;

    comments::delete(pool, &member, WEB, id).await.unwrap();
    assert_eq!(seen(pool, &other, issue).await, [None]);
    let placeholder = comments::get(pool, &member, id).await.unwrap().0;
    assert!(placeholder.is_deleted());
    assert_eq!(placeholder.comment.deleted_by, Some(member.user()));
    assert!(placeholder.can.restore && !placeholder.can.update && !placeholder.can.delete);
    // Deleting again changes nothing; editing a deleted comment is refused.
    comments::delete(pool, &member, WEB, id).await.unwrap();
    let edit = comments::update(
        pool,
        &member,
        WEB,
        id,
        CommentChange {
            text: Some("x".to_owned()),
            version: placeholder.comment.version,
            ..CommentChange::default()
        },
    )
    .await;
    assert!(matches!(edit, Err(Error::Conflict(_))));
    // Only who could delete it restores it.
    let not_theirs = comments::get(pool, &other, id).await.unwrap().0;
    assert!(!not_theirs.can.restore);
    assert!(matches!(
        comments::restore(pool, &other, WEB, id).await,
        Err(Error::Forbidden(_))
    ));
    let (back, _) = comments::restore(pool, &member, WEB, id).await.unwrap();
    assert!(!back.is_deleted());
    assert_eq!(seen(pool, &other, issue).await, [Some("Oops".to_owned())]);
    let kinds: Vec<String> = prostor_store::issue_history::all_of(pool, issue)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        kinds,
        ["created", "comment", "comment_deleted", "comment_restored"]
    );
}

#[tokio::test]
async fn cnt_007_hist_010_pages_around_a_comment() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Crash").await.issue.id;
    let mut ids = Vec::new();
    for n in 1..=9 {
        ids.push(comment(pool, &admin, issue, &format!("Comment {n}")).await);
    }
    let target = ids[4];
    let page = activity::page(
        pool,
        &admin,
        issue,
        Query {
            limit: 4,
            around_comment: Some(target),
            ..query()
        },
    )
    .await
    .unwrap();
    let texts: Vec<String> = page
        .items
        .iter()
        .map(|i| match i {
            Item::Comment { comment, .. } => comment.comment.text.clone(),
            Item::Change { entry, .. } => entry.kind.clone(),
        })
        .collect();
    assert_eq!(texts, ["Comment 7", "Comment 6", "Comment 5", "Comment 4"]);
    // Both ways go on from there.
    let newer = activity::page(
        pool,
        &admin,
        issue,
        Query {
            from: page.newer,
            direction: Direction::Newer,
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(newer.items.len(), 2);
    assert_eq!(newer.newer, None);
    let older = activity::page(
        pool,
        &admin,
        issue,
        Query {
            from: page.older,
            ..query()
        },
    )
    .await
    .unwrap();
    // Comments 3, 2, 1 and the record of creation.
    assert_eq!(older.items.len(), 4);
    assert_eq!(older.older, None);
    // The newest page has nothing newer; a page from a cursor has.
    let first = activity::page(
        pool,
        &admin,
        issue,
        Query {
            limit: 2,
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(first.newer, None);
    assert!(first.older.is_some());
    // A comment of another issue, or one the viewer may not see, is not found.
    let elsewhere = create(pool, &admin, web, "Other").await.issue.id;
    assert!(matches!(
        activity::page(
            pool,
            &admin,
            elsewhere,
            Query {
                around_comment: Some(target),
                ..query()
            }
        )
        .await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn hist_011_changes_are_grouped_with_their_authors_comment() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let view = create(pool, &admin, web, "Crash").await;
    let issue = view.issue.id;
    let posted = comment(pool, &member, issue, "Renaming it").await;
    let renamed = issues::update(
        pool,
        &member,
        WEB,
        issue,
        IssueChange {
            summary: Some("Crash on start".to_owned()),
            text_version: Some(view.issue.text_version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap()
    .done
    .0;
    // Another author's change right after is not grouped.
    issues::update(
        pool,
        &admin,
        WEB,
        issue,
        IssueChange {
            summary: Some("Crash on every start".to_owned()),
            text_version: Some(renamed.issue.text_version),
            ..IssueChange::default()
        },
    )
    .await
    .unwrap();
    let page = activity::page(pool, &admin, issue, query()).await.unwrap();
    let groups: Vec<(String, Option<CommentId>)> = page
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Change {
                entry,
                after_comment,
            } => Some((entry.kind.clone(), *after_comment)),
            Item::Comment { .. } => None,
        })
        .collect();
    assert_eq!(
        groups,
        [
            ("summary".to_owned(), None),
            ("summary".to_owned(), Some(posted)),
            ("created".to_owned(), None),
        ]
    );
    // Only changes: the filter leaves comments out (HIST-008).
    let changes = activity::page(
        pool,
        &admin,
        issue,
        Query {
            comments: false,
            ..query()
        },
    )
    .await
    .unwrap();
    assert_eq!(changes.items.len(), 3);
}

#[tokio::test]
async fn acc_047_acc_048_submitters_comment_on_their_own_issues() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let submitter = custom(
        pool,
        &admin,
        "sam",
        &[
            "project.basic",
            "issue.create",
            "comment.create",
            "comment.update.own",
        ],
        web,
    )
    .await;
    let own = create(pool, &submitter, web, "My problem").await.issue.id;
    let reply = comment(pool, &admin, own, "We are on it").await;
    let mine = comment(pool, &submitter, own, "Thanks").await;
    // Without `comment.read` they see their own comments only.
    assert_eq!(
        seen(pool, &submitter, own).await,
        [Some("Thanks".to_owned())]
    );
    assert!(comments::get(pool, &submitter, mine).await.is_ok());
    assert!(matches!(
        comments::get(pool, &submitter, reply).await,
        Err(Error::NotFound(_))
    ));
    // Others' issues stay out of reach.
    let foreign = create(pool, &admin, web, "Not theirs").await.issue.id;
    let refused = comments::create(
        pool,
        &submitter,
        WEB,
        foreign,
        NewComment {
            text: "x".to_owned(),
            ..NewComment::default()
        },
    )
    .await;
    assert!(matches!(refused, Err(Error::NotFound(_))));
}

#[tokio::test]
async fn cnt_054_mentions_are_stored_and_offered_to_those_who_see_the_issue() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(
        pool,
        &admin,
        "jsmith",
        PredefinedRole::ProjectMember,
        &[web],
    )
    .await;
    let outsider = create_user(pool, &admin, "jsmythe").await;
    let issue = create(pool, &admin, web, "Crash").await.issue.id;

    let (posted, d) = comments::create(
        pool,
        &admin,
        WEB,
        issue,
        NewComment {
            text: "@JSmith and @jsmythe, and @nobody; not `@admin`".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(posted.comment.mentioned_users, [member.user(), outsider]);
    assert!(
        posted
            .comment
            .html
            .contains(&format!("<a href=\"tf-user:{}\"", member.user()))
    );
    assert!(posted.comment.html.contains("@nobody"));
    let mut named: Vec<UserId> = d.refs.users.iter().map(|u| u.id).collect();
    named.sort_unstable();
    assert_eq!(named, [member.user(), outsider]);

    // Candidates: matching people who can see the issue.
    let candidates = prostor_service::people::mention_candidates(pool, &admin, issue, "js", 10)
        .await
        .unwrap();
    assert_eq!(
        candidates.iter().map(|u| u.id).collect::<Vec<_>>(),
        [member.user()]
    );
    // Without the directory, nobody is offered.
    let no_directory = Access::compile(
        member.user(),
        [Grant {
            permissions: PredefinedRole::ProjectMember.permissions(),
            scope: Scope::Project(web),
        }],
        vec![],
        CapabilitySet::default(),
    );
    assert!(
        prostor_service::people::mention_candidates(pool, &no_directory, issue, "js", 10)
            .await
            .unwrap()
            .is_empty()
    );
    // Descriptions and text fields mention too.
    let described = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            project: web,
            summary: "Ask".to_owned(),
            description: "cc @jsmith".to_owned(),
            publish: true,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(described.0.issue.mentioned_users, [member.user()]);
    assert_eq!(described.1.refs.users[0].id, member.user());
}

#[tokio::test]
async fn cnt_056_issue_ids_resolve_for_each_viewer() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let app = project(pool, &admin, "App", "APP").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let visible = create(pool, &admin, web, "Visible").await.issue.id;
    let hidden = create(pool, &admin, app, "Hidden").await.issue.id;
    let moving = create(pool, &admin, web, "Moving").await.issue.id;
    issues::moves::move_issue(pool, &admin, WEB, moving, app, false)
        .await
        .unwrap();
    projects::update(
        pool,
        &admin,
        web,
        ProjectChange {
            key: Some("SITE".to_owned()),
            version: projects::get(pool, &admin, web)
                .await
                .unwrap()
                .project
                .version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    let issue = create(pool, &admin, web, "Notes").await.issue.id;
    let (posted, _) = comments::create(
        pool,
        &admin,
        WEB,
        issue,
        NewComment {
            text: "See WEB-1, APP-1, WEB-2 and NOPE-1; `WEB-1` is code".to_owned(),
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(
        posted.comment.issue_tokens,
        ["WEB-1", "APP-1", "WEB-2", "NOPE-1"]
    );
    // The former key and the former number lead to the issues; the admin sees all three.
    let (_, d) = comments::get(pool, &admin, posted.comment.id)
        .await
        .unwrap();
    let mut found: Vec<(String, IssueId)> = d
        .refs
        .issues
        .iter()
        .map(|m| (m.token.clone(), m.issue.id))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("APP-1".to_owned(), hidden),
            ("WEB-1".to_owned(), visible),
            ("WEB-2".to_owned(), moving),
        ]
    );
    let readable: Vec<String> = d.refs.issues.iter().map(|m| m.readable_id()).collect();
    assert!(readable.contains(&"SITE-1".to_owned()));
    assert!(readable.contains(&"APP-2".to_owned()));
    // A member of one project resolves only what they may see.
    let (_, d) = comments::get(pool, &member, posted.comment.id)
        .await
        .unwrap();
    let tokens: Vec<&str> = d.refs.issues.iter().map(|m| m.token.as_str()).collect();
    assert_eq!(tokens, ["WEB-1"]);
    // Previews resolve the same way.
    let (rendered, refs) = text::preview(
        pool,
        &member,
        "WEB-1 APP-1",
        prostor_markup::Profile::Full,
        Some(issue),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(rendered.issue_tokens, ["WEB-1", "APP-1"]);
    assert_eq!(refs.issues.len(), 1);
}

#[tokio::test]
async fn prj_006_project_descriptions_render_in_the_basic_profile() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    create(pool, &admin, web, "Crash").await;
    let version = projects::get(pool, &admin, web)
        .await
        .unwrap()
        .project
        .version;
    let row = projects::update(
        pool,
        &admin,
        web,
        ProjectChange {
            description: Some("# Goals\n\n- [ ] ship\n\nWEB-1 by @admin".to_owned()),
            version,
            ..ProjectChange::default()
        },
    )
    .await
    .unwrap();
    let html = row.project.description_html;
    assert!(html.contains("<h1>Goals</h1>"));
    assert!(html.contains("<input type=\"checkbox\" disabled=\"\">"));
    assert!(html.contains("WEB-1 by @admin"));
    assert!(!html.contains("tf-"));
}

#[tokio::test(flavor = "multi_thread")]
async fn content_render_job_brings_old_texts_up_to_date() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = issues::create(
        pool,
        &admin,
        WEB,
        NewIssue {
            project: web,
            summary: "Crash".to_owned(),
            description: "**bold** WEB-1".to_owned(),
            publish: true,
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done
    .0
    .issue
    .id;
    let id = comment(pool, &admin, issue, "_hi_ @admin").await;
    // Pretend the texts came from older rules.
    let mut conn = pool.acquire().await.unwrap();
    for statement in [
        "update issues set description_html = '', render_version = 0",
        "update comments set html = '', mentioned_users = '{}', render_version = 0",
        "update projects set description_html = '', render_version = 0",
        "update instance set render_version = 0",
    ] {
        sqlx::query(statement).execute(&mut *conn).await.unwrap();
    }
    drop(conn);
    assert!(text::ensure_rendered(pool).await.unwrap());
    // Queued once.
    assert!(!text::ensure_rendered(pool).await.unwrap());
    text::run_render(
        pool,
        text::RenderJob {
            version: prostor_markup::RENDER_VERSION,
        },
    )
    .await
    .unwrap();
    let view = issues::get(pool, &admin, issue).await.unwrap().0;
    assert_eq!(
        view.issue.description_html,
        "<p><strong>bold</strong> <a href=\"tf-issue:WEB-1\" rel=\"noopener noreferrer nofollow\">WEB-1</a></p>\n"
    );
    let shown = comments::get(pool, &admin, id).await.unwrap().0;
    assert_eq!(shown.comment.mentioned_users, [admin.user()]);
    assert_eq!(shown.comment.render_version, prostor_markup::RENDER_VERSION);
    assert_eq!(
        prostor_store::content::instance_version(pool)
            .await
            .unwrap(),
        prostor_markup::RENDER_VERSION
    );
}

#[tokio::test]
async fn acc_006_acc_032_deleted_accounts_and_groups_leave_comment_audiences() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let leaver = person(pool, &admin, "leo", PredefinedRole::ProjectMember, &[web]).await;
    let heir = person(pool, &admin, "hal", PredefinedRole::ProjectMember, &[web]).await;
    let old = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "Old".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let new = groups::create(
        pool,
        &admin,
        groups::NewGroup {
            name: "New".to_owned(),
            ..groups::NewGroup::default()
        },
    )
    .await
    .unwrap();
    let issue = create(pool, &admin, web, "Crash").await.issue.id;
    let (posted, _) = comments::create(
        pool,
        &leaver,
        WEB,
        issue,
        NewComment {
            text: "Limited".to_owned(),
            audience: Some(AudienceInput {
                users: vec![leaver.user()],
                groups: vec![old.id],
            }),
            silent: false,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .done;
    users::delete(pool, &admin, &[leaver.user()], heir.user())
        .await
        .unwrap();
    groups::delete(pool, &admin, &[old.id], new.id)
        .await
        .unwrap();
    let row = prostor_store::comments::get(pool, posted.comment.id)
        .await
        .unwrap()
        .unwrap();
    // Authorship stays; the audience passes on.
    assert_eq!(row.author_id, leaver.user());
    assert_eq!(row.audience_users, [heir.user()]);
    assert_eq!(row.audience_groups, [new.id]);
}
