//! The API error type and everything that turns a failure into the one error format
//! (architecture.md → API conventions): handler errors, rejected JSON bodies and query strings,
//! unknown endpoints, unsupported methods and panics.

use std::any::Any;
use std::fmt;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
use taskmaster_api::error::{ErrorBody, ErrorCode, FieldError};
use taskmaster_api::search::{QueryProblem, QuerySeverity};
use taskmaster_domain::links::Refusal;

/// An error answer of the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    code: ErrorCode,
    message: String,
    fields: Vec<FieldError>,
    /// Permission keys the caller lacks (ACC-050).
    missing_permissions: Vec<String>,
    /// The problems of an invalid query, at UTF-16 offsets.
    problems: Vec<QueryProblem>,
    /// Seconds to wait, sent as `Retry-After`.
    retry_after: Option<u64>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            fields: Vec::new(),
            missing_permissions: Vec::new(),
            problems: Vec::new(),
            retry_after: None,
        }
    }

    /// `invalid_query` with the problems of a query at their places; each error problem is
    /// also a field error on `q`.
    pub fn invalid_query(problems: Vec<QueryProblem>) -> Self {
        let fields = problems
            .iter()
            .filter(|p| p.severity == QuerySeverity::Error)
            .map(|p| {
                let code = serde_json::to_value(p.code)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let message = format!("{code} at {}..{}", p.from, p.to);
                FieldError::new("q", code, message)
            })
            .collect();
        Self {
            fields,
            problems,
            ..Self::new(ErrorCode::InvalidQuery, "The query has errors")
        }
    }

    /// `forbidden`, naming the permissions the caller lacks.
    pub fn missing(permissions: Vec<String>) -> Self {
        let mut error = Self::new(
            ErrorCode::Forbidden,
            format!("Missing permissions: {}", permissions.join(", ")),
        );
        error.missing_permissions = permissions;
        error
    }

    /// `invalid_request` listing every bad parameter.
    pub fn invalid(fields: Vec<FieldError>) -> Self {
        Self {
            code: ErrorCode::InvalidRequest,
            message: "Some parameters are invalid".to_owned(),
            fields,
            missing_permissions: Vec::new(),
            problems: Vec::new(),
            retry_after: None,
        }
    }

    /// `rate_limited` with how long to wait.
    pub fn rate_limited(wait: std::time::Duration) -> Self {
        let mut error = Self::new(
            ErrorCode::RateLimited,
            "Too many failed attempts; wait before trying again",
        );
        // Rounded up: retrying a moment too early would fail again.
        error.retry_after = Some(wait.as_secs() + u64::from(wait.subsec_nanos() > 0));
        error
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }

    /// An unexpected failure: logged in full, answered without details.
    pub fn internal(error: impl fmt::Display) -> Self {
        tracing::error!(%error, "internal error");
        Self::new(ErrorCode::Internal, "Internal server error")
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            code: self.code,
            message: self.message.clone(),
            fields: self.fields.clone(),
            missing_permissions: self.missing_permissions.clone(),
            problems: self.problems.clone(),
        }
    }
}

/// The HTTP status of each error code.
pub fn status(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::InvalidRequest | ErrorCode::InvalidQuery => StatusCode::BAD_REQUEST,
        ErrorCode::Unauthenticated | ErrorCode::InvalidCredentials => StatusCode::UNAUTHORIZED,
        ErrorCode::Forbidden | ErrorCode::PasswordChangeRequired => StatusCode::FORBIDDEN,
        ErrorCode::NotFound | ErrorCode::GravatarNotFound => StatusCode::NOT_FOUND,
        ErrorCode::GravatarUnavailable => StatusCode::BAD_GATEWAY,
        ErrorCode::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        ErrorCode::Conflict
        | ErrorCode::LastAdministrator
        | ErrorCode::LinkTypeLocked
        | ErrorCode::ParentTaken
        | ErrorCode::LinkCycle
        | ErrorCode::HierarchyConflict => StatusCode::CONFLICT,
        ErrorCode::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = status(self.code);
        let body = ErrorBody {
            code: self.code,
            message: self.message,
            fields: self.fields,
            missing_permissions: self.missing_permissions,
            problems: self.problems,
        };
        let mut response = (status, Json(body)).into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::PoolTimedOut => {
                tracing::warn!("no free database connection in time");
                Self::unavailable("The database is busy; try again")
            }
            other => Self::internal(other),
        }
    }
}

