//! The default field set (FLD-070) and the fields a new project gets from its template (PRJ-019,
//! PRJ-021…PRJ-024, FLD-020). Both run inside the caller's transaction.

use std::collections::HashMap;

use prostor_domain::fields::FieldKind;
use prostor_domain::fields::builtin::{BuiltinField, FieldSeed, SetSeed, SharedSet};
use prostor_domain::fields::templates::{SetChoice, TemplateFieldRef, auto_attach_set, fields_of};
use prostor_domain::projects::Template;
use prostor_domain::settings::Language;
use prostor_domain::{GroupId, ProjectId, SetId, rank};
use prostor_store::fields::{self, Attachment, Field, NewField};
use prostor_store::value_sets::{self, NewValue};
use prostor_store::{PgPool, instance};
use sqlx::PgConnection;

use crate::{Error, Result};

/// Creates the default field set once, in the instance language, when the setup is complete and it
/// does not exist yet; `true` when it was created now. Every node calls this at startup for
/// instances set up before fields existed; the setup wizard creates it in its own transaction.
pub async fn ensure_default_fields(pool: &PgPool, language: Language) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let created = default_fields(&mut tx, language).await?;
    tx.commit().await?;
    Ok(created)
}

/// Creates the default field set inside the caller's transaction, unless it exists.
pub async fn default_fields(conn: &mut PgConnection, language: Language) -> Result<bool> {
    if !instance::lock_unseeded_fields(conn).await? {
        return Ok(false);
    }
    let mut sets: HashMap<SharedSet, (SetId, Vec<i64>)> = HashMap::new();
    let mut last = fields::last_rank(&mut *conn).await?;
    for field in BuiltinField::ALL {
        if fields::by_builtin(&mut *conn, field.key()).await?.is_some() {
            continue;
        }
        let seed = field.seed();
        let name = seed.name.get(language);
        if fields::find(&mut *conn, name, seed.kind.key(), seed.multi)
            .await?
            .is_some()
        {
            // Someone already made a field with this name and type; keep theirs.
            continue;
        }
        let (set, defaults) = match seed.set {
            Some(shared) => {
                if let Some(found) = sets.get(&shared) {
                    (Some(found.0), found.1.clone())
                } else {
                    let kind = seed.kind.set_kind().map_or("enum", |kind| kind.key());
                    let made = create_set(conn, kind, shared.seed(), None, language).await?;
                    sets.insert(shared, made.clone());
                    (Some(made.0), made.1)
                }
            }
            None => (None, Vec::new()),
        };
        let rank = rank::between(last.as_deref(), None).map_err(internal)?;
        insert_builtin(conn, field, &seed, set, &defaults, &rank, language).await?;
        last = Some(rank);
    }
    instance::fields_seeded(conn).await?;
    Ok(true)
}

async fn insert_builtin(
    conn: &mut PgConnection,
    field: BuiltinField,
    seed: &FieldSeed,
    set: Option<SetId>,
    defaults: &[i64],
    rank: &str,
    language: Language,
) -> Result<Field> {
    let aliases: Vec<String> = seed
        .aliases
        .get(language)
        .iter()
        .map(|alias| (*alias).to_owned())
        .collect();
    let id = fields::insert(
        conn,
        &NewField {
            name: seed.name.get(language),
            kind: seed.kind.key(),
            multi: seed.multi,
            aliases: &aliases,
            builtin: Some(field.key()),
            auto_attach: seed.auto_attach,
            rank,
            set_id: set,
            can_be_empty: seed.can_be_empty,
            empty_label: if seed.can_be_empty {
                seed.empty_label.get(language)
            } else {
                ""
            },
            private: false,
            default_values: defaults,
        },
    )
    .await?;
    fields::get(&mut *conn, id)
        .await?
        .ok_or_else(|| Error::Internal("the new field is gone".to_owned()))
}

/// A free name for a new set: `base`, else `base 2`, `base 3`…
pub(crate) async fn free_set_name(
    conn: &mut PgConnection,
    kind: &str,
    base: &str,
) -> Result<String> {
    let taken = value_sets::names_like(&mut *conn, kind, base).await?;
    let lower = base.to_lowercase();
    if !taken.contains(&lower) {
        return Ok(base.to_owned());
    }
    for n in 2..10_000 {
        let candidate = format!("{base} {n}");
        if !taken.contains(&candidate.to_lowercase()) {
            return Ok(candidate);
        }
    }
    Err(Error::Conflict("no free name for a new value set"))
}

