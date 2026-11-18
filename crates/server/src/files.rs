//! Files (D-033): `/uploads` (the one endpoint with a raw body, D-016), the attachments of issues,
//! signed file links under `/files`, avatars and project logos. Rules and flows live in
//! `taskmaster_service::{uploads, attachments, files, pictures}`.
//!
//! Files are sent with `ServeFile` (ranges for video, conditional requests) and headers that keep
//! a browser from running them: the kind comes from the bytes, everything but pictures, three
//! video formats and PDF downloads, `nosniff`, and a sandboxing Content-Security-Policy
//! (architecture.md §8).

use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use taskmaster_api::error::{ErrorBody, ErrorCode};
use taskmaster_api::files::{
    Attachment, AttachmentPage, AttachmentPatch, AttachmentQuery, FileLinkQuery, NewAttachments,
    Picture, PictureInput, Upload, UploadQuery,
};
use taskmaster_domain::access::Access;
use taskmaster_domain::auth::Scope;
use taskmaster_domain::files::Kind;
use taskmaster_domain::{AttachmentId, IssueId, ProjectId, UploadId, UserId};
use taskmaster_service::attachments::{self, AttachmentChange, Listed};
use taskmaster_service::files::{self, LinkQuery};
use taskmaster_service::{pictures, uploads};
use time::OffsetDateTime;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::{require_any_scope, require_scope};
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::issues::{audience_input, origin};
use crate::present::{self, FileLinks, avatar_url, logo_url};

const WRITE: &[Scope] = &[Scope::TrackerWrite, Scope::AdminWrite];
const READ: &[Scope] = &[Scope::TrackerRead, Scope::AdminRead];

fn not_found() -> ApiError {
    ApiError::not_found("No such file")
}

/// `Content-Disposition` with an ASCII fallback and the real name (RFC 6266).
fn disposition(inline: bool, name: &str) -> HeaderValue {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let value = format!(
        "{}; filename=\"{fallback}\"; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        present::encode_segment(name)
    );
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// Sends a stored file with the headers that keep it inert.
async fn send(
    path: PathBuf,
    kind: Kind,
    name: &str,
    cache: HeaderValue,
    request: Request,
) -> Result<Response, ApiError> {
    let mime = kind
        .content_type()
        .parse()
        .unwrap_or(mime_guess::mime::APPLICATION_OCTET_STREAM);
    let (parts, _) = request.into_parts();
    let request = Request::from_parts(parts, Body::empty());
    let response = ServeFile::new_with_mime(path, &mime)
        .oneshot(request)
        .await
        .map_err(ApiError::internal)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(not_found());
    }
    let mut response = response.map(Body::new);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_DISPOSITION,
        disposition(kind.inline(), name),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, cache);
    // Chromium's PDF viewer refuses to run inside a sandbox.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(if kind == Kind::Pdf {
            "default-src 'none'; img-src 'self'; media-src 'self'; style-src 'unsafe-inline'"
        } else {
            "default-src 'none'; img-src 'self'; media-src 'self'; style-src 'unsafe-inline'; sandbox"
        }),
    );
    Ok(response)
}

// Uploads.

/// Sends a file (CNT-017, CNT-019): the request body is the file's bytes, of any type. The file
/// waits a day to be attached, or made an avatar or logo, and is removed after that.
#[utoipa::path(
    post,
    path = "/uploads",
    operation_id = "upload_file",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(UploadQuery),
    request_body(content = Vec<u8>, content_type = "application/octet-stream", description = "The file's bytes"),
    responses(
        (status = CREATED, description = "Stored", body = Upload),
        (status = BAD_REQUEST, description = "A bad name, or the upload broke off", body = ErrorBody),
        (status = FORBIDDEN, description = "The guest account cannot upload", body = ErrorBody),
        (status = CONFLICT, description = "Too many files are waiting to be attached", body = ErrorBody),
        (status = PAYLOAD_TOO_LARGE, description = "Larger than the limit in the bootstrap", body = ErrorBody),
    ),
)]
pub async fn upload(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<UploadQuery>,
    request: Request,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, WRITE)?;
    let limit = state.upload_limit();
    let declared = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|size| size > limit) {
        return Err(taskmaster_service::Error::TooLarge(limit).into());
    }
    let body = request.into_body().into_data_stream();
    let stored = uploads::receive(
        &state.db,
        &state.files,
        &viewer.access,
        &query.name,
        body,
        limit,
    )
    .await?;
    Ok(json(StatusCode::CREATED, present::upload(&stored)))
}

