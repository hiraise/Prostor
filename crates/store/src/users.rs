//! Users and their built-in passwords. Rules for the values: `prostor_domain::accounts`.
//!
//! A deleted account keeps its row without personal data (ACC-006): `deleted_at` is set and
//! `replaced_by` names the account its ID resolves to (ACC-013). Lookups by login and the directory
//! skip deleted accounts; loading by ID returns them, so authorship can show a placeholder.

use prostor_domain::UserId;
use sqlx::{PgConnection, PgExecutor, PgPool};
use time::OffsetDateTime;

/// A built-in account the server itself needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    /// Made by the setup wizard; the default target of recovery.
    Admin,
    /// Stands for anonymous visitors; disabled until an administrator enables it.
    Guest,
}

impl Builtin {
    pub fn as_str(self) -> &'static str {
        match self {
            Builtin::Admin => "admin",
            Builtin::Guest => "guest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub email_verified_at: Option<OffsetDateTime>,
    /// `admin` or `guest` for the built-in accounts ([`Builtin`]).
    pub builtin: Option<String>,
    pub avatar_color: i16,
    /// The uploaded avatar in the file store (ACC-017); `None` shows the generated one.
    pub avatar_key: Option<String>,
    pub disabled_at: Option<OffsetDateTime>,
    pub disabled_reason: Option<String>,
    pub disabled_label: Option<String>,
    pub invited_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
    pub version: i64,
    pub created_at: OffsetDateTime,
}

impl User {
    pub fn is(&self, builtin: Builtin) -> bool {
        self.builtin.as_deref() == Some(builtin.as_str())
    }

    pub fn is_guest(&self) -> bool {
        self.is(Builtin::Guest)
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A user to create. Values are already validated.
#[derive(Debug, Clone)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub full_name: &'a str,
    pub email: Option<&'a str>,
    pub builtin: Option<Builtin>,
    pub disabled: bool,
    /// The account waits for its invitation to be accepted (ACC-004).
    pub invited: bool,
}

/// Unique constraints of the table; a violation names one of these.
pub const USERNAME_KEY: &str = "users_username_key";
pub const EMAIL_KEY: &str = "users_email_key";

pub async fn insert(conn: &mut PgConnection, user: &NewUser<'_>) -> Result<UserId, sqlx::Error> {
    sqlx::query_scalar!(
        "insert into users (username, full_name, email, builtin, disabled_at, invited_at)
         values ($1, $2, $3, $4, case when $5 then now() end, case when $6 then now() end)
         returning id",
        user.username,
        user.full_name,
        user.email,
        user.builtin.map(Builtin::as_str),
        user.disabled,
        user.invited,
    )
    .fetch_one(conn)
    .await
}

pub async fn get(db: impl PgExecutor<'_>, id: UserId) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where id = $1",
        id,
    )
    .fetch_optional(db)
    .await
}

/// Users by ID, in any order; missing IDs are skipped. One query for a whole page of references.
pub async fn get_many(db: impl PgExecutor<'_>, ids: &[UserId]) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where id = any($1)",
        ids,
    )
    .fetch_all(db)
    .await
}

/// The user, locked for a change.
pub async fn lock(conn: &mut PgConnection, id: UserId) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where id = $1 for update",
        id,
    )
    .fetch_optional(conn)
    .await
}

/// Several users locked for a change, in ID order (the order every locker uses, so two bulk
/// changes cannot deadlock).
pub async fn lock_many(conn: &mut PgConnection, ids: &[UserId]) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where id = any($1) order by id for update",
        ids,
    )
    .fetch_all(conn)
    .await
}

/// The account an ID stands for now: itself, or the replacement of a deleted account (ACC-013).
pub async fn resolve(conn: &mut PgConnection, id: UserId) -> Result<Option<UserId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select coalesce(replaced_by, id) as "id!" from users where id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await
}

/// The built-in account of this kind, locked for a change.
pub async fn lock_builtin(
    conn: &mut PgConnection,
    builtin: Builtin,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where builtin = $1 and deleted_at is null for update",
        builtin.as_str(),
    )
    .fetch_optional(conn)
    .await
}

/// A user with the password, for signing in.
#[derive(Debug, Clone)]
pub struct WithPassword {
    pub user: User,
    pub password_hash: Option<String>,
    pub must_change: bool,
}

