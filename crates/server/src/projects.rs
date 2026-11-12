//! `/projects`: the list, creating from a template, settings, favorites, archive, deletion,
//! ownership and the people of a project (PRJ).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use trackflow_api::accounts::UserRef;
use trackflow_api::error::{ErrorBody, FieldError};
use trackflow_api::page::{Page, PageQuery};
use trackflow_api::projects::{
    Deletion, DeletionState, NewProject, OwnerChange, Project, ProjectCheck, ProjectCheckQuery,
    ProjectKind, ProjectPatch, ProjectPeople, ProjectQuery, ProjectSection, TeamChange,
    TemplateKey,
};
use trackflow_domain::access::Access;
use trackflow_domain::auth::Scope;
use trackflow_domain::projects::Template;
use trackflow_domain::settings::SYSTEM_LANGUAGE;
use trackflow_domain::{ProjectId, UserId};
use trackflow_service::people::Names;
use trackflow_service::projects::{self, NewProjectInput, ProjectChange, ProjectRow};
use trackflow_service::teams;
use trackflow_store::jobs::JobState;
use trackflow_store::projects::Section;

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::{group_ref, user_ref};

const PROJECTS_MAX_LIMIT: u32 = 100;
const MEMBERS_MAX_LIMIT: u32 = 100;

pub fn template_key(template: &str) -> TemplateKey {
    match Template::parse(template) {
        Some(Template::Scrum) => TemplateKey::Scrum,
        Some(Template::Kanban) => TemplateKey::Kanban,
        Some(Template::Blank) | None => TemplateKey::Blank,
    }
}

fn template(key: TemplateKey) -> Template {
    match key {
        TemplateKey::Blank => Template::Blank,
        TemplateKey::Scrum => Template::Scrum,
        TemplateKey::Kanban => Template::Kanban,
    }
}

/// A project as the viewer sees it.
pub fn project(names: &Names, viewer: &Access, row: &ProjectRow) -> Project {
    let p = &row.project;
    Project {
        id: p.id,
        key: p.key.clone(),
        name: p.name.clone(),
        description: p.description.clone(),
        description_html: p.description_html.clone(),
        kind: if p.kind == "service_desk" {
            ProjectKind::ServiceDesk
        } else {
            ProjectKind::Standard
        },
        template: template_key(&p.template),
        owner: row.owner.as_ref().map(|user| user_ref(names, viewer, user)),
        logo_color: p.logo_color,
        logo_url: crate::present::logo_url(p),
        next_number: p.next_number,
        team: row.team.as_ref().map(group_ref),
        team_size: row.team_size,
        former_keys: row.former_keys.clone(),
        default_audience: row.audience.as_ref().map(group_ref),
        favorite: row.favorite,
        archived_at: p.archived_at,
        deletion: p
            .deleting_at
            .and(row.deletion.as_ref())
            .map(|job| Deletion {
                state: match job.state {
                    JobState::Failed | JobState::Cancelled => DeletionState::Failed,
                    JobState::Running => DeletionState::Running,
                    JobState::Queued | JobState::Finished => DeletionState::Queued,
                },
                error: job.last_error.clone(),
            }),
        version: p.version,
        created_at: p.created_at,
    }
}

