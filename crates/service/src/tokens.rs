//! Personal API tokens (AUTH-029, AUTH-030): created with a name and scopes, shown once, used as a
//! bearer token, deleted to revoke.

use serde_json::json;
use taskmaster_domain::UserId;
use taskmaster_domain::accounts::plain_text;
use taskmaster_domain::auth::{Principal, Scopes, TOKEN_NAME_MAX};
use taskmaster_domain::validation::Problems;
use taskmaster_store::api_tokens::{self, ApiToken, NewToken};
use taskmaster_store::{PgPool, audit};

use crate::secrets::{self, Secret};
use crate::{Error, Result};

/// A token as its owner chose it.
#[derive(Debug, Clone)]
pub struct NewTokenInput {
    pub name: String,
    pub scopes: Vec<String>,
}

/// A new token and its secret, which is never shown again.
#[derive(Debug, Clone)]
pub struct Created {
    pub token: ApiToken,
    pub secret: Secret,
}

/// Creates a token for the caller.
pub async fn create(pool: &PgPool, principal: &Principal, input: NewTokenInput) -> Result<Created> {
    let mut problems = Problems::new();
    let name = problems.check("name", plain_text(&input.name, TOKEN_NAME_MAX, "name"));
    let scopes = problems.check("scopes", Scopes::parse(&input.scopes));
    problems.finish().map_err(Error::Invalid)?;
    let (Some(name), Some(scopes)) = (name, scopes) else {
        return Err(Error::Internal("validated values are missing".to_owned()));
    };

    let secret = secrets::token()?;
    let mut tx = pool.begin().await?;
    let token = api_tokens::insert(
        &mut tx,
        &NewToken {
            user: principal.user,
            name: &name,
            scopes: &scopes.keys(),
            token_hash: &secret.hash,
            created_by: principal.user,
        },
    )
    .await?;
    audit::record(
        &mut tx,
        principal.actor(),
        "api_token.created",
        &format!("user:{}", principal.user),
        json!({ "token": token.id, "name": token.name, "scopes": token.scopes }),
    )
    .await?;
    tx.commit().await?;
    Ok(Created { token, secret })
}

/// A page of the user's tokens, newest first, after token `before`.
pub async fn list(
    pool: &PgPool,
    owner: UserId,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<ApiToken>> {
    Ok(api_tokens::list_for_user(pool, owner, before, limit).await?)
}

/// Deletes one of the caller's tokens; it stops working at once.
pub async fn delete(pool: &PgPool, principal: &Principal, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    let token = api_tokens::delete(&mut tx, principal.user, id)
        .await?
        .ok_or(Error::NotFound("no such token"))?;
    audit::record(
        &mut tx,
        principal.actor(),
        "api_token.deleted",
        &format!("user:{}", principal.user),
        json!({ "token": token.id, "name": token.name }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
