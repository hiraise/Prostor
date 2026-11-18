//! What a field of an issue may hold for the viewer (FLD-001…FLD-014): the pickers of the issue
//! page and the new-issue form ask here. Drafts are issues too.

use prostor_domain::access::Access;
use prostor_domain::access::permission::Permission;
use prostor_domain::fields::{FieldKind, may_edit_set};
use prostor_domain::issues::rights;
use prostor_domain::{FieldId, IssueId, SetId};
use prostor_store::groups::{self, Group};
use prostor_store::users::{self, User};
use prostor_store::value_sets::{self, Value};
use prostor_store::{PgPool, issues};

use super::{checked, values};
use crate::groups::team_visible;
use crate::{Error, Result};

/// Most options in one answer.
pub const LIMIT_MAX: i64 = 50;
const SEARCH_MAX: usize = 100;

/// Options of a field: only the member of its kind is filled.
#[derive(Debug, Clone, Default)]
pub struct FieldOptions {
    pub values: Vec<Value>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    /// More match than were answered.
    pub more: bool,
    /// The viewer may add a new value to the field's set from the picker.
    pub can_add_value: bool,
    /// The field's set of values or people, where a new value goes.
    pub set: Option<SetId>,
}

/// Options of `field` of `issue` matching `text` (name, username or value name, folded), at most
/// `limit`. Values come in their set's order without archived ones; people by name with the
/// viewer first; groups the viewer may know. A private field the viewer cannot update is not
/// found.
pub async fn field_options(
    pool: &PgPool,
    viewer: &Access,
    issue: IssueId,
    field: FieldId,
    text: &str,
    limit: i64,
) -> Result<FieldOptions> {
    let text: String = prostor_query::fold_value(text)
        .chars()
        .take(SEARCH_MAX)
        .collect();
    let limit = limit.clamp(1, LIMIT_MAX);
    let mut conn = pool.acquire().await?;
    let found = issues::get(&mut *conn, issue).await?;
    let (issue, _) = checked(&mut conn, viewer, found).await?;
    let fields = values::of_project(&mut conn, issue.project_id).await?;
    let f = fields
        .get(field)
        .filter(|f| !f.private() || rights::may_update_private(viewer, issue.project_id).is_ok())
        .ok_or(Error::NotFound("no such field"))?;
    let mut out = FieldOptions {
        set: f.attached.set_id,
        ..FieldOptions::default()
    };
    match (f.kind.kind, f.attached.set_id) {
        (FieldKind::Group, _) => {
            let teams = match viewer.projects_with(Permission::ProjectBasic) {
                prostor_domain::access::Projects::All => None,
                prostor_domain::access::Projects::Only(ids) => Some(ids),
            };
            let mut found =
                groups::named_like(&mut *conn, &text, teams.as_deref(), limit + 1).await?;
            found.retain(|g| team_visible(viewer, g));
            out.more = found.len() > usize::try_from(limit).unwrap_or(usize::MAX);
            found.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
            out.groups = found;
        }
        (FieldKind::User, Some(set)) => {
            let ids =
                value_sets::user_options(&mut *conn, set, &text, viewer.user(), limit + 1).await?;
            out.more = ids.len() > usize::try_from(limit).unwrap_or(usize::MAX);
            let ids = &ids[..ids.len().min(usize::try_from(limit).unwrap_or(usize::MAX))];
            let found = users::get_many(&mut *conn, ids).await?;
            out.users = ids
                .iter()
                .filter_map(|id| found.iter().find(|u| u.id == *id))
                .cloned()
                .collect();
        }
        (kind, Some(set)) if kind.set_kind().is_some() => {
            let mut found = value_sets::options(&mut *conn, set, &text, limit + 1).await?;
            out.more = found.len() > usize::try_from(limit).unwrap_or(usize::MAX);
            found.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
            out.values = found;
            let usage = value_sets::usage(&mut *conn, set).await?;
            out.can_add_value = may_edit_set(viewer, &usage.projects).is_ok();
        }
        _ => {}
    }
    Ok(out)
}
