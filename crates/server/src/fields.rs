//! `/fields`, `/value-sets` and `/projects/{id}/fields`: custom fields of the instance, value sets
//! with their values or members, and the fields of a project (FLD).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use prostor_api::error::ErrorBody;
use prostor_api::fields::{
    AttachField, BuiltinField, DeleteValueQuery, Field, FieldDeletionImpact, FieldKind, FieldPatch,
    FieldQuery, FieldRef, IssueImpact, MoveAfter, NewField, NewFieldSet, NewValue, NewValueSet,
    ProjectField, ProjectFieldPatch, ProjectFields, SetKind, SetMembersChange, SetUse, SetValue,
    UsedIn, ValuePatch, ValueRef, ValueSet, ValueSetPatch, ValueSetQuery, ValueSetRef,
    ValueSetSummary,
};
use prostor_api::page::{Page, PageQuery};
use prostor_domain::access::Access;
use prostor_domain::auth::Scope;
use prostor_domain::fields::{self as domain, builtin::BuiltinField as DomainBuiltin};
use prostor_domain::{FieldId, SetId};
use prostor_service::fields::{
    self as fields_service, FieldRow, FieldUpdate, NewFieldInput, SetSource,
};
use prostor_service::people::Names;
use prostor_service::project_fields::{self, AttachInput, ProjectFieldRow, SettingsChange};
use prostor_service::value_sets::{self, SetRow, ValueInput};
use prostor_store::value_sets::{Value as StoredValue, ValueSet as StoredSet};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, QueryParams, json};
use crate::present::{group_ref, project_ref, user_ref};

const FIELDS_MAX_LIMIT: u32 = 200;
const SETS_MAX_LIMIT: u32 = 100;

pub fn kind_key(kind: &str) -> FieldKind {
    match domain::FieldKind::parse(kind) {
        Some(domain::FieldKind::String) | None => FieldKind::String,
        Some(domain::FieldKind::Text) => FieldKind::Text,
        Some(domain::FieldKind::Integer) => FieldKind::Integer,
        Some(domain::FieldKind::Float) => FieldKind::Float,
        Some(domain::FieldKind::Date) => FieldKind::Date,
        Some(domain::FieldKind::DateTime) => FieldKind::DateTime,
        Some(domain::FieldKind::Duration) => FieldKind::Duration,
        Some(domain::FieldKind::Enum) => FieldKind::Enum,
        Some(domain::FieldKind::State) => FieldKind::State,
        Some(domain::FieldKind::User) => FieldKind::User,
        Some(domain::FieldKind::Group) => FieldKind::Group,
        Some(domain::FieldKind::Version) => FieldKind::Version,
        Some(domain::FieldKind::Build) => FieldKind::Build,
        Some(domain::FieldKind::Owned) => FieldKind::Owned,
    }
}

fn domain_kind(kind: FieldKind) -> domain::FieldKind {
    match kind {
        FieldKind::String => domain::FieldKind::String,
        FieldKind::Text => domain::FieldKind::Text,
        FieldKind::Integer => domain::FieldKind::Integer,
        FieldKind::Float => domain::FieldKind::Float,
        FieldKind::Date => domain::FieldKind::Date,
        FieldKind::DateTime => domain::FieldKind::DateTime,
        FieldKind::Duration => domain::FieldKind::Duration,
        FieldKind::Enum => domain::FieldKind::Enum,
        FieldKind::State => domain::FieldKind::State,
        FieldKind::User => domain::FieldKind::User,
        FieldKind::Group => domain::FieldKind::Group,
        FieldKind::Version => domain::FieldKind::Version,
        FieldKind::Build => domain::FieldKind::Build,
        FieldKind::Owned => domain::FieldKind::Owned,
    }
}

fn set_kind_key(kind: &str) -> SetKind {
    match domain::SetKind::parse(kind) {
        Some(domain::SetKind::Enum) | None => SetKind::Enum,
        Some(domain::SetKind::State) => SetKind::State,
        Some(domain::SetKind::User) => SetKind::User,
        Some(domain::SetKind::Version) => SetKind::Version,
        Some(domain::SetKind::Build) => SetKind::Build,
        Some(domain::SetKind::Owned) => SetKind::Owned,
    }
}