/// Throws away one's own staged upload.
#[utoipa::path(
    delete,
    path = "/uploads/{id}",
    operation_id = "discard_upload",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Upload ID")),
    responses(
        (status = NO_CONTENT, description = "Removed"),
        (status = NOT_FOUND, description = "No such upload of the caller", body = ErrorBody),
    ),
)]
pub async fn discard_upload(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<UploadId>,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, WRITE)?;
    uploads::discard(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// One's own staged upload, for previews before it is attached (CNT-034).
#[utoipa::path(
    get,
    path = "/uploads/{id}/{name}",
    operation_id = "get_upload_file",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Upload ID"),
        ("name" = String, Path, description = "The file's name; not checked"),
    ),
    responses(
        (status = OK, description = "The file", content_type = "application/octet-stream", body = Vec<u8>),
        (status = NOT_FOUND, description = "No such upload of the caller", body = ErrorBody),
    ),
)]
pub async fn upload_file(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, _name)): Path<(UploadId, String)>,
    request: Request,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, READ)?;
    let found = uploads::get(&state.db, &viewer.access, id).await?;
    let path = state.files.path(&found.storage_key).ok_or_else(not_found)?;
    send(
        path,
        Kind::of_content_type(&found.content_type),
        &found.name,
        HeaderValue::from_static("private, max-age=86400"),
        request,
    )
    .await
}

// Attachments.

fn listed(state: &AppState, viewer: &Access, listed: &Listed) -> Vec<Attachment> {
    let names = state.names();
    let signer = state.signer();
    let links = FileLinks::new(&signer, viewer, &listed.link_secret);
    listed
        .files
        .iter()
        .map(|f| {
            present::attachment(&names, viewer, &links, f, &|id| listed.user(id), &|id| {
                listed.group(id)
            })
        })
        .collect()
}

