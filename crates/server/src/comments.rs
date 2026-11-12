//! `/issues/{id}/comments` and `/comments`: posting, reading, editing, deleting and restoring
//! comments (CNT-001…CNT-005). Rules and flows live in `trackflow_service::comments`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use trackflow_api::comments::{
    Comment, CommentAbilities, CommentAnswer, CommentDeletion, CommentPatch, NewComment,
};
use trackflow_api::error::ErrorBody;
use trackflow_api::issues::Audience;
use trackflow_domain::access::Access;
use trackflow_domain::auth::Scope;
use trackflow_domain::{CommentId, GroupId, IssueId, UserId};
use trackflow_service::attachments::AttachmentView;
use trackflow_service::comments::{self, CommentChange, CommentView};
use trackflow_service::files::Signer;
use trackflow_service::people::Names;
use trackflow_service::text::Refs;
use trackflow_store::groups::Group;
use trackflow_store::users::User;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, json};
use crate::issues::{audience_input, origin, placeholder};
use crate::present::{FileLinks, attachment, group_ref, rule_messages, text_refs, user_ref};

/// A comment as the viewer sees it; a deleted one is a placeholder (CNT-005).
#[allow(clippy::too_many_arguments)]
pub fn comment_from<'a, 'f>(
    names: &Names,
    signer: &Signer,
    viewer: &Access,
    view: &CommentView,
    user: &dyn Fn(UserId) -> Option<&'a User>,
    group: &dyn Fn(GroupId) -> Option<&'a Group>,
    refs: &Refs,
    files: impl Iterator<Item = &'f AttachmentView>,
) -> Comment {
    let c = &view.comment;
    let person = |id| user(id).map(|u| user_ref(names, viewer, u));
    let deleted = c.deleted_at.map(|at| CommentDeletion {
        at,
        by: c.deleted_by.and_then(person),
    });
    let shown = deleted.is_none();
    Comment {
        id: c.id,
        issue: c.issue_id,
        author: person(c.author_id).unwrap_or_else(|| placeholder(c.author_id)),
        text: if shown { c.text.clone() } else { String::new() },
        html: if shown { c.html.clone() } else { String::new() },
        refs: if shown {
            text_refs(
                names,
                viewer,
                signer,
                refs,
                &c.mentioned_users,
                &c.issue_tokens,
                &c.file_names,
            )
        } else {
            trackflow_api::content::TextRefs::default()
        },
        created_at: c.created_at,
        updated_at: c.updated_at,
        updater: c.updated_by.filter(|_| shown).and_then(person),
        audience: if shown {
            Audience {
                users: c
                    .audience_users
                    .iter()
                    .filter_map(|id| person(*id))
                    .collect(),
                groups: c
                    .audience_groups
                    .iter()
                    .filter_map(|id| group(*id))
                    .map(group_ref)
                    .collect(),
            }
        } else {
            Audience {
                users: Vec::new(),
                groups: Vec::new(),
            }
        },
        files: if shown {
            let links = FileLinks::new(signer, viewer, &refs.link_secret);
            files
                .map(|f| attachment(names, viewer, &links, f, user, group))
                .collect()
        } else {
            Vec::new()
        },
        deleted,
        can: CommentAbilities {
            update: view.can.update,
            delete: view.can.delete,
            restore: view.can.restore,
        },
        version: c.version,
    }
}

fn one(state: &AppState, viewer: &Access, (view, d): &comments::One) -> Comment {
    comment_from(
        &state.names(),
        &state.signer(),
        viewer,
        view,
        &|id| d.user(id),
        &|id| d.group(id),
        &d.refs,
        d.files_of(view.comment.id),
    )
}

/// Posts a comment on an issue (CNT-001, CNT-003).
#[utoipa::path(
    post,
    path = "/issues/{id}/comments",
    operation_id = "create_comment",
    tag = "comments",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Issue ID")),
    request_body = NewComment,
    responses(
        (status = CREATED, description = "Posted", body = CommentAnswer),
        (status = BAD_REQUEST, description = "Empty or too long text, unknown audience", body = ErrorBody),
        (status = FORBIDDEN, description = "No `comment.create`, or `silent` without `issue.silent`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such issue", body = ErrorBody),
        (status = CONFLICT, description = "A draft, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(issue): Path<IssueId>,
    JsonBody(request): JsonBody<NewComment>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let created = comments::create(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        issue,
        comments::NewComment {
            text: request.text,
            audience: request.audience.map(audience_input),
            silent: request.silent,
            uploads: request.uploads,
        },
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        CommentAnswer {
            comment: one(&state, &viewer.access, &created.done),
            messages: rule_messages(&created.messages),
        },
    ))
}

/// One comment; a placeholder when deleted (CNT-005).
#[utoipa::path(
    get,
    path = "/comments/{id}",
    operation_id = "get_comment",
    tag = "comments",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Comment ID")),
    responses(
        (status = OK, description = "The comment", body = Comment),
        (status = NOT_FOUND, description = "No such comment, or the caller may not see it", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<CommentId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = comments::get(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, one(&state, &viewer.access, &found)))
}

/// Changes a comment's text or audience (CNT-004).
#[utoipa::path(
    patch,
    path = "/comments/{id}",
    operation_id = "update_comment",
    tag = "comments",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Comment ID")),
    request_body = CommentPatch,
    responses(
        (status = OK, description = "Changed", body = Comment),
        (status = BAD_REQUEST, description = "Empty or too long text, unknown audience", body = ErrorBody),
        (status = FORBIDDEN, description = "No `comment.update.own` or `comment.update.any`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such comment", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else, deleted, or the project is archived", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<CommentId>,
    JsonBody(request): JsonBody<CommentPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let changed = comments::update(
        &state.db,
        &viewer.access,
        origin(&viewer.principal),
        id,
        CommentChange {
            text: request.text,
            audience: request.audience.map(audience_input),
            version: request.version,
            silent: request.silent,
            add_uploads: request.add_uploads,
        },
    )
    .await?;
    Ok(json(StatusCode::OK, one(&state, &viewer.access, &changed)))
}

/// Deletes a comment; a placeholder stays and it can be restored (CNT-005).
#[utoipa::path(
    delete,
    path = "/comments/{id}",
    operation_id = "delete_comment",
    tag = "comments",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Comment ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "No `comment.delete.own` or `comment.delete.any`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such comment", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<CommentId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    comments::delete(&state.db, &viewer.access, origin(&viewer.principal), id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Brings a deleted comment back with its text and audience (CNT-005).
#[utoipa::path(
    post,
    path = "/comments/{id}/restore",
    operation_id = "restore_comment",
    tag = "comments",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Comment ID")),
    responses(
        (status = OK, description = "Restored", body = Comment),
        (status = FORBIDDEN, description = "No right to delete it", body = ErrorBody),
        (status = NOT_FOUND, description = "No such comment", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn restore(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<CommentId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let restored =
        comments::restore(&state.db, &viewer.access, origin(&viewer.principal), id).await?;
    Ok(json(StatusCode::OK, one(&state, &viewer.access, &restored)))
}