impl From<taskmaster_service::Error> for ApiError {
    fn from(error: taskmaster_service::Error) -> Self {
        use taskmaster_service::Error;
        match error {
            Error::Invalid(problems) => Self::invalid(
                problems
                    .into_iter()
                    .map(|problem| {
                        FieldError::new(problem.path, problem.problem.code(), problem.message)
                    })
                    .collect(),
            ),
            Error::InvalidCredentials => Self::new(
                ErrorCode::InvalidCredentials,
                "The login or password is not correct",
            ),
            Error::NotFound(message) => Self::not_found(message),
            Error::Forbidden(missing) => {
                Self::missing(missing.keys().into_iter().map(str::to_owned).collect())
            }
            Error::Refused(message) => Self::new(ErrorCode::Forbidden, message),
            Error::Conflict(message) => Self::new(ErrorCode::Conflict, message),
            Error::LastAdministrator => Self::new(
                ErrorCode::LastAdministrator,
                "This would leave no enabled system administrator",
            ),
            Error::TooLarge(limit) => Self::new(
                ErrorCode::PayloadTooLarge,
                format!("The file is larger than {limit} bytes"),
            ),
            Error::GravatarNotFound => Self::new(
                ErrorCode::GravatarNotFound,
                "Gravatar has no picture for this address",
            ),
            Error::GravatarUnavailable(problem) => {
                tracing::warn!(%problem, "Gravatar import failed");
                Self::new(
                    ErrorCode::GravatarUnavailable,
                    "Gravatar is not available; try again later",
                )
            }
            Error::Link(refusal) => Self::new(
                match refusal {
                    Refusal::Locked => ErrorCode::LinkTypeLocked,
                    Refusal::ParentTaken => ErrorCode::ParentTaken,
                    Refusal::Cycle => ErrorCode::LinkCycle,
                    Refusal::HierarchyConflict => ErrorCode::HierarchyConflict,
                },
                refusal.message(),
            ),
            // Handlers that run queries convert these themselves, with the text for the offsets
            // (`crate::search::run`); here only the codes are known, every offset is 0.
            Error::InvalidQuery(problems) => {
                Self::invalid_query(crate::search::problems("", &problems))
            }
            Error::Database(error) => error.into(),
            Error::Internal(message) => Self::internal(message),
        }
    }
}

/// A JSON request body. A malformed body, a wrong content type or a body that is too large
/// answers in the API error format.
#[derive(Debug, Clone, Copy, Default, FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct JsonBody<T>(pub T);

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        let code = match rejection.status() {
            StatusCode::PAYLOAD_TOO_LARGE => ErrorCode::PayloadTooLarge,
            StatusCode::UNSUPPORTED_MEDIA_TYPE => ErrorCode::UnsupportedMediaType,
            _ => ErrorCode::InvalidRequest,
        };
        Self::new(code, rejection.body_text())
    }
}

/// Query-string parameters. Values that do not parse answer `invalid_request`.
#[derive(Debug, Clone, Copy, Default, FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct QueryParams<T>(pub T);

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::new(ErrorCode::InvalidRequest, rejection.body_text())
    }
}

/// A JSON success response with a status code.
pub fn json<T: Serialize>(status: StatusCode, body: T) -> Response {
    (status, Json(body)).into_response()
}

/// Fallback for paths under `/api` that match no endpoint.
pub async fn api_not_found() -> ApiError {
    ApiError::not_found("No such API endpoint")
}

/// Fallback for a known path with a method it does not support.
pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        ErrorCode::MethodNotAllowed,
        "This endpoint does not support the method",
    )
}

/// Turns a panic in a handler into a logged `internal` error instead of a dropped connection.
pub fn panic_response(panic: Box<dyn Any + Send + 'static>) -> Response {
    let detail = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned());
    tracing::error!(panic = %detail, "request handler panicked");
    ApiError::new(ErrorCode::Internal, "Internal server error").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_its_own_error_status() {
        // Three codes share a status: a failed sign-in is a 401, and clients tell it from an ended
        // session by the code; a session that must choose a new password is a 403 told from other
        // refusals the same way.
        assert_eq!(
            status(ErrorCode::InvalidCredentials),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(ErrorCode::PasswordChangeRequired),
            StatusCode::FORBIDDEN
        );
        // So is the last-administrator guard, a conflict of its own.
        assert_eq!(status(ErrorCode::LastAdministrator), StatusCode::CONFLICT);
        // A missing Gravatar picture is a 404 of its own, so the form can say what happened.
        assert_eq!(status(ErrorCode::GravatarNotFound), StatusCode::NOT_FOUND);
        let codes = [
            ErrorCode::GravatarUnavailable,
            ErrorCode::InvalidRequest,
            ErrorCode::Unauthenticated,
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::MethodNotAllowed,
            ErrorCode::Conflict,
            ErrorCode::PayloadTooLarge,
            ErrorCode::UnsupportedMediaType,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
            ErrorCode::Unavailable,
        ];
        let statuses: std::collections::HashSet<_> = codes.iter().map(|c| status(*c)).collect();
        assert_eq!(statuses.len(), codes.len());
        assert!(
            statuses
                .iter()
                .all(|s| s.is_client_error() || s.is_server_error())
        );
    }

    #[test]
    fn a_panic_becomes_an_internal_error() {
        let response = panic_response(Box::new("boom"));
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
