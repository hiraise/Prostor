//! Serving the built web app (a single-page app).
//!
//! Release builds embed `web/build` into the binary; debug builds read it from disk on every request,
//! so a rebuilt frontend shows up without recompiling the server.

use std::borrow::Cow;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};

use crate::AppState;

/// Source of web app files, keyed by path relative to the build root (`index.html`, `_app/...`).
pub trait Assets: Send + Sync {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>>;
}

#[derive(rust_embed::Embed)]
#[folder = "../../web/build"]
#[allow_missing = true]
struct Embedded;

/// The web app built into the binary (or read from `web/build` in debug builds).
pub struct EmbeddedAssets;

/// No web app: for tests and the benchmark, which only call the API.
pub struct NoAssets;

impl Assets for NoAssets {
    fn get(&self, _path: &str) -> Option<Cow<'static, [u8]>> {
        None
    }
}

impl Assets for EmbeddedAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        <Embedded as rust_embed::Embed>::get(path).map(|file| file.data)
    }
}

const SHELL: &str = "index.html";
/// Build output of the bundler; a missing file here is a stale or wrong URL, never an app route.
const BUNDLE_PREFIX: &str = "_app/";
/// Content-hashed files: safe to cache forever.
const IMMUTABLE_PREFIX: &str = "_app/immutable/";

/// Fallback handler: a file if it exists, otherwise the app shell, which routes on the client.
pub async fn serve(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty() && path != SHELL {
        if let Some(bytes) = state.web.get(path) {
            return file(path, bytes);
        }
        if path.starts_with(BUNDLE_PREFIX) {
            return StatusCode::NOT_FOUND.into_response();
        }
    }

    match state.web.get(SHELL) {
        Some(bytes) => shell(bytes),
        None => not_built(),
    }
}

fn file(path: &str, bytes: Cow<'static, [u8]>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with(IMMUTABLE_PREFIX) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = bytes.into_owned().into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// The app shell. Its Content Security Policy is a `<meta>` tag the build writes with the hashes
/// of the shell's inline scripts (`web/vite.config.ts`); what a tag cannot say comes here: no
/// framing by other sites, and no full URLs leaking to them (architecture.md §8).
fn shell(bytes: Cow<'static, [u8]>) -> Response {
    let mut response = Html(bytes.into_owned()).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'none'"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn not_built() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Html("<!doctype html><title>Prostor</title><p>The web app is not built. Run <code>make web</code>.</p>"),
    )
        .into_response()
}