fn domain_set_kind(kind: SetKind) -> domain::SetKind {
    match kind {
        SetKind::Enum => domain::SetKind::Enum,
        SetKind::State => domain::SetKind::State,
        SetKind::User => domain::SetKind::User,
        SetKind::Version => domain::SetKind::Version,
        SetKind::Build => domain::SetKind::Build,
        SetKind::Owned => domain::SetKind::Owned,
    }
}

fn builtin_key(key: Option<&str>) -> Option<BuiltinField> {
    Some(match DomainBuiltin::parse(key?)? {
        DomainBuiltin::Priority => BuiltinField::Priority,
        DomainBuiltin::Type => BuiltinField::Type,
        DomainBuiltin::State => BuiltinField::State,
        DomainBuiltin::Assignee => BuiltinField::Assignee,
        DomainBuiltin::Component => BuiltinField::Component,
        DomainBuiltin::FixVersions => BuiltinField::FixVersions,
        DomainBuiltin::AffectedVersions => BuiltinField::AffectedVersions,
        DomainBuiltin::FixedInBuild => BuiltinField::FixedInBuild,
        DomainBuiltin::DueDate => BuiltinField::DueDate,
        DomainBuiltin::Estimation => BuiltinField::Estimation,
        DomainBuiltin::SpentTime => BuiltinField::SpentTime,
    })
}

fn set_ref(set: &StoredSet) -> ValueSetRef {
    ValueSetRef {
        id: set.id,
        name: set.name.clone(),
        kind: set_kind_key(&set.kind),
    }
}

fn value_ref(value: &StoredValue) -> ValueRef {
    ValueRef {
        id: value.id,
        name: value.name.clone(),
        color: value.color,
        archived: value.archived,
    }
}

pub fn field_ref(field: &prostor_store::fields::Field) -> FieldRef {
    FieldRef {
        id: field.id,
        name: field.name.clone(),
        kind: kind_key(&field.kind),
        multi: field.multi,
        builtin: builtin_key(field.builtin.as_deref()),
        can_be_empty: field.can_be_empty,
        empty_label: field.empty_label.clone(),
    }
}

fn field(names: &Names, viewer: &Access, row: &FieldRow) -> Field {
    let f = &row.field;
    Field {
        id: f.id,
        name: f.name.clone(),
        kind: kind_key(&f.kind),
        multi: f.multi,
        aliases: f.aliases.clone(),
        builtin: builtin_key(f.builtin.as_deref()),
        auto_attach: f.auto_attach,
        set: row.set.as_ref().map(set_ref),
        can_be_empty: f.can_be_empty,
        empty_label: f.empty_label.clone(),
        private: f.private,
        default_values: row.default_values.iter().map(value_ref).collect(),
        default_users: row
            .default_users
            .iter()
            .map(|user| user_ref(names, viewer, user))
            .collect(),
        project_count: row.project_count,
        version: f.version,
        created_at: f.created_at,
    }
}

fn value_set(names: &Names, viewer: &Access, row: &SetRow) -> ValueSet {
    ValueSet {
        id: row.set.id,
        name: row.set.name.clone(),
        kind: set_kind_key(&row.set.kind),
        values: row
            .values
            .iter()
            .map(|v| SetValue {
                id: v.id,
                name: v.name.clone(),
                description: v.description.clone(),
                color: v.color,
                archived: v.archived,
                resolved: v.resolved,
                released: v.released,
                release_date: v.release_date,
                start_date: v.start_date,
                assembled_on: v.assembled_on,
                owner: v
                    .owner_id
                    .and_then(|id| row.owners.iter().find(|u| u.id == id))
                    .map(|user| user_ref(names, viewer, user)),
                version: v.version,
            })
            .collect(),
        users: row
            .users
            .iter()
            .map(|user| user_ref(names, viewer, user))
            .collect(),
        groups: row.groups.iter().map(group_ref).collect(),
        used_in: UsedIn {
            items: row.projects.iter().map(project_ref).collect(),
            hidden: row.hidden_projects,
        },
        field_count: i64::try_from(row.fields.len()).unwrap_or(i64::MAX),
        editable: row.editable,
        version: row.set.version,
        created_at: row.set.created_at,
    }
}

