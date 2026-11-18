//! taskmaster server: the HTTP router and its parts. The binary in `main.rs` only wires them together,
//! so integration tests can build the same router with test doubles and a test database.

pub mod access;
pub mod accounts;
pub mod admin;
pub mod admin_mail;
pub mod auth;
pub mod boards;
pub mod bootstrap;
pub mod cli;
pub mod client;
pub mod commands;
pub mod comments;
pub mod config;
pub mod content;
pub mod cors;
pub mod error;
pub mod fields;
pub mod files;
pub mod groups;
pub mod health;
pub mod issue_links;
pub mod issues;
pub mod lifecycle;
pub mod links;
pub mod live;
pub mod notifications;
pub mod openapi;
pub mod present;
pub mod projects;
pub mod request_id;
pub mod roles;
pub mod rules;
pub mod saved_searches;
pub mod search;
pub mod session;
pub mod settings;
pub mod setup;
pub mod sharing;
pub mod startup;
pub mod throttle;
pub mod users;
pub mod web;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::DefaultBodyLimit;
use axum::http::{Extensions, HeaderMap, HeaderValue, Request, StatusCode, Version, header};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{any, get};
use taskmaster_files::FileStore;
use taskmaster_service::Passwords;
use taskmaster_service::files::Signer;
use taskmaster_service::people::Names;
use taskmaster_service::pictures::Gravatar;
use taskmaster_store::PgPool;
use tower_http::LatencyUnit;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::predicate::{DefaultPredicate, Predicate};
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::access::AccessCache;
use crate::client::IpRange;
use crate::lifecycle::Lifecycle;
use crate::request_id::RequestId;
use crate::settings::SettingsCache;
use crate::throttle::Throttle;

/// Server version, reported by health, bootstrap and the API document.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    /// Files of the built web app.
    pub web: Arc<dyn web::Assets>,
    pub db: PgPool,
    pub lifecycle: Arc<Lifecycle>,
    pub settings: SettingsCache,
    pub passwords: Passwords,
    pub throttle: Arc<Throttle>,
    /// Set once the first-run setup is known to be complete; it never becomes pending again.
    pub setup_done: Arc<AtomicBool>,
    /// Reverse proxies whose `X-Forwarded-For` is believed (`TASKMASTER_TRUSTED_PROXIES`).
    pub trusted_proxies: Arc<[IpRange]>,
    /// The setup code from `TASKMASTER_SETUP_CODE`, instead of a random one.
    pub setup_code: Option<Arc<str>>,
    /// Compiled access per person (ACC-062).
    pub access: AccessCache,
    /// How people appear to each other; keyed by the instance secret, loaded at startup.
    pub names: Arc<OnceLock<Names>>,
    /// Uploaded files (D-033).
    pub files: FileStore,
    /// Signs file links; keyed by the instance secret, loaded at startup.
    pub signer: Arc<OnceLock<Signer>>,
    /// The largest upload whatever the settings say (`TASKMASTER_UPLOAD_CEILING_BYTES`).
    pub upload_ceiling: u64,
    /// The Gravatar import; `None` when turned off (ACC-017).
    pub gravatar: Option<Gravatar>,
    /// The live gateway (ISS-048); started with the node.
    pub live: live::Gateway,
}

impl AppState {
    /// A node that has not started yet: [`startup::start`] brings it up.
    pub fn new(web: Arc<dyn web::Assets>, db: PgPool) -> Self {
        Self {
            web,
            db,
            lifecycle: Arc::new(Lifecycle::default()),
            settings: SettingsCache::default(),
            passwords: Passwords::per_core(),
            throttle: Arc::new(Throttle::default()),
            setup_done: Arc::new(AtomicBool::new(false)),
            trusted_proxies: Arc::from(Vec::new()),
            setup_code: None,
            access: AccessCache::default(),
            names: Arc::new(OnceLock::new()),
            // Nodes that keep files are given their directory with `with_files`.
            files: FileStore::new(std::env::temp_dir().join("taskmaster-files")),
            signer: Arc::new(OnceLock::new()),
            upload_ceiling: config::DEFAULT_UPLOAD_CEILING,
            gravatar: None,
            live: live::Gateway::default(),
        }
    }

    pub fn with_live(mut self, live: live::Gateway) -> Self {
        self.live = live;
        self
    }