/// The files of an issue the caller sees, its comments' included, oldest first (CNT-017,
/// CNT-018).
#[utoipa::path(
    get,
    path = "/issues/{id}/attachments",
    operation_id = "list_attachments",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID"), AttachmentQuery),
    responses(
        (status = OK, description = "A page of files", body = AttachmentPage),
        (status = BAD_REQUEST, description = "Invalid limit", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(issue): Path<IssueId>,
    QueryParams(query): QueryParams<AttachmentQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let limit = match query.limit {
        None => attachments::PAGE_MAX,
        Some(limit) if (1..=100).contains(&limit) => i64::from(limit),
        Some(_) => {
            return Err(ApiError::invalid(vec![
                taskmaster_api::error::FieldError::new(
                    "limit",
                    "out_of_range",
                    "limit must be between 1 and 100",
                ),
            ]));
        }
    };
    let found = attachments::list(&state.db, &viewer.access, issue, query.cursor, limit).await?;
    Ok(json(StatusCode::OK, page(&state, &viewer.access, &found)))
}

/// A page of files as it goes out.
pub(crate) fn page(state: &AppState, viewer: &Access, found: &Listed) -> AttachmentPage {
    AttachmentPage {
        attachments: listed(state, viewer, found),
        next: found.next,
    }
}

/// Attaches the caller's staged uploads to an issue or draft (CNT-017, ISS-007). Names already on
/// the issue get ` (2)`, ` (3)`… before the extension.
#[utoipa::path(
    post,
    path = "/issues/{id}/attachments",
    operation_id = "attach_files",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = NewAttachments,
    responses(
        (status = CREATED, description = "The new files", body = AttachmentPage),
        (status = BAD_REQUEST, description = "No such upload", body = ErrorBody),
        (status = FORBIDDEN, description = "No right to change the issue or `attachment.create`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "Too many files, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn attach(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(issue): Path<IssueId>,
    JsonBody(request): JsonBody<NewAttachments>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let added = attachments::attach(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        issue,
        &request.uploads,
        request.silent,
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        AttachmentPage {
            attachments: listed(&state, &viewer.access, &added),
            next: None,
        },
    ))
}

/// One file the caller sees.
#[utoipa::path(
    get,
    path = "/attachments/{id}",
    operation_id = "get_attachment",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Attachment ID")),
    responses(
        (status = OK, description = "The file", body = Attachment),
        (status = NOT_FOUND, description = "No such file", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<AttachmentId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = attachments::get(&state.db, &viewer.access, id).await?;
    let one = listed(&state, &viewer.access, &found)
        .pop()
        .ok_or_else(not_found)?;
    Ok(json(StatusCode::OK, one))
}

/// Renames a file or changes whom it is limited to (CNT-020, CNT-023).
#[utoipa::path(
    patch,
    path = "/attachments/{id}",
    operation_id = "update_attachment",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Attachment ID")),
    request_body = AttachmentPatch,
    responses(
        (status = OK, description = "Changed", body = Attachment),
        (status = BAD_REQUEST, description = "A bad or taken name, an unknown audience", body = ErrorBody),
        (status = FORBIDDEN, description = "No `attachment.create` (own) or `attachment.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such file", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<AttachmentId>,
    JsonBody(request): JsonBody<AttachmentPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let changed = attachments::update(
        &state.db,
        &viewer.access,
        id,
        AttachmentChange {
            name: request.name,
            audience: request.audience.map(audience_input),
            version: request.version,
        },
    )
    .await?;
    let one = listed(&state, &viewer.access, &changed)
        .pop()
        .ok_or_else(not_found)?;
    Ok(json(StatusCode::OK, one))
}

/// Deletes a file (CNT-024): it disappears and its links stop working at once.
#[utoipa::path(
    delete,
    path = "/attachments/{id}",
    operation_id = "delete_attachment",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Attachment ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "Another person's file without `attachment.delete`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such file", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<AttachmentId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    attachments::delete(&state.db, &viewer.access, origin(&viewer.principal), id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// A file by a signed link (CNT-026). No session: the link names the person it was made for, and
/// their access now decides. Any problem answers 404.
#[utoipa::path(
    get,
    path = "/files/{id}/{name}",
    operation_id = "get_file",
    tag = "files",
    params(
        ("id" = i64, Path, description = "Attachment ID"),
        ("name" = String, Path, description = "The file's name; not checked"),
        FileLinkQuery,
    ),
    responses(
        (status = OK, description = "The file", content_type = "application/octet-stream", body = Vec<u8>),
        (status = PARTIAL_CONTENT, description = "A range of the file"),
        (status = NOT_FOUND, description = "No such file, or the link does not work", body = ErrorBody),
    ),
)]
pub async fn serve(
    State(state): State<AppState>,
    Path((id, _name)): Path<(AttachmentId, String)>,
    QueryParams(query): QueryParams<FileLinkQuery>,
    request: Request,
) -> Result<Response, ApiError> {
    let now = OffsetDateTime::now_utc();
    let link = LinkQuery {
        user: query.u,
        expires: query.e,
        signature: query.s,
        thumbnail: query.thumb == Some(1),
    };
    let (file, user) = files::check_link(&state.db, &state.signer(), id, &link, now)
        .await
        .map_err(|_| not_found())?;
    let access = state
        .access
        .get(&state.db, user)
        .await
        .map_err(|_| not_found())?;
    let expires = OffsetDateTime::from_unix_timestamp(link.expires).map_err(|_| not_found())?;
    let served = files::open(&state.db, &access, file, link.thumbnail, expires)
        .await
        .map_err(|_| not_found())?;
    let path = state.files.path(&served.key).ok_or_else(not_found)?;
    let seconds = (served.expires - now).whole_seconds().max(0);
    let cache = HeaderValue::from_str(&format!("private, max-age={seconds}"))
        .unwrap_or_else(|_| HeaderValue::from_static("private, no-cache"));
    send(path, served.kind, &served.name, cache, request).await
}

// Avatars (ACC-017).

fn may_change(viewer: &Viewer, user: UserId) -> Result<(), ApiError> {
    if user == viewer.principal.user {
        require_any_scope(&viewer.principal, WRITE)
    } else {
        require_scope(&viewer.principal, Scope::AdminWrite)
    }
}

fn picture(url: Option<String>) -> Response {
    json(StatusCode::OK, Picture { url })
}

/// Makes a staged picture someone's avatar: its middle square (ACC-017). One's own with
/// `user.self`, others' with `user.update`.
#[utoipa::path(
    put,
    path = "/users/{id}/avatar",
    operation_id = "set_avatar",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    request_body = PictureInput,
    responses(
        (status = OK, description = "The avatar now", body = Picture),
        (status = BAD_REQUEST, description = "No such upload, or not a picture we read", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
    ),
)]
pub async fn set_avatar(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(user): Path<UserId>,
    JsonBody(request): JsonBody<PictureInput>,
) -> Result<Response, ApiError> {
    may_change(&viewer, user)?;
    let key = pictures::set_avatar(
        &state.db,
        &state.files,
        &viewer.access,
        user,
        request.upload,
    )
    .await?;
    Ok(picture(key.map(|k| avatar_url(user, &k))))
}

/// Goes back to the generated avatar.
#[utoipa::path(
    delete,
    path = "/users/{id}/avatar",
    operation_id = "reset_avatar",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = NO_CONTENT, description = "Removed"),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account", body = ErrorBody),
    ),
)]
pub async fn reset_avatar(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(user): Path<UserId>,
) -> Result<Response, ApiError> {
    may_change(&viewer, user)?;
    pictures::reset_avatar(&state.db, &state.files, &viewer.access, user).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Takes the person's Gravatar picture as their avatar, once (ACC-017). The server downloads it;
/// browsers never contact Gravatar.
#[utoipa::path(
    post,
    path = "/users/{id}/avatar/gravatar",
    operation_id = "import_gravatar",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = OK, description = "The avatar now", body = Picture),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such account, the import is off, or `gravatar_not_found`", body = ErrorBody),
        (status = BAD_GATEWAY, description = "`gravatar_unavailable`", body = ErrorBody),
    ),
)]
pub async fn import_gravatar(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(user): Path<UserId>,
) -> Result<Response, ApiError> {
    may_change(&viewer, user)?;
    let gravatar = state
        .gravatar
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "The Gravatar import is off"))?;
    let key =
        pictures::import_gravatar(&state.db, &state.files, gravatar, &viewer.access, user).await?;
    Ok(picture(key.map(|k| avatar_url(user, &k))))
}