fn project_field(names: &Names, viewer: &Access, row: &ProjectFieldRow) -> ProjectField {
    ProjectField {
        field: field_ref(&row.field),
        aliases: row.field.aliases.clone(),
        set: row.set.as_ref().map(set_ref),
        shared_with: row.shared_with,
        field_default: row.field_default,
        can_be_empty: row.attached.can_be_empty,
        empty_label: row.attached.empty_label.clone(),
        default_values: row.default_values.iter().map(value_ref).collect(),
        default_users: row
            .default_users
            .iter()
            .map(|user| user_ref(names, viewer, user))
            .collect(),
        private: row.attached.private,
        required: row.required,
        version: row.attached.version,
    }
}

/// Fields of the instance in its order (FLD-022, FLD-025). Needs `system.read` or
/// `project.update` in a project.
#[utoipa::path(
    get,
    path = "/fields",
    operation_id = "list_fields",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(FieldQuery, PageQuery),
    responses(
        (status = OK, description = "A page of fields", body = Page<Field>),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<FieldQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<FieldId>(FIELDS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = fields_service::list(
        &state.db,
        &viewer.access,
        filter.q.as_deref().unwrap_or_default(),
        filter.unused,
        page.after,
        usize::try_from(page.fetch_limit()).unwrap_or(usize::MAX),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |row| row.field.id);
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        Page {
            items: page
                .items
                .iter()
                .map(|row| field(&names, &viewer.access, row))
                .collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Creates a field of the instance (FLD-015). Needs `system.write`.
#[utoipa::path(
    post,
    path = "/fields",
    operation_id = "create_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    request_body = NewField,
    responses(
        (status = CREATED, description = "Created", body = Field),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewField>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = fields_service::create(
        &state.db,
        &viewer.access,
        NewFieldInput {
            name: request.name,
            kind: domain_kind(request.kind),
            multi: request.multi,
            aliases: request.aliases,
            set: match request.set {
                Some(NewFieldSet::Existing { set_id }) => SetSource::Existing(set_id),
                Some(NewFieldSet::New { name }) => SetSource::New { name },
                None => SetSource::New { name: None },
            },
            can_be_empty: request.can_be_empty,
            empty_label: request.empty_label,
            private: request.private,
            default_values: request.default_values,
            auto_attach: request.auto_attach,
        },
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        field(&state.names(), &viewer.access, &row),
    ))
}

/// One field.
#[utoipa::path(
    get,
    path = "/fields/{id}",
    operation_id = "get_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    responses(
        (status = OK, description = "The field", body = Field),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let row = fields_service::get(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        field(&state.names(), &viewer.access, &row),
    ))
}

/// Changes a field (FLD-016…FLD-020). Name and aliases: `system.write`, or `project.update` in
/// every project using a field that is not attached automatically; the rest: `system.write`.
#[utoipa::path(
    patch,
    path = "/fields/{id}",
    operation_id = "update_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    request_body = FieldPatch,
    responses(
        (status = OK, description = "Changed", body = Field),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
        (status = CONFLICT, description = "Changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<FieldPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = fields_service::update(
        &state.db,
        &viewer.access,
        id,
        FieldUpdate {
            name: request.name,
            aliases: request.aliases,
            auto_attach: request.auto_attach,
            can_be_empty: request.can_be_empty,
            empty_label: request.empty_label,
            private: request.private,
            default_values: request.default_values,
            version: request.version,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        field(&state.names(), &viewer.access, &row),
    ))
}

/// Moves a field in the instance order (FLD-022). Needs `system.write`.
#[utoipa::path(
    post,
    path = "/fields/{id}/move",
    operation_id = "move_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    request_body = MoveAfter,
    responses(
        (status = OK, description = "Moved", body = Field),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
    ),
)]
pub async fn move_field(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<MoveAfter>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = fields_service::move_after(&state.db, &viewer.access, id, request.after).await?;
    Ok(json(
        StatusCode::OK,
        field(&state.names(), &viewer.access, &row),
    ))
}