    /// The fallback mail server of the environment (D-026) and the key that seals secret
    /// settings (NTF-004); both feed the settings cache, which owns the outgoing mail state.
    pub fn with_secret_mail(
        mut self,
        env_mail: Option<crate::settings::EnvMail>,
        secret_key: Option<taskmaster_domain::secret::Key>,
    ) -> Self {
        self.settings = self
            .settings
            .clone()
            .with_env_mail(env_mail)
            .with_secret_key(secret_key);
        self
    }

    /// Where files go, the largest upload and the Gravatar import.
    pub fn with_files(
        mut self,
        files: FileStore,
        upload_ceiling: u64,
        gravatar: Option<Gravatar>,
    ) -> Self {
        self.files = files;
        self.upload_ceiling = upload_ceiling;
        self.gravatar = gravatar;
        self
    }

    /// Signs file links. The startup loads the key before any request that needs it is served.
    pub fn signer(&self) -> Signer {
        self.signer
            .get()
            .cloned()
            .unwrap_or_else(|| Signer::with_secret(b"not-ready"))
    }

    /// The largest upload now: the setting, within the deployment's ceiling (CNT-019, ADM-004).
    pub fn upload_limit(&self) -> u64 {
        self.settings
            .get()
            .get(&taskmaster_domain::settings::UPLOAD_MAX_BYTES)
            .min(self.upload_ceiling)
    }

    /// How people appear to each other. The startup loads the key before any request that needs
    /// it is served.
    pub fn names(&self) -> Names {
        self.names
            .get()
            .cloned()
            .unwrap_or_else(|| Names::with_secret(b"not-ready"))
    }

    pub fn with_setup_code(mut self, code: Option<String>) -> Self {
        self.setup_code = code.map(Arc::from);
        self
    }

    pub fn with_trusted_proxies(mut self, proxies: Vec<IpRange>) -> Self {
        self.trusted_proxies = Arc::from(proxies);
        self
    }

    pub fn with_throttle(mut self, throttle: Throttle) -> Self {
        self.throttle = Arc::new(throttle);
        self
    }
}

/// Endpoints that answer at any time, even while the node starts.
fn open_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(health::live))
        .routes(routes!(health::ready))
}

