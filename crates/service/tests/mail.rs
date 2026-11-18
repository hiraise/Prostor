//! The admin mail settings and what they gate (NTF-002, NTF-004, NTF-006, NTF-007, ADM-051;
//! plan step 1.10 §6.4) on a real database: secrets are sealed and never audited or answered,
//! the sender and reply-to ride the transport, the test send shows the transport's complaint,
//! the instance switch decides whether account emails leave at all, and a recovery use reports
//! to the built-in administrator.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use taskmaster_domain::access::Access;
use taskmaster_domain::secret::Key;
use taskmaster_domain::settings::Language;
use taskmaster_domain::settings::Snapshot;
use taskmaster_service::access;
use taskmaster_service::mail::{self, EnvMail, Mail, MailSettingsChange as Change, MailSource};
use taskmaster_service::recovery::{self, Target};
use taskmaster_service::setup::{self, Setup};
use taskmaster_service::users;
use taskmaster_store::PgPool;
use taskmaster_store::sessions::Origin as SessionOrigin;
use taskmaster_testkit::TestDb;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-08 09:00 UTC);
/// Mailpit of `make db-up`, the transport a successful test send goes through.
const MAILPIT: &str = "smtp://127.0.0.1:1025";

fn passwords() -> taskmaster_service::Passwords {
    taskmaster_service::Passwords::new(4)
}

fn key() -> Key {
    Key::from_base64("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=").unwrap()
}