/// The account whose username or email is `login`, ignoring case. Usernames cannot contain `@`,
/// so at most one account matches.
pub async fn find_by_login(
    conn: &mut PgConnection,
    login: &str,
) -> Result<Option<WithPassword>, sqlx::Error> {
    let row = sqlx::query!(
        r#"select u.id, u.username, u.full_name, u.email, u.email_verified_at, u.builtin,
                  u.avatar_color, u.avatar_key, u.disabled_at, u.disabled_reason, u.disabled_label,
                  u.invited_at, u.deleted_at, u.version, u.created_at,
                  p.hash as "password_hash?", p.must_change as "must_change?"
           from users u left join passwords p on p.user_id = u.id
           where (lower(u.username) = lower($1) or lower(u.email) = lower($1))
             and u.deleted_at is null
           limit 1"#,
        login,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| WithPassword {
        user: User {
            id: row.id,
            username: row.username,
            full_name: row.full_name,
            email: row.email,
            email_verified_at: row.email_verified_at,
            builtin: row.builtin,
            avatar_color: row.avatar_color,
            avatar_key: row.avatar_key,
            disabled_at: row.disabled_at,
            disabled_reason: row.disabled_reason,
            disabled_label: row.disabled_label,
            invited_at: row.invited_at,
            deleted_at: row.deleted_at,
            version: row.version,
            created_at: row.created_at,
        },
        password_hash: row.password_hash,
        must_change: row.must_change.unwrap_or(false),
    }))
}

/// Same as [`find_by_login`], locking the user row.
pub async fn lock_by_login(
    conn: &mut PgConnection,
    login: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users
         where (lower(username) = lower($1) or lower(email) = lower($1)) and deleted_at is null
         limit 1 for update",
        login,
    )
    .fetch_optional(conn)
    .await
}

/// The account with this email, ignoring case.
pub async fn find_by_email(
    conn: &mut PgConnection,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where lower(email) = lower($1)",
        email,
    )
    .fetch_optional(conn)
    .await
}

/// Whether the username and the email are taken by another account than `except`.
pub async fn taken(
    conn: &mut PgConnection,
    username: &str,
    email: &str,
    except: Option<UserId>,
) -> Result<(bool, bool), sqlx::Error> {
    let row = sqlx::query!(
        r#"select
             exists(select 1 from users where lower(username) = lower($1) and deleted_at is null
                    and id is distinct from $3) as "username!",
             exists(select 1 from users where lower(email) = lower($2)
                    and id is distinct from $3) as "email!""#,
        username,
        email,
        except,
    )
    .fetch_one(conn)
    .await?;
    Ok((row.username, row.email))
}

/// Usernames in use that start with `prefix` (ignoring case), lowercased, for picking a free
/// numbered one.
pub async fn usernames_starting_with(
    conn: &mut PgConnection,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select lower(username) as "name!" from users
           where lower(username) like replace(replace(replace(lower($1), '\', '\\'), '%', '\%'),
                                              '_', '\_') || '%'
             and deleted_at is null"#,
        prefix,
    )
    .fetch_all(conn)
    .await
}

/// Changes of identity fields; `None` keeps a value. Values are already validated.
#[derive(Debug, Clone, Default)]
pub struct IdentityChange<'a> {
    pub username: Option<&'a str>,
    pub full_name: Option<&'a str>,
    /// A new email; it starts unverified.
    pub email: Option<&'a str>,
}

