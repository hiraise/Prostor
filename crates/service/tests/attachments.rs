//! Files on a real database (step 1.4b): staged uploads, attachments of issues, drafts and
//! comments, names, rights and audiences, soft deletion, signed links, picture inspection,
//! avatars with the Gravatar import, project logos, and what deleting accounts, groups and
//! projects does to files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use bytes::Bytes;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use serde_json::json;
use time::OffsetDateTime;
use time::macros::datetime;
use trackflow_domain::access::capability::CapabilitySet;
use trackflow_domain::access::role::PredefinedRole;
use trackflow_domain::access::{Access, Grant, Scope};
use trackflow_domain::files::ui_link_expiry;
use trackflow_domain::issues::history::Origin;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::Language;
use trackflow_domain::{IssueId, ProjectId, UploadId, UserId};
use trackflow_files::FileStore;
use trackflow_service::auth::SignedIn;
use trackflow_service::comments::{self, CommentChange, NewComment};
use trackflow_service::files::{self, LinkQuery, Signer};
use trackflow_service::issues::activity::{self, Item, Query};
use trackflow_service::issues::{self, AudienceInput, NewIssue};
use trackflow_service::people::Names;
use trackflow_service::pictures::{self, Gravatar};
use trackflow_service::projects::{self, NewProjectInput};
use trackflow_service::setup::{self, Setup};
use trackflow_service::users::{self, NewAccount};
use trackflow_service::{Error, Passwords, access, attachments, groups, roles, uploads};
use trackflow_store::PgPool;
use trackflow_store::access::Holder;
use trackflow_store::issue_history::Direction;
use trackflow_store::sessions::Origin as SessionOrigin;
use trackflow_testkit::TestDb;

const T0: OffsetDateTime = datetime!(2026-10-01 09:00 UTC);
const WEB: Origin = Origin::Web;
const MAX: u64 = 10 * 1024 * 1024;

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
    let role = trackflow_store::roles::builtin_id(pool, role.key())
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

async fn create(pool: &PgPool, actor: &Access, project: ProjectId, summary: &str) -> IssueId {
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
    .issue
    .id
}

fn store() -> (tempfile::TempDir, FileStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = FileStore::new(dir.path());
    (dir, store)
}

fn body(bytes: &[u8]) -> futures_util::stream::Iter<std::vec::IntoIter<Result<Bytes, Infallible>>> {
    let chunks: Vec<Result<Bytes, Infallible>> = bytes
        .chunks(1000)
        .map(|c| Ok(Bytes::copy_from_slice(c)))
        .collect();
    futures_util::stream::iter(chunks)
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([10, 120, 200])));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

async fn upload(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    name: &str,
    bytes: &[u8],
) -> UploadId {
    files.prepare().await.unwrap();
    uploads::receive(pool, files, actor, name, body(bytes), MAX)
        .await
        .unwrap()
        .id
}

async fn attach(
    pool: &PgPool,
    files: &FileStore,
    actor: &Access,
    issue: IssueId,
    name: &str,
    bytes: &[u8],
) -> trackflow_store::attachments::Attachment {
    let id = upload(pool, files, actor, name, bytes).await;
    let listed = attachments::attach(pool, actor, WEB, issue, &[id], false)
        .await
        .unwrap();
    listed.files.into_iter().next().unwrap().file
}

/// Names of the files a viewer sees on an issue.
async fn seen(pool: &PgPool, viewer: &Access, issue: IssueId) -> Vec<String> {
    attachments::list(pool, viewer, issue, None, 100)
        .await
        .map(|l| l.files.into_iter().map(|f| f.file.name).collect())
        .unwrap_or_default()
}

