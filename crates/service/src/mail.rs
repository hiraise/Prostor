//! Account emails (D-026): invitations, password resets, email verifications and test messages.
//! A use case renders its email and queues it as a job inside its own transaction, so a change
//! that rolls back sends nothing; the job delivers it over SMTP and then forgets the payload,
//! which may hold a one-time link. Notifications (NTF, step 1.10) build on the same queue.
//!
//! The transport itself comes from the admin mail settings (NTF-004, NTF-006, plan step 1.10
//! §5.1), with the environment as the fallback while it is the only source (D-026); the instance
//! email switch (NTF-002) gates everything the queue carries.

use std::time::Duration;

use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use taskmaster_domain::Actor;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::permission::Permission;
use taskmaster_domain::secret::Key;
use taskmaster_domain::settings::Language;
use taskmaster_domain::settings::{
    EMAIL_ENABLED, MAIL_FROM, MAIL_REPLY_TO, SMTP_PASSWORD, SMTP_URL, SMTP_USERNAME, Snapshot,
};
use taskmaster_store::PgPool;
use taskmaster_store::jobs::{self, Enqueue, JobContext, JobError, JobKind};
use taskmaster_store::settings as store;

use taskmaster_domain::notifications::{Content, Kind, Line, Words};
use taskmaster_domain::text::{Op, diff_words};

use crate::{Error, Result};

/// One email to send. Rendered in full before it is queued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub text: String,
    /// Empty when the person asked for plain text only (NTF-066): the message then carries one
    /// part.
    pub html: String,
    /// Threading headers (NTF-064), when the email belongs to a thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threading: Option<Threading>,
    /// The sender's display name (NTF-008): shown when the service address carries none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
}

/// How one email threads in a client (NTF-064): its own `Message-ID` and, when it answers an
/// earlier one of the same thread, that first message's ID for `In-Reply-To` and `References`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Threading {
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
}

impl JobKind for Mail {
    const KIND: &'static str = "mail.send";
    const CANCELLABLE: bool = false;
    const MAX_ATTEMPTS: i32 = 8;
}

/// How account emails are written for this request: whether email works at all, where links
/// point and the language (the instance language until people choose their own, UI-013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letters {
    /// An outgoing mail server is configured.
    pub enabled: bool,
    /// Absolute address of the instance, without a trailing slash (NTF-010): the base URL
    /// setting, else the origin of the request.
    pub base_url: Option<String>,
    pub language: Language,
}

impl Letters {
    /// Refuses when email cannot be sent: the use case needs it to do anything useful.
    pub fn require(&self) -> Result<&str> {
        if !self.enabled {
            return Err(Error::Conflict("outgoing email is not configured"));
        }
        self.base_url.as_deref().ok_or(Error::Conflict(
            "set the address of taskmaster to send links",
        ))
    }

    /// An address in the web app with a one-time secret after `#`: browsers never send the part
    /// after `#` to a server, so the secret stays out of logs and referrers.
    fn link(base: &str, page: &str, secret: &str) -> String {
        format!("{base}/{page}#{secret}")
    }

    /// The host people know the instance by.
    pub(crate) fn site(base: &str) -> String {
        url::Url::parse(base)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| base.to_owned())
    }
}

/// Queues an email inside the caller's transaction.
pub async fn queue(conn: &mut PgConnection, mail: &Mail, initiator: Actor) -> Result<()> {
    jobs::enqueue(
        conn,
        mail,
        Enqueue {
            initiator,
            ..Enqueue::default()
        },
    )
    .await?;
    Ok(())
}

/// The invitation to an account an administrator created (ACC-004).
pub fn invitation(letters: &Letters, base: &str, to: &str, inviter: &str, secret: &str) -> Mail {
    let link = Letters::link(base, "invitation", secret);
    let site = Letters::site(base);
    let (subject, lines, button) = match letters.language {
        Language::En => (
            format!("{inviter} invited you to TaskMaster"),
            vec![
                "Hello,".to_owned(),
                format!(
                    "{inviter} has created an account for you in TaskMaster at {site}. To join, choose a password:"
                ),
                "The link works for 14 days. If you did not expect this email, ignore it."
                    .to_owned(),
            ],
            "Join TaskMaster",
        ),
        Language::Ru => (
            format!("{inviter} приглашает вас в TaskMaster"),
            vec![
                "Здравствуйте!".to_owned(),
                format!(
                    "{inviter} приглашает вас в TaskMaster ({site}): для вас уже создана учётная запись. Чтобы войти, задайте пароль:"
                ),
                "Ссылка действует 14 дней. Если вы не ждали этого письма, просто удалите его."
                    .to_owned(),
            ],
            "Присоединиться",
        ),
    };
    render(to, subject, &lines, 2, &link, button)
}