/// Creates a set with the values of a seed; returns it with its default value IDs. `owner`
/// prefixes the name with a project's name.
async fn create_set(
    conn: &mut PgConnection,
    kind: &str,
    seed: SetSeed,
    owner: Option<&str>,
    language: Language,
) -> Result<(SetId, Vec<i64>)> {
    let base = match owner {
        Some(project) => format!("{project}: {}", seed.name.get(language)),
        None => seed.name.get(language).to_owned(),
    };
    let name = free_set_name(conn, kind, &truncate(&base)).await?;
    let set = value_sets::insert(conn, &name, kind).await?;
    let ranks = rank::sequence(seed.values.len());
    let mut defaults = Vec::new();
    for (index, (value, rank)) in seed.values.iter().zip(&ranks).enumerate() {
        let id = value_sets::insert_value(
            conn,
            set,
            &NewValue {
                name: value.name.get(language),
                color: value.color,
                rank,
                resolved: value.resolved,
                ..NewValue::default()
            },
        )
        .await?;
        if seed.default == Some(index) {
            defaults.push(id);
        }
    }
    Ok((set, defaults))
}

/// Set names are limited; a long project name is cut, not refused.
fn truncate(name: &str) -> String {
    name.chars()
        .take(prostor_domain::fields::SET_NAME_MAX - 6)
        .collect()
}

/// A new empty set of a project, named after it.
pub(crate) async fn project_set(
    conn: &mut PgConnection,
    kind: &str,
    project_name: &str,
    name: &str,
) -> Result<SetId> {
    let name = free_set_name(conn, kind, &truncate(&format!("{project_name}: {name}"))).await?;
    Ok(value_sets::insert(conn, &name, kind).await?)
}

/// What a new project is created with.
pub(crate) struct NewProjectFields<'a> {
    pub project: ProjectId,
    pub project_name: &'a str,
    pub team: GroupId,
    pub template: Template,
    pub language: Language,
}

/// One field to attach to a new project.
struct Planned {
    field: Field,
    set: Option<SetId>,
    can_be_empty: bool,
    empty_label: String,
    defaults: Vec<i64>,
}

/// Attaches the template's fields to a new project (PRJ-019, PRJ-021): built-in templates bring
/// their own fields and sets; the blank template attaches the fields marked for new projects
/// (FLD-020). Sets of people, components, versions and builds are new and belong to the project;
/// a set of people holds the project team (PRJ-054).
pub(crate) async fn attach_template(
    conn: &mut PgConnection,
    new: &NewProjectFields<'_>,
) -> Result<()> {
    let mut planned: Vec<Planned> = Vec::new();
    // New sets made for the project, by the set of the field they replace, so fix and affected
    // versions share one list of versions.
    let mut made: HashMap<Option<SetId>, SetId> = HashMap::new();
    // Projects made at once would both create a missing template field.
    fields::lock_templates(&mut *conn).await?;
    match fields_of(new.template) {
        None => {
            for field in fields::auto_attached(&mut *conn).await? {
                let Some(kind) = FieldKind::parse(&field.kind) else {
                    continue;
                };
                if planned
                    .iter()
                    .any(|p| p.field.name.to_lowercase() == field.name.to_lowercase())
                {
                    continue;
                }
                let choice = auto_attach_set(kind);
                let (set, defaults) = project_set_for(conn, new, &field, choice, &mut made).await?;
                planned.push(Planned {
                    can_be_empty: field.can_be_empty,
                    empty_label: field.empty_label.clone(),
                    set,
                    defaults,
                    field,
                });
            }
        }
        Some(template) => {
            for item in template {
                let field = template_field(conn, item.field, new.language).await?;
                let (set, defaults) =
                    project_set_for(conn, new, &field, item.set, &mut made).await?;
                let empty_label = match (item.can_be_empty, item.empty_label) {
                    (false, _) => String::new(),
                    (true, Some(label)) => label.get(new.language).to_owned(),
                    (true, None) => field.empty_label.clone(),
                };
                planned.push(Planned {
                    can_be_empty: item.can_be_empty,
                    empty_label,
                    set,
                    defaults,
                    field,
                });
            }
        }
    }
    let ranks = rank::sequence(planned.len());
    for (plan, rank) in planned.iter().zip(&ranks) {
        fields::attach(
            conn,
            &Attachment {
                project_id: new.project,
                field_id: plan.field.id,
                set_id: plan.set,
                rank,
                can_be_empty: plan.can_be_empty,
                empty_label: &plan.empty_label,
                default_values: &plan.defaults,
                private: plan.field.private,
            },
        )
        .await?;
    }
    Ok(())
}