fn change() -> Change {
    Change {
        smtp_url: Some(Some(MAILPIT.to_owned())),
        smtp_username: Some(Some("tracker".to_owned())),
        smtp_password: Some(Some("hunter2".to_owned())),
        mail_from: Some("TaskMaster <tracker@example.com>".to_owned()),
        mail_reply_to: Some("replies@example.com".to_owned()),
        email_enabled: None,
    }
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
            email: "chief@example.com".to_owned(),
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

/// A plain member, for refusals.
async fn member(pool: &PgPool, admin: &Access, username: &str) -> Access {
    let user = users::create(
        pool,
        &passwords(),
        admin,
        users::NewAccount {
            username: username.to_owned(),
            full_name: format!("{username} Person"),
            email: format!("{username}@example.com"),
            password: "correct horse".to_owned(),
            must_change_password: false,
        },
    )
    .await
    .unwrap()
    .id;
    let role = taskmaster_store::roles::builtin_id(
        pool,
        taskmaster_domain::access::role::PredefinedRole::ProjectMember.key(),
    )
    .await
    .unwrap()
    .unwrap();
    taskmaster_service::roles::assign(
        pool,
        admin,
        role,
        taskmaster_store::access::Holder::User(user),
        &[None],
    )
    .await
    .unwrap();
    access::load(pool, user).await.unwrap()
}

/// The settings snapshot of this database, secrets decrypted.
async fn snapshot(pool: &PgPool, key: &Key) -> Snapshot {
    let rows = taskmaster_store::settings::load(pool).await.unwrap();
    let (secrets, unreadable) = taskmaster_store::settings::load_secrets(pool, Some(key))
        .await
        .unwrap();
    assert!(
        unreadable.is_empty(),
        "secrets must open under the key they were sealed with"
    );
    let (snapshot, errors) = Snapshot::from_stored(rows.into_iter().chain(secrets));
    assert!(errors.is_empty(), "{errors:?}");
    snapshot
}

#[tokio::test]
async fn ntf_004_transport_settings() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let plain = member(pool, &admin, "plain").await;
    let key = key();

    // A member reads and saves nothing (NTF-004: the low-level admin permission).
    for error in [
        mail::mail_settings(pool, &plain, Some(&key), None)
            .await
            .unwrap_err(),
        mail::save_mail_settings(pool, &plain, &change(), Some(&key), None)
            .await
            .unwrap_err(),
    ] {
        assert!(
            matches!(error, taskmaster_service::Error::Forbidden(_)),
            "{error:?}"
        );
    }

    // Without the instance key the screen refuses to save secrets and says so.
    let error = mail::save_mail_settings(pool, &admin, &change(), None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, taskmaster_service::Error::Refused(ref message) if message.contains("TASKMASTER_SECRET_KEY")),
        "{error:?}"
    );

    // The view answers no secret value, only that one is stored.
    let view = mail::save_mail_settings(pool, &admin, &change(), Some(&key), None)
        .await
        .unwrap();
    assert_eq!(view.source, MailSource::Settings);
    assert!(view.configured);
    assert!(!view.email_enabled, "the switch starts off (NTF-002)");
    assert_eq!(view.mail_from, "TaskMaster <tracker@example.com>");
    assert!(view.has_smtp_url && view.has_username && view.has_password);

    // The secrets are sealed: no plaintext in either settings table.
    let sealed: Vec<(String, Vec<u8>)> =
        sqlx::query_as("select key, value from setting_secrets order by key")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(sealed.len(), 3);
    for (setting, value) in &sealed {
        assert!(!value.is_empty(), "{setting} is stored");
        assert!(
            !value.windows(7).any(|window| window == b"hunter2"),
            "{setting} holds the password only as ciphertext"
        );
    }
    let open: Vec<String> = sqlx::query_scalar("select key from settings order by key")
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(
        !open.iter().any(|key| key.starts_with("smtp")),
        "secrets never sit in settings"
    );

    // The audit trail says a secret changed, never what it was.
    for setting in ["smtp_url", "smtp_username", "smtp_password"] {
        let events = taskmaster_store::audit::for_target(pool, &format!("setting:{setting}"), 5)
            .await
            .unwrap();
        assert_eq!(events.len(), 1, "{setting}");
        assert_eq!(events[0].details["secret"], serde_json::json!(true));
        assert_eq!(events[0].details.get("old"), None);
        assert_eq!(events[0].details.get("new"), None);
    }
    // The open settings carry their values in the audit, as before.
    let events = taskmaster_store::audit::for_target(pool, "setting:mail_from", 5)
        .await
        .unwrap();
    assert_eq!(events[0].details["new"], "TaskMaster <tracker@example.com>");

    // A bad server address is refused with the field named; the sender must come with it.
    let broken = Change {
        smtp_url: Some(Some("http://not-smtp".to_owned())),
        ..change()
    };
    let error = mail::save_mail_settings(pool, &admin, &broken, Some(&key), None)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, taskmaster_service::Error::Invalid(problems) if problems.iter().any(|p| p.path == "smtp_url")),
        "{error:?}"
    );
    let no_sender = Change {
        mail_from: Some(String::new()),
        ..change()
    };
    let error = mail::save_mail_settings(pool, &admin, &no_sender, Some(&key), None)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, taskmaster_service::Error::Invalid(problems) if problems.iter().any(|p| p.path == "mail_from")),
        "{error:?}"
    );

    // Clearing a secret removes its row; the view says so.
    let cleared = mail::save_mail_settings(
        pool,
        &admin,
        &Change {
            smtp_password: Some(None),
            ..Change::default()
        },
        Some(&key),
        None,
    )
    .await
    .unwrap();
    assert!(!cleared.has_password);
    let left: Vec<String> = sqlx::query_scalar("select key from setting_secrets")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(left, ["smtp_url", "smtp_username"]);
}

#[tokio::test]
async fn ntf_006_from_reply_to() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let key = key();

    let view = mail::save_mail_settings(pool, &admin, &change(), Some(&key), None)
        .await
        .unwrap();
    assert_eq!(view.mail_reply_to, "replies@example.com");

    // The saved sender and reply-to ride every message (NTF-006); empty reply-to means none.
    let letters = mail::Letters {
        enabled: true,
        base_url: Some("https://tracker.example.com".to_owned()),
        language: Language::En,
    };
    let outgoing = mail::outgoing(&snapshot(pool, &key).await, None);
    let mailer = outgoing
        .transport
        .as_ref()
        .expect("the settings name a transport");
    let message = mailer
        .message(&mail::test_message(
            &letters,
            "https://tracker.example.com",
            "a@example.com",
        ))
        .unwrap();
    let wire = String::from_utf8_lossy(&message.formatted()).into_owned();
    assert!(
        wire.contains("From: TaskMaster <tracker@example.com>"),
        "{wire}"
    );
    assert!(
        wire.contains("Reply-To: <replies@example.com>")
            || wire.contains("Reply-To: replies@example.com"),
        "{wire}"
    );

    // A reply-to that is not an address names its field.
    let error = mail::save_mail_settings(
        pool,
        &admin,
        &Change {
            mail_reply_to: Some("no-at-sign".to_owned()),
            ..Change::default()
        },
        Some(&key),
        None,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, taskmaster_service::Error::Invalid(problems) if problems.iter().any(|p| p.path == "mail_reply_to")),
        "{error:?}"
    );

    // Clearing the reply-to: replies go back to the sender.
    mail::save_mail_settings(
        pool,
        &admin,
        &Change {
            mail_reply_to: Some(String::new()),
            ..Change::default()
        },
        Some(&key),
        None,
    )
    .await
    .unwrap();
    let outgoing = mail::outgoing(&snapshot(pool, &key).await, None);
    let message = outgoing
        .transport
        .as_ref()
        .unwrap()
        .message(&mail::test_message(
            &letters,
            "https://tracker.example.com",
            "a@example.com",
        ))
        .unwrap();
    assert!(!String::from_utf8_lossy(&message.formatted()).contains("Reply-To:"));
}

