//! Request IDs: every response carries `x-request-id`, and every log line of a request has it.
//!
//! A valid ID sent by the client or a proxy is kept, so one ID follows a request through all hops;
//! otherwise the server makes one (UUID v7, ordered by time). Users quote it when reporting a
//! problem.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// The ID of the current request, available as a request extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub Arc<str>);

pub fn header() -> HeaderName {
    HeaderName::from_static("x-request-id")
}

pub async fn middleware(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(header())
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_valid(value))
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::now_v7().simple().to_string());
    request
        .extensions_mut()
        .insert(RequestId(Arc::from(id.as_str())));

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(header(), value);
    }
    response
}

/// Short and plain, so a client cannot put arbitrary text into logs.
fn is_valid(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_ids_only() {
        assert!(is_valid("0192f3a4-5b6c-7d8e-9f00-112233445566"));
        assert!(is_valid("lb:abc.123_x"));
        assert!(!is_valid(""));
        assert!(!is_valid(&"a".repeat(65)));
        assert!(!is_valid("two words"));
        assert!(!is_valid("quote\"d"));
    }
}
