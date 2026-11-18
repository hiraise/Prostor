//! `/saved-searches`: queries under a name, shared like tags (QRY-063…QRY-068). Rules live in
//! `prostor_service::saved_searches`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use prostor_api::error::ErrorBody;
use prostor_api::page::{Page, PageQuery};
use prostor_api::saved_searches::{
    BuiltinSearchKey, NewSavedSearch, SavedSearch, SavedSearchPatch, SearchSharing,
};
use prostor_api::sharing::ScopeQuery;
use prostor_domain::access::Access;
use prostor_domain::auth::Scope;
use prostor_domain::settings::SYSTEM_LANGUAGE;
use prostor_service::saved_searches::{self as service, SearchChange, Sharing};
use prostor_service::tags;
use prostor_store::saved_searches::SavedSearchId;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::search::{problems, run_error};
use crate::sharing::{Shown, search_listing, shared_with};

const MAX_LIMIT: u32 = 200;

fn builtin(key: Option<&str>) -> Option<BuiltinSearchKey> {
    match key? {
        "assigned_to_me" => Some(BuiltinSearchKey::AssignedToMe),
        "commented_by_me" => Some(BuiltinSearchKey::CommentedByMe),
        "reported_by_me" => Some(BuiltinSearchKey::ReportedByMe),
        "all_issues" => Some(BuiltinSearchKey::AllIssues),
        _ => None,
    }
}

/// Saved searches with their owners and audiences: one load for the whole page.
async fn shown(
    state: &AppState,
    viewer: &Access,
    found: &[service::SavedSearch],
) -> Result<Vec<SavedSearch>, ApiError> {
    let shared: Vec<_> = found.iter().map(|s| s.row.sharing()).collect();
    let parties = tags::parties(&state.db, &shared).await?;
    let shown = Shown::new(state.names(), viewer, parties);
    found
        .iter()
        .zip(&shared)
        .map(|(s, sharing)| {
            let row = &s.row;
            Ok(SavedSearch {
                id: row.id,
                name: row.name.clone(),
                owner: shown.owner(sharing)?,
                query: s.text.clone(),
                problems: problems(&s.text, &s.problems),
                builtin: builtin(row.builtin.as_deref()),
                project: row.project_id,
                sharing: shown.sees_sharing(sharing).then(|| SearchSharing {
                    view: shown.audience(&sharing.view),
                    edit: shown.audience(&sharing.edit),
                }),
                abilities: shown.abilities(sharing),
                version: row.version,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
        })
        .collect()
}

async fn one(
    state: &AppState,
    viewer: &Access,
    found: service::SavedSearch,
) -> Result<SavedSearch, ApiError> {
    shown(state, viewer, std::slice::from_ref(&found))
        .await?
        .pop()
        .ok_or_else(|| prostor_service::Error::Internal("the search is missing".to_owned()).into())
}

/// Saved searches by name (QRY-063, QRY-065): the caller's own with the predefined ones (QRY-068),
/// others' shared with them, or both.
#[utoipa::path(
    get,
    path = "/saved-searches",
    operation_id = "list_saved_searches",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(ScopeQuery, PageQuery),
    responses(
        (status = OK, description = "A page of saved searches", body = Page<SavedSearch>),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<ScopeQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = query
        .resolve::<(String, SavedSearchId)>(MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let language = state.settings.get().get(&SYSTEM_LANGUAGE);
    let rows = service::list(
        &state.db,
        &viewer.access,
        language,
        search_listing(filter.scope),
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |s| (s.row.name.to_lowercase(), s.row.id));
    Ok(json(
        StatusCode::OK,
        Page {
            items: shown(&state, &viewer.access, &page.items).await?,
            next_cursor: page.next_cursor,
        },
    ))
}

/// Saves a query under a name (QRY-063). A query with errors answers `invalid_query`.
#[utoipa::path(
    post,
    path = "/saved-searches",
    operation_id = "create_saved_search",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    request_body = NewSavedSearch,
    responses(
        (status = CREATED, description = "Created", body = SavedSearch),
        (status = BAD_REQUEST, description = "Invalid or taken name, or the query has errors", body = ErrorBody),
        (status = CONFLICT, description = "Too many saved searches", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewSavedSearch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let created = service::create(&state.db, &viewer.access, &request.name, &request.query)
        .await
        .map_err(|e| run_error(&request.query, e))?;
    Ok(json(
        StatusCode::CREATED,
        one(&state, &viewer.access, created).await?,
    ))
}

/// One saved search the caller views, edits or may delete.
#[utoipa::path(
    get,
    path = "/saved-searches/{id}",
    operation_id = "get_saved_search",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Saved search ID")),
    responses(
        (status = OK, description = "The saved search", body = SavedSearch),
        (status = NOT_FOUND, description = "No such saved search", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<SavedSearchId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let found = service::get(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        one(&state, &viewer.access, found).await?,
    ))
}

/// Renames a saved search or changes its query (QRY-064), shares it or gives it to another owner
/// (QRY-065, ACC-066). Sharing without the share-views capability or the rights answers
/// `forbidden`.
#[utoipa::path(
    patch,
    path = "/saved-searches/{id}",
    operation_id = "update_saved_search",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Saved search ID")),
    request_body = SavedSearchPatch,
    responses(
        (status = OK, description = "The saved search now", body = SavedSearch),
        (status = BAD_REQUEST, description = "Invalid or taken name, or the query has errors", body = ErrorBody),
        (status = FORBIDDEN, description = "May not edit or share it", body = ErrorBody),
        (status = NOT_FOUND, description = "No such saved search", body = ErrorBody),
        (status = CONFLICT, description = "Changed by someone else", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<SavedSearchId>,
    JsonBody(patch): JsonBody<SavedSearchPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    let text = patch.query.clone().unwrap_or_default();
    let updated = service::update(
        &state.db,
        &viewer.access,
        id,
        SearchChange {
            name: patch.name,
            query: patch.query,
            sharing: patch.sharing.map(|s| Sharing {
                view: shared_with(s.view),
                edit: shared_with(s.edit),
            }),
            owner: patch.owner,
            version: patch.version,
        },
    )
    .await
    .map_err(|e| run_error(&text, e))?;
    Ok(json(
        StatusCode::OK,
        one(&state, &viewer.access, updated).await?,
    ))
}

/// Deletes a saved search: its owner or a user administrator (QRY-064). A predefined one stays
/// gone (QRY-068).
#[utoipa::path(
    delete,
    path = "/saved-searches/{id}",
    operation_id = "delete_saved_search",
    tag = "issues",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Saved search ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "Neither the owner nor a user administrator", body = ErrorBody),
        (status = NOT_FOUND, description = "No such saved search", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<SavedSearchId>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    service::delete(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
