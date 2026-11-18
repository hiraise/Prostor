//! Accounts as administrators and their owners manage them (ACC-002…ACC-017, AUTH-010,
//! AUTH-014): the directory and lookup, profiles by tier, creating and inviting, editing identity,
//! disabling and enabling, deleting with a replacement, forcing a password change, and emails
//! that confirm an address or test it.

use prostor_domain::access::Access;
use prostor_domain::access::identity::{self, Tier};
use prostor_domain::access::permission::Permission;
use prostor_domain::accounts::{
    DisabledLabel, Email, FullName, Username, check_new_password, disable_reason,
    full_name_from_email, numbered_username, split_emails, username_from_email,
};
use prostor_domain::auth::LinkPurpose;
use prostor_domain::validation::{Problem, Problems, Rejection};
use prostor_domain::{GroupId, ProjectId, UserId};
use prostor_store::fields as store_fields;
use prostor_store::groups::{self, Group};
use prostor_store::projects::Project;
use prostor_store::users::{self, DirectoryFilter, IdentityChange, NewUser, PasswordState, User};
use prostor_store::{
    PgPool, access as assignments, account_links, api_tokens, audit, projects, sessions, teams,
    value_sets,
};
use serde_json::json;
use time::OffsetDateTime;

use crate::access::{changed, keep_an_administrator};
use crate::links::{issue, send_verification};
use crate::mail::{self, Letters};
use crate::passwords::{Passwords, Verified};
use crate::{Error, Result};

/// Most accounts one bulk action or invitation takes.
pub const BULK_MAX: usize = 100;
/// Longest search text.
const SEARCH_MAX: usize = 100;

/// Which accounts the directory lists by state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StateFilter {
    #[default]
    All,
    Enabled,
    Disabled,
}

/// A row of the directory: the account and its explicit groups.
#[derive(Debug, Clone)]
pub struct DirectoryRow {
    pub user: User,
    pub groups: Vec<Group>,
    /// Projects whose team includes the account, that the viewer may see (ACC-002).
    pub projects: Vec<Project>,
}

/// A page of the directory (ACC-002) or of a people picker (ACC-070), by name. `after` is the
/// `(lowercased name, id)` of the previous page's last row.
pub async fn directory(
    pool: &PgPool,
    viewer: &Access,
    text: &str,
    state: StateFilter,
    after: Option<(String, UserId)>,
    limit: i64,
) -> Result<Vec<DirectoryRow>> {
    viewer.require(Permission::UserDirectory)?;
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let (name, id) = after.unwrap_or_default();
    let mut conn = pool.acquire().await?;
    let rows = users::directory(
        &mut conn,
        &DirectoryFilter {
            text: &text,
            enabled: state != StateFilter::Disabled,
            disabled: state != StateFilter::Enabled,
        },
        (&name, id),
        limit,
    )
    .await?;
    let ids: Vec<UserId> = rows.iter().map(|user| user.id).collect();
    let pairs = groups::memberships(&mut *conn, &ids).await?;
    let mut group_ids: Vec<GroupId> = pairs.iter().map(|(_, group)| *group).collect();
    group_ids.sort_unstable();
    group_ids.dedup();
    let mut found = groups::get_many(&mut *conn, &group_ids).await?;
    found.sort_by_key(|group| group.name.to_lowercase());
    let in_teams = teams::projects_of_users(&mut *conn, &ids).await?;
    let mut project_ids: Vec<ProjectId> = in_teams.iter().map(|(_, project)| *project).collect();
    project_ids.sort_unstable();
    project_ids.dedup();
    let mut found_projects = projects::get_many(&mut *conn, &project_ids).await?;
    found_projects.retain(|project| crate::projects::can_see(viewer, project));
    found_projects.sort_by_key(|project| project.name.to_lowercase());
    Ok(rows
        .into_iter()
        .map(|user| {
            let groups = found
                .iter()
                .filter(|group| pairs.contains(&(user.id, group.id)))
                .cloned()
                .collect();
            let projects = found_projects
                .iter()
                .filter(|project| in_teams.contains(&(user.id, project.id)))
                .cloned()
                .collect();
            DirectoryRow {
                user,
                groups,
                projects,
            }
        })
        .collect())
}

