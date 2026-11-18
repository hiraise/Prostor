//! Capabilities switched on per group (ACC-064, ACC-066): seeing them needs `system.read`,
//! changing them `system.write`.

use prostor_domain::GroupId;
use prostor_domain::access::Access;
use prostor_domain::access::capability::Capability;
use prostor_domain::access::permission::Permission;
use prostor_domain::validation::Problem;
use prostor_store::{PgPool, access as assignments, audit, groups};
use serde_json::json;

use crate::access::changed;
use crate::{Error, Result};

/// One capability and the groups it is on for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGroups {
    pub capability: Capability,
    pub groups: Vec<GroupId>,
}

pub async fn list(pool: &PgPool, viewer: &Access) -> Result<Vec<CapabilityGroups>> {
    viewer.require(Permission::SystemRead)?;
    let pairs = assignments::capability_groups(pool).await?;
    Ok(Capability::ALL
        .into_iter()
        .map(|capability| CapabilityGroups {
            capability,
            groups: pairs
                .iter()
                .filter(|(key, _)| key == capability.key())
                .map(|(_, group)| *group)
                .collect(),
        })
        .collect())
}

/// Replaces the groups a capability is on for; no groups switches it off where that is allowed.
pub async fn set(
    pool: &PgPool,
    actor: &Access,
    capability: Capability,
    group_ids: &[GroupId],
) -> Result<CapabilityGroups> {
    actor.require(Permission::SystemWrite)?;
    let mut ids = group_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() && !capability.can_be_switched_off() {
        return Err(Error::field(
            "groups",
            Problem::Required,
            "this capability cannot be switched off",
        ));
    }
    let mut tx = pool.begin().await?;
    let found = groups::lock_many(&mut tx, &ids).await?;
    if found.len() != ids.len() {
        return Err(Error::field("groups", Problem::Invalid, "no such group"));
    }
    let old = assignments::set_capability_groups(&mut tx, capability.key(), &ids).await?;
    audit::record(
        &mut tx,
        prostor_domain::Actor::User(actor.user()),
        "capability.changed",
        &format!("capability:{}", capability.key()),
        json!({ "old": old, "new": ids }),
    )
    .await?;
    changed(&mut tx).await?;
    tx.commit().await?;
    Ok(CapabilityGroups {
        capability,
        groups: ids,
    })
}
