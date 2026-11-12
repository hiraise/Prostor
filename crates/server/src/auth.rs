//! Recognizing the caller (D-023): a browser session in an HTTP-only cookie, or a personal API token
//! in `Authorization: Bearer`. Handlers ask for [`Caller`] (either) or [`SessionCaller`] (only a
//! signed-in browser: changing credentials is never possible with a token).
//!
//! Cookies are sent by the browser on its own, so requests that change something with a cookie
//! must come from our own pages: the `Origin` must be this server (architecture.md §8). Bodies are
//! JSON only, and the cookie is `SameSite=Lax`, which already stops cross-site forms.

use std::net::IpAddr;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, header};
use time::{Duration, OffsetDateTime};
use trackflow_api::error::ErrorCode;
use trackflow_domain::auth::{Principal, Scope};
use trackflow_domain::settings::{BASE_URL, session_policy};
use trackflow_store::sessions;

use crate::AppState;
use crate::client::client_ip;
use crate::error::ApiError;

/// Name of the session cookie.
pub const SESSION_COOKIE: &str = "trackflow_session";
/// The cookie goes only to the API, not with every static file.
const COOKIE_PATH: &str = "/api";

/// The signed-in caller: a browser session or an API token.
#[derive(Debug, Clone)]
pub struct Caller(pub Principal);

impl Caller {
    /// Refuses a token without `scope`. Sessions may do everything their user may.
    pub fn require(&self, scope: Scope) -> Result<(), ApiError> {
        require_scope(&self.0, scope)
    }
}

/// Refuses a token without `scope`.
pub fn require_scope(principal: &Principal, scope: Scope) -> Result<(), ApiError> {
    if principal.allows(scope) {
        Ok(())
    } else {
        Err(ApiError::new(
            ErrorCode::Forbidden,
            format!("The API token does not have the {} scope", scope.key()),
        ))
    }
}

/// Refuses a token that has none of `scopes`.
pub fn require_any_scope(principal: &Principal, scopes: &[Scope]) -> Result<(), ApiError> {
    if scopes.iter().any(|scope| principal.allows(*scope)) {
        Ok(())
    } else {
        let keys: Vec<&str> = scopes.iter().map(|scope| scope.key()).collect();
        Err(ApiError::new(
            ErrorCode::Forbidden,
            format!("The API token needs one of the scopes {}", keys.join(", ")),
        ))
    }
}

/// A caller signed in with a browser session.
#[derive(Debug, Clone)]
pub struct SessionCaller {
    pub principal: Principal,
    pub session: i64,
}

/// Recognizes the caller without the checks [`Caller`] adds.
async fn authenticate(parts: &mut Parts, state: &AppState) -> Result<Principal, ApiError> {
    let now = OffsetDateTime::now_utc();
    if let Some(authorization) = parts.headers.get(header::AUTHORIZATION) {
        // The scheme name is case-insensitive (RFC 9110).
        let token = authorization
            .to_str()
            .ok()
            .and_then(|value| value.trim().split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, token)| token.trim())
            .ok_or_else(|| {
                unauthenticated("Send an API token as `Authorization: Bearer <token>`")
            })?;
        return trackflow_service::auth::authenticate_token(&state.db, token, now)
            .await?
            .ok_or_else(|| unauthenticated("The API token is not valid"));
    }

    let Some(cookie) = cookie(&parts.headers, SESSION_COOKIE) else {
        return Err(unauthenticated("Sign in or send an API token"));
    };
    let policy = session_policy(&state.settings.get());
    let principal = trackflow_service::auth::authenticate_session(&state.db, cookie, policy, now)
        .await?
        .ok_or_else(|| unauthenticated("The session has ended; sign in again"))?;
    if !is_safe(&parts.method) && !same_origin(parts, state) {
        return Err(cross_site());
    }
    Ok(principal)
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let principal = authenticate(parts, state).await?;
        if principal.must_change_password() {
            return Err(ApiError::new(
                ErrorCode::PasswordChangeRequired,
                "Choose a new password first",
            ));
        }
        Ok(Caller(principal))
    }
}

impl FromRequestParts<AppState> for SessionCaller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let Caller(principal) = Caller::from_request_parts(parts, state).await?;
        session_of(principal)
    }
}

/// A signed-in browser session, also one that must choose a new password first (AUTH-010):
/// for reading the session and changing the password.
#[derive(Debug, Clone)]
pub struct PendingSession(pub SessionCaller);

impl FromRequestParts<AppState> for PendingSession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let principal = authenticate(parts, state).await?;
        session_of(principal).map(PendingSession)
    }
}

fn session_of(principal: Principal) -> Result<SessionCaller, ApiError> {
    let session = principal.session_id().ok_or_else(|| {
        ApiError::new(
            ErrorCode::Forbidden,
            "This needs a signed-in session; API tokens cannot manage credentials",
        )
    })?;
    Ok(SessionCaller { principal, session })
}