#[tokio::test]
async fn ntf_007_admin_test_send() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let plain = member(pool, &admin, "plain").await;
    let key = key();

    // Only administrators send to any address.
    let mailer = mail::Mailer::new(MAILPIT, "TaskMaster <tracker@example.com>").unwrap();
    let error = mail::send_admin_test(
        &plain,
        "anyone@example.com",
        "https://t.example.com",
        Language::En,
        &mailer,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, taskmaster_service::Error::Forbidden(_)),
        "{error:?}"
    );

    // A bad address names its field.
    let error = mail::send_admin_test(
        &admin,
        "not-an-address",
        "https://t.example.com",
        Language::En,
        &mailer,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, taskmaster_service::Error::Invalid(problems) if problems.iter().any(|p| p.path == "to")),
        "{error:?}"
    );

    // A dead transport answers with its complaint in full (NTF-007).
    let dead = mail::Mailer::new("smtp://127.0.0.1:1", "taskmaster <t@example.com>").unwrap();
    let error = mail::send_admin_test(
        &admin,
        "anyone@example.com",
        "https://t.example.com",
        Language::En,
        &dead,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, taskmaster_service::Error::Refused(complaint) if !complaint.is_empty()),
        "{error:?}"
    );

    // The saved settings send: Mailpit takes the message. The anonymous shape (no credentials,
    // NTF-004): a server that offers no authentication works.
    mail::save_mail_settings(
        pool,
        &admin,
        &Change {
            smtp_url: Some(Some(MAILPIT.to_owned())),
            mail_from: Some("TaskMaster <tracker@example.com>".to_owned()),
            ..Change::default()
        },
        Some(&key),
        None,
    )
    .await
    .unwrap();
    let outgoing = mail::outgoing(&snapshot(pool, &key).await, None);
    mail::send_admin_test(
        &admin,
        "anyone@example.com",
        "https://tracker.example.com",
        Language::En,
        outgoing.transport.as_ref().unwrap(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ntf_002_switch_gates_account_emails() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let admin = admin(pool).await;
    let key = key();
    let env = Some(EnvMail {
        url: "smtp://127.0.0.1:1025".to_owned(),
        from: "taskmaster <dev@example.com>".to_owned(),
    });

    // Nothing stored: the environment is the source and email is on (D-026).
    let empty = Snapshot::default();
    let outgoing = mail::outgoing(&empty, env.as_ref());
    assert_eq!(outgoing.source, MailSource::Env);
    assert!(outgoing.enabled);

    // Account emails ride that state: refused while off, queued while on.
    let off = mail::Letters {
        enabled: false,
        base_url: Some("https://tracker.example.com".to_owned()),
        language: Language::En,
    };
    assert!(matches!(
        off.require(),
        Err(taskmaster_service::Error::Conflict(_))
    ));
    assert!(matches!(
        users::send_test_message(pool, &admin, &off).await,
        Err(taskmaster_service::Error::Conflict(_))
    ));
    users::send_test_message(
        pool,
        &admin,
        &mail::Letters {
            enabled: true,
            base_url: Some("https://tracker.example.com".to_owned()),
            language: Language::En,
        },
    )
    .await
    .unwrap();

    // Saved settings hold the switch off until it is switched on (NTF-002)…
    let view = mail::save_mail_settings(pool, &admin, &change(), Some(&key), env.as_ref())
        .await
        .unwrap();
    assert!(!view.email_enabled);
    let saved = snapshot(pool, &key).await;
    let outgoing = mail::outgoing(&saved, env.as_ref());
    assert_eq!(outgoing.source, MailSource::Settings);
    assert!(
        !outgoing.enabled,
        "settings email starts off, environment or not"
    );

    // …switching on without a transport is refused, with one it works.
    let no_transport = Change {
        smtp_url: Some(Some(String::new())),
        mail_from: Some(String::new()),
        email_enabled: Some(true),
        ..Change::default()
    };
    let error = mail::save_mail_settings(pool, &admin, &no_transport, Some(&key), None)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, taskmaster_service::Error::Invalid(problems) if problems.iter().any(|p| p.path == "email_enabled")),
        "{error:?}"
    );
    let view = mail::save_mail_settings(
        pool,
        &admin,
        &Change {
            email_enabled: Some(true),
            ..Change::default()
        },
        Some(&key),
        env.as_ref(),
    )
    .await
    .unwrap();
    assert!(view.email_enabled);
    let outgoing = mail::outgoing(&snapshot(pool, &key).await, env.as_ref());
    assert!(outgoing.enabled);

    // Switching off again is a plain save; the environment does not override it.
    let view = mail::save_mail_settings(
        pool,
        &admin,
        &Change {
            email_enabled: Some(false),
            ..Change::default()
        },
        Some(&key),
        env.as_ref(),
    )
    .await
    .unwrap();
    assert!(!view.email_enabled);
}