/// The projects using a field (FLD-025): those the caller may see, and how many others.
#[utoipa::path(
    get,
    path = "/fields/{id}/projects",
    operation_id = "list_field_projects",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    responses(
        (status = OK, description = "Projects", body = UsedIn),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
    ),
)]
pub async fn projects(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let (found, hidden) = fields_service::projects_of(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        UsedIn {
            items: found.iter().map(project_ref).collect(),
            hidden,
        },
    ))
}

/// What deleting a field takes away (FLD-023). Needs `system.write`.
#[utoipa::path(
    get,
    path = "/fields/{id}/deletion-impact",
    operation_id = "get_field_deletion_impact",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    responses(
        (status = OK, description = "What would be removed", body = FieldDeletionImpact),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
    ),
)]
pub async fn deletion_impact(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let (projects, issues, boards) =
        fields_service::deletion_impact(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        FieldDeletionImpact {
            projects,
            issues,
            boards,
        },
    ))
}

/// Deletes a field from every project and the instance (FLD-023). Needs `system.write`.
#[utoipa::path(
    delete,
    path = "/fields/{id}",
    operation_id = "delete_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Field ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such field", body = ErrorBody),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    fields_service::delete(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Value sets by name (FLD-025, FLD-029).
#[utoipa::path(
    get,
    path = "/value-sets",
    operation_id = "list_value_sets",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(ValueSetQuery, PageQuery),
    responses(
        (status = OK, description = "A page of sets", body = Page<ValueSetSummary>),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
    ),
)]
pub async fn list_sets(
    State(state): State<AppState>,
    viewer: Viewer,
    QueryParams(filter): QueryParams<ValueSetQuery>,
    QueryParams(query): QueryParams<PageQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let page = query
        .resolve::<(String, SetId)>(SETS_MAX_LIMIT)
        .map_err(ApiError::invalid)?;
    let rows = value_sets::list(
        &state.db,
        &viewer.access,
        filter.kind.map(domain_set_kind),
        filter.q.as_deref().unwrap_or_default(),
        page.after.clone(),
        page.fetch_limit(),
    )
    .await?;
    let page = Page::from_rows(rows, page.limit, |row| {
        (row.set.name.to_lowercase(), row.set.id)
    });
    Ok(json(
        StatusCode::OK,
        Page {
            items: page
                .items
                .iter()
                .map(|row| ValueSetSummary {
                    id: row.set.id,
                    name: row.set.name.clone(),
                    kind: set_kind_key(&row.set.kind),
                    projects: row.projects,
                    fields: row.fields,
                    version: row.set.version,
                })
                .collect::<Vec<_>>(),
            next_cursor: page.next_cursor,
        },
    ))
}