/// A link to choose a new password (AUTH-008).
pub fn password_reset(
    letters: &Letters,
    base: &str,
    to: &str,
    username: &str,
    secret: &str,
) -> Mail {
    let link = Letters::link(base, "reset-password", secret);
    let site = Letters::site(base);
    let (subject, lines, button) = match letters.language {
        Language::En => (
            "Reset your TaskMaster password".to_owned(),
            vec![
                format!("Someone asked to reset the password of the TaskMaster account {username} at {site}. To choose a new password, open this link:"),
                "The link works once, for one hour. If you did not ask for this, ignore this email: your password stays the same.".to_owned(),
            ],
            "Choose a new password",
        ),
        Language::Ru => (
            "Сброс пароля в taskmaster".to_owned(),
            vec![
                format!("Кто-то запросил сброс пароля учётной записи {username} в TaskMaster ({site}). Чтобы задать новый пароль, откройте ссылку:"),
                "Ссылка действует один час и один раз. Если вы не запрашивали сброс, ничего не делайте: пароль останется прежним.".to_owned(),
            ],
            "Задать новый пароль",
        ),
    };
    render(to, subject, &lines, 1, &link, button)
}

/// A link that confirms an address (AUTH-014).
pub fn email_verification(
    letters: &Letters,
    base: &str,
    to: &str,
    username: &str,
    secret: &str,
) -> Mail {
    let link = Letters::link(base, "verify-email", secret);
    let site = Letters::site(base);
    let (subject, lines, button) = match letters.language {
        Language::En => (
            "Confirm your email for taskmaster".to_owned(),
            vec![
                format!(
                    "Confirm that {to} is the address of the TaskMaster account {username} at {site}:"
                ),
                "The link works for 3 days. If you did not expect this email, ignore it."
                    .to_owned(),
            ],
            "Confirm the address",
        ),
        Language::Ru => (
            "Подтвердите адрес для taskmaster".to_owned(),
            vec![
                format!(
                    "Подтвердите, что {to} — адрес учётной записи {username} в TaskMaster ({site}):"
                ),
                "Ссылка действует 3 дня. Если вы не ждали этого письма, ничего не делайте."
                    .to_owned(),
            ],
            "Подтвердить адрес",
        ),
    };
    render(to, subject, &lines, 1, &link, button)
}

/// A message that shows email reaches the person (ACC-017, NTF-007).
pub fn test_message(letters: &Letters, base: &str, to: &str) -> Mail {
    let site = Letters::site(base);
    let (subject, lines, button) = match letters.language {
        Language::En => (
            "Test message from taskmaster".to_owned(),
            vec![format!(
                "This is a test message from taskmaster at {site}. Email to {to} arrives."
            )],
            "Open taskmaster",
        ),
        Language::Ru => (
            "Проверочное письмо от taskmaster".to_owned(),
            vec![format!(
                "Это проверочное письмо от taskmaster ({site}). Письма на адрес {to} доходят."
            )],
            "Открыть taskmaster",
        ),
    };
    render(to, subject, &lines, 1, base, button)
}

/// The report of one `recover-admin` use to the built-in administrator (ADM-051): the command
/// reaches the operator's console, the report reaches the account's owner.
pub fn recovery_report(
    letters: &Letters,
    base: &str,
    to: &str,
    username: &str,
    created: bool,
    sessions_ended: u64,
) -> Mail {
    let site = Letters::site(base);
    let granted = if created {
        format!(
            "created the account {username} with a one-time password and system administrator \
             rights"
        )
    } else {
        format!(
            "gave the account {username} a new one-time password and system administrator rights"
        )
    };
    let (subject, mut lines, button) = match letters.language {
        Language::En => (
            "Administrator access recovered in taskmaster".to_owned(),
            vec![format!(
                "The recovery command ran at the taskmaster server {site} and {granted}."
            )],
            "Open taskmaster",
        ),
        Language::Ru => (
            "Восстановление доступа администратора в taskmaster".to_owned(),
            vec![format!(
                "На сервере taskmaster {site} сработала команда восстановления: она {granted}."
            )],
            "Открыть taskmaster",
        ),
    };
    if sessions_ended > 0 {
        lines.push(match letters.language {
            Language::En => format!("Its {sessions_ended} sign-in sessions were ended."),
            Language::Ru => format!("Завершено сеансов входа: {sessions_ended}."),
        });
    }
    lines.push(match letters.language {
        Language::En => "The command needs shell access to the server. If you did not expect \
             this email, find out who has that access."
            .to_owned(),
        Language::Ru => "Для запуска команды нужен доступ к серверу. Если вы не ожидали это \
             письмо, выясните, у кого он есть."
            .to_owned(),
    });
    render(to, subject, &lines, lines.len() - 1, base, button)
}

/// Text and HTML of one email: paragraphs, with the link (and its button) after paragraph
/// `link_after`.
fn render(
    to: &str,
    subject: String,
    lines: &[String],
    link_after: usize,
    link: &str,
    button: &str,
) -> Mail {
    let mut text = String::new();
    let mut html = String::from(
        "<!doctype html><html><body style=\"font-family:system-ui,sans-serif;font-size:15px;\
         line-height:1.5;color:#1d2433\">",
    );
    for (index, line) in lines.iter().enumerate() {
        text.push_str(line);
        text.push_str("\n\n");
        html.push_str(&format!("<p>{}</p>", escape(line)));
        if index + 1 == link_after {
            text.push_str(link);
            text.push_str("\n\n");
            html.push_str(&format!(
                "<p><a href=\"{href}\" style=\"display:inline-block;padding:10px 16px;\
                 border-radius:6px;background:#1f3a8a;color:#ffffff;text-decoration:none\">\
                 {button}</a></p><p style=\"font-size:13px;color:#4a5568\">{href}</p>",
                href = escape(link),
                button = escape(button),
            ));
        }
    }
    html.push_str("</body></html>");
    Mail {
        to: to.to_owned(),
        subject,
        text: text.trim_end().to_owned(),
        html,
        threading: None,
        sender_name: None,
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

// ---- The transport of this node (NTF-002, NTF-004; plan step 1.10 §5.1) ----

/// The mail server of the environment: `TASKMASTER_SMTP_URL` and `TASKMASTER_MAIL_FROM`, the source
/// of the transport until the admin settings exist (D-026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvMail {
    pub url: String,
    pub from: String,
}

/// Which source the transport of this node comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailSource {
    /// The admin mail settings (NTF-004).
    Settings,
    /// The environment (D-026): no mail setting has been stored.
    Env,
    /// No transport at all.
    None,
}