/// One profile as the viewer may see it (ACC-015).
#[derive(Debug, Clone)]
pub struct Profile {
    pub user: User,
    pub tier: Tier,
    /// Every group of the account, built-in ones and project teams included (tier details and
    /// up).
    pub groups: Option<Vec<Group>>,
    /// Projects whose team includes the account, that the viewer may see (tier details and up).
    pub projects: Vec<Project>,
    /// The password's state (full tier only).
    pub password: Option<PasswordState>,
}

/// A profile by ID; an ID of a deleted account resolves to its replacement (ACC-013). Accounts
/// the viewer may not see are "not found".
pub async fn profile(pool: &PgPool, viewer: &Access, id: UserId) -> Result<Profile> {
    let mut conn = pool.acquire().await?;
    let id = users::resolve(&mut conn, id)
        .await?
        .ok_or(Error::NotFound("no such account"))?;
    let tier = identity::tier(viewer, id);
    if tier == Tier::Hidden {
        return Err(Error::NotFound("no such account"));
    }
    let user = users::get(&mut *conn, id)
        .await?
        .filter(|user| !user.is_deleted())
        .ok_or(Error::NotFound("no such account"))?;
    let groups = if tier >= Tier::Details {
        let ids = groups::of_user(&mut *conn, user.id, user.is_guest()).await?;
        let mut found = groups::get_many(&mut *conn, &ids).await?;
        found.sort_by_key(|group| (group.builtin.is_none(), group.name.to_lowercase()));
        Some(found)
    } else {
        None
    };
    let password = if tier == Tier::Full {
        Some(users::password_state(&mut conn, user.id).await?)
    } else {
        None
    };
    let team_projects: Vec<ProjectId> = groups
        .iter()
        .flatten()
        .filter_map(|group| group.team_of)
        .collect();
    let mut found_projects = projects::get_many(&mut *conn, &team_projects).await?;
    found_projects.retain(|project| crate::projects::can_see(viewer, project));
    found_projects.sort_by_key(|project| project.name.to_lowercase());
    Ok(Profile {
        user,
        tier,
        groups,
        projects: found_projects,
        password,
    })
}

/// A new account an administrator types in (ACC-003).
#[derive(Clone)]
pub struct NewAccount {
    pub username: String,
    pub full_name: String,
    pub email: String,
    pub password: String,
    /// Ask for a new password at the first sign-in (AUTH-010).
    pub must_change_password: bool,
}

/// Creates an account with a password (ACC-003). It joins the auto-join groups and gets nothing
/// else: roles and groups are set on its profile afterwards.
pub async fn create(
    pool: &PgPool,
    passwords: &Passwords,
    actor: &Access,
    account: NewAccount,
) -> Result<User> {
    actor.require(Permission::UserCreate)?;
    actor.require(Permission::UserDirectory)?;
    let mut problems = Problems::new();
    let username = problems.check("username", Username::parse(&account.username));
    let full_name = problems.check("full_name", FullName::parse(&account.full_name));
    let email = problems.check("email", Email::parse(&account.email));
    problems.check("password", check_new_password(&account.password));
    let (Some(username), Some(full_name), Some(email)) = (username, full_name, email) else {
        return Err(Error::Invalid(problems.into_vec()));
    };
    problems.finish().map_err(Error::Invalid)?;
    let hash = passwords.hash(&account.password).await?;

    let mut tx = pool.begin().await?;
    taken(&mut tx, username.as_str(), email.as_str(), None).await?;
    let id = users::insert(
        &mut tx,
        &NewUser {
            username: username.as_str(),
            full_name: full_name.as_str(),
            email: Some(email.as_str()),
            builtin: None,
            disabled: false,
            invited: false,
        },
    )
    .await?;
    users::set_password(&mut tx, id, &hash, account.must_change_password).await?;
    groups::join_auto(&mut tx, id).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "user.created",
        &format!("user:{id}"),
        json!({ "must_change_password": account.must_change_password }),
    )
    .await?;
    tx.commit().await?;
    reload(pool, id).await
}