/// Creates an empty set (FLD-029). Needs `system.write`.
#[utoipa::path(
    post,
    path = "/value-sets",
    operation_id = "create_value_set",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    request_body = NewValueSet,
    responses(
        (status = CREATED, description = "Created", body = ValueSet),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
    ),
)]
pub async fn create_set(
    State(state): State<AppState>,
    viewer: Viewer,
    JsonBody(request): JsonBody<NewValueSet>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::create(
        &state.db,
        &viewer.access,
        &request.name,
        domain_set_kind(request.kind),
    )
    .await?;
    Ok(json(
        StatusCode::CREATED,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// One set with its values or members and the projects using it (FLD-031).
#[utoipa::path(
    get,
    path = "/value-sets/{id}",
    operation_id = "get_value_set",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Set ID")),
    responses(
        (status = OK, description = "The set", body = ValueSet),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set", body = ErrorBody),
    ),
)]
pub async fn get_set(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let row = value_sets::get(&state.db, &viewer.access, id).await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// Renames a set. Needs `system.write`, or `project.update` in every project using it.
#[utoipa::path(
    patch,
    path = "/value-sets/{id}",
    operation_id = "rename_value_set",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Set ID")),
    request_body = ValueSetPatch,
    responses(
        (status = OK, description = "Renamed", body = ValueSet),
        (status = BAD_REQUEST, description = "Invalid or taken name", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set", body = ErrorBody),
        (status = CONFLICT, description = "Changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn rename_set(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<ValueSetPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::rename(
        &state.db,
        &viewer.access,
        id,
        &request.name,
        request.version,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// Deletes a set nobody uses. Needs `system.write`.
#[utoipa::path(
    delete,
    path = "/value-sets/{id}",
    operation_id = "delete_value_set",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Set ID")),
    responses(
        (status = NO_CONTENT, description = "Deleted"),
        (status = FORBIDDEN, description = "No `system.write`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set", body = ErrorBody),
        (status = CONFLICT, description = "A field or a project uses it", body = ErrorBody),
    ),
)]
pub async fn delete_set(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    value_sets::delete(&state.db, &viewer.access, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Adds a value right after another one or at the end (FLD-032). A set shared by projects the
/// caller does not configure cannot change (FLD-031).
#[utoipa::path(
    post,
    path = "/value-sets/{id}/values",
    operation_id = "add_value",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Set ID")),
    request_body = NewValue,
    responses(
        (status = OK, description = "The set now", body = ValueSet),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set", body = ErrorBody),
        (status = CONFLICT, description = "A user set", body = ErrorBody),
    ),
)]
pub async fn add_value(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<NewValue>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::add_value(
        &state.db,
        &viewer.access,
        id,
        ValueInput {
            name: Some(request.name),
            description: Some(request.description),
            color: Some(request.color),
            archived: request.archived.then_some(true),
            resolved: request.resolved,
            released: request.released,
            release_date: request.release_date.map(Some),
            start_date: request.start_date.map(Some),
            assembled_on: request.assembled_on.map(Some),
            owner: request.owner_id.map(Some),
        },
        request.after,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// Changes a value (FLD-043…FLD-049).
#[utoipa::path(
    patch,
    path = "/value-sets/{id}/values/{value_id}",
    operation_id = "update_value",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Set ID"),
        ("value_id" = i64, Path, description = "Value ID"),
    ),
    request_body = ValuePatch,
    responses(
        (status = OK, description = "The set now", body = ValueSet),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set or value", body = ErrorBody),
        (status = CONFLICT, description = "Changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn update_value(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, value_id)): Path<(i64, i64)>,
    JsonBody(request): JsonBody<ValuePatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::update_value(
        &state.db,
        &viewer.access,
        id,
        value_id,
        ValueInput {
            name: request.name,
            description: request.description,
            color: request.color,
            archived: request.archived,
            resolved: request.resolved,
            released: request.released,
            release_date: request.release_date,
            start_date: request.start_date,
            assembled_on: request.assembled_on,
            owner: request.owner_id,
        },
        request.version,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// Moves a value right after another one, or first (FLD-037).
#[utoipa::path(
    post,
    path = "/value-sets/{id}/values/{value_id}/move",
    operation_id = "move_value",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Set ID"),
        ("value_id" = i64, Path, description = "Value ID"),
    ),
    request_body = MoveAfter,
    responses(
        (status = OK, description = "The set now", body = ValueSet),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set or value", body = ErrorBody),
    ),
)]
pub async fn move_value(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, value_id)): Path<(i64, i64)>,
    JsonBody(request): JsonBody<MoveAfter>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row =
        value_sets::move_value(&state.db, &viewer.access, id, value_id, request.after).await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// How many issues hold a value (FLD-032): shown before deleting it.
#[utoipa::path(
    get,
    path = "/value-sets/{id}/values/{value_id}/usage",
    operation_id = "get_value_usage",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Set ID"),
        ("value_id" = i64, Path, description = "Value ID"),
    ),
    responses(
        (status = OK, description = "Issues holding it", body = IssueImpact),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set or value", body = ErrorBody),
    ),
)]
pub async fn value_usage(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, value_id)): Path<(i64, i64)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let issues = value_sets::value_usage(&state.db, &viewer.access, id, value_id).await?;
    Ok(json(StatusCode::OK, IssueImpact { issues }))
}