/// Endpoints that need a ready node (database reachable, migrations applied).
fn ready_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(bootstrap::get))
        .routes(routes!(setup::run))
        .routes(routes!(session::sign_in, session::get, session::sign_out))
        .routes(routes!(users::me))
        .routes(routes!(users::change_password))
        .routes(routes!(users::list_tokens, users::create_token))
        .routes(routes!(users::delete_token))
        .routes(routes!(users::preferences, users::update_preferences))
        .routes(routes!(roles::my_access))
        .routes(routes!(accounts::test_email))
        .routes(routes!(admin_mail::settings, admin_mail::save))
        .routes(routes!(admin_mail::test))
        .routes(routes!(admin::settings, admin::save))
        .routes(routes!(admin::jobs))
        .routes(routes!(admin::cancel))
        .routes(routes!(admin::retry))
        .routes(routes!(accounts::list, accounts::create))
        .routes(routes!(accounts::invite))
        .routes(routes!(accounts::get, accounts::update))
        .routes(routes!(accounts::disable))
        .routes(routes!(accounts::enable))
        .routes(routes!(accounts::deletion_impact))
        .routes(routes!(accounts::delete))
        .routes(routes!(accounts::require_password_change))
        .routes(routes!(accounts::resend_invitation))
        .routes(routes!(accounts::send_verification))
        .routes(routes!(accounts::roles))
        .routes(routes!(groups::list, groups::create))
        .routes(routes!(groups::get, groups::update))
        .routes(routes!(groups::members, groups::add_members))
        .routes(routes!(groups::remove_members))
        .routes(routes!(groups::deletion_impact))
        .routes(routes!(groups::delete))
        .routes(routes!(roles::catalog))
        .routes(routes!(roles::list, roles::create))
        .routes(routes!(roles::get, roles::update, roles::delete))
        .routes(routes!(roles::grant_permission, roles::revoke_permission))
        .routes(routes!(roles::role_assignments))
        .routes(routes!(roles::list_assignments, roles::assign))
        .routes(routes!(roles::revoke))
        .routes(routes!(roles::capabilities))
        .routes(routes!(roles::set_capability))
        .routes(routes!(projects::list, projects::create))
        .routes(routes!(projects::check))
        .routes(routes!(projects::by_key))
        .routes(routes!(projects::get, projects::update, projects::delete))
        .routes(routes!(rules::list, rules::update))
        .routes(routes!(projects::star, projects::unstar))
        .routes(routes!(projects::archive))
        .routes(routes!(projects::restore))
        .routes(routes!(projects::transfer))
        .routes(routes!(projects::people))
        .routes(routes!(projects::team_users))
        .routes(routes!(projects::add_to_team))
        .routes(routes!(projects::remove_from_team))
        .routes(routes!(fields::project_list, fields::project_attach))
        .routes(routes!(fields::project_update, fields::project_detach))
        .routes(routes!(fields::project_detach_impact))
        .routes(routes!(fields::project_move))
        .routes(routes!(fields::project_fork))
        .routes(routes!(fields::list, fields::create))
        .routes(routes!(fields::get, fields::update, fields::delete))
        .routes(routes!(fields::move_field))
        .routes(routes!(fields::projects))
        .routes(routes!(fields::deletion_impact))
        .routes(routes!(fields::list_sets, fields::create_set))
        .routes(routes!(
            fields::get_set,
            fields::rename_set,
            fields::delete_set
        ))
        .routes(routes!(fields::add_value))
        .routes(routes!(fields::update_value, fields::delete_value))
        .routes(routes!(fields::value_usage))
        .routes(routes!(fields::move_value))
        .routes(routes!(fields::add_members))
        .routes(routes!(fields::remove_members))
        .routes(routes!(search::list, issues::create))
        .routes(routes!(search::count))
        .routes(routes!(search::fields))
        .routes(routes!(search::counts))
        .routes(routes!(search::check))
        .routes(routes!(search::assist))
        .routes(routes!(boards::list, boards::create))
        .routes(routes!(boards::get, boards::update, boards::delete))
        .routes(routes!(boards::impact))
        .routes(routes!(boards::update_profile))
        .routes(routes!(boards::add_column))
        .routes(routes!(boards::update_column, boards::delete_column))
        .routes(routes!(boards::view))
        .routes(routes!(boards::cell))
        .routes(routes!(boards::cards_batch))
        .routes(routes!(boards::add_card))
        .routes(routes!(boards::move_card))
        .routes(routes!(boards::remove_card))
        .routes(routes!(boards::add_lane))
        .routes(routes!(boards::move_lane))
        .routes(routes!(boards::remove_lane))
        .routes(routes!(boards::cell_draft))
        .routes(routes!(boards::field_boards))
        .routes(routes!(boards::list_sprints, boards::create_sprint))
        .routes(routes!(boards::update_sprint, boards::delete_sprint))
        .routes(routes!(boards::start_sprint))
        .routes(routes!(boards::finish_sprint))
        .routes(routes!(boards::archive_sprint))
        .routes(routes!(boards::restore_sprint))
        .routes(routes!(boards::carry_sprint))
        .routes(routes!(boards::backlog))
        .routes(routes!(
            boards::order_backlog,
            boards::discard_backlog_order
        ))
        .routes(routes!(boards::list_issue_boards, boards::add_issue_board))
        .routes(routes!(boards::remove_issue_board))
        .routes(routes!(saved_searches::list, saved_searches::create))
        .routes(routes!(
            saved_searches::get,
            saved_searches::update,
            saved_searches::delete
        ))
        .routes(routes!(
            notifications::settings,
            notifications::set_settings
        ))
        .routes(routes!(notifications::inbox))
        .routes(routes!(notifications::summary, notifications::read))
        .routes(routes!(
            notifications::subscriptions,
            notifications::subscribe
        ))
        .routes(routes!(notifications::set_kinds, notifications::remove))
        .routes(routes!(
            notifications::watchers,
            notifications::set_watchers
        ))
        .routes(routes!(issues::drafts, issues::delete_drafts))
        .routes(routes!(issues::batch))
        .routes(routes!(issues::export))
        .routes(routes!(issues::clone_drafts))
        .routes(routes!(issues::field_options))
        .routes(routes!(issues::get, issues::update, issues::delete))
        .routes(routes!(issues::publish))
        .routes(routes!(issues::clone))
        .routes(routes!(issues::set_field))
        .routes(routes!(issues::add_values))
        .routes(routes!(issues::remove_values))
        .routes(routes!(issues::move_preview))
        .routes(routes!(issues::move_issue))
        .routes(routes!(issues::add_star, issues::remove_star))
        .routes(routes!(issues::add_vote, issues::remove_vote))
        .routes(routes!(issues::add_tag, issues::remove_tag))
        .routes(routes!(issues::activity))
        .routes(routes!(issues::page))
        .routes(routes!(issues::mention_candidates))
        .routes(routes!(commands::apply))
        .routes(routes!(commands::assist))
        .routes(routes!(commands::apply_bulk))
        .routes(routes!(commands::bulk_progress))
        .routes(routes!(comments::create))
        .routes(routes!(comments::get, comments::update, comments::delete))
        .routes(routes!(comments::restore))
        .routes(routes!(content::preview))
        .routes(routes!(files::discard_upload))
        .routes(routes!(files::upload_file))
        .routes(routes!(files::list, files::attach))
        .routes(routes!(files::get, files::update, files::delete))
        .routes(routes!(files::serve))
        .routes(routes!(files::set_avatar, files::reset_avatar))
        .routes(routes!(files::import_gravatar))
        .routes(routes!(files::avatar))
        .routes(routes!(files::set_logo, files::reset_logo))
        .routes(routes!(files::logo))
        // The one raw body (D-016): streamed into the file store with our own limit.
        .merge(
            OpenApiRouter::new()
                .routes(routes!(files::upload))
                .layer(DefaultBodyLimit::disable()),
        )
        .routes(routes!(issue_links::list_types, issue_links::create_type))
        .routes(routes!(issue_links::update_type, issue_links::delete_type))
        .routes(routes!(issue_links::type_impact))
        .routes(routes!(issue_links::restore_type))
        .routes(routes!(issue_links::list, issue_links::add))
        .routes(routes!(issue_links::group_page))
        .routes(routes!(issue_links::remove))
        .routes(routes!(issues::candidates))
        .routes(routes!(issues::list_tags, issues::create_tag))
        .routes(routes!(
            issues::get_tag,
            issues::update_tag,
            issues::delete_tag
        ))
        .routes(routes!(issues::tag_usage))
        .routes(routes!(links::request_reset))
        .routes(routes!(links::confirm_reset))
        .routes(routes!(links::inspect))
        .routes(routes!(links::accept_invitation))
        .routes(routes!(links::confirm_email))
}