/// An uploaded avatar, for people who see its owner by name. Its URL changes with the picture,
/// so it is cached for good.
#[utoipa::path(
    get,
    path = "/users/{id}/avatar/{file}",
    operation_id = "get_avatar",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Account ID"),
        ("file" = String, Path, description = "As in `avatar_url`"),
    ),
    responses(
        (status = OK, description = "The picture", content_type = "image/jpeg", body = Vec<u8>),
        (status = NOT_FOUND, description = "No such picture", body = ErrorBody),
    ),
)]
pub async fn avatar(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((user, file)): Path<(UserId, String)>,
    request: Request,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, READ)?;
    let key = pictures::avatar_file(&state.db, &state.names(), &viewer.access, user, &file).await?;
    send_picture(&state, &key, request).await
}

async fn send_picture(state: &AppState, key: &str, request: Request) -> Result<Response, ApiError> {
    let path = state.files.path(key).ok_or_else(not_found)?;
    let kind = if key.ends_with(".png") {
        Kind::Png
    } else {
        Kind::Jpeg
    };
    send(
        path,
        kind,
        pictures::file_of(key),
        HeaderValue::from_static("private, max-age=31536000, immutable"),
        request,
    )
    .await
}

// Project logos (PRJ-007).

/// Makes a staged picture the project's logo: the whole picture inside a square (PRJ-007).
#[utoipa::path(
    put,
    path = "/projects/{id}/logo",
    operation_id = "set_project_logo",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = PictureInput,
    responses(
        (status = OK, description = "The logo now", body = Picture),
        (status = BAD_REQUEST, description = "No such upload, or not a picture we read", body = ErrorBody),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn set_logo(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(project): Path<ProjectId>,
    JsonBody(request): JsonBody<PictureInput>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    pictures::set_logo(
        &state.db,
        &state.files,
        &viewer.access,
        project,
        request.upload,
    )
    .await?;
    let found = taskmaster_store::projects::get(&state.db, project)
        .await?
        .ok_or_else(not_found)?;
    Ok(picture(logo_url(&found)))
}

/// Goes back to the generated logo.
#[utoipa::path(
    delete,
    path = "/projects/{id}/logo",
    operation_id = "reset_project_logo",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = NO_CONTENT, description = "Removed"),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn reset_logo(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(project): Path<ProjectId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    pictures::reset_logo(&state.db, &state.files, &viewer.access, project).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// An uploaded logo, for people who see the project.
#[utoipa::path(
    get,
    path = "/projects/{id}/logo/{file}",
    operation_id = "get_project_logo",
    tag = "files",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Project ID"),
        ("file" = String, Path, description = "As in `logo_url`"),
    ),
    responses(
        (status = OK, description = "The picture", content_type = "image/png", body = Vec<u8>),
        (status = NOT_FOUND, description = "No such picture", body = ErrorBody),
    ),
)]
pub async fn logo(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((project, file)): Path<(ProjectId, String)>,
    request: Request,
) -> Result<Response, ApiError> {
    require_any_scope(&viewer.principal, READ)?;
    let key = pictures::logo_file(&state.db, &viewer.access, project, &file).await?;
    send_picture(&state, &key, request).await
}