/// Deletes a value (FLD-032); it stops being a default. Issues holding it get the replacement or
/// lose it, in the background.
#[utoipa::path(
    delete,
    path = "/value-sets/{id}/values/{value_id}",
    operation_id = "delete_value",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Set ID"),
        ("value_id" = i64, Path, description = "Value ID"),
        DeleteValueQuery,
    ),
    responses(
        (status = OK, description = "The set now", body = ValueSet),
        (status = BAD_REQUEST, description = "Invalid replacement", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set or value", body = ErrorBody),
    ),
)]
pub async fn delete_value(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, value_id)): Path<(i64, i64)>,
    QueryParams(query): QueryParams<DeleteValueQuery>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::delete_value(&state.db, &viewer.access, id, value_id, query.replacement)
        .await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// Adds accounts and groups to a user set (FLD-040).
#[utoipa::path(
    post,
    path = "/value-sets/{id}/members",
    operation_id = "add_set_members",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Set ID")),
    request_body = SetMembersChange,
    responses(
        (status = OK, description = "The set now", body = ValueSet),
        (status = BAD_REQUEST, description = "Too many", body = ErrorBody),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set, account or group", body = ErrorBody),
        (status = CONFLICT, description = "Not a user set", body = ErrorBody),
    ),
)]
pub async fn add_members(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<SetMembersChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::add_members(
        &state.db,
        &viewer.access,
        id,
        &request.users,
        &request.groups,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// Removes accounts and groups from a user set (FLD-040).
#[utoipa::path(
    post,
    path = "/value-sets/{id}/members/remove",
    operation_id = "remove_set_members",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Set ID")),
    request_body = SetMembersChange,
    responses(
        (status = OK, description = "The set now", body = ValueSet),
        (status = FORBIDDEN, description = "Missing permissions", body = ErrorBody),
        (status = NOT_FOUND, description = "No such set", body = ErrorBody),
        (status = CONFLICT, description = "Not a user set", body = ErrorBody),
    ),
)]
pub async fn remove_members(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<SetMembersChange>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = value_sets::remove_members(
        &state.db,
        &viewer.access,
        id,
        &request.users,
        &request.groups,
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        value_set(&state.names(), &viewer.access, &row),
    ))
}

/// The fields of a project in its order (FLD-051). Needs `project.update`.
#[utoipa::path(
    get,
    path = "/projects/{id}/fields",
    operation_id = "list_project_fields",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = OK, description = "The fields", body = ProjectFields),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn project_list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let rows = project_fields::list(&state.db, &viewer.access, id).await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        ProjectFields {
            items: rows
                .iter()
                .map(|row| project_field(&names, &viewer.access, row))
                .collect(),
        },
    ))
}

/// Attaches an existing field or a new one (FLD-052, FLD-053) at the end of the project's order.
#[utoipa::path(
    post,
    path = "/projects/{id}/fields",
    operation_id = "attach_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = AttachField,
    responses(
        (status = CREATED, description = "Attached", body = ProjectField),
        (status = BAD_REQUEST, description = "Invalid or taken values", body = ErrorBody),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "Attached already, too many, or archived", body = ErrorBody),
    ),
)]
pub async fn project_attach(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<AttachField>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let input = match request {
        AttachField::Existing { field_id, set } => AttachInput::Existing {
            field: field_id,
            set: match set {
                SetUse::Shared => project_fields::SetUse::Shared,
                SetUse::Copy => project_fields::SetUse::Copy,
            },
        },
        AttachField::New {
            name,
            kind,
            multi,
            aliases,
        } => AttachInput::New {
            name,
            kind: domain_kind(kind),
            multi,
            aliases,
        },
    };
    let row = project_fields::attach(&state.db, &viewer.access, id, input).await?;
    Ok(json(
        StatusCode::CREATED,
        project_field(&state.names(), &viewer.access, &row),
    ))
}

