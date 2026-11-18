//! A taskmaster node inside the benchmark's process, called as a client calls the server: whole
//! requests through the router, from authentication to the compressed JSON answer (D-035).
//!
//! Requests skip TCP and HTTP parsing, which the server's own work dwarfs. The client asks for gzip
//! as browsers do, since compressing is the server's time, and decodes the answer: decompression
//! and parsing on this side cost well under a millisecond for the pages measured.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use taskmaster_server::startup::{self, Services};
use taskmaster_server::web::NoAssets;
use taskmaster_server::{AppState, router};
use taskmaster_store::PgConnectOptions;
use taskmaster_store::jobs::{Handlers, RunnerConfig};
use tokio_util::sync::CancellationToken;
use tower::{Layer as _, ServiceExt as _};
use tower_http::decompression::{Decompression, DecompressionLayer};

pub const HOST: &str = "bench.taskmaster.test";
/// Requests one client sends at once: the issue page's four.
pub const REQUESTS_AT_ONCE: usize = 4;

/// A started node over the benchmark database.
pub struct Node {
    service: Decompression<axum::Router>,
    state: AppState,
    services: Option<Services>,
    stop: CancellationToken,
    /// The node's file store; the measured requests read no files.
    _files: tempfile::TempDir,
}

impl Node {
    /// Starts a node with a pool for `clients` concurrent clients, each sending up to
    /// [`REQUESTS_AT_ONCE`] requests at a time, as the server starts: the
    /// schema is current, the settings and caches load, the job runner starts without handlers,
    /// so nothing queued runs.
    pub async fn start(options: &PgConnectOptions, clients: usize) -> anyhow::Result<Self> {
        let pool = taskmaster_store::pool(options, u32::try_from(clients * REQUESTS_AT_ONCE)? + 2);
        let files = tempfile::tempdir()?;
        let state = AppState::new(Arc::new(NoAssets), pool).with_files(
            taskmaster_files::FileStore::new(files.path()),
            1 << 20,
            None,
        );
        let stop = CancellationToken::new();
        let runner = RunnerConfig {
            workers: 1,
            poll_interval: Duration::from_secs(60),
            shutdown_grace: Duration::from_secs(1),
            ..RunnerConfig::default()
        };
        // A real node's kinds: the ones the scenarios enqueue (the bulk job of step 2.1).
        let mut handlers = Handlers::new();
        let jobs_pool = state.db.clone();
        handlers.add::<taskmaster_service::commands::bulk::BulkWork, _, _>(move |context, job| {
            let pool = jobs_pool.clone();
            async move { taskmaster_service::commands::bulk::run(&pool, &context, job).await }
        });
        let services = startup::start(&state, options, handlers, runner, &stop)
            .await
            .context("the node did not start")?
            .context("the node stopped while starting")?;
        Ok(Self {
            service: DecompressionLayer::new().layer(router(state.clone())),
            state,
            services: Some(services),
            stop,
            _files: files,
        })
    }

    /// Sends one request as `token`'s owner and answers the raw file: the status, the headers
    /// (the export's counts live there) and the bytes, undecoded beyond the transport's gzip.
    pub async fn download(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: &Value,
    ) -> anyhow::Result<(StatusCode, http::HeaderMap, Vec<u8>)> {
        let request = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header(header::HOST, HOST)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::ACCEPT_ENCODING, "gzip")
            .header(header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(body)?))?;
        let response = self.service.clone().oneshot(request).await?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|error| anyhow::anyhow!("{method} {path}: {error}"))?
            .to_bytes();
        Ok((status, headers, bytes.to_vec()))
    }

    /// Sends one request as `token`'s owner and answers the decoded JSON; an answer other than
    /// 2xx is an error that names the request.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let mut request = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header(header::HOST, HOST)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::ACCEPT_ENCODING, "gzip");
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                axum::body::Body::from(serde_json::to_vec(json)?)
            }
            None => axum::body::Body::empty(),
        };
        let response = self.service.clone().oneshot(request.body(body)?).await?;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|error| anyhow::anyhow!("{method} {path}: {error}"))?
            .to_bytes();
        if !status.is_success() {
            bail!(
                "{method} {path} answered {status}: {}",
                String::from_utf8_lossy(&bytes)
            );
        }
        if status == StatusCode::NO_CONTENT || bytes.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Serves the node on a free local port, as the binary does, until the node stops: for clients
    /// that need a real connection (live sockets).
    pub async fn serve(&self) -> anyhow::Result<SocketAddr> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let app = router(self.state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        let stop = self.stop.clone();
        tokio::spawn(async move {
            let served = axum::serve(listener, app)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await;
            if let Err(error) = served {
                tracing::warn!(%error, "the benchmark's server stopped");
            }
        });
        Ok(address)
    }

    /// Stops the node's background services.
    pub async fn stop(mut self) {
        self.stop.cancel();
        if let Some(services) = self.services.take() {
            services.stop().await;
        }
    }
}
