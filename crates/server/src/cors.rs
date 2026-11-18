//! Cross-origin access to `/api/v1` (INT-056, plan step 1.12 §7.2): the origins the
//! `api.allowed_origins` setting lists may call the API from browsers, with tokens — a page on a
//! listed origin reads the answers of its `fetch` calls. Nothing else changes: an origin that is
//! not listed gets no headers and the browser keeps the answer to itself, and no answer ever
//! allows credentials, so cookie sessions stay same-origin however the list reads.

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use taskmaster_domain::settings::API_ALLOWED_ORIGINS;

use crate::AppState;

/// The methods a listed origin may call; a preflight naming another is answered without it.
const METHODS: HeaderValue =
    HeaderValue::from_static("GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS");

/// How long a browser may remember one preflight (10 minutes).
const MAX_AGE: HeaderValue = HeaderValue::from_static("600");

/// Answers cross-origin requests for listed origins; every other request passes untouched.
pub async fn middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // The origin as it will go back into an answer; anything the list could not hold — absent,
    // not ASCII, too long — passes through untouched.
    let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .filter(|origin| origin.len() <= 2048 && origin.is_ascii())
        .and_then(|origin| HeaderValue::from_str(origin).ok())
    else {
        return next.run(request).await;
    };
    let listed = state.settings.get();
    let matches = listed
        .get(&API_ALLOWED_ORIGINS)
        .iter()
        .any(|one| Some(one.as_str()) == origin.to_str().ok());
    if !matches {
        return next.run(request).await;
    }
    // A preflight answers here; the request it asks about comes separately, with its own Origin.
    if request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
    {
        return preflight(request.headers(), origin);
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    response
}

/// The preflight answer: what a listed origin may do, never with credentials.
fn preflight(headers: &HeaderMap, origin: HeaderValue) -> Response {
    // What the browser asked to send goes back as allowed — the request still needs its token,
    // and the header names a browser sends carry nothing of ours.
    let asked = headers
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 1024 && value.is_ascii())
        .and_then(|value| HeaderValue::from_str(value).ok())
        .unwrap_or(HeaderValue::from_static("Authorization, Content-Type"));
    let mut response = Response::new(axum::body::Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    let out = response.headers_mut();
    out.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    out.insert(header::ACCESS_CONTROL_ALLOW_METHODS, METHODS);
    out.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, asked);
    out.insert(header::ACCESS_CONTROL_MAX_AGE, MAX_AGE);
    out.insert(header::VARY, HeaderValue::from_static("Origin"));
    response
}