/// The outgoing email of this node: the transport, and whether the instance switch (NTF-002)
/// lets anything be sent. Rebuilt whenever a setting changes.
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub transport: Option<Mailer>,
    /// Whether email leaves this node at all: a transport exists and the switch is on.
    pub enabled: bool,
    pub source: MailSource,
}

impl Default for Outgoing {
    /// Before the first reload: no transport, nothing sends.
    fn default() -> Self {
        Self {
            transport: None,
            enabled: false,
            source: MailSource::None,
        }
    }
}

/// Where a transport comes from, given the mail settings and the environment fallback. The URL
/// setting decides the source (D-026: the environment answers while it is the only source); a
/// settings URL needs the settings sender, an environment URL takes the settings sender when one
/// is set, else the environment's.
fn transport_of(
    url_setting: &str,
    from_setting: &str,
    username: &str,
    password: &str,
    reply_to: &str,
    env: Option<&EnvMail>,
) -> (Option<Mailer>, MailSource) {
    let url_setting = url_setting.trim();
    let from_setting = from_setting.trim();
    // A settings URL needs the settings sender; an environment URL takes the settings sender
    // when one is set, else the environment's.
    if !url_setting.is_empty() {
        if from_setting.is_empty() {
            return (None, MailSource::Settings);
        }
        let mailer = Mailer::configure(
            url_setting,
            from_setting,
            non_empty(username),
            non_empty(password),
            non_empty(reply_to),
        )
        .ok();
        return (mailer, MailSource::Settings);
    }
    let Some(env) = env else {
        return (None, MailSource::None);
    };
    let from = if from_setting.is_empty() {
        env.from.as_str()
    } else {
        from_setting
    };
    let mailer = Mailer::configure(
        env.url.as_str(),
        from,
        non_empty(username),
        non_empty(password),
        non_empty(reply_to),
    )
    .ok();
    (mailer, MailSource::Env)
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then(|| value.trim())
}

/// The instance email switch (NTF-002): the stored switch decides once any mail setting exists;
/// until then the environment counts as configured and on (D-026). A stored switch can only hold
/// `on` — `off` is the default, so storing it removes the row.
fn switch_of(stored_switch: bool, stored_url: bool, env_configured: bool) -> bool {
    if stored_switch {
        true
    } else if stored_url {
        false
    } else {
        env_configured
    }
}

/// Builds this node's outgoing email state from a settings snapshot and the environment.
pub fn outgoing(snapshot: &Snapshot, env: Option<&EnvMail>) -> Outgoing {
    let (transport, source) = transport_of(
        &snapshot.get(&SMTP_URL),
        &snapshot.get(&MAIL_FROM),
        &snapshot.get(&SMTP_USERNAME),
        &snapshot.get(&SMTP_PASSWORD),
        &snapshot.get(&MAIL_REPLY_TO),
        env,
    );
    let switch = switch_of(
        snapshot.is_stored(EMAIL_ENABLED.key),
        snapshot.is_stored(SMTP_URL.key),
        env.is_some(),
    );
    Outgoing {
        enabled: transport.is_some() && switch,
        transport,
        source,
    }
}

// ---- The admin mail settings (NTF-002, NTF-004, NTF-006, NTF-007; plan step 1.10 §5.1) ----

/// The mail settings as the administration screen sees them: no secret value ever appears, only
/// whether one is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailSettingsView {
    /// Where the transport of this node comes from.
    pub source: MailSource,
    /// Whether a transport exists at all, switch or not.
    pub configured: bool,
    /// The instance email switch as it stands (NTF-002).
    pub email_enabled: bool,
    pub mail_from: String,
    pub mail_reply_to: String,
    pub has_smtp_url: bool,
    pub has_username: bool,
    pub has_password: bool,
}

/// A change of the mail settings. Secret fields are write-only: `None` keeps the stored value,
/// `Some(None)` clears it, `Some(Some(value))` stores `value`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailSettingsChange {
    pub smtp_url: Option<Option<String>>,
    pub smtp_username: Option<Option<String>>,
    pub smtp_password: Option<Option<String>>,
    pub mail_from: Option<String>,
    pub mail_reply_to: Option<String>,
    pub email_enabled: Option<bool>,
}

