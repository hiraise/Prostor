//! `/projects/{id}/rules`: the rules tab of project settings (AUT-023, step 1.9 §3.11) — the
//! registry with the project's toggles and what each rule lacks, and the toggle itself.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use prostor_api::error::{ErrorBody, FieldError};
use prostor_api::rules::{NeedView, RuleView, RulesPatch, RulesView};
use prostor_domain::auth::Scope;
use prostor_domain::rules::{Need, Rule};
use prostor_domain::settings::{Language, SYSTEM_LANGUAGE};
use prostor_service::rules::tab::{self, RuleItem, RulesView as TabView};

use crate::AppState;
use crate::access::Viewer;
use crate::auth::require_scope;
use crate::error::{ApiError, JsonBody, json};

/// The rules of a project (AUT-023): every rule of the registry, on or off there, with what each
/// lacks. Anyone who sees the project may read — the menus outside settings ask whether a rule
/// runs (clone, step 1.9 §3.13); only the `PATCH` takes the settings right.
#[utoipa::path(
    get,
    path = "/projects/{id}/rules",
    operation_id = "list_project_rules",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    responses(
        (status = OK, description = "The project's rules", body = RulesView),
        (status = FORBIDDEN, description = "No tracker access", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::TrackerRead)?;
    let view = tab::list(&state.db, &viewer.access, id).await?;
    Ok(json(StatusCode::OK, rules_view(&view, language(&state))))
}

/// Turns one rule on or off (AUT-023). Needs `project.update`; an archived project does not
/// change, and a stale version answers `conflict`.
#[utoipa::path(
    patch,
    path = "/projects/{id}/rules",
    operation_id = "update_project_rule",
    tag = "projects",
    security(("session" = []), ("token" = [])),
    params(("id" = i64, Path, description = "Project ID")),
    request_body = RulesPatch,
    responses(
        (status = OK, description = "The project's rules", body = RulesView),
        (status = BAD_REQUEST, description = "No such rule", body = ErrorBody),
        (status = FORBIDDEN, description = "No `project.update`", body = ErrorBody),
        (status = NOT_FOUND, description = "No such project", body = ErrorBody),
        (status = CONFLICT, description = "Archived, or changed meanwhile", body = ErrorBody),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    JsonBody(request): JsonBody<RulesPatch>,
) -> Result<Response, ApiError> {
    require_scope(&viewer.principal, Scope::AdminWrite)?;
    let Some(rule) = Rule::parse(&request.rule) else {
        return Err(ApiError::invalid(vec![FieldError::new(
            "rule",
            "invalid",
            "no rule of this key",
        )]));
    };
    let view = tab::toggle(
        &state.db,
        &viewer.access,
        id,
        rule,
        request.enabled,
        request.version,
    )
    .await?;
    Ok(json(StatusCode::OK, rules_view(&view, language(&state))))
}

/// The tab over the wire (§3.11): rule keys, names in the instance language, needs as codes the
/// web renders.
fn rules_view(view: &TabView, language: Language) -> RulesView {
    RulesView {
        items: view
            .items
            .iter()
            .map(|item| rule_view(item, language))
            .collect(),
        version: view.version,
    }
}

fn rule_view(item: &RuleItem, language: Language) -> RuleView {
    RuleView {
        rule: item.rule.key().to_owned(),
        name: item.rule.name(language).to_owned(),
        enabled: item.enabled,
        needs_setup: item
            .needs
            .iter()
            .map(|need| need_view(need, language))
            .collect(),
    }
}

fn need_view(need: &Need, language: Language) -> NeedView {
    match need {
        Need::Field { name, kind } => NeedView {
            code: "field".to_owned(),
            params: BTreeMap::from([
                ("kind".to_owned(), kind.key().to_owned()),
                ("name".to_owned(), name.get(language).to_owned()),
            ]),
        },
        Need::Value { field, value } => {
            let mut params = BTreeMap::from([("field".to_owned(), field.clone())]);
            if let Some(value) = value {
                params.insert("value".to_owned(), value.get(language).to_owned());
            }
            NeedView {
                code: "value".to_owned(),
                params,
            }
        }
    }
}

/// The instance language the tab speaks, as the rest of the server does.
fn language(state: &AppState) -> Language {
    state.settings.get().get(&SYSTEM_LANGUAGE)
}