/// Kinds and payloads of the queued jobs of a kind.
async fn queued(pool: &PgPool, kind: &str) -> Vec<serde_json::Value> {
    sqlx::query_scalar::<_, serde_json::Value>(
        "select payload from jobs where kind = $1 and state = 'queued' order by id",
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .unwrap()
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

/// The files named by the attachment records a viewer sees, oldest first: `+name` or `-name`.
async fn file_records(pool: &PgPool, viewer: &Access, issue: IssueId) -> Vec<String> {
    let page = activity::page(pool, viewer, issue, query()).await.unwrap();
    let mut out = Vec::new();
    for item in page.items.iter().rev() {
        if let Item::Change { entry, .. } = item
            && entry.kind == "attachments"
        {
            for (sign, side) in [("-", &entry.removed), ("+", &entry.added)] {
                for f in side
                    .as_ref()
                    .and_then(|j| j.as_array())
                    .into_iter()
                    .flatten()
                {
                    out.push(format!("{sign}{}", f["name"].as_str().unwrap()));
                }
            }
        }
    }
    out
}

#[tokio::test]
async fn cnt_017_files_are_attached_with_unique_names_and_history() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Crash").await;
    let shot = attach(pool, &files, &member, issue, "shot.png", &png(10, 10)).await;
    assert_eq!(shot.content_type, "image/png");
    // A text file claiming to be anything stays "other": the kind comes from the bytes.
    let second = attach(pool, &files, &member, issue, "SHOT.png", b"not a picture").await;
    assert_eq!(second.name, "SHOT (2).png");
    assert_eq!(second.content_type, "application/octet-stream");
    assert_eq!(
        seen(pool, &admin, issue).await,
        ["shot.png", "SHOT (2).png"]
    );
    assert_eq!(
        file_records(pool, &admin, issue).await,
        ["+shot.png", "+SHOT (2).png"]
    );
    // The upload is used up.
    let id = upload(pool, &files, &member, "log.txt", b"log").await;
    attachments::attach(pool, &member, WEB, issue, &[id], false)
        .await
        .unwrap();
    assert!(matches!(
        attachments::attach(pool, &member, WEB, issue, &[id], false).await,
        Err(Error::Invalid(_))
    ));
    // Someone else's upload is not theirs to attach.
    let foreign = upload(pool, &files, &admin, "a.txt", b"a").await;
    assert!(matches!(
        attachments::attach(pool, &member, WEB, issue, &[foreign], false).await,
        Err(Error::Invalid(_))
    ));
    // A reader without `attachment.create` cannot attach.
    let reader = custom(
        pool,
        &admin,
        "rex",
        &["project.basic", "issue.read", "issue.update"],
        web,
    )
    .await;
    let theirs = upload(pool, &files, &reader, "r.txt", b"r").await;
    assert!(matches!(
        attachments::attach(pool, &reader, WEB, issue, &[theirs], false).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn iss_007_drafts_take_files_without_history() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let first = upload(pool, &files, &member, "spec.pdf", b"%PDF-1.7 ...").await;
    let (draft, _) = issues::create(
        pool,
        &member,
        WEB,
        NewIssue {
            project: web,
            summary: "Draft".to_owned(),
            uploads: vec![first],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done;
    let draft = draft.issue.id;
    assert_eq!(seen(pool, &member, draft).await, ["spec.pdf"]);
    let more = upload(pool, &files, &member, "spec.pdf", b"%PDF-1.7 again").await;
    attachments::attach(pool, &member, WEB, draft, &[more], false)
        .await
        .unwrap();
    assert_eq!(
        seen(pool, &member, draft).await,
        ["spec.pdf", "spec (2).pdf"]
    );
    issues::publish(pool, &member, WEB, draft).await.unwrap();
    // Files of a draft have no records: the issue starts as it is.
    assert!(file_records(pool, &member, draft).await.is_empty());
    assert_eq!(seen(pool, &admin, draft).await.len(), 2);
}

#[tokio::test]
async fn cnt_018_comments_carry_files_with_their_audience() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let other = person(pool, &admin, "oz", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Crash").await;
    let shot = upload(pool, &files, &member, "shot.png", &png(4, 4)).await;
    // Files alone make a comment.
    let (posted, described) = comments::create(
        pool,
        &member,
        WEB,
        issue,
        NewComment {
            text: String::new(),
            uploads: vec![shot],
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    let names: Vec<String> = described
        .files_of(posted.comment.id)
        .map(|f| f.file.name.clone())
        .collect();
    assert_eq!(names, ["shot.png"]);
    assert_eq!(seen(pool, &other, issue).await, ["shot.png"]);
    // A restricted comment hides its files from those outside it.
    let secret = upload(pool, &files, &member, "secret.txt", b"s").await;
    let (restricted, _) = comments::create(
        pool,
        &member,
        WEB,
        issue,
        NewComment {
            text: "Only for me".to_owned(),
            audience: Some(AudienceInput {
                users: vec![member.user()],
                groups: vec![],
            }),
            uploads: vec![secret],
            ..NewComment::default()
        },
    )
    .await
    .unwrap()
    .done;
    assert_eq!(seen(pool, &member, issue).await, ["shot.png", "secret.txt"]);
    assert_eq!(seen(pool, &other, issue).await, ["shot.png"]);
    assert!(
        !file_records(pool, &other, issue)
            .await
            .contains(&"+secret.txt".to_owned())
    );
    // Edits add files; an empty text stays allowed while files are there.
    let late = upload(pool, &files, &member, "late.txt", b"l").await;
    let (edited, d) = comments::update(
        pool,
        &member,
        WEB,
        posted.comment.id,
        CommentChange {
            add_uploads: vec![late],
            version: posted.comment.version,
            ..CommentChange::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(d.files_of(edited.comment.id).count(), 2);
    // A deleted comment takes its files out of sight until restored.
    comments::delete(pool, &member, WEB, restricted.comment.id)
        .await
        .unwrap();
    assert_eq!(seen(pool, &member, issue).await, ["shot.png", "late.txt"]);
    // A text with no files is still refused when empty.
    assert!(matches!(
        comments::create(pool, &member, WEB, issue, NewComment::default()).await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn cnt_019_uploads_have_a_limit_and_a_queue() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    files.prepare().await.unwrap();
    let admin = admin(pool).await;
    let refused = uploads::receive(pool, &files, &admin, "big.bin", body(&[0; 2048]), 1024).await;
    assert!(matches!(refused, Err(Error::TooLarge(1024))));
    assert!(matches!(
        uploads::receive(pool, &files, &admin, "a/b", body(b"x"), 1024).await,
        Err(Error::Invalid(_))
    ));
    let id = upload(pool, &files, &admin, "ok.bin", &[1; 1024]).await;
    // Each upload expires a day later unless used.
    let expiring = queued(pool, "uploads.expire").await;
    assert_eq!(expiring, [json!({ "upload": id })]);
    uploads::run_expiry(pool, uploads::UploadExpiry { upload: id })
        .await
        .unwrap();
    assert!(uploads::get(pool, &admin, id).await.is_err());
    let removal = queued(pool, "files.remove").await;
    assert_eq!(removal.len(), 1);
    // The guest account cannot upload.
    let guest: i64 = sqlx::query_scalar("select id from users where builtin = 'guest'")
        .fetch_one(pool)
        .await
        .unwrap();
    let guest = access::load(pool, guest).await.unwrap();
    assert!(matches!(
        uploads::receive(pool, &files, &guest, "g.txt", body(b"g"), 1024).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn cnt_020_renaming_keeps_names_unique() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let other = custom(
        pool,
        &admin,
        "oz",
        &[
            "project.basic",
            "issue.read",
            "issue.update",
            "attachment.create",
        ],
        web,
    )
    .await;
    let issue = create(pool, &admin, web, "Crash").await;
    let log = attach(pool, &files, &member, issue, "log.txt", b"1").await;
    attach(pool, &files, &member, issue, "notes.txt", b"2").await;
    let taken = attachments::update(
        pool,
        &member,
        log.id,
        attachments::AttachmentChange {
            name: Some("NOTES.txt".to_owned()),
            version: log.version,
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(taken, Err(Error::Invalid(_))));
    let renamed = attachments::update(
        pool,
        &member,
        log.id,
        attachments::AttachmentChange {
            name: Some("server.log".to_owned()),
            version: log.version,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.files[0].file.name, "server.log");
    // A stale version is refused.
    let stale = attachments::update(
        pool,
        &member,
        log.id,
        attachments::AttachmentChange {
            name: Some("x.log".to_owned()),
            version: log.version,
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(stale, Err(Error::Conflict(_))));
    // Others' files need `attachment.update`.
    let refused = attachments::update(
        pool,
        &other,
        log.id,
        attachments::AttachmentChange {
            name: Some("mine.log".to_owned()),
            version: renamed.files[0].file.version,
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(refused, Err(Error::Forbidden(_))));
}

#[tokio::test]
async fn cnt_022_pictures_get_sizes_and_thumbnails() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let issue = create(pool, &admin, web, "Crash").await;
    let big = attach(pool, &files, &admin, issue, "big.png", &png(1000, 500)).await;
    let small = attach(pool, &files, &admin, issue, "small.png", &png(20, 10)).await;
    let text = attach(pool, &files, &admin, issue, "a.txt", b"text").await;
    let jobs = queued(pool, "attachments.inspect").await;
    assert_eq!(
        jobs,
        [
            json!({ "attachment": big.id }),
            json!({ "attachment": small.id })
        ]
    );
    for id in [big.id, small.id, text.id] {
        attachments::run_inspection(
            pool,
            &files,
            attachments::ImageInspection { attachment: id },
        )
        .await
        .unwrap();
    }
    let get = |id| async move {
        trackflow_store::attachments::get(pool, id)
            .await
            .unwrap()
            .unwrap()
    };
    let big = get(big.id).await;
    assert_eq!((big.width, big.height), (Some(1000), Some(500)));
    let thumbnail = files
        .read(big.thumbnail_key.as_deref().unwrap())
        .await
        .unwrap();
    let decoded = image::load_from_memory(&thumbnail).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (480, 240));
    let small = get(small.id).await;
    assert_eq!(
        (small.width, small.height, small.thumbnail_key),
        (Some(20), Some(10), None)
    );
    let text = get(text.id).await;
    assert_eq!((text.width, text.thumbnail_key), (None, None));
}

fn link_of(
    signer: &Signer,
    viewer: &Access,
    secret: &[u8],
    file: &trackflow_store::attachments::Attachment,
) -> LinkQuery {
    let link = signer.link(
        viewer.user(),
        secret,
        file,
        ui_link_expiry(OffsetDateTime::now_utc()),
    );
    LinkQuery {
        user: link.user,
        expires: link.expires,
        signature: link.signature,
        thumbnail: false,
    }
}

/// Opens a link the way the server does: the signature, then the person's access now.
async fn open(
    pool: &PgPool,
    signer: &Signer,
    id: i64,
    link: &LinkQuery,
) -> Result<files::Served, Error> {
    let now = OffsetDateTime::now_utc();
    let (file, user) = files::check_link(pool, signer, id, link, now).await?;
    let access = access::load(pool, user).await?;
    let expires = OffsetDateTime::from_unix_timestamp(link.expires).unwrap();
    files::open(pool, &access, file, link.thumbnail, expires).await
}

#[tokio::test]
async fn cnt_024_cnt_026_links_follow_the_person_and_the_file() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let signer = Signer::load(pool).await.unwrap();
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let other = person(pool, &admin, "oz", PredefinedRole::ProjectMember, &[web]).await;
    let issue = create(pool, &admin, web, "Crash").await;
    let file = attach(pool, &files, &member, issue, "log.txt", b"log").await;
    let listed = attachments::list(pool, &member, issue, None, 100)
        .await
        .unwrap();
    let link = link_of(&signer, &member, &listed.link_secret, &file);
    let served = open(pool, &signer, file.id, &link).await.unwrap();
    assert_eq!(
        (served.name.as_str(), served.key.as_str()),
        ("log.txt", file.storage_key.as_str())
    );
    // Tampered, expired, for someone else, or for the thumbnail: nothing.
    let mut tampered = link.clone();
    tampered.signature.replace_range(0..2, "AA");
    assert!(open(pool, &signer, file.id, &tampered).await.is_err());
    let expired = LinkQuery {
        expires: OffsetDateTime::now_utc().unix_timestamp() - 1,
        ..link.clone()
    };
    assert!(open(pool, &signer, file.id, &expired).await.is_err());
    assert!(
        open(
            pool,
            &signer,
            file.id,
            &LinkQuery {
                user: other.user(),
                ..link.clone()
            }
        )
        .await
        .is_err()
    );
    assert!(
        open(
            pool,
            &signer,
            file.id,
            &LinkQuery {
                thumbnail: true,
                ..link.clone()
            }
        )
        .await
        .is_err()
    );
    // A narrowed audience closes the link at once.
    let current = attachments::get(pool, &member, file.id)
        .await
        .unwrap()
        .files[0]
        .file
        .clone();
    attachments::update(
        pool,
        &admin,
        file.id,
        attachments::AttachmentChange {
            audience: Some(AudienceInput {
                users: vec![admin.user()],
                groups: vec![],
            }),
            version: current.version,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let other_listed = attachments::list(pool, &other, issue, None, 100)
        .await
        .unwrap();
    assert!(other_listed.files.is_empty());
    let others_link = link_of(&signer, &other, &other_listed.link_secret, &file);
    assert!(open(pool, &signer, file.id, &others_link).await.is_err());
    // The author still sees their own file.
    assert!(open(pool, &signer, file.id, &link).await.is_ok());
    // A disabled person's links stop working.
    users::disable(
        pool,
        &admin,
        &[member.user()],
        "",
        trackflow_domain::accounts::DisabledLabel::Left,
    )
    .await
    .unwrap();
    assert!(open(pool, &signer, file.id, &link).await.is_err());
    users::enable(pool, &admin, &[member.user()]).await.unwrap();
    assert!(open(pool, &signer, file.id, &link).await.is_ok());
    // Deleting: others need `attachment.delete`; the author may; links stop; a record stays.
    assert!(matches!(
        attachments::delete(pool, &other, WEB, file.id).await,
        Err(Error::NotFound(_))
    ));
    attachments::delete(pool, &member, WEB, file.id)
        .await
        .unwrap();
    assert!(open(pool, &signer, file.id, &link).await.is_err());
    assert!(seen(pool, &member, issue).await.is_empty());
    assert_eq!(
        file_records(pool, &member, issue).await,
        ["+log.txt", "-log.txt"]
    );
    // The file stays for the trash.
    assert!(files.path(&file.storage_key).unwrap().exists());
}

#[tokio::test]
async fn cnt_024_others_files_need_the_delete_permission() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let plain = custom(
        pool,
        &admin,
        "pat",
        &["project.basic", "issue.read", "attachment.create"],
        web,
    )
    .await;
    let issue = create(pool, &admin, web, "Crash").await;
    let file = attach(pool, &files, &member, issue, "log.txt", b"log").await;
    assert!(matches!(
        attachments::delete(pool, &plain, WEB, file.id).await,
        Err(Error::Forbidden(_))
    ));
    attachments::delete(pool, &admin, WEB, file.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn acc_047_acc_048_acc_049_submitters_and_audiences() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let submitter = custom(
        pool,
        &admin,
        "sam",
        &["project.basic", "issue.create", "attachment.create"],
        web,
    )
    .await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let own = create(pool, &submitter, web, "My problem").await;
    attach(pool, &files, &submitter, own, "mine.txt", b"m").await;
    attach(pool, &files, &admin, own, "reply.txt", b"r").await;
    // Without `issue.read` the submitter sees the files they attached, not others'.
    assert_eq!(seen(pool, &submitter, own).await, ["mine.txt"]);
    let foreign = create(pool, &admin, web, "Not theirs").await;
    let theirs = upload(pool, &files, &submitter, "x.txt", b"x").await;
    assert!(matches!(
        attachments::attach(pool, &submitter, WEB, foreign, &[theirs], false).await,
        Err(Error::NotFound(_))
    ));
    // A restricted file: its audience, its author and the bypass see it.
    let file = attach(pool, &files, &admin, foreign, "plan.txt", b"p").await;
    attachments::update(
        pool,
        &admin,
        file.id,
        attachments::AttachmentChange {
            audience: Some(AudienceInput {
                users: vec![admin.user()],
                groups: vec![],
            }),
            version: file.version,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(seen(pool, &member, foreign).await.is_empty());
    assert_eq!(seen(pool, &admin, foreign).await, ["plan.txt"]);
    assert!(file_records(pool, &member, foreign).await.is_empty());
}

/// A tiny stand-in for Gravatar: a picture for one hash, 404 for others; under `/slow/` it
/// answers too late.
async fn gravatar_stub(known: String, picture: Vec<u8>) -> String {
    let answer = move |hash: String| {
        if hash == known {
            (StatusCode::OK, picture.clone()).into_response()
        } else {
            StatusCode::NOT_FOUND.into_response()
        }
    };
    let late = answer.clone();
    let app = Router::new()
        .route(
            "/avatar/{hash}",
            get(move |Path(hash): Path<String>| {
                let answer = answer.clone();
                async move { answer(hash) }
            }),
        )
        .route(
            "/slow/avatar/{hash}",
            get(move |Path(hash): Path<String>| {
                let late = late.clone();
                async move {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    late(hash)
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{address}/avatar/")
}

fn email_hash(email: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(email.trim().to_lowercase().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn acc_017_avatars_from_uploads_and_gravatar() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let names = Names::with_secret(b"test");
    // Someone who sees the directory sees the picture; a pseudonymous viewer does not.
    let pip = create_user(pool, &admin, "pip").await;
    let outsider = Access::compile(
        pip,
        [Grant {
            permissions: PredefinedRole::ProjectMember.permissions(),
            scope: Scope::Project(web),
        }],
        vec![],
        CapabilitySet::default(),
    );
    let photo = upload(pool, &files, &member, "me.png", &png(600, 300)).await;
    let key = pictures::set_avatar(pool, &files, &member, member.user(), photo)
        .await
        .unwrap()
        .unwrap();
    let file = pictures::file_of(&key);
    assert_eq!(
        pictures::avatar_file(pool, &names, &admin, member.user(), file)
            .await
            .unwrap(),
        key
    );
    assert!(
        pictures::avatar_file(pool, &names, &outsider, member.user(), file)
            .await
            .is_err()
    );
    assert!(
        pictures::avatar_file(pool, &names, &admin, member.user(), "old.png")
            .await
            .is_err()
    );
    let stored = image::load_from_memory(&files.read(&key).await.unwrap()).unwrap();
    assert_eq!((stored.width(), stored.height()), (256, 256));
    // Others' avatars need `user.update`; files that are not pictures are refused.
    let text = upload(pool, &files, &member, "a.txt", b"text").await;
    assert!(matches!(
        pictures::set_avatar(pool, &files, &member, member.user(), text).await,
        Err(Error::Invalid(_))
    ));
    let photo = upload(pool, &files, &member, "me.png", &png(10, 10)).await;
    assert!(matches!(
        pictures::set_avatar(pool, &files, &member, admin.user(), photo).await,
        Err(Error::Forbidden(_))
    ));
    // Resetting removes the old picture by a job.
    pictures::reset_avatar(pool, &files, &member, member.user())
        .await
        .unwrap();
    let removal = queued(pool, "files.remove").await;
    assert!(
        removal
            .iter()
            .any(|j| j["keys"].as_array().unwrap().contains(&json!(key)))
    );
    assert!(
        pictures::avatar_file(pool, &names, &admin, member.user(), file)
            .await
            .is_err()
    );

    // Gravatar: found, missing, slow.
    let base = gravatar_stub(email_hash("mia@example.com"), png(80, 80)).await;
    let gravatar = Gravatar::new(&base)
        .unwrap()
        .with_timeout(Duration::from_millis(500));
    let imported = pictures::import_gravatar(pool, &files, &gravatar, &member, member.user())
        .await
        .unwrap()
        .unwrap();
    assert!(files.path(&imported).unwrap().exists());
    assert!(matches!(
        pictures::import_gravatar(pool, &files, &gravatar, &admin, admin.user()).await,
        Err(Error::GravatarNotFound)
    ));
    let slow = Gravatar::new(&base.replace("/avatar/", "/slow/avatar/"))
        .unwrap()
        .with_timeout(Duration::from_millis(300));
    assert!(matches!(
        pictures::import_gravatar(pool, &files, &slow, &member, member.user()).await,
        Err(Error::GravatarUnavailable(_))
    ));
    // Nobody listens there: unavailable, not missing.
    let closed = Gravatar::new("http://127.0.0.1:9/avatar/").unwrap();
    assert!(matches!(
        pictures::import_gravatar(pool, &files, &closed, &member, member.user()).await,
        Err(Error::GravatarUnavailable(_))
    ));
}

#[tokio::test]
async fn prj_007_projects_have_uploaded_logos() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
    let admin = admin(pool).await;
    let web = project(pool, &admin, "Web", "WEB").await;
    let member = person(pool, &admin, "mia", PredefinedRole::ProjectMember, &[web]).await;
    let picture = upload(pool, &files, &admin, "logo.jpg", &png(300, 100)).await;
    let key = pictures::set_logo(pool, &files, &admin, web, picture)
        .await
        .unwrap()
        .unwrap();
    assert!(key.ends_with(".png"));
    let file = pictures::file_of(&key);
    assert_eq!(
        pictures::logo_file(pool, &member, web, file).await.unwrap(),
        key
    );
    // Members see it but may not change it.
    let theirs = upload(pool, &files, &member, "x.png", &png(10, 10)).await;
    assert!(matches!(
        pictures::set_logo(pool, &files, &member, web, theirs).await,
        Err(Error::Forbidden(_))
    ));
    let elsewhere = project(pool, &admin, "App", "APP").await;
    assert!(
        pictures::logo_file(pool, &member, elsewhere, file)
            .await
            .is_err()
    );
    // Archived projects are read-only.
    projects::set_archived(pool, &admin, web, true)
        .await
        .unwrap();
    assert!(matches!(
        pictures::reset_logo(pool, &files, &admin, web).await,
        Err(Error::Conflict(_))
    ));
    projects::set_archived(pool, &admin, web, false)
        .await
        .unwrap();
    pictures::reset_logo(pool, &files, &admin, web)
        .await
        .unwrap();
    assert!(pictures::logo_file(pool, &member, web, file).await.is_err());
}

#[tokio::test]
async fn acc_006_acc_032_prj_064_deletions_take_files_along() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let (_dir, files) = store();
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
    let issue = create(pool, &admin, web, "Crash").await;
    let file = attach(pool, &files, &leaver, issue, "log.txt", b"log").await;
    attachments::update(
        pool,
        &leaver,
        file.id,
        attachments::AttachmentChange {
            audience: Some(AudienceInput {
                users: vec![leaver.user()],
                groups: vec![old.id],
            }),
            version: file.version,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let photo = upload(pool, &files, &leaver, "me.png", &png(40, 40)).await;
    let avatar = pictures::set_avatar(pool, &files, &leaver, leaver.user(), photo)
        .await
        .unwrap()
        .unwrap();
    let staged = upload(pool, &files, &leaver, "later.txt", b"later").await;
    let staged_key = trackflow_store::uploads::get(pool, staged)
        .await
        .unwrap()
        .unwrap()
        .storage_key;
    let draft_file = upload(pool, &files, &leaver, "draft.txt", b"d").await;
    let (draft, _) = issues::create(
        pool,
        &leaver,
        WEB,
        NewIssue {
            project: web,
            uploads: vec![draft_file],
            ..NewIssue::default()
        },
    )
    .await
    .unwrap()
    .done;
    let draft_key = attachments::list(pool, &leaver, draft.issue.id, None, 10)
        .await
        .unwrap()
        .files[0]
        .file
        .storage_key
        .clone();
    sqlx::query("delete from jobs").execute(pool).await.unwrap();
    users::delete(pool, &admin, &[leaver.user()], heir.user())
        .await
        .unwrap();
    groups::delete(pool, &admin, &[old.id], new.id)
        .await
        .unwrap();
    let row = trackflow_store::attachments::get(pool, file.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.audience_users, [heir.user()]);
    assert_eq!(row.audience_groups, [new.id]);
    // Authorship stays.
    assert_eq!(row.author_id, leaver.user());
    let removed: Vec<serde_json::Value> = queued(pool, "files.remove")
        .await
        .into_iter()
        .flat_map(|j| j["keys"].as_array().unwrap().clone())
        .collect();
    for key in [&avatar, &staged_key, &draft_key] {
        assert!(removed.contains(&json!(key)), "{key} in {removed:?}");
    }
    // The removal job deletes the files, and a second run is fine.
    let job = files::FilesRemoval {
        keys: vec![avatar.clone(), staged_key.clone()],
    };
    files::run_removal(&files, job.clone()).await.unwrap();
    files::run_removal(&files, job).await.unwrap();
    assert!(!files.path(&avatar).unwrap().exists());

    // A deleted project's files go with its issues.
    sqlx::query("delete from jobs").execute(pool).await.unwrap();
    let logo = upload(pool, &files, &admin, "logo.png", &png(30, 30)).await;
    let logo_key = pictures::set_logo(pool, &files, &admin, web, logo)
        .await
        .unwrap()
        .unwrap();
    projects::delete(pool, &admin, web).await.unwrap();
    projects::run_deletion(pool, projects::ProjectDeletion { project: web })
        .await
        .unwrap();
    let removed: Vec<serde_json::Value> = queued(pool, "files.remove")
        .await
        .into_iter()
        .flat_map(|j| j["keys"].as_array().unwrap().clone())
        .collect();
    assert!(removed.contains(&json!(file.storage_key)), "{removed:?}");
    assert!(removed.contains(&json!(logo_key)), "{removed:?}");
}