impl MailSettingsChange {
    fn touches_secrets(&self) -> bool {
        [&self.smtp_url, &self.smtp_username, &self.smtp_password]
            .into_iter()
            .any(|field| field.as_ref().is_some_and(|value| value.is_some()))
    }
}

/// Reads the mail settings for the screen. Needs `system.read`.
pub async fn mail_settings(
    pool: &PgPool,
    actor: &Access,
    secret_key: Option<&Key>,
    env: Option<&EnvMail>,
) -> Result<MailSettingsView> {
    actor.require(Permission::SystemRead)?;
    let mut tx = pool.begin().await?;
    let view = mail_settings_in(&mut tx, secret_key, env).await?;
    tx.commit().await?;
    Ok(view)
}

async fn mail_settings_in(
    conn: &mut sqlx::PgConnection,
    secret_key: Option<&Key>,
    env: Option<&EnvMail>,
) -> Result<MailSettingsView> {
    let from = store::value(conn, MAIL_FROM.key)
        .await?
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let reply_to = store::value(conn, MAIL_REPLY_TO.key)
        .await?
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let url = stored_secret(conn, secret_key, SMTP_URL.key).await?;
    let username = stored_secret(conn, secret_key, SMTP_USERNAME.key).await?;
    let password = stored_secret(conn, secret_key, SMTP_PASSWORD.key).await?;
    let (transport, source) = transport_of(
        url.as_deref().unwrap_or_default(),
        &from,
        username.as_deref().unwrap_or_default(),
        password.as_deref().unwrap_or_default(),
        &reply_to,
        env,
    );
    let enabled_row = store::value(conn, EMAIL_ENABLED.key)
        .await?
        .is_some_and(|value| value == serde_json::json!(true));
    Ok(MailSettingsView {
        configured: transport.is_some(),
        email_enabled: transport.is_some()
            && switch_of(
                enabled_row,
                url.as_deref().is_some_and(|v| !v.is_empty()),
                env.is_some(),
            ),
        source: if transport.is_some() {
            source
        } else {
            MailSource::None
        },
        mail_from: from,
        mail_reply_to: reply_to,
        has_smtp_url: url.is_some_and(|v| !v.is_empty()),
        has_username: username.is_some_and(|v| !v.is_empty()),
        has_password: password.is_some_and(|v| !v.is_empty()),
    })
}

/// The decrypted value of a secret setting; `None` when nothing is stored.
async fn stored_secret(
    conn: &mut sqlx::PgConnection,
    secret_key: Option<&Key>,
    key: &str,
) -> Result<Option<String>> {
    Ok(match secret_key {
        Some(secret_key) => store::secret_value(conn, secret_key, key)
            .await?
            .and_then(|value| value.as_str().map(str::to_owned)),
        None => store::secret_exists(conn, key)
            .await?
            .then(String::new)
            .filter(|value| !value.is_empty()),
    })
}

