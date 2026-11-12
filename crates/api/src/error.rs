//! The one error format of the API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::search::QueryProblem;

/// Body of every error response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// Stable machine-readable reason; the UI shows its own translated text for it.
    pub code: ErrorCode,
    /// English explanation for developers and logs; not meant to be shown to users as is.
    pub message: String,
    /// Problems with individual parameters or body fields; present only for `invalid_request`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldError>,
    /// Permission keys the caller lacks; present only for some `forbidden` answers (ACC-050).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_permissions: Vec<String>,
    /// The problems of a query with their places in its text; present only for `invalid_query`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<QueryProblem>,
}

/// Why a request failed. Each code has one HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 400: the request is malformed or has invalid parameters.
    InvalidRequest,
    /// 400: the query has errors; `problems` says which and where, `fields` repeats them on `q`.
    InvalidQuery,
    /// 401: no valid credentials: no session, an ended session or an unknown token.
    Unauthenticated,
    /// 401: a sign-in with a login and password that do not match an account that may sign in.
    InvalidCredentials,
    /// 403: signed in, but not allowed. Objects the caller cannot see answer `not_found` instead.
    Forbidden,
    /// 403: the session may only choose a new password (AUTH-010) until it does.
    PasswordChangeRequired,
    /// 404: no such resource or endpoint.
    NotFound,
    /// 405: the resource does not support this method.
    MethodNotAllowed,
    /// 409: the change conflicts with the current state, for example a stale version.
    Conflict,
    /// 409: the change would leave nobody able to administer the instance.
    LastAdministrator,
    /// 409: the link type is built in and cannot be changed or deleted (LNK-002).
    LinkTypeLocked,
    /// 409: the issue already has a parent of this hierarchy link type (LNK-013).
    ParentTaken,
    /// 409: the link would make an issue its own ancestor (LNK-013).
    LinkCycle,
    /// 409: the type cannot become a hierarchy: some issues have several sources of it or its
    /// links form a loop (LNK-020).
    HierarchyConflict,
    /// 404: Gravatar has no picture for the person's address.
    GravatarNotFound,
    /// 502: Gravatar could not be reached or answered with something unusable.
    GravatarUnavailable,
    /// 413: the request body is too large.
    PayloadTooLarge,
    /// 415: the request body is not JSON.
    UnsupportedMediaType,
    /// 429: too many requests; retry later.
    RateLimited,
    /// 500: unexpected server error.
    Internal,
    /// 503: the server is starting, stopping or cannot reach its database; retry later.
    Unavailable,
}

/// One invalid parameter or body field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FieldError {
    /// Parameter name or path of the body field, such as `limit` or `values.2.name`.
    pub path: String,
    /// Machine-readable reason: `required`, `invalid`, `too_short`, `too_long`, `out_of_range`,
    /// `taken` (another record uses the value), `incorrect` (does not match what is stored, such
    /// as a current password), `unknown` (an unexpected field).
    pub code: String,
    /// English explanation.
    pub message: String,
}

impl FieldError {
    pub fn new(
        path: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}
