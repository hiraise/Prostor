//! A test node over a fresh database and helpers to call it as a browser page would.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use prostor_server::startup::{self, Services};
use prostor_server::throttle::Throttle;
use prostor_server::web::NoAssets;
use prostor_server::{AppState, router};
use prostor_store::jobs::{Handlers, RunnerConfig};
use prostor_testkit::TestDb;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

pub mod live;

pub const CODE: &str = "HTTP-TEST-2345";

/// The base64 of 32 zero bytes: a key secret settings can be sealed with in tests.
pub const SECRET_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// The test instance's secret-settings key.
pub fn secret_key() -> Option<prostor_domain::secret::Key> {
    prostor_domain::secret::Key::from_base64(SECRET_KEY).ok()
}
pub const HOST: &str = "tracker.example.com";

pub struct Node {
    pub state: AppState,
    services: Option<Services>,
    _db: TestDb,
    /// The node's file store directory.
    pub files: tempfile::TempDir,
}

/// Uploads larger than this are refused by the test node's ceiling.
pub const UPLOAD_CEILING: u64 = 64 * 1024;

impl Node {
    pub async fn start(throttle: Throttle) -> Self {
        Self::start_with(throttle, None).await
    }

    /// A node that can send email; nothing is delivered (no job handlers run), the emails stay
    /// in the queue for tests to read. The transport comes from the environment, like a
    /// deployment before anyone opens the mail settings screen (D-026).
    pub async fn start_mailing() -> Self {
        Self::start_with(
            Throttle::default(),
            Some(prostor_service::mail::EnvMail {
                url: "smtp://127.0.0.1:1".to_owned(),
                from: "Prostor <t@example.com>".to_owned(),
            }),
        )
        .await
    }

    pub async fn start_with(
        throttle: Throttle,
        env_mail: Option<prostor_service::mail::EnvMail>,
    ) -> Self {
        Self::start_full(throttle, env_mail, prostor_server::live::Gateway::default()).await
    }

    /// A node whose live gateway has other limits.
    pub async fn start_live(config: prostor_server::live::Config) -> Self {
        Self::start_full(
            Throttle::default(),
            None,
            prostor_server::live::Gateway::with_config(config),
        )
        .await
    }

    async fn start_full(
        throttle: Throttle,
        env_mail: Option<prostor_service::mail::EnvMail>,
        live: prostor_server::live::Gateway,
    ) -> Self {
        let db = TestDb::new().await;
        let files = tempfile::tempdir().unwrap();
        let state = AppState::new(Arc::new(NoAssets), db.pool().clone())
            .with_setup_code(Some(CODE.to_owned()))
            .with_throttle(throttle)
            .with_secret_mail(env_mail, secret_key())
            .with_live(live)
            .with_files(
                prostor_files::FileStore::new(files.path()),
                UPLOAD_CEILING,
                None,
            );
        let runner = RunnerConfig {
            workers: 1,
            poll_interval: Duration::from_millis(100),
            shutdown_grace: Duration::from_secs(1),
            ..RunnerConfig::default()
        };
        let services = startup::start(
            &state,
            db.options(),
            Handlers::new(),
            runner,
            &CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
        Self {
            state,
            services: Some(services),
            _db: db,
            files,
        }
    }

    /// Sends raw bytes with extra headers and answers the raw response, for files.
    pub async fn raw(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        headers: &[(header::HeaderName, &str)],
        body: Vec<u8>,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST);
        if !headers.iter().any(|(name, _)| name == header::ORIGIN) {
            request = request.header(header::ORIGIN, format!("https://{HOST}"));
        }
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, format!("prostor_session={cookie}"));
        }
        for (name, value) in headers {
            request = request.header(name, *value);
        }
        let response = router(self.state.clone())
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, bytes.to_vec())
    }

    /// The secret in the newest queued email's link.
    pub async fn last_link(&self) -> String {
        let text: String = sqlx::query_scalar!(
            r#"select payload->>'text' as "text!" from jobs where kind = 'mail.send'
               order by id desc limit 1"#
        )
        .fetch_one(&self.state.db)
        .await
        .unwrap();
        text.split_whitespace()
            .find_map(|word| word.split_once('#').map(|(_, secret)| secret.to_owned()))
            .unwrap()
    }

    pub async fn stop(mut self) {
        if let Some(services) = self.services.take() {
            services.stop().await;
        }
    }

    /// Sends a request as a browser page of this server would; `credential` is a cookie value
    /// (`session=…`) or a bearer token (`tfp_…`).
    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        credential: Option<&str>,
        body: Option<Value>,
    ) -> Answer {
        self.send(
            method,
            uri,
            credential,
            body,
            Some(&format!("https://{HOST}")),
        )
        .await
    }

    pub async fn send(
        &self,
        method: Method,
        uri: &str,
        credential: Option<&str>,
        body: Option<Value>,
        origin: Option<&str>,
    ) -> Answer {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST);
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        match credential {
            Some(token)
                if token.starts_with("tfp_")
                    || token.starts_with("Basic")
                    || token.starts_with("bearer") =>
            {
                let value = if token.starts_with("tfp_") {
                    format!("Bearer {token}")
                } else {
                    token.to_owned()
                };
                request = request.header(header::AUTHORIZATION, value);
            }
            Some(cookie) => {
                request = request.header(header::COOKIE, format!("prostor_session={cookie}"));
            }
            None => {}
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = router(self.state.clone()).oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        Answer {
            status,
            headers,
            body,
        }
    }

    pub async fn set_up(&self) -> String {
        let answer = self
            .call(Method::POST, "/api/v1/setup", None, Some(setup_body(CODE)))
            .await;
        assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
        answer.session_cookie().expect("setup signs in")
    }

    pub async fn sign_in(&self, password: &str, remember: bool) -> Answer {
        self.call(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "admin", "password": password, "remember": remember })),
        )
        .await
    }
}

pub struct Answer {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Value,
}

impl Answer {
    pub fn set_cookie(&self) -> &str {
        self.headers
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    }

    pub fn session_cookie(&self) -> Option<String> {
        self.set_cookie()
            .split(';')
            .next()?
            .strip_prefix("prostor_session=")
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    pub fn code(&self) -> &str {
        self.body["code"].as_str().unwrap_or("")
    }
}

pub fn setup_body(code: &str) -> Value {
    json!({
        "code": code,
        "username": "admin",
        "full_name": "Anna Admin",
        "email": "anna@example.com",
        "password": "correct horse",
        "system_language": "en",
        "base_url": format!("https://{HOST}"),
        "time_zone": "Europe/Berlin",
    })
}