/// Invites people by email (ACC-004): an account is created for each address at once, so its
/// access can be set up before the person joins, and an email with a link to choose a password
/// goes out. An address that already has an account stops the whole invitation.
pub async fn invite(
    pool: &PgPool,
    actor: &Access,
    letters: &Letters,
    emails: &str,
    now: OffsetDateTime,
) -> Result<Vec<User>> {
    actor.require(Permission::UserCreate)?;
    actor.require(Permission::UserDirectory)?;
    let base = letters.require()?;
    let typed = split_emails(emails);
    if typed.is_empty() {
        return Err(Error::field(
            "emails",
            Problem::Required,
            "enter at least one address",
        ));
    }
    if typed.len() > BULK_MAX {
        return Err(Error::field(
            "emails",
            Problem::TooLong,
            format!("invite at most {BULK_MAX} addresses at once"),
        ));
    }
    let mut problems = Problems::new();
    let parsed: Vec<Email> = typed
        .iter()
        .filter_map(|raw| {
            problems.check(
                "emails",
                Email::parse(raw).map_err(|rejection| {
                    Rejection::new(rejection.problem, format!("{raw}: {}", rejection.message))
                }),
            )
        })
        .collect();
    problems.finish().map_err(Error::Invalid)?;

    let mut tx = pool.begin().await?;
    let inviter = users::get(&mut *tx, actor.user())
        .await?
        .map_or_else(|| "prostor".to_owned(), |user| user.full_name);
    let mut problems = Problems::new();
    for email in &parsed {
        if users::find_by_email(&mut tx, email.as_str())
            .await?
            .is_some()
        {
            problems.add(
                "emails",
                Rejection::new(
                    Problem::Taken,
                    format!("{}: an account has this address", email.as_str()),
                ),
            );
        }
    }
    problems.finish().map_err(Error::Invalid)?;
    let mut created = Vec::with_capacity(parsed.len());
    let mut chosen: Vec<String> = Vec::new();
    for email in &parsed {
        let base_name = username_from_email(email);
        let mut used = users::usernames_starting_with(&mut tx, &base_name).await?;
        used.extend(chosen.iter().cloned());
        let username = (0..)
            .map(|n| numbered_username(&base_name, n))
            .find(|candidate| !used.contains(&candidate.to_lowercase()))
            .unwrap_or_else(|| base_name.clone());
        chosen.push(username.to_lowercase());
        let id = users::insert(
            &mut tx,
            &NewUser {
                username: &username,
                full_name: &full_name_from_email(email),
                email: Some(email.as_str()),
                builtin: None,
                disabled: false,
                invited: true,
            },
        )
        .await?;
        groups::join_auto(&mut tx, id).await?;
        let user = users::get(&mut *tx, id)
            .await?
            .ok_or(Error::Internal("the invited account is gone".to_owned()))?;
        let secret = issue(&mut tx, &user, LinkPurpose::Invitation, email.as_str(), now).await?;
        let message = mail::invitation(letters, base, email.as_str(), &inviter, &secret.value);
        mail::queue(&mut tx, &message, actor_of(actor)).await?;
        audit::record(
            &mut tx,
            actor_of(actor),
            "user.invited",
            &format!("user:{id}"),
            json!({ "email": email.as_str() }),
        )
        .await?;
        created.push(user);
    }
    tx.commit().await?;
    Ok(created)
}