/// The queued emails of this database, oldest first.
async fn queued_mail(pool: &PgPool) -> Vec<Mail> {
    let payloads: Vec<serde_json::Value> =
        sqlx::query_scalar("select payload from jobs where kind = 'mail.send' order by id")
            .fetch_all(pool)
            .await
            .unwrap();
    payloads
        .into_iter()
        .map(|payload| serde_json::from_value(payload).unwrap())
        .collect()
}

fn letters(enabled: bool) -> mail::Letters {
    mail::Letters {
        enabled,
        base_url: Some("https://tracker.example.com".to_owned()),
        language: Language::En,
    }
}

/// A `recover-admin` use reports to the built-in administrator's address when email is on
/// (ADM-051) — whoever was recovered — and leaves nothing in the queue while it is off.
#[tokio::test]
async fn adm_051_recovery_email() {
    let db = TestDb::new().await;
    let pool = db.pool();
    // The setup wizard's administrator is the built-in one (chief@example.com).
    let admin = admin(pool).await;
    drop(admin);

    // Email off: the audit row stays the only trace of the use.
    recovery::recover(pool, &passwords(), Target::BuiltinAdmin, &letters(false))
        .await
        .unwrap();
    assert!(
        queued_mail(pool).await.is_empty(),
        "nothing reports while email is off"
    );

    // A new administrator through the command: the report goes to the built-in administrator,
    // not to the account the command created, and never carries the one-time password.
    let created = recovery::recover(
        pool,
        &passwords(),
        Target::New {
            username: "ops".to_owned(),
            email: "ops@example.com".to_owned(),
            full_name: Some("Ops".to_owned()),
        },
        &letters(true),
    )
    .await
    .unwrap();
    let emails = queued_mail(pool).await;
    assert_eq!(emails.len(), 1, "{emails:?}");
    let report = &emails[0];
    assert_eq!(report.to, "chief@example.com");
    assert_eq!(
        report.subject,
        "Administrator access recovered in taskmaster"
    );
    assert!(report.text.contains("ops"), "{}", report.text);
    assert!(
        report.text.contains("tracker.example.com"),
        "{}",
        report.text
    );
    assert!(report.html.contains("https://tracker.example.com"));
    assert!(
        !report.text.contains(&created.password),
        "the one-time password stays on the operator's console"
    );

    // The system queues the report, as it queues notification emails.
    let initiator: Vec<Option<i64>> =
        sqlx::query_scalar("select initiator_id from jobs where kind = 'mail.send' order by id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert!(initiator.iter().all(|id| id.is_none()));
}