/// The OpenAPI document of every `/api/v1` endpoint.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut document = OpenApiRouter::<AppState>::with_openapi(openapi::ApiDoc::openapi())
        .merge(open_routes())
        .merge(ready_routes())
        .into_openapi();
    // Filled from Cargo metadata, where no license is set yet.
    document.info.license = None;
    document
}

/// Builds the full router: versioned API under `/api/v1`, everything else is the web app.
pub fn router(state: AppState) -> Router {
    let (open, _) = open_routes().split_for_parts();
    let (ready, _) = ready_routes().split_for_parts();
    // The live connection is a WebSocket, outside the API document (its messages are in it).
    let ready =
        ready
            .route("/live", get(live::upgrade))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                lifecycle::require_ready,
            ));
    let document = Bytes::from(serde_json::to_vec(&openapi()).unwrap_or_default());

    let api_v1 = Router::new()
        .merge(open)
        .merge(ready)
        .route(
            "/openapi.json",
            get(move || async move {
                (
                    [(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    )],
                    document.clone(),
                )
                    .into_response()
            }),
        )
        .method_not_allowed_fallback(error::method_not_allowed)
        .fallback(error::api_not_found)
        // Cross-origin answers for the origins `api.allowed_origins` lists (INT-056); every
        // other request, from any origin the list does not name, passes untouched.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            cors::middleware,
        ));

    Router::new()
        .nest("/api/v1", api_v1)
        // Unknown API versions and the bare prefix answer as API, never with the web shell.
        .route("/api", any(error::api_not_found))
        .route("/api/", any(error::api_not_found))
        .route("/api/{*rest}", any(error::api_not_found))
        .fallback(web::serve)
        .with_state(state)
        // Layers run outside in: request ID, trace, panic guard, compression, router.
        // Brotli's default level (11) is far too slow for dynamic responses; 4 compresses better
        // than gzip at a similar speed. The same level applies to gzip and zstd.
        // Files sent with ranges stay as they are: a compressed body cannot answer a range.
        .layer(
            CompressionLayer::new()
                .quality(CompressionLevel::Precise(4))
                .compress_when(DefaultPredicate::new().and(
                    |_: StatusCode, _: Version, headers: &HeaderMap, _: &Extensions| {
                        !headers.contains_key(header::ACCEPT_RANGES)
                    },
                )),
        )
        .layer(CatchPanicLayer::custom(error::panic_response))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Micros),
                ),
        )
        .layer(middleware::from_fn(request_id::middleware))
}

/// Log span of a request. The query string is left out: it may carry tokens.
fn request_span(request: &Request<Body>) -> tracing::Span {
    let id = request
        .extensions()
        .get::<RequestId>()
        .map_or("-", |id| &*id.0);
    tracing::info_span!(
        "request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id = %id,
    )
}
