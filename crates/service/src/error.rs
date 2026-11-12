use thiserror::Error;
use trackflow_domain::access::Denied;
use trackflow_domain::access::permission::PermissionSet;
use trackflow_domain::validation::{FieldProblem, Problem, Rejection};

/// Why a use case did not happen. The server maps each variant to an API error code.
#[derive(Debug, Error)]
pub enum Error {
    /// Input values break the domain rules; every problem is listed.
    #[error("some values are invalid")]
    Invalid(Vec<FieldProblem>),
    /// The login and password do not match an account that may sign in.
    #[error("the login or password is not correct")]
    InvalidCredentials,
    /// No such object, or the caller may not know it exists.
    #[error("{0}")]
    NotFound(&'static str),
    /// The caller lacks these permissions (ACC-050: refusals name them).
    #[error("missing permissions: {0:?}")]
    Forbidden(PermissionSet),
    /// Not allowed for a reason other than a missing permission: not the owner of a shared
    /// thing, or without the share-views capability (ACC-066).
    #[error("{0}")]
    Refused(std::borrow::Cow<'static, str>),
    /// The current state does not allow the change.
    #[error("{0}")]
    Conflict(&'static str),
    /// The change would leave no enabled system administrator (D-025).
    #[error("this would leave no enabled system administrator")]
    LastAdministrator,
    /// A file is larger than the upload limit (CNT-019).
    #[error("the file is larger than {0} bytes")]
    TooLarge(u64),
    /// Gravatar has no picture for the person's address (ACC-017).
    #[error("Gravatar has no picture for this address")]
    GravatarNotFound,
    /// Gravatar could not be reached or answered with something unusable.
    #[error("Gravatar is not available: {0}")]
    GravatarUnavailable(String),
    /// A link or a link type change breaks the link rules (D-034).
    #[error("{}", .0.message())]
    Link(trackflow_domain::links::Refusal),
    /// A search query has errors (QRY-002); every problem (notes too) with its span.
    #[error("the query has errors")]
    InvalidQuery(Vec<trackflow_query::Problem>),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// An unexpected failure outside the database (random source, hashing).
    #[error("{0}")]
    Internal(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<Denied> for Error {
    fn from(denied: Denied) -> Self {
        Error::Forbidden(denied.missing)
    }
}

impl From<trackflow_domain::sharing::Refusal> for Error {
    fn from(refusal: trackflow_domain::sharing::Refusal) -> Self {
        use trackflow_domain::sharing::Refusal;
        Error::Refused(
            match refusal {
                Refusal::NotOwner => "only the owner or a user administrator may do this",
                Refusal::NoCapability => "sharing is not switched on for you",
            }
            .into(),
        )
    }
}

impl Error {
    /// One invalid field.
    pub fn field(path: &str, problem: Problem, message: impl Into<String>) -> Self {
        Self::Invalid(vec![FieldProblem {
            path: path.to_owned(),
            problem,
            message: message.into(),
        }])
    }

    /// One invalid field, from a domain rule.
    pub fn rejected(path: &str, rejection: Rejection) -> Self {
        Self::field(path, rejection.problem, rejection.message)
    }
}
