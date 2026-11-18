//! How people appear to a viewer (ACC-015, ACC-016): with their names, under a pseudonym when the
//! viewer may not see the directory, or as a placeholder when the account was deleted (ACC-006).
//! Every answer that names a person goes through [`Names::show`].

use std::sync::Arc;

use sha2::{Digest, Sha256};
use taskmaster_domain::IssueId;
use taskmaster_domain::UserId;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::identity::{pseudonym, sees_name};
use taskmaster_domain::access::permission::Permission;
use taskmaster_store::issues as store_issues;
use taskmaster_store::users::{self, DirectoryFilter, User};
use taskmaster_store::{PgPool, instance};

use crate::Result;
use crate::secrets::random_bytes;

/// How a person is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// With their real name.
    Named,
    /// Under a pseudonym number, stable for this viewer.
    Masked(u32),
    /// The account was deleted.
    Deleted,
}

/// A person as one viewer sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: UserId,
    /// Real values for [`Shown::Named`]; English stand-ins otherwise (the UI shows its own text).
    pub username: String,
    pub full_name: String,
    pub avatar_color: i16,
    /// The uploaded avatar's key, only for people shown by name (ACC-017).
    pub avatar_key: Option<String>,
    pub shown: Shown,
    pub disabled: bool,
}

/// Turns accounts into what a viewer may see of them.
#[derive(Clone)]
pub struct Names {
    /// The instance secret that keys pseudonyms.
    secret: Arc<[u8]>,
}

impl std::fmt::Debug for Names {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Names(***)")
    }
}

impl Names {
    /// Loads the instance secret, creating it on the first start.
    pub async fn load(pool: &PgPool) -> Result<Self> {
        let generated = random_bytes::<32>()?;
        let secret = instance::secret(pool, &generated).await?;
        Ok(Self::with_secret(&secret))
    }

    pub fn with_secret(secret: &[u8]) -> Self {
        Self {
            secret: Arc::from(secret),
        }
    }

    pub fn show(&self, viewer: &Access, user: &User) -> Person {
        if user.is_deleted() {
            return Person {
                id: user.id,
                username: "deleted".to_owned(),
                full_name: "Deleted user".to_owned(),
                avatar_color: user.avatar_color,
                avatar_key: None,
                shown: Shown::Deleted,
                disabled: true,
            };
        }
        if !sees_name(viewer, user.id) {
            let number = pseudonym(self.digest(viewer.user(), user.id));
            return Person {
                id: user.id,
                username: format!("user-{number}"),
                full_name: format!("User {number}"),
                avatar_color: (number % 8) as i16,
                avatar_key: None,
                shown: Shown::Masked(number),
                disabled: false,
            };
        }
        Person {
            id: user.id,
            username: user.username.clone(),
            full_name: user.full_name.clone(),
            avatar_color: user.avatar_color,
            avatar_key: user.avatar_key.clone(),
            shown: Shown::Named,
            disabled: user.disabled_at.is_some(),
        }
    }

    fn digest(&self, viewer: UserId, target: UserId) -> [u8; 4] {
        let mut hasher = Sha256::new();
        hasher.update(&self.secret);
        hasher.update(viewer.to_be_bytes());
        hasher.update(target.to_be_bytes());
        let digest = hasher.finalize();
        [digest[0], digest[1], digest[2], digest[3]]
    }
}

/// Most people offered for a mention.
pub const MENTION_MAX: i64 = 20;
/// How many directory matches are checked for access to the issue.
const MENTION_SCAN: i64 = 50;

/// Whom the mention list offers while typing `@text` in an issue's text (CNT-054): people of the
/// directory whose name or username starts with the text and who can see the issue. Empty
/// without `user.directory`: pseudonymous viewers learn no names (D-025).
pub async fn mention_candidates(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
    text: &str,
    limit: i64,
) -> Result<Vec<User>> {
    let mut conn = pool.acquire().await?;
    let found = store_issues::get(&mut *conn, issue).await?;
    let (issue, project) = crate::issues::checked(&mut conn, viewer, found).await?;
    if !viewer.has(Permission::UserDirectory) {
        return Ok(Vec::new());
    }
    let text: String = text.trim().to_lowercase().chars().take(100).collect();
    let matches = users::directory(
        &mut conn,
        &DirectoryFilter {
            text: &text,
            enabled: true,
            disabled: false,
        },
        ("", 0),
        MENTION_SCAN,
    )
    .await?;
    let matches: Vec<User> = matches.into_iter().filter(|u| !u.is_guest()).collect();
    let accesses = crate::access::compile_many(&mut conn, &matches).await?;
    Ok(matches
        .into_iter()
        .zip(accesses)
        .filter(|(_, access)| crate::issues::can_read(access, &issue, &project))
        .map(|(user, _)| user)
        .take(usize::try_from(limit.clamp(1, MENTION_MAX)).unwrap_or(1))
        .collect())
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;

    fn user(id: UserId) -> User {
        User {
            id,
            username: format!("u{id}"),
            full_name: format!("User Number {id}"),
            email: None,
            email_verified_at: None,
            builtin: None,
            avatar_color: 3,
            avatar_key: None,
            disabled_at: None,
            disabled_reason: None,
            disabled_label: None,
            invited_at: None,
            deleted_at: None,
            version: 1,
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn acc_016_pseudonyms_are_stable_per_viewer() {
        let names = Names::with_secret(b"secret");
        let nobody = Access::none(1);
        let other = Access::none(2);
        let seen = names.show(&nobody, &user(5));
        assert!(matches!(seen.shown, Shown::Masked(_)));
        assert!(!seen.full_name.contains("Number"));
        assert_eq!(names.show(&nobody, &user(5)), seen);
        assert_ne!(names.show(&other, &user(5)).shown, seen.shown);
        // Oneself is always named.
        assert_eq!(names.show(&nobody, &user(1)).shown, Shown::Named);
    }

    #[test]
    fn acc_006_deleted_accounts_are_placeholders() {
        let names = Names::with_secret(b"secret");
        let mut gone = user(5);
        gone.deleted_at = Some(OffsetDateTime::UNIX_EPOCH);
        let shown = names.show(&Access::none(1), &gone);
        assert_eq!(shown.shown, Shown::Deleted);
        assert_eq!(shown.full_name, "Deleted user");
    }
}
