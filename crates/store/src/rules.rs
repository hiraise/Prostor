//! The per-project toggles of the built-in behaviors (AUT-023, step 1.9). Rows are written by
//! project creation (the template's set) and by the settings tab only; a rule with no row is off.

use prostor_domain::ProjectId;
use prostor_domain::projects::Template;
use prostor_domain::rules::{Rule, template_set};
use sqlx::{PgConnection, PgExecutor};

/// One toggle row of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toggle {
    pub rule: Rule,
    pub enabled: bool,
}

/// A project's toggles, by rule key. One primary-key read; unknown keys (a newer node wrote
/// them) are skipped.
pub async fn of_project(
    db: impl PgExecutor<'_>,
    project: ProjectId,
) -> Result<Vec<Toggle>, sqlx::Error> {
    let rows = sqlx::query!(
        "select rule, enabled from project_rules where project_id = $1 order by rule",
        project
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(Toggle {
                rule: Rule::parse(&row.rule)?,
                enabled: row.enabled,
            })
        })
        .collect())
}

/// Turns a rule on or off for a project; `true` when the row changed.
pub async fn set_enabled(
    conn: &mut PgConnection,
    project: ProjectId,
    rule: Rule,
    enabled: bool,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into project_rules (project_id, rule, enabled) values ($1, $2, $3)
         on conflict (project_id, rule) do update set enabled = excluded.enabled
             where project_rules.enabled <> excluded.enabled",
        project,
        rule.key(),
        enabled
    )
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Writes the template's rule set for a new project (AUT-022): one row per rule, on. Runs inside
/// the creating transaction, after the template's fields are attached.
pub async fn seed_project(
    conn: &mut PgConnection,
    project: ProjectId,
    template: Template,
) -> Result<(), sqlx::Error> {
    for rule in template_set(template) {
        sqlx::query!(
            "insert into project_rules (project_id, rule) values ($1, $2) on conflict do nothing",
            project,
            rule.key()
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