/// Projects the caller may see (PRJ-010, PRJ-012, PRJ-013): active ones with the caller's
/// favorites first, archived ones, or those being deleted that the caller may delete.
#[utoipa::path(
    get,
    path = "/projects",
    operation_id = "list_projects",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(ProjectQuery, PageQuery),
    responses(
        (status = OK, description = "A page of projects", body = Page<Project>),
        (status = BAD_REQUEST, description = "Invalid limit or cursor", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<ProjectQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let page = query
        .resolve::<(i32, String, ProjectId)>(PROJECTS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let section = match filter.section.unwrap_or_default() {
        ProjectSection::Active => Section::Active,
        ProjectSection::Archived => Section::Archived,
        ProjectSection::Deleting => Section::Deleting,
    };
    let rows = projects::list_filtered(
        &state.db,
        &viewer.access,
        section,
        filter.q.as_deref().unwrap_or_default(),
        filter.in_teams.unwrap_or(false),
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |row| {
        let starred = row.favorite && section == Section::Active;
        (
            i32::from(!starred),
            row.project.name.to_lowercase(),
            row.project.id,
        )
    });
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        Page {
            items: page
                .items
                .iter()
                .map(|row| project(&names, &viewer.access, row))
                .collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Checks a name and a key before creating or renaming (PRJ-018): problems of each, and a free key
/// for the name. Needs `project.create`, or `project.update` in some project.
#[utoipa::path(
    get,
    path = "/projects/check",
    operation_id = "check_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(ProjectCheckQuery),
    responses(
        (status = OK, description = "Problems and a suggested key", body = ProjectCheck),
        (status = FORBIDDEN, description = "Neither `project.create` nor `project.update`", body = ErrorBody),
    ),
)]
pub async fn check(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(query): QueryParams<ProjectCheckQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let check = projects::check(
        &state.db,
        &viewer.access,
        query.name.as_deref(),
        query.key.as_deref(),
    )
    .await?;
    let mut response = json(
        StatusCode::OK,
        ProjectCheck {
            suggested_key: check.suggested_key,
            fields: check
                .problems
                .into_iter()
                .map(|p| FieldError::new(p.path, p.problem.code(), p.message))
                .collect(),
        },
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

/// Creates a project from a template (PRJ-017, PRJ-019, PRJ-021): the caller owns and administers
/// it, joins its team, and the template's fields are attached. Needs `project.create`.
#[utoipa::path(
    post,
    path = "/projects",
    operation_id = "create_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    request_body = NewProject,
    responses(
        (status = CREATED, description = "Created", body = Project),
        (status = BAD_REQUEST, description = "Invalid or taken name or key", body = ErrorBody),
        (status = FORBIDDEN, description = "No `project.create`", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewProject>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let language = state.settings.get().get(&SYSTEM_LANGUAGE);
    let row = projects::create(
        &state.db,
        &viewer.access,
        NewProjectInput {
            template: template(request.template),
            name: request.name,
            key: request.key,
            first_number: request.first_number,
            description: request.description,
        },
        language,
    )
    .await?;
    state.access.clear();
    let access = state.access.get(&state.db, viewer.principal.user).await?;
    Ok(json(
        StatusCode::CREATED,
        project(&state.names(), &access, &row),
    ))
}

/// One project (PRJ-014).
#[utoipa::path(
    get,
    path = "/projects/{id}",
    operation_id = "get_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = OK, description = "The project", body = Project),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let row = projects::get(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        project(&state.names(), &viewer.access, &row),
    ))
}

/// The project with a key, current or former (PRJ-005): a former key answers with the project, so
/// the caller can move to its current key.
#[utoipa::path(
    get,
    path = "/projects/by-key/{key}",
    operation_id = "get_project_by_key",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("key" = String, Path, description = "Project key, in any letter case")),
    responses(
        (status = OK, description = "The project", body = Project),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn by_key(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let row = projects::by_key(&state.db, &viewer.access, &key).await?;
    Ok(json(
        StatusCode::OK,
        project(&state.names(), &viewer.access, &row),
    ))
}

/// Changes name, key, description and default audience (PRJ-038, PRJ-039). Needs
/// `project.update`; an archived project does not change.
#[utoipa::path(
    patch,
    path = "/projects/{id}",
    operation_id = "update_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = ProjectPatch,
    responses(
        (status = OK, description = "Changed", body = Project),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "Archived, or changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<ProjectPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = projects::update(
        &state.db,
        &viewer.access,
        id,
        ProjectChange {
            name: request.name,
            key: request.key,
            description: request.description,
            audience: request.default_audience_id,
            version: request.version,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        project(&state.names(), &viewer.access, &row),
    ))
}

/// Starts deleting a project in the background (PRJ-064, PRJ-065): nobody can use it from now on.
/// Asking again restarts a deletion that failed. Needs `project.delete`.
#[utoipa::path(
    delete,
    path = "/projects/{id}",
    operation_id = "delete_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = ACCEPTED, description = "Deletion started"),
        (status = FORBIDDEN, description = "No `project.delete`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    projects::delete(&state.db, &viewer.access, id).await?;
    state.access.clear();
    Ok(StatusCode::ACCEPTED.into_response())
}

/// Stars a project for the caller (PRJ-012).
#[utoipa::path(
    put,
    path = "/projects/{id}/favorite",
    operation_id = "star_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = NO_CONTENT, description = "Starred"),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn star(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    projects::set_favorite(&state.db, &viewer.access, id, true).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Unstars a project (PRJ-012).
#[utoipa::path(
    delete,
    path = "/projects/{id}/favorite",
    operation_id = "unstar_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = NO_CONTENT, description = "Unstarred"),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn unstar(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerWrite)?;
    projects::set_favorite(&state.db, &viewer.access, id, false).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Archives a project (PRJ-061): read-only, out of pickers and searches. Needs `project.update`.
#[utoipa::path(
    post,
    path = "/projects/{id}/archive",
    operation_id = "archive_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = OK, description = "Archived", body = Project),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "Archived already", body = ErrorBody),
    ),
)]
pub async fn archive(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = projects::set_archived(&state.db, &viewer.access, id, true).await?;
    Ok(json(
        StatusCode::OK,
        project(&state.names(), &viewer.access, &row),
    ))
}