/// The set a field uses in a new project, with its default values there.
async fn project_set_for(
    conn: &mut PgConnection,
    new: &NewProjectFields<'_>,
    field: &Field,
    choice: SetChoice,
    made: &mut HashMap<Option<SetId>, SetId>,
) -> Result<(Option<SetId>, Vec<i64>)> {
    let Some(kind) = FieldKind::parse(&field.kind).and_then(FieldKind::set_kind) else {
        return Ok((None, Vec::new()));
    };
    let base_name = match field.set_id {
        Some(set) => value_sets::get(&mut *conn, set)
            .await?
            .map_or_else(|| field.name.clone(), |set| set.name),
        None => field.name.clone(),
    };
    match choice {
        SetChoice::Shared => Ok((field.set_id, field.default_values.clone())),
        SetChoice::Team => {
            let set = project_set(conn, kind.key(), new.project_name, &base_name).await?;
            value_sets::add_groups(conn, set, &[new.team]).await?;
            Ok((Some(set), Vec::new()))
        }
        SetChoice::Empty => {
            if let Some(set) = made.get(&field.set_id) {
                return Ok((Some(*set), Vec::new()));
            }
            let set = project_set(conn, kind.key(), new.project_name, &base_name).await?;
            made.insert(field.set_id, set);
            Ok((Some(set), Vec::new()))
        }
        SetChoice::Values(seed) => {
            let (set, defaults) =
                create_set(conn, kind.key(), seed, Some(new.project_name), new.language).await?;
            Ok((Some(set), defaults))
        }
    }
}

/// The instance's field for a template entry: a built-in one by its key, another by name and
/// type; created when missing.
async fn template_field(
    conn: &mut PgConnection,
    reference: TemplateFieldRef,
    language: Language,
) -> Result<Field> {
    let (builtin, name, kind, multi, aliases) = match reference {
        TemplateFieldRef::Builtin(builtin) => {
            if let Some(found) = fields::by_builtin(&mut *conn, builtin.key()).await? {
                return Ok(found);
            }
            let seed = builtin.seed();
            (
                Some(builtin),
                seed.name.get(language),
                seed.kind,
                seed.multi,
                seed.aliases.get(language),
            )
        }
        TemplateFieldRef::Named(new) => (
            None,
            new.name.get(language),
            new.kind,
            new.multi,
            new.aliases.get(language),
        ),
    };
    if let Some(found) = fields::find(&mut *conn, name, kind.key(), multi).await? {
        return Ok(found);
    }
    // A field of its own: its set is an empty set named after it, or the default set of a
    // built-in field.
    let set = match (kind.set_kind(), builtin.and_then(|b| b.seed().set)) {
        (Some(set_kind), Some(shared)) => {
            Some(create_set(conn, set_kind.key(), shared.seed(), None, language).await?)
        }
        (Some(set_kind), None) => {
            let set_name = free_set_name(conn, set_kind.key(), &truncate(name)).await?;
            Some((
                value_sets::insert(conn, &set_name, set_kind.key()).await?,
                Vec::new(),
            ))
        }
        (None, _) => None,
    };
    let aliases: Vec<String> = aliases.iter().map(|alias| (*alias).to_owned()).collect();
    let last = fields::last_rank(&mut *conn).await?;
    let rank = rank::between(last.as_deref(), None).map_err(internal)?;
    let empty_label = builtin.map_or("", |b| b.seed().empty_label.get(language));
    let can_be_empty = builtin.is_none_or(|b| b.seed().can_be_empty);
    let id = fields::insert(
        conn,
        &NewField {
            name,
            kind: kind.key(),
            multi,
            aliases: &aliases,
            builtin: builtin.map(BuiltinField::key),
            auto_attach: false,
            rank: &rank,
            set_id: set.as_ref().map(|(id, _)| *id),
            can_be_empty,
            empty_label: if can_be_empty { empty_label } else { "" },
            private: false,
            default_values: set.as_ref().map(|(_, d)| d.as_slice()).unwrap_or_default(),
        },
    )
    .await?;
    fields::get(&mut *conn, id)
        .await?
        .ok_or_else(|| Error::Internal("the new field is gone".to_owned()))
}

pub(crate) fn internal(error: impl std::fmt::Display) -> Error {
    Error::Internal(error.to_string())
}