/// Facts about the request that sign-in and setup need before anyone is signed in.
#[derive(Debug, Clone)]
pub struct RequestInfo {
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    /// The request comes from a page of this server (or from a client that is not a browser).
    pub same_origin: bool,
    /// The session cookie, if the browser sent one.
    pub session_cookie: Option<String>,
    /// The page's origin (`https://host[:port]`) when the request comes from this server's own
    /// pages: where links in emails point when no base URL is set.
    pub origin: Option<String>,
}

impl RequestInfo {
    /// Refuses requests sent by another site's pages.
    pub fn require_same_origin(&self) -> Result<(), ApiError> {
        if self.same_origin {
            Ok(())
        } else {
            Err(cross_site())
        }
    }

    pub fn origin(&self) -> sessions::Origin {
        sessions::Origin {
            ip: self.ip.map(|ip| ip.to_string()),
            user_agent: self.user_agent.clone(),
        }
    }
}

impl FromRequestParts<AppState> for RequestInfo {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        Ok(RequestInfo {
            ip: client_ip(parts, &state.trusted_proxies),
            user_agent: parts
                .headers
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            same_origin: same_origin(parts, state),
            session_cookie: cookie(&parts.headers, SESSION_COOKIE).map(str::to_owned),
            origin: parts
                .headers
                .get(header::ORIGIN)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| url::Url::parse(value).ok())
                .filter(|_| same_origin(parts, state))
                .map(|url| url.origin().ascii_serialization()),
        })
    }
}

fn unauthenticated(message: &str) -> ApiError {
    ApiError::new(ErrorCode::Unauthenticated, message)
}

fn cross_site() -> ApiError {
    ApiError::new(
        ErrorCode::Forbidden,
        "Requests from other sites are refused (the Origin header does not match this server)",
    )
}

fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// The value of a cookie in the request.
pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

/// Whether a request comes from this server's own pages. Browsers send `Origin` with every request
/// that may change something; it must name this server (the `Host` the request was sent to, or
/// the configured base URL). Without `Origin` the request is not from a browser page, unless
/// `Sec-Fetch-Site` says it came from another site.
fn same_origin(parts: &Parts, state: &AppState) -> bool {
    let headers = &parts.headers;
    let Some(origin) = headers.get(header::ORIGIN) else {
        let site = headers
            .get("sec-fetch-site")
            .and_then(|value| value.to_str().ok());
        return !matches!(site, Some("cross-site" | "same-site"));
    };
    let Some(origin) = origin
        .to_str()
        .ok()
        .and_then(|origin| url::Url::parse(origin).ok())
    else {
        return false;
    };
    let Some(authority) = authority(&origin) else {
        return false;
    };
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase);
    if host.as_deref() == Some(authority.as_str()) {
        return true;
    }
    state
        .settings
        .get()
        .get(&BASE_URL)
        .and_then(|base| url::Url::parse(&base).ok())
        .is_some_and(|base| base.origin() == origin.origin())
}

/// `host[:port]` of a URL as a browser writes it in `Host` (no port when it is the default).
fn authority(url: &url::Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// Cookies carry `Secure` when the instance is served over HTTPS (its base URL says so).
fn secure(state: &AppState) -> bool {
    state
        .settings
        .get()
        .get(&BASE_URL)
        .is_some_and(|base| base.starts_with("https://"))
}

/// `Set-Cookie` for a new session. Without a lifetime the browser forgets it when it closes.
pub fn session_cookie(state: &AppState, value: &str, lifetime: Option<Duration>) -> HeaderValue {
    let mut cookie =
        format!("{SESSION_COOKIE}={value}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax");
    if secure(state) {
        cookie.push_str("; Secure");
    }
    if let Some(lifetime) = lifetime {
        cookie.push_str(&format!("; Max-Age={}", lifetime.whole_seconds()));
    }
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// `Set-Cookie` that removes the session cookie.
pub fn clear_session_cookie(state: &AppState) -> HeaderValue {
    let mut cookie =
        format!("{SESSION_COOKIE}=; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure(state) {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_session_cookie_among_others() {
        let mut headers = HeaderMap::new();
        headers.append(header::COOKIE, HeaderValue::from_static("theme=dark; a=b"));
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("trackflow_session=abc123; x=y"),
        );
        assert_eq!(cookie(&headers, SESSION_COOKIE), Some("abc123"));
        assert_eq!(cookie(&headers, "theme"), Some("dark"));
        assert_eq!(cookie(&headers, "missing"), None);
        let mut empty = HeaderMap::new();
        empty.insert(
            header::COOKIE,
            HeaderValue::from_static("trackflow_session="),
        );
        assert_eq!(cookie(&empty, SESSION_COOKIE), None);
    }

    #[test]
    fn authorities_match_the_host_header() {
        let parse = |url: &str| authority(&url::Url::parse(url).unwrap()).unwrap();
        assert_eq!(parse("http://localhost:5173"), "localhost:5173");
        assert_eq!(parse("https://Tracker.Example.com"), "tracker.example.com");
        assert_eq!(
            parse("https://tracker.example.com:443"),
            "tracker.example.com"
        );
        assert_eq!(parse("http://[::1]:7700"), "[::1]:7700");
    }
}