/// Restores an archived project (PRJ-063). Needs `project.update`.
#[utoipa::path(
    post,
    path = "/projects/{id}/restore",
    operation_id = "restore_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = OK, description = "Restored", body = Project),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "Not archived", body = ErrorBody),
    ),
)]
pub async fn restore(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = projects::set_archived(&state.db, &viewer.access, id, false).await?;
    Ok(json(
        StatusCode::OK,
        project(&state.names(), &viewer.access, &row),
    ))
}

/// Gives the project a new owner, who becomes its administrator (PRJ-053). The previous owner
/// keeps their roles.
#[utoipa::path(
    post,
    path = "/projects/{id}/owner",
    operation_id = "transfer_project",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = OwnerChange,
    responses(
        (status = OK, description = "The project now", body = Project),
        (status = BAD_REQUEST, description = "No such account, or it is disabled", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions, named in the answer", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn transfer(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<OwnerChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = projects::transfer(&state.db, &viewer.access, id, request.user_id).await?;
    state.access.clear();
    Ok(json(
        StatusCode::OK,
        project(&state.names(), &viewer.access, &row),
    ))
}

/// The people of a project (PRJ-046): owner, team, team roles, direct roles and global roles that
/// reach it. Needs `project.full`.
#[utoipa::path(
    get,
    path = "/projects/{id}/people",
    operation_id = "get_project_people",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = OK, description = "The people", body = ProjectPeople),
        (status = FORBIDDEN, description = "No `project.full`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn people(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let people = teams::people(&state.db, &viewer.access, id).await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        ProjectPeople {
            owner: people
                .owner
                .as_ref()
                .map(|user| user_ref(&names, &viewer.access, user)),
            team: group_ref(&people.team),
            team_size: people.team_size,
            team_groups: people.team_groups.iter().map(group_ref).collect(),
            assignments: crate::roles::assignment_list(&names, &viewer.access, &people.assignments),
            global: crate::roles::assignment_list(&names, &viewer.access, &people.global),
        },
    ))
}

/// Accounts added to the team directly, by name (PRJ-046). Needs `project.full`.
#[utoipa::path(
    get,
    path = "/projects/{id}/team/users",
    operation_id = "list_team_users",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID"), trackflow_api::access::TextQuery, PageQuery),
    responses(
        (status = OK, description = "A page of accounts", body = Page<UserRef>),
        (status = FORBIDDEN, description = "No `project.full`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn team_users(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    QueryParams(filter): QueryParams<trackflow_api::access::TextQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<(String, UserId)>(MEMBERS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = teams::team_users(
        &state.db,
        &viewer.access,
        id,
        filter.q.as_deref().unwrap_or_default(),
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |user| (user.full_name.clone(), user.id));
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        Page {
            items: page
                .items
                .iter()
                .map(|user| user_ref(&names, &viewer.access, user))
                .collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Adds accounts and groups to the team (PRJ-048). Needs `project.update` and every permission the
/// team roles give (ACC-050).
#[utoipa::path(
    post,
    path = "/projects/{id}/team",
    operation_id = "add_to_team",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = TeamChange,
    responses(
        (status = OK, description = "Those added now", body = TeamChange),
        (status = BAD_REQUEST, description = "Nothing chosen, or a team chosen", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions, named in the answer", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project, account or group", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn add_to_team(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<TeamChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let changed = teams::add(
        &state.db,
        &viewer.access,
        id,
        &request.users,
        &request.groups,
    )
    .await?;
    state.access.clear();
    Ok(json(
        StatusCode::OK,
        TeamChange {
            users: changed.users,
            groups: changed.groups,
        },
    ))
}

/// Removes accounts added directly and groups from the team (PRJ-050).
#[utoipa::path(
    post,
    path = "/projects/{id}/team/remove",
    operation_id = "remove_from_team",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = TeamChange,
    responses(
        (status = OK, description = "Those removed now", body = TeamChange),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "The project is archived", body = ErrorBody),
    ),
)]
pub async fn remove_from_team(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<TeamChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let changed = teams::remove(
        &state.db,
        &viewer.access,
        id,
        &request.users,
        &request.groups,
    )
    .await?;
    state.access.clear();
    Ok(json(
        StatusCode::OK,
        TeamChange {
            users: changed.users,
            groups: changed.groups,
        },
    ))
}