/// Saves the mail settings (NTF-004, NTF-006, NTF-002): validates the whole set by building the
/// transport it names, then stores each change with its audit event. Needs `system.write`; a
/// secret value needs the instance key (`TASKMASTER_SECRET_KEY`). Switching email on requires a
/// transport.
pub async fn save_mail_settings(
    pool: &PgPool,
    actor: &Access,
    change: &MailSettingsChange,
    secret_key: Option<&Key>,
    env: Option<&EnvMail>,
) -> Result<MailSettingsView> {
    actor.require(Permission::SystemWrite)?;
    if change.touches_secrets() && secret_key.is_none() {
        return Err(Error::Refused(
            "this instance has no TASKMASTER_SECRET_KEY: set one (32 bytes, base64) to store mail secrets"
                .into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let url = pick(
        &change.smtp_url,
        stored_secret(&mut tx, secret_key, SMTP_URL.key).await?,
    );
    let username = pick(
        &change.smtp_username,
        stored_secret(&mut tx, secret_key, SMTP_USERNAME.key).await?,
    );
    let password = pick(
        &change.smtp_password,
        stored_secret(&mut tx, secret_key, SMTP_PASSWORD.key).await?,
    );
    let from = change
        .mail_from
        .clone()
        .unwrap_or(stored_open(&mut tx, MAIL_FROM.key).await?);
    let reply_to = change
        .mail_reply_to
        .clone()
        .unwrap_or(stored_open(&mut tx, MAIL_REPLY_TO.key).await?);
    let enabled = change.email_enabled.unwrap_or(
        store::value(&mut tx, EMAIL_ENABLED.key)
            .await?
            .is_some_and(|v| v == serde_json::json!(true)),
    );

    // The whole set must name a working transport before anything is stored (NTF-004): each
    // part is checked where the screen can point at it.
    if !url.is_empty()
        && AsyncSmtpTransport::<Tokio1Executor>::from_url(&Mailer::starttls_by_default(&url))
            .is_err()
    {
        return Err(Error::field(
            "smtp_url",
            taskmaster_domain::validation::Problem::Invalid,
            "must be an SMTP address: smtp://host:port or smtps://host:port, credentials optional",
        ));
    }
    for (path, value) in [("mail_from", &from), ("mail_reply_to", &reply_to)] {
        if !value.is_empty() && value.parse::<Mailbox>().is_err() {
            return Err(Error::field(
                path,
                taskmaster_domain::validation::Problem::Invalid,
                "must be an email address, like `TaskMaster <tracker@example.com>`",
            ));
        }
    }
    if !url.is_empty() && from.is_empty() {
        return Err(Error::field(
            "mail_from",
            taskmaster_domain::validation::Problem::Invalid,
            "set the sender address that goes with this mail server",
        ));
    }
    let (transport, _) = transport_of(&url, &from, &username, &password, &reply_to, env);
    if enabled && transport.is_none() {
        return Err(Error::field(
            "email_enabled",
            taskmaster_domain::validation::Problem::Invalid,
            "these settings name no mail server: fill in the server and the sender before switching email on",
        ));
    }

    let changer = taskmaster_domain::Actor::User(actor.user());
    apply_secret(&mut tx, SMTP_URL.key, &change.smtp_url, changer, secret_key).await?;
    apply_secret(
        &mut tx,
        SMTP_USERNAME.key,
        &change.smtp_username,
        changer,
        secret_key,
    )
    .await?;
    apply_secret(
        &mut tx,
        SMTP_PASSWORD.key,
        &change.smtp_password,
        changer,
        secret_key,
    )
    .await?;
    apply_open(
        &mut tx,
        MAIL_FROM.key,
        change.mail_from.as_deref().map(as_json),
        changer,
    )
    .await?;
    apply_open(
        &mut tx,
        MAIL_REPLY_TO.key,
        change.mail_reply_to.as_deref().map(as_json),
        changer,
    )
    .await?;
    apply_open(
        &mut tx,
        EMAIL_ENABLED.key,
        change.email_enabled.map(|on| serde_json::json!(on)),
        changer,
    )
    .await?;
    let view = mail_settings_in(&mut tx, secret_key, env).await?;
    tx.commit().await?;
    Ok(view)
}

/// The stored value of an open setting, as text; empty when nothing is stored.
async fn stored_open(conn: &mut sqlx::PgConnection, key: &str) -> Result<String> {
    Ok(store::value(conn, key)
        .await?
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default())
}

/// `Some(None)` clears, `None` keeps; the resulting value.
fn pick(field: &Option<Option<String>>, current: Option<String>) -> String {
    field
        .clone()
        .unwrap_or(current)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn as_json(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.trim().to_owned())
}

async fn apply_secret(
    conn: &mut sqlx::PgConnection,
    key: &'static str,
    field: &Option<Option<String>>,
    actor: taskmaster_domain::Actor,
    secret_key: Option<&Key>,
) -> Result<()> {
    let Some(value) = field else { return Ok(()) };
    let value = value
        .as_ref()
        .map(|value| serde_json::Value::String(value.clone()));
    let change = taskmaster_domain::settings::prepare(key, value.as_ref())?;
    crate::settings::apply(conn, &change, actor, secret_key).await?;
    Ok(())
}

async fn apply_open(
    conn: &mut sqlx::PgConnection,
    key: &'static str,
    value: Option<serde_json::Value>,
    actor: taskmaster_domain::Actor,
) -> Result<()> {
    if let Some(value) = value {
        let change = taskmaster_domain::settings::prepare(key, Some(&value))?;
        crate::settings::apply(conn, &change, actor, None).await?;
    }
    Ok(())
}

/// Sends a test message to any address through the saved transport, answering the transport's
/// complaint in full (NTF-007). Needs `system.write`. The switch does not gate it: an
/// administrator tests before switching email on.
pub async fn send_admin_test(
    actor: &Access,
    to: &str,
    base: &str,
    language: Language,
    mailer: &Mailer,
) -> Result<()> {
    actor.require(Permission::SystemWrite)?;
    let to = to.trim();
    if to.parse::<Mailbox>().is_err() {
        return Err(Error::field(
            "to",
            taskmaster_domain::validation::Problem::Invalid,
            "must be an email address",
        ));
    }
    let letters = Letters {
        enabled: true,
        base_url: Some(base.to_owned()),
        language,
    };
    let mail = test_message(&letters, base, to);
    mailer
        .probe(&mail)
        .await
        .map_err(|complaint| Error::Refused(complaint.into()))
}

/// The outgoing mail server, from the admin mail settings or the environment (NTF-004, D-026).
#[derive(Clone)]
pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    /// Where replies go instead of the sender (NTF-006).
    reply_to: Option<Mailbox>,
}

impl std::fmt::Debug for Mailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mailer")
            .field("from", &self.from)
            .field("reply_to", &self.reply_to)
            .finish()
    }
}

impl Mailer {
    /// `url`: `smtp://host:port` (STARTTLS when offered, `?tls=required` to insist),
    /// `smtps://host:port` (TLS from the start), with optional `user:password@`. `from`: an
    /// address, optionally with a name: `TaskMaster <tracker@example.com>`.
    pub fn new(url: &str, from: &str) -> std::result::Result<Self, String> {
        Self::configure(url, from, None, None, None)
    }