/// Sends a new invitation link to an account that has not joined yet; the old link stops
/// working (ACC-004, our addition: the reference cannot resend).
pub async fn resend_invitation(
    pool: &PgPool,
    actor: &Access,
    letters: &Letters,
    id: UserId,
    now: OffsetDateTime,
) -> Result<()> {
    actor.require(Permission::UserCreate)?;
    let base = letters.require()?;
    let mut tx = pool.begin().await?;
    let user = lock_visible(&mut tx, id).await?;
    let (Some(_), Some(email)) = (user.invited_at, user.email.clone()) else {
        return Err(Error::Conflict("the person has joined already"));
    };
    let inviter = users::get(&mut *tx, actor.user())
        .await?
        .map_or_else(|| "prostor".to_owned(), |user| user.full_name);
    let secret = issue(&mut tx, &user, LinkPurpose::Invitation, &email, now).await?;
    let message = mail::invitation(letters, base, &email, &inviter, &secret.value);
    mail::queue(&mut tx, &message, actor_of(actor)).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "user.invitation_resent",
        &format!("user:{id}"),
        json!({ "email": email }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Changes of identity fields (ACC-009, ACC-017); `None` keeps a value.
#[derive(Clone, Default)]
pub struct IdentityInput {
    pub username: Option<String>,
    pub full_name: Option<String>,
    pub email: Option<String>,
    /// Needed when people change their own email and have a password.
    pub current_password: Option<String>,
    /// The version the change was made on (architecture.md §10).
    pub version: i64,
}

/// Changes someone's name, username or email. Owners need `user.self`, everybody else
/// `user.update`. A new email starts unverified and gets a verification link (AUTH-014); people
/// changing their own address confirm it with their password first.
pub async fn update_identity(
    pool: &PgPool,
    passwords: &Passwords,
    actor: &Access,
    letters: &Letters,
    id: UserId,
    input: IdentityInput,
    now: OffsetDateTime,
) -> Result<User> {
    let own = actor.user() == id;
    actor.require(if own {
        Permission::UserSelf
    } else {
        Permission::UserUpdate
    })?;
    let mut conn = pool.acquire().await?;
    let current = lock_visible(&mut conn, id).await?;
    drop(conn);
    let mut problems = Problems::new();
    // An unchanged name is no change, so accounts keep names reserved after they were taken
    // (ACC-017).
    let username = input
        .username
        .as_deref()
        .filter(|raw| raw.trim() != current.username.as_str())
        .and_then(|raw| problems.check("username", Username::parse(raw)));
    let full_name = input
        .full_name
        .as_deref()
        .and_then(|raw| problems.check("full_name", FullName::parse(raw)));
    let email = input
        .email
        .as_deref()
        .and_then(|raw| problems.check("email", Email::parse(raw)));
    problems.finish().map_err(Error::Invalid)?;

    if current.is_guest() {
        return Err(Error::Conflict("the guest account has no profile to edit"));
    }
    let new_email = email.filter(|email| {
        current
            .email
            .as_deref()
            .is_none_or(|old| !old.eq_ignore_ascii_case(email.as_str()))
    });
    if own && new_email.is_some() {
        let mut conn = pool.acquire().await?;
        let stored = users::lock_password(&mut conn, id).await?;
        drop(conn);
        if let Some(stored) = stored {
            let typed = input.current_password.as_deref().unwrap_or_default();
            if typed.is_empty() {
                return Err(Error::field(
                    "current_password",
                    Problem::Required,
                    "the current password is required to change the email",
                ));
            }
            if !matches!(
                passwords.verify(typed, Some(&stored)).await?,
                Verified::Match { .. }
            ) {
                return Err(Error::field(
                    "current_password",
                    Problem::Incorrect,
                    "the current password is not correct",
                ));
            }
        }
    }

    let mut tx = pool.begin().await?;
    let user = lock_visible(&mut tx, id).await?;
    if user.version != input.version {
        return Err(Error::Conflict("the profile was changed by someone else"));
    }
    let mut problems = Problems::new();
    if let Some(username) = &username {
        let (taken, _) = users::taken(&mut tx, username.as_str(), "", Some(id)).await?;
        if taken {
            problems.add(
                "username",
                Rejection::new(Problem::Taken, "the username is taken"),
            );
        }
    }
    if let Some(email) = &new_email {
        let (_, taken) = users::taken(&mut tx, "", email.as_str(), Some(id)).await?;
        if taken {
            problems.add(
                "email",
                Rejection::new(Problem::Taken, "another account has this email"),
            );
        }
    }
    problems.finish().map_err(Error::Invalid)?;
    users::update_identity(
        &mut tx,
        id,
        &IdentityChange {
            username: username.as_ref().map(Username::as_str),
            full_name: full_name.as_ref().map(FullName::as_str),
            email: new_email.as_ref().map(Email::as_str),
        },
    )
    .await?;
    let mut details = serde_json::Map::new();
    if let Some(username) = &username {
        details.insert(
            "username".to_owned(),
            json!({ "old": user.username, "new": username.as_str() }),
        );
    }
    if let Some(full_name) = &full_name {
        details.insert(
            "full_name".to_owned(),
            json!({ "old": user.full_name, "new": full_name.as_str() }),
        );
    }
    if let Some(email) = &new_email {
        details.insert(
            "email".to_owned(),
            json!({ "old": user.email, "new": email.as_str() }),
        );
        account_links::delete_for_user(&mut tx, id, Some(LinkPurpose::EmailVerification.key()))
            .await?;
        if letters.enabled && letters.base_url.is_some() {
            let updated = users::get(&mut *tx, id)
                .await?
                .ok_or(Error::Internal("the account is gone".to_owned()))?;
            send_verification(
                &mut tx,
                letters,
                &updated,
                email.as_str(),
                actor_of(actor),
                now,
            )
            .await?;
        }
    }
    audit::record(
        &mut tx,
        actor_of(actor),
        "user.updated",
        &format!("user:{id}"),
        serde_json::Value::Object(details),
    )
    .await?;
    tx.commit().await?;
    reload(pool, id).await
}

/// Sends a link that confirms the account's current address (AUTH-014).
pub async fn request_verification(
    pool: &PgPool,
    actor: &Access,
    letters: &Letters,
    id: UserId,
    now: OffsetDateTime,
) -> Result<()> {
    actor.require(if actor.user() == id {
        Permission::UserSelf
    } else {
        Permission::UserUpdate
    })?;
    letters.require()?;
    let mut tx = pool.begin().await?;
    let user = lock_visible(&mut tx, id).await?;
    let Some(email) = user.email.clone() else {
        return Err(Error::Conflict("the account has no email"));
    };
    if user.email_verified_at.is_some() {
        return Err(Error::Conflict("the email is verified already"));
    }
    send_verification(&mut tx, letters, &user, &email, actor_of(actor), now).await?;
    tx.commit().await?;
    Ok(())
}

/// Sends a test message to the caller's own address (ACC-017, NTF-007).
pub async fn send_test_message(pool: &PgPool, actor: &Access, letters: &Letters) -> Result<String> {
    actor.require(Permission::UserSelf)?;
    let base = letters.require()?;
    let mut tx = pool.begin().await?;
    let user = lock_visible(&mut tx, actor.user()).await?;
    let email = user
        .email
        .clone()
        .ok_or(Error::Conflict("the account has no email"))?;
    mail::queue(
        &mut tx,
        &mail::test_message(letters, base, &email),
        actor_of(actor),
    )
    .await?;
    tx.commit().await?;
    Ok(email)
}

/// Disables accounts (ACC-005): no sign-in, no tokens, sessions end at once; everything they did
/// stays. Nobody disables themselves, the guest or the last administrator.
pub async fn disable(
    pool: &PgPool,
    actor: &Access,
    ids: &[UserId],
    reason: &str,
    label: DisabledLabel,
) -> Result<()> {
    actor.require(Permission::UserUpdate)?;
    let ids = bulk(ids)?;
    if ids.contains(&actor.user()) {
        return Err(Error::Conflict("you cannot disable your own account"));
    }
    let reason =
        disable_reason(reason).map_err(|rejection| Error::rejected("reason", rejection))?;
    let mut tx = pool.begin().await?;
    let found = lock_all(&mut tx, &ids).await?;
    if found.iter().any(User::is_guest) {
        return Err(Error::Conflict(
            "the guest account is disabled until guest access arrives",
        ));
    }
    users::disable(
        &mut tx,
        &ids,
        Some(reason.as_str()).filter(|reason| !reason.is_empty()),
        label.key(),
    )
    .await?;
    sessions::delete_for_users(&mut tx, &ids).await?;
    keep_an_administrator(&mut tx).await?;
    for id in &ids {
        audit::record(
            &mut tx,
            actor_of(actor),
            "user.disabled",
            &format!("user:{id}"),
            json!({ "reason": reason, "label": label.key() }),
        )
        .await?;
    }
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Enables disabled accounts again (ACC-005).
pub async fn enable(pool: &PgPool, actor: &Access, ids: &[UserId]) -> Result<()> {
    actor.require(Permission::UserUpdate)?;
    let ids = bulk(ids)?;
    let mut tx = pool.begin().await?;
    let found = lock_all(&mut tx, &ids).await?;
    if found.iter().any(User::is_guest) {
        return Err(Error::Conflict("guest access arrives in a later version"));
    }
    users::enable(&mut tx, &ids).await?;
    for id in &ids {
        audit::record(
            &mut tx,
            actor_of(actor),
            "user.enabled",
            &format!("user:{id}"),
            json!({}),
        )
        .await?;
    }
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// What deleting accounts takes away, shown before it happens (architecture.md §9.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeletionImpact {
    pub accounts: i64,
    pub sessions: i64,
    pub tokens: i64,
    pub memberships: i64,
    pub assignments: i64,
}

pub async fn deletion_impact(
    pool: &PgPool,
    actor: &Access,
    ids: &[UserId],
) -> Result<DeletionImpact> {
    actor.require(Permission::UserDelete)?;
    let ids = bulk(ids)?;
    let counts = users::deletion_impact(pool, &ids).await?;
    Ok(DeletionImpact {
        accounts: counts.accounts,
        sessions: counts.sessions,
        tokens: counts.tokens,
        memberships: counts.memberships,
        assignments: counts.assignments,
    })
}

/// Deletes accounts, irreversibly (ACC-006). Each becomes a row without personal data whose ID
/// resolves to `replacement` (ACC-013); credentials, sessions, memberships and direct roles go.
/// Authorship keeps pointing at the deleted row and shows a placeholder. Steps that add
/// references to accounts extend this function (architecture.md §6).
pub async fn delete(
    pool: &PgPool,
    actor: &Access,
    ids: &[UserId],
    replacement: UserId,
) -> Result<()> {
    actor.require(Permission::UserDelete)?;
    let ids = bulk(ids)?;
    if ids.contains(&actor.user()) {
        return Err(Error::Conflict("you cannot delete your own account here"));
    }
    if ids.contains(&replacement) {
        return Err(Error::field(
            "replacement",
            Problem::Invalid,
            "the replacement cannot be one of the deleted accounts",
        ));
    }
    let mut every = ids.clone();
    every.push(replacement);
    let mut tx = pool.begin().await?;
    let found = lock_all(&mut tx, &every).await?;
    if found
        .iter()
        .any(|user| user.is_guest() && user.id != replacement)
    {
        return Err(Error::Conflict("the guest account cannot be deleted"));
    }
    api_tokens::delete_for_users(&mut tx, &ids).await?;
    sessions::delete_for_users(&mut tx, &ids).await?;
    groups::remove_from_all(&mut tx, &ids).await?;
    assignments::delete_for_users(&mut tx, &ids).await?;
    account_links::delete_for_users(&mut tx, &ids).await?;
    users::delete_passwords(&mut tx, &ids).await?;
    users::mark_deleted(&mut tx, &ids, replacement).await?;
    // Projects and value owners move to the replacement; to keep a placeholder when that is the
    // guest, they stay with the deleted account (architecture.md §6).
    let heir = found
        .iter()
        .find(|user| user.id == replacement)
        .filter(|user| !user.is_guest())
        .map(|user| user.id);
    if let Some(heir) = heir {
        projects::replace_owner(&mut tx, &ids, heir).await?;
        value_sets::replace_owner(&mut tx, &ids, Some(heir)).await?;
    }
    // Issues (ACC-006): audiences and user fields that cannot be empty pass to the heir; other
    // user values, stars, votes, unshared tags and drafts go; shared tags pass to
    // the heir (D-038). Authorship stays.
    if let Some(heir) = heir {
        prostor_store::issues::replace_audience_users(&mut tx, &ids, heir).await?;
        prostor_store::comments::replace_audience_users(&mut tx, &ids, heir).await?;
        prostor_store::attachments::replace_audience_users(&mut tx, &ids, heir).await?;
    }
    prostor_store::issue_values::replace_users(&mut tx, &ids, heir).await?;
    prostor_store::issues::forget_users(&mut tx, &ids).await?;
    // Files (D-033): avatars, staged uploads and the files of drafts leave the file store.
    let drafts = prostor_store::issues::draft_ids_of(&mut *tx, &ids).await?;
    let mut keys = prostor_store::attachments::keys_of_issues(&mut *tx, &drafts).await?;
    keys.extend(users::take_avatars(&mut tx, &ids).await?);
    keys.extend(prostor_store::uploads::delete_of_owners(&mut tx, &ids).await?);
    crate::files::queue_removal(&mut tx, keys).await?;
    prostor_store::issues::delete_drafts_of(&mut tx, &ids).await?;
    prostor_store::tags::pass_from_deleted(&mut tx, &ids, heir).await?;
    prostor_store::saved_searches::pass_from_deleted(&mut tx, &ids, heir).await?;
    prostor_store::boards::pass_from_deleted(&mut tx, &ids, heir).await?;
    value_sets::remove_users_everywhere(&mut tx, &ids).await?;
    store_fields::replace_default_users(&mut tx, &ids, heir).await?;
    projects::remove_favorites_of(&mut tx, &ids).await?;
    keep_an_administrator(&mut tx).await?;
    for user in found.iter().filter(|user| user.id != replacement) {
        audit::record(
            &mut tx,
            actor_of(actor),
            "user.deleted",
            &format!("user:{}", user.id),
            json!({
                "username": user.username,
                "full_name": user.full_name,
                "email": user.email,
                "replacement": replacement,
            }),
        )
        .await?;
    }
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Asks someone to choose a new password at their next sign-in (AUTH-010). Not for oneself: the
/// account page changes one's own password directly.
pub async fn require_password_change(pool: &PgPool, actor: &Access, id: UserId) -> Result<()> {
    actor.require(Permission::UserUpdate)?;
    if actor.user() == id {
        return Err(Error::Conflict(
            "change your own password on your account page",
        ));
    }
    let mut tx = pool.begin().await?;
    let user = lock_visible(&mut tx, id).await?;
    if !users::require_password_change(&mut tx, user.id).await? {
        return Err(Error::Conflict("the account has no password"));
    }
    users::touch(&mut tx, &[user.id]).await?;
    audit::record(
        &mut tx,
        actor_of(actor),
        "user.password_change_required",
        &format!("user:{id}"),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Refuses a username or email another account has.
async fn taken(
    conn: &mut sqlx::PgConnection,
    username: &str,
    email: &str,
    except: Option<UserId>,
) -> Result<()> {
    let (username_taken, email_taken) = users::taken(conn, username, email, except).await?;
    let mut problems = Problems::new();
    if username_taken {
        problems.add(
            "username",
            Rejection::new(Problem::Taken, "the username is taken"),
        );
    }
    if email_taken {
        problems.add(
            "email",
            Rejection::new(Problem::Taken, "another account has this email"),
        );
    }
    problems.finish().map_err(Error::Invalid)
}

/// The account, locked; deleted ones are "not found".
async fn lock_visible(conn: &mut sqlx::PgConnection, id: UserId) -> Result<User> {
    users::lock(conn, id)
        .await?
        .filter(|user| !user.is_deleted())
        .ok_or(Error::NotFound("no such account"))
}

/// All of these accounts, locked in ID order; any missing or deleted one is "not found".
async fn lock_all(conn: &mut sqlx::PgConnection, ids: &[UserId]) -> Result<Vec<User>> {
    let found = users::lock_many(conn, ids).await?;
    if found.len() != ids.len() || found.iter().any(User::is_deleted) {
        return Err(Error::NotFound("no such account"));
    }
    Ok(found)
}

/// Validates the IDs of a bulk action: at least one, at most [`BULK_MAX`], each once.
fn bulk(ids: &[UserId]) -> Result<Vec<UserId>> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(Error::field(
            "users",
            Problem::Required,
            "choose at least one account",
        ));
    }
    if ids.len() > BULK_MAX {
        return Err(Error::field(
            "users",
            Problem::TooLong,
            format!("choose at most {BULK_MAX} accounts"),
        ));
    }
    Ok(ids)
}

fn actor_of(access: &Access) -> prostor_domain::Actor {
    prostor_domain::Actor::User(access.user())
}

async fn reload(pool: &PgPool, id: UserId) -> Result<User> {
    users::get(pool, id)
        .await?
        .ok_or(Error::NotFound("the account no longer exists"))
}
