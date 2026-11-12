//! The rules tab of project settings (AUT-023, step 1.9 §3.11): every rule of the registry with
//! the project's toggles and what each still lacks, and the toggle itself. Needs are computed on
//! read, in the instance language; the optimistic version is the project's, as in every tab.

use sqlx::PgConnection;
use trackflow_domain::ProjectId;
use trackflow_domain::access::Access;
use trackflow_domain::access::permission::Permission;
use trackflow_domain::rules::needs as needs_of;
use trackflow_domain::rules::{Need, ProjectSnapshot, Rule, SnapshotField, SnapshotValue};
use trackflow_domain::settings::Language;
use trackflow_store::PgPool;
use trackflow_store::{projects, rules as store_rules, value_sets};

use crate::Error;
use crate::issues::values;
use crate::projects::{lock_visible, require, require_active, visible};

use super::instance_language;

/// One rule of the tab: its toggle and what the project lacks for it.
#[derive(Debug, Clone)]
pub struct RuleItem {
    pub rule: Rule,
    pub enabled: bool,
    pub needs: Vec<Need>,
}

/// What both endpoints answer (§3.11): the rules with the project's optimistic version.
#[derive(Debug, Clone)]
pub struct RulesView {
    pub items: Vec<RuleItem>,
    pub version: i64,
}

/// The tab (AUT-023): every rule of the registry, on or off in this project, with its needs.
/// Anyone who sees the project may read: the menus outside settings ask whether a rule runs
/// (clone, step 1.9 §3.13) — only [`toggle`] takes the settings right.
pub async fn list(pool: &PgPool, viewer: &Access, id: ProjectId) -> Result<RulesView, Error> {
    let mut conn = pool.acquire().await?;
    let project = visible(&mut conn, viewer, id).await?;
    let toggles = store_rules::of_project(&mut *conn, id).await?;
    let language = instance_language(&mut conn).await?;
    let items = items(&mut conn, id, language, &toggles).await?;
    Ok(RulesView {
        items,
        version: project.version,
    })
}

/// Turns one rule on or off (AUT-023): the settings right of the other tabs, an active project
/// and its optimistic version. A change moves the version, as every settings change does.
pub async fn toggle(
    pool: &PgPool,
    actor: &Access,
    id: ProjectId,
    rule: Rule,
    enabled: bool,
    version: i64,
) -> Result<RulesView, Error> {
    let mut tx = pool.begin().await?;
    let project = lock_visible(&mut tx, actor, id).await?;
    require(actor, Permission::ProjectUpdate, id)?;
    require_active(&project)?;
    if project.version != version {
        return Err(Error::Conflict("the project was changed by someone else"));
    }
    let version = if store_rules::set_enabled(&mut tx, id, rule, enabled).await? {
        projects::touch(&mut tx, &[id]).await?;
        project.version + 1
    } else {
        project.version
    };
    let toggles = store_rules::of_project(&mut *tx, id).await?;
    let language = instance_language(&mut tx).await?;
    let items = items(&mut tx, id, language, &toggles).await?;
    tx.commit().await?;
    Ok(RulesView { items, version })
}

/// Every rule of the registry against the project's attached fields: on or off, with what it
/// lacks. The snapshot lives here: it borrows the fields and values of one read.
async fn items(
    conn: &mut PgConnection,
    id: ProjectId,
    language: Language,
    toggles: &[store_rules::Toggle],
) -> Result<Vec<RuleItem>, Error> {
    let fields = values::of_project(&mut *conn, id).await?;
    let mut sets: Vec<i64> = fields
        .0
        .iter()
        .filter_map(|field| field.attached.set_id)
        .collect();
    sets.sort_unstable();
    sets.dedup();
    let set_values = value_sets::values_of(&mut *conn, &sets).await?;
    let snapshot = ProjectSnapshot {
        language,
        fields: fields
            .0
            .iter()
            .map(|field| SnapshotField {
                name: &field.field.name,
                kind: field.kind.kind,
                builtin: field.field.builtin.as_deref(),
                multi: field.kind.multi,
                default: !field.attached.default_values.is_empty(),
                values: field
                    .attached
                    .set_id
                    .map(|set| {
                        set_values
                            .iter()
                            .filter(|value| value.set_id == set)
                            .map(|value| SnapshotValue {
                                name: &value.name,
                                resolved: value.resolved,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
    };
    Ok(Rule::ALL
        .iter()
        .map(|&rule| RuleItem {
            rule,
            enabled: toggles
                .iter()
                .any(|toggle| toggle.rule == rule && toggle.enabled),
            needs: needs_of(rule, &snapshot),
        })
        .collect())
}