    /// In lettre a bare `smtp://` address is plaintext, and a submission server answers the
    /// EHLO of an unencrypted connection without AUTH — "no compatible authentication
    /// mechanism". Our addresses say STARTTLS when offered, so the parameter is added unless
    /// the address chooses its own.
    fn starttls_by_default(url: &str) -> String {
        match url::Url::parse(url) {
            Ok(mut parsed)
                if parsed.scheme() == "smtp"
                    && !parsed.query_pairs().any(|(key, _)| key == "tls") =>
            {
                parsed.query_pairs_mut().append_pair("tls", "opportunistic");
                parsed.to_string()
            }
            _ => url.to_owned(),
        }
    }

    /// The full transport of the mail settings (NTF-004, NTF-006): a separate `username` and
    /// `password` when the URL carries none, and a reply-to address.
    pub fn configure(
        url: &str,
        from: &str,
        username: Option<&str>,
        password: Option<&str>,
        reply_to: Option<&str>,
    ) -> std::result::Result<Self, String> {
        let mut builder =
            AsyncSmtpTransport::<Tokio1Executor>::from_url(&Self::starttls_by_default(url))
                .map_err(|error| format!("invalid SMTP address: {error}"))?;
        if let Some(username) = username.filter(|name| !name.is_empty()) {
            builder = builder.credentials(Credentials::new(
                username.to_owned(),
                password.unwrap_or_default().to_owned(),
            ));
        }
        let transport = builder.timeout(Some(Duration::from_secs(30))).build();
        let from = from
            .parse::<Mailbox>()
            .map_err(|error| format!("invalid sender address {from:?}: {error}"))?;
        let reply_to = match reply_to.filter(|address| !address.is_empty()) {
            Some(address) => Some(
                address
                    .parse::<Mailbox>()
                    .map_err(|error| format!("invalid reply address {address:?}: {error}"))?,
            ),
            None => None,
        };
        Ok(Self {
            transport,
            from,
            reply_to,
        })
    }

    /// The message of an email as it goes on the wire: sender, reply-to, recipient, both bodies.
    /// The sender takes the email's display name when the service address has none of its own
    /// (NTF-008), and the threading headers ride along when there are any (NTF-064).
    pub fn message(&self, mail: &Mail) -> std::result::Result<Message, String> {
        let to = mail
            .to
            .parse::<Mailbox>()
            .map_err(|error| format!("invalid recipient {to:?}: {error}", to = mail.to))?;
        let from = match mail.sender_name.as_deref().filter(|name| !name.is_empty()) {
            Some(name) if self.from.name.is_none() => {
                Mailbox::new(Some(name.to_owned()), self.from.email.clone())
            }
            _ => self.from.clone(),
        };
        let mut builder = Message::builder().from(from).to(to).subject(&mail.subject);
        if let Some(reply_to) = &self.reply_to {
            builder = builder.reply_to(reply_to.clone());
        }
        if let Some(threading) = &mail.threading {
            builder = builder.message_id(Some(threading.message_id.clone()));
            if let Some(first) = &threading.first {
                builder = builder.in_reply_to(first.clone()).references(first.clone());
            }
        }
        let message = if mail.html.is_empty() {
            builder.body(mail.text.clone())
        } else {
            builder.multipart(MultiPart::alternative_plain_html(
                mail.text.clone(),
                mail.html.clone(),
            ))
        };
        message.map_err(|error| format!("cannot build the message: {error}"))
    }

    /// Sends one email. A temporary failure (network, a 4xx answer) asks for a retry; a permanent
    /// one (a 5xx answer, a malformed address) fails the job.
    pub async fn deliver(&self, mail: &Mail) -> std::result::Result<(), JobError> {
        let message = self.message(mail).map_err(JobError::Fail)?;
        match self.transport.send(message).await {
            Ok(_) => Ok(()),
            Err(error) if error.is_permanent() => Err(JobError::Fail(error.to_string())),
            Err(error) => Err(JobError::Retry(error.to_string())),
        }
    }

    /// Sends one email now and answers the transport's complaint in full, for the admin test
    /// send (NTF-007): what the screen shows is what the server saw.
    pub async fn probe(&self, mail: &Mail) -> std::result::Result<(), String> {
        let message = self.message(mail)?;
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// The job handler of [`Mail`]: delivers it, then forgets the payload.
    pub async fn run(&self, context: JobContext, mail: Mail) -> std::result::Result<(), JobError> {
        // A run after the payload was forgotten: the email went out already.
        if mail.to.is_empty() {
            return Ok(());
        }
        self.deliver(&mail).await?;
        context.forget_payload().await
    }
}

// ---- The emails of notifications (NTF-008, NTF-059, NTF-060, NTF-062–NTF-064; plan step 1.10
// ---- §5.2–§5.3) ----

/// The facts a `created` email shows beyond its content's lines (§5.2): the whole issue as it
/// starts, everything visible to anyone who can read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewItem<'a> {
    /// The description rendered and stored (D-031), for the HTML body.
    pub description_html: &'a str,
    /// The description as written, for the plain-text body.
    pub description_text: &'a str,
    /// The issue's tags.
    pub tags: Vec<String>,
}