/// Applies identity changes and bumps the version.
pub async fn update_identity(
    conn: &mut PgConnection,
    id: UserId,
    change: &IdentityChange<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update users set
           username = coalesce($2, username),
           full_name = coalesce($3, full_name),
           email = coalesce($4, email),
           email_verified_at = case when $4::text is not null
                                         and lower($4) is distinct from lower(email)
                                    then null else email_verified_at end,
           version = version + 1,
           updated_at = now()
         where id = $1",
        id,
        change.username,
        change.full_name,
        change.email,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks the email verified if it is still `email` (AUTH-014). Returns whether it was.
pub async fn verify_email(
    conn: &mut PgConnection,
    id: UserId,
    email: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "update users set email_verified_at = coalesce(email_verified_at, now()),
                          version = version + 1, updated_at = now()
         where id = $1 and lower(email) = lower($2) and deleted_at is null",
        id,
        email,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Ends an invitation (ACC-004): the account is a regular one from now on.
pub async fn accept_invitation(conn: &mut PgConnection, id: UserId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update users set invited_at = null, version = version + 1, updated_at = now()
         where id = $1",
        id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Disables accounts with a reason and a label (ACC-005); already disabled ones get the new
/// reason and label. Returns how many rows changed.
pub async fn disable(
    conn: &mut PgConnection,
    ids: &[UserId],
    reason: Option<&str>,
    label: &str,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "update users set disabled_at = coalesce(disabled_at, now()), disabled_reason = $2,
                          disabled_label = $3, version = version + 1, updated_at = now()
         where id = any($1) and deleted_at is null",
        ids,
        reason,
        label,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Enables accounts again (ACC-005). Returns how many were disabled.
pub async fn enable(conn: &mut PgConnection, ids: &[UserId]) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "update users set disabled_at = null, disabled_reason = null, disabled_label = null,
                          version = version + 1, updated_at = now()
         where id = any($1) and disabled_at is not null and deleted_at is null",
        ids,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Turns accounts into rows without personal data that resolve to `replacement` (ACC-006,
/// ACC-013). IDs that already resolved to one of them resolve to the replacement too, so every
/// alias is one step. Credentials, memberships and assignments are removed by their own modules.
pub async fn mark_deleted(
    conn: &mut PgConnection,
    ids: &[UserId],
    replacement: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update users set replaced_by = $2 where replaced_by = any($1)",
        ids,
        replacement,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "update users set deleted_at = now(), replaced_by = $2, full_name = '', email = null,
                          email_verified_at = null, disabled_at = coalesce(disabled_at, now()),
                          disabled_reason = null, disabled_label = null, invited_at = null,
                          version = version + 1, updated_at = now()
         where id = any($1) and deleted_at is null",
        ids,
        replacement,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The password hash of a user, locked for a change.
pub async fn lock_password(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "select hash from passwords where user_id = $1 for update",
        user
    )
    .fetch_optional(conn)
    .await
}

/// Sets or replaces a user's password hash; `must_change` asks for a new one at the next sign-in
/// (AUTH-010).
pub async fn set_password(
    conn: &mut PgConnection,
    user: UserId,
    hash: &str,
    must_change: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into passwords (user_id, hash, must_change) values ($1, $2, $3)
         on conflict (user_id) do update
         set hash = excluded.hash, must_change = excluded.must_change, changed_at = now()",
        user,
        hash,
        must_change,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Asks for a new password at the next sign-in (AUTH-010). Returns `false` when the account has no
/// password.
pub async fn require_password_change(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "update passwords set must_change = true where user_id = $1",
        user
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Facts about a user's password for their profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordState {
    pub has_password: bool,
    pub must_change: bool,
}

pub async fn password_state(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<PasswordState, sqlx::Error> {
    let must_change =
        sqlx::query_scalar!("select must_change from passwords where user_id = $1", user)
            .fetch_optional(conn)
            .await?;
    Ok(PasswordState {
        has_password: must_change.is_some(),
        must_change: must_change.unwrap_or(false),
    })
}

/// Removes passwords (account deletion).
pub async fn delete_passwords(conn: &mut PgConnection, ids: &[UserId]) -> Result<(), sqlx::Error> {
    sqlx::query!("delete from passwords where user_id = any($1)", ids)
        .execute(conn)
        .await?;
    Ok(())
}

/// Replaces a hash only if it is still the one that was read, for upgrading the hash parameters
/// at sign-in without a lock.
pub async fn upgrade_password_hash(
    pool: &PgPool,
    user: UserId,
    old: &str,
    new: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update passwords set hash = $3 where user_id = $1 and hash = $2",
        user,
        old,
        new,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Enables an account (recovery, ADM-051).
pub async fn restore(conn: &mut PgConnection, user: UserId) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update users set disabled_at = null, disabled_reason = null, disabled_label = null,
                          version = version + 1, updated_at = now()
         where id = $1",
        user,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Bumps the version after a change stored elsewhere (memberships, password state).
pub async fn touch(conn: &mut PgConnection, ids: &[UserId]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "update users set version = version + 1, updated_at = now() where id = any($1)",
        ids
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Accounts by lowercased username (mentions, CNT-054); deleted accounts are left out.
pub async fn by_usernames(
    db: impl PgExecutor<'_>,
    usernames: &[String],
) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                created_at
         from users where lower(username) = any($1) and deleted_at is null",
        usernames
    )
    .fetch_all(db)
    .await
}

/// Which accounts a directory page shows (ACC-002, ACC-070).
#[derive(Debug, Clone)]
pub struct DirectoryFilter<'a> {
    /// Lowercased text the full name (or a word of it), username, email or email domain starts
    /// with; empty matches everyone.
    pub text: &'a str,
    pub enabled: bool,
    pub disabled: bool,
}

/// One page of the directory, ordered by lowercased name: accounts after `(full name, id)` of the
/// previous page's last row (`("", 0)` for the first). The cursor keeps the name as stored and
/// SQL lowercases it, so both sides use the same rules. The scan follows the name index and checks the filters on each row:
/// none of them has an index of its own to lose, and the table holds one row per person.
pub async fn directory(
    conn: &mut PgConnection,
    filter: &DirectoryFilter<'_>,
    after: (&str, UserId),
    limit: i64,
) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"select id, username, full_name, email, email_verified_at, builtin, avatar_color, avatar_key,
                  disabled_at, disabled_reason, disabled_label, invited_at, deleted_at, version,
                  created_at
           from users,
                lateral (select replace(replace(replace(lower($1), '\', '\\'), '%', '\%'), '_', '\_')
                         || '%' as pattern) p
           where deleted_at is null
             and (lower(full_name) like p.pattern
                  or lower(full_name) like '% ' || p.pattern
                  or lower(username) like p.pattern
                  or lower(email) like p.pattern
                  or split_part(lower(email), '@', 2) like p.pattern)
             and ((disabled_at is null and $2) or (disabled_at is not null and $3))
             and (lower(full_name), id) > (lower($4), $5)
           order by lower(full_name), id
           limit $6"#,
        filter.text,
        filter.enabled,
        filter.disabled,
        after.0,
        after.1,
        limit,
    )
    .fetch_all(conn)
    .await
}

/// What deleting these accounts would remove (ACC-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletionCounts {
    pub accounts: i64,
    pub sessions: i64,
    pub tokens: i64,
    pub memberships: i64,
    pub assignments: i64,
}

pub async fn deletion_impact(
    db: impl PgExecutor<'_>,
    ids: &[UserId],
) -> Result<DeletionCounts, sqlx::Error> {
    sqlx::query_as!(
        DeletionCounts,
        r#"select
             (select count(*) from users where id = any($1) and deleted_at is null) as "accounts!",
             (select count(*) from sessions where user_id = any($1)) as "sessions!",
             (select count(*) from api_tokens where user_id = any($1)) as "tokens!",
             (select count(*) from group_members where user_id = any($1)) as "memberships!",
             (select count(*) from role_assignments where user_id = any($1)) as "assignments!""#,
        ids,
    )
    .fetch_one(db)
    .await
}

// Avatars and file links (ACC-017, CNT-026).

/// Sets or clears a person's uploaded avatar; answers the key it replaced, whose file the caller
/// removes after the commit.
pub async fn set_avatar_key(
    conn: &mut PgConnection,
    id: UserId,
    key: Option<&str>,
) -> Result<Option<String>, sqlx::Error> {
    let old = sqlx::query_scalar!("select avatar_key from users where id = $1 for update", id)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    sqlx::query!("update users set avatar_key = $2 where id = $1", id, key)
        .execute(conn)
        .await?;
    Ok(old)
}

/// Clears the avatars of accounts being deleted; answers their keys.
pub async fn take_avatars(
    conn: &mut PgConnection,
    ids: &[UserId],
) -> Result<Vec<String>, sqlx::Error> {
    let keys = sqlx::query_scalar!(
        r#"update users u set avatar_key = null
           from (select id, avatar_key from users where id = any($1) and avatar_key is not null
                 for update) old
           where u.id = old.id
           returning old.avatar_key as "key!""#,
        ids
    )
    .fetch_all(conn)
    .await?;
    Ok(keys)
}

/// The person's file link secret, stored on first use from `generated`.
pub async fn link_secret(
    db: impl PgExecutor<'_>,
    id: UserId,
    generated: &[u8],
) -> Result<Vec<u8>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"update users set file_link_secret = coalesce(file_link_secret, $2) where id = $1
           returning file_link_secret as "secret!""#,
        id,
        generated
    )
    .fetch_one(db)
    .await
}

/// The link secrets people have now, for checking links; people without one are left out.
pub async fn link_secrets(
    db: impl PgExecutor<'_>,
    ids: &[UserId],
) -> Result<Vec<(UserId, Vec<u8>)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"select id, file_link_secret as "secret!" from users
           where id = any($1) and file_link_secret is not null"#,
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.secret)).collect())
}