/// Changes a field's settings in the project (FLD-055, FLD-056, FLD-060).
#[utoipa::path(
    patch,
    path = "/projects/{id}/fields/{field_id}",
    operation_id = "update_project_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Project ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    request_body = ProjectFieldPatch,
    responses(
        (status = OK, description = "Changed", body = ProjectField),
        (status = BAD_REQUEST, description = "Invalid values", body = ErrorBody),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or field", body = ErrorBody),
        (status = CONFLICT, description = "Changed meanwhile, or archived", body = ErrorBody),
    ),
)]
pub async fn project_update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field_id)): Path<(i64, i64)>,
    JsonBody(request): JsonBody<ProjectFieldPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = project_fields::update(
        &state.db,
        &viewer.access,
        id,
        field_id,
        SettingsChange {
            can_be_empty: request.can_be_empty,
            empty_label: request.empty_label,
            default_values: request.default_values,
            private: request.private,
            version: request.version,
        },
    )
    .await?;
    Ok(json(
        StatusCode::OK,
        project_field(&state.names(), &viewer.access, &row),
    ))
}

/// Moves a field in the project's order (FLD-058); answers with every field in order.
#[utoipa::path(
    post,
    path = "/projects/{id}/fields/{field_id}/move",
    operation_id = "move_project_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Project ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    request_body = MoveAfter,
    responses(
        (status = OK, description = "The fields now", body = ProjectFields),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or field", body = ErrorBody),
    ),
)]
pub async fn project_move(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field_id)): Path<(i64, i64)>,
    JsonBody(request): JsonBody<MoveAfter>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let rows =
        project_fields::move_after(&state.db, &viewer.access, id, field_id, request.after).await?;
    let names = state.names();
    Ok(json(
        StatusCode::OK,
        ProjectFields {
            items: rows
                .iter()
                .map(|row| project_field(&names, &viewer.access, row))
                .collect(),
        },
    ))
}

/// Gives the project its own copy of a shared set (FLD-030).
#[utoipa::path(
    post,
    path = "/projects/{id}/fields/{field_id}/fork-set",
    operation_id = "fork_project_field_set",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Project ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    responses(
        (status = OK, description = "The field with its own set", body = ProjectField),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or field", body = ErrorBody),
        (status = CONFLICT, description = "No set, or archived", body = ErrorBody),
    ),
)]
pub async fn project_fork(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field_id)): Path<(i64, i64)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let row = project_fields::fork_set(&state.db, &viewer.access, id, field_id).await?;
    Ok(json(
        StatusCode::OK,
        project_field(&state.names(), &viewer.access, &row),
    ))
}

/// How many of the project's issues hold a value of the field (FLD-054): shown before detaching.
#[utoipa::path(
    get,
    path = "/projects/{id}/fields/{field_id}/detach-impact",
    operation_id = "get_detach_impact",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Project ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    responses(
        (status = OK, description = "Issues losing a value", body = IssueImpact),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or field", body = ErrorBody),
        (status = CONFLICT, description = "Archived", body = ErrorBody),
    ),
)]
pub async fn project_detach_impact(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field_id)): Path<(i64, i64)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminRead)?;
    let issues = project_fields::detach_impact(&state.db, &viewer.access, id, field_id).await?;
    Ok(json(StatusCode::OK, IssueImpact { issues }))
}

/// Detaches a field from the project (FLD-054); its values in the project's issues go.
#[utoipa::path(
    delete,
    path = "/projects/{id}/fields/{field_id}",
    operation_id = "detach_field",
    tag = "fields",
    security(("session" = []), ("token" = [])),
    params(
        ("id" = i64, Path, description = "Project ID"),
        ("field_id" = i64, Path, description = "Field ID"),
    ),
    responses(
        (status = NO_CONTENT, description = "Detached"),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project or field", body = ErrorBody),
        (status = CONFLICT, description = "Archived", body = ErrorBody),
    ),
)]
pub async fn project_detach(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((id, field_id)): Path<(i64, i64)>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    project_fields::detach(&state.db, &viewer.access, id, field_id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