/// Everything one fired notification's email needs (§5.2): the sweep resolves the batch's facts
/// once; the content, the author's name and the email settings arrive per recipient.
#[derive(Clone)]
pub struct NotificationEmail<'a> {
    pub to: &'a str,
    pub kind: Kind,
    /// What the recipient may see of the change, as their inbox row holds it (plan §3.5).
    pub content: &'a Content,
    /// The author as this recipient may see them (ACC-017).
    pub author: &'a str,
    /// The fired time in the instance time zone, already in words.
    pub at: &'a str,
    /// Why this email arrived, in words (NTF-063); the footer shows them all.
    pub reasons: &'a [String],
    /// The new item's facts, for a `created` batch.
    pub new_item: Option<NewItem<'a>>,
    /// Whether the HTML body is wanted (NTF-066); the plain text always is.
    pub html: bool,
    /// Absolute address of the instance, without a trailing slash (NTF-010).
    pub base: &'a str,
    pub words: &'a Words,
    /// This email's place in the person's thread about the issue (NTF-064).
    pub threading: Threading,
}

/// Renders the email of one fired notification: the subject names the kind (NTF-062), the body
/// carries the new item or the change summary — prose edits as a highlighted word diff
/// (NTF-060) — and the footer answers why it arrived with a link to the settings (NTF-063).
pub fn notification(email: NotificationEmail<'_>) -> Mail {
    let words = email.words;
    let issue = &email.content.issue;
    let subject = format!(
        "{}: {} {}",
        words.kind_word(email.kind),
        issue.readable_id,
        crate::notifications::short(&issue.summary)
    );

    // The lines the body shows: a new item opens with its own summary and description, so their
    // empty-old lines stay out (§5.2); a change shows every line it may.
    let lines: Vec<&Line> = email
        .content
        .lines
        .iter()
        .filter(|line| {
            let opening_text =
                matches!(line, Line::Field { old, text: true, .. } if old.is_empty());
            !(email.new_item.is_some() && opening_text)
        })
        .collect();

    let mut text = String::new();
    text.push_str(&format!("{} — {}\n", issue.readable_id, issue.summary));
    let by = if email.new_item.is_some() {
        words.by_new
    } else {
        words.by_change
    };
    text.push_str(&format!("{by}: {}, {}\n", email.author, email.at));

    let mut html = String::from(
        "<!doctype html><html><body style=\"font-family:system-ui,sans-serif;font-size:15px;\
         line-height:1.5;color:#1d2433\">",
    );
    html.push_str(&format!(
        "<p style=\"font-size:17px\"><b>{}</b> — {}</p>",
        escape(&issue.readable_id),
        escape(&issue.summary)
    ));
    html.push_str(&format!(
        "<p>{}: {}, {}</p>",
        escape(by),
        escape(email.author),
        escape(email.at)
    ));

    if let Some(new_item) = &email.new_item {
        text.push_str(&format!(
            "\n{}:\n{}\n",
            words.description, new_item.description_text
        ));
        html.push_str(&format!(
            "<p><b>{}</b></p><div style=\"border-left:3px solid #c9d2e3;padding-left:12px\">{}</div>",
            escape(words.description),
            new_item.description_html
        ));
        if !new_item.tags.is_empty() {
            let line = format!("{}: {}", words.tags, new_item.tags.join(", "));
            text.push_str(&format!("\n{line}\n"));
            html.push_str(&format!("<p>{}</p>", escape(&line)));
        }
    }

    if !lines.is_empty() {
        let heading = if email.new_item.is_some() {
            words.initial
        } else {
            words.changes
        };
        text.push_str(&format!("\n{heading}:\n"));
        html.push_str(&format!(
            "<p style=\"margin-bottom:4px\"><b>{}</b></p><ul style=\"margin-top:0\">",
            escape(heading)
        ));
        for line in lines {
            let (plain, shown) = line_shown(line);
            text.push_str(&format!("  {plain}\n"));
            html.push_str(&format!("<li style=\"margin:2px 0\">{shown}</li>"));
        }
        html.push_str("</ul>");
    }

    // The issue link: a button in HTML, a plain address in text (NTF-010's absolute links).
    let link = format!("{}/issues/{}", email.base, issue.readable_id);
    text.push_str(&format!("\n{link}\n"));
    html.push_str(&format!(
        "<p><a href=\"{href}\" style=\"display:inline-block;padding:10px 16px;border-radius:6px;\
         background:#1f3a8a;color:#ffffff;text-decoration:none\">{}</a></p>",
        escape(words.open),
        href = escape(&link)
    ));

    // The footer: every reason in words, then the settings link; no mute link (NTF-063, §1).
    text.push_str(&format!("\n{}\n", words.footer_why));
    for reason in email.reasons {
        text.push_str(&format!("  — {reason}\n"));
    }
    let settings = format!("{}/account", email.base);
    text.push_str(&format!("\n{}: {settings}\n", words.settings_link));
    html.push_str(&format!(
        "<hr style=\"border:none;border-top:1px solid #e2e8f0\"><p style=\"font-size:13px;color:#4a5568\">{}</p><ul style=\"font-size:13px;color:#4a5568;margin-top:0\">",
        escape(words.footer_why)
    ));
    for reason in email.reasons {
        html.push_str(&format!(
            "<li style=\"margin:2px 0\">{}</li>",
            escape(reason)
        ));
    }
    html.push_str(&format!(
        "</ul><p style=\"font-size:13px;color:#4a5568\"><a href=\"{href}\">{}</a></p>",
        escape(words.settings_link),
        href = escape(&settings)
    ));
    html.push_str("</body></html>");

    Mail {
        to: email.to.to_owned(),
        subject,
        text: text.trim_end().to_owned(),
        html: if email.html { html } else { String::new() },
        threading: Some(email.threading),
        sender_name: (!email.author.is_empty()).then(|| email.author.to_owned()),
    }
}

/// One line of the content as the two bodies show it: the plain text keeps «old → new», the HTML
/// highlights a prose edit's word diff (NTF-060).
fn line_shown(line: &Line) -> (String, String) {
    match line {
        Line::Text { text } => (text.clone(), escape(text)),
        Line::Comment { excerpt, .. } => (
            format!("> {excerpt}"),
            format!(
                "<blockquote style=\"margin:4px 0;padding:2px 10px;border-left:3px solid #c9d2e3\">{}</blockquote>",
                escape(excerpt)
            ),
        ),
        Line::Field {
            name,
            old,
            new,
            text: false,
        } => {
            let plain = if old.is_empty() {
                format!("{name}: {new}")
            } else {
                format!("{name}: {old} → {new}")
            };
            let shown = if old.is_empty() {
                format!("{}: {}", escape(name), escape(new))
            } else {
                format!("{}: {} → {}", escape(name), escape(old), escape(new))
            };
            (plain, shown)
        }
        Line::Field {
            name,
            old,
            new,
            text: true,
        } => (
            format!("{name}: {old} → {new}"),
            format!("{}: {}", escape(name), diff_html(old, new)),
        ),
    }
}

/// A prose edit's word diff as the HTML body highlights it (NTF-060): removed words struck
/// through, added ones underlined.
fn diff_html(old: &str, new: &str) -> String {
    let mut out = String::new();
    for part in diff_words(old, new) {
        match part.op {
            Op::Same => out.push_str(&escape(&part.text)),
            Op::Removed => out.push_str(&format!(
                "<del style=\"color:#b42318\">{}</del>",
                escape(&part.text)
            )),
            Op::Added => out.push_str(&format!(
                "<ins style=\"color:#067647;text-decoration-thickness:2px\">{}</ins>",
                escape(&part.text)
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(language: Language) -> Letters {
        Letters {
            enabled: true,
            base_url: Some("https://tracker.example.com".to_owned()),
            language,
        }
    }

    #[test]
    fn links_keep_the_secret_after_the_hash() {
        let mail = password_reset(
            &letters(Language::En),
            "https://tracker.example.com",
            "anna@example.com",
            "anna",
            "SECRET",
        );
        assert!(
            mail.text
                .contains("https://tracker.example.com/reset-password#SECRET")
        );
        assert!(mail.text.contains("account anna at tracker.example.com"));
        assert!(
            mail.html
                .contains("href=\"https://tracker.example.com/reset-password#SECRET\"")
        );
        assert_eq!(mail.to, "anna@example.com");
    }

    #[test]
    fn emails_speak_the_instance_language_and_escape_names() {
        let mail = invitation(
            &letters(Language::Ru),
            "https://tracker.example.com",
            "bob@example.com",
            "Анна <script>",
            "S",
        );
        assert!(mail.subject.starts_with("Анна <script> приглашает"));
        assert!(mail.html.contains("Анна &lt;script&gt;"));
        assert!(!mail.html.contains("<script>"));
        assert!(mail.text.contains("/invitation#S"));
    }

    // The SMTP transport's pool needs a runtime.
    #[tokio::test]
    async fn email_needs_a_server_and_an_address() {
        let mut off = letters(Language::En);
        off.enabled = false;
        assert!(matches!(off.require(), Err(Error::Conflict(_))));
        let mut nowhere = letters(Language::En);
        nowhere.base_url = None;
        assert!(nowhere.require().is_err());
        assert_eq!(
            letters(Language::En).require().unwrap(),
            "https://tracker.example.com"
        );
        assert!(Mailer::new("smtp://127.0.0.1:1025", "taskmaster <t@example.com>").is_ok());
        assert!(Mailer::new("smtp://127.0.0.1:1025", "not an address").is_err());
    }

    #[test]
    fn a_bare_smtp_address_means_starttls_when_offered() {
        // lettre alone keeps `smtp://` plaintext and a submission server then hides AUTH
        // behind STARTTLS (NTF-004): the transport refused with "no compatible mechanism".
        assert_eq!(
            Mailer::starttls_by_default("smtp://smtp.example.com:587"),
            "smtp://smtp.example.com:587?tls=opportunistic"
        );
        assert_eq!(
            Mailer::starttls_by_default("smtp://robot:pw@smtp.example.com:587"),
            "smtp://robot:pw@smtp.example.com:587?tls=opportunistic"
        );
        // What the address chose stands; `smtps://` encrypts on its own.
        assert_eq!(
            Mailer::starttls_by_default("smtp://smtp.example.com:587?tls=required"),
            "smtp://smtp.example.com:587?tls=required"
        );
        assert_eq!(
            Mailer::starttls_by_default("smtps://smtp.example.com:465"),
            "smtps://smtp.example.com:465"
        );
        // Anything else reaches lettre unchanged and gets its error there.
        assert_eq!(Mailer::starttls_by_default("nonsense"), "nonsense");
    }
}
