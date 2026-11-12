//! Sharing in answers (ISS-037, QRY-065, ACC-066): owners, audiences and the caller's abilities.

use std::collections::HashMap;

use trackflow_api::accounts::UserRef;
use trackflow_api::sharing::{ListingScope, SharedWith, SharedWithInput, SharingAbilities};
use trackflow_domain::access::Access;
use trackflow_domain::sharing::{self, Shared};
use trackflow_domain::{GroupId, UserId};
use trackflow_service::Error;
use trackflow_service::people::Names;
use trackflow_service::tags::Parties;
use trackflow_store::groups::Group;
use trackflow_store::users::User;

use crate::error::ApiError;
use crate::present::{group_ref, user_ref};

/// The people and groups of a page of shared items, by ID.
pub struct Shown<'a> {
    names: Names,
    viewer: &'a Access,
    users: HashMap<UserId, User>,
    groups: HashMap<GroupId, Group>,
}

impl<'a> Shown<'a> {
    pub fn new(names: Names, viewer: &'a Access, parties: Parties) -> Self {
        Self {
            names,
            viewer,
            users: parties.users.into_iter().map(|u| (u.id, u)).collect(),
            groups: parties.groups.into_iter().map(|g| (g.id, g)).collect(),
        }
    }

    pub fn owner(&self, shared: &Shared) -> Result<UserRef, ApiError> {
        self.users
            .get(&shared.owner)
            .map(|u| user_ref(&self.names, self.viewer, u))
            .ok_or_else(|| Error::Internal("the owner is missing".to_owned()).into())
    }

    /// Unknown IDs (an account deleted for good) are left out.
    pub fn audience(&self, audience: &sharing::Audience) -> SharedWith {
        SharedWith {
            users: audience
                .users
                .iter()
                .filter_map(|id| self.users.get(id))
                .map(|u| user_ref(&self.names, self.viewer, u))
                .collect(),
            groups: audience
                .groups
                .iter()
                .filter_map(|id| self.groups.get(id))
                .map(group_ref)
                .collect(),
        }
    }

    /// The audiences are shown to those who may change the item: owner, editors, user
    /// administrators.
    pub fn sees_sharing(&self, shared: &Shared) -> bool {
        shared.can_edit(self.viewer)
    }

    pub fn abilities(&self, shared: &Shared) -> SharingAbilities {
        SharingAbilities {
            apply: false,
            edit: shared.can_edit(self.viewer),
            delete: shared.may_delete(self.viewer),
            share: shared.may_share(self.viewer).is_ok(),
            transfer: shared.may_transfer(self.viewer).is_ok(),
        }
    }

    /// A tag's abilities: the shared ones and putting it on issues.
    pub fn tag_abilities(&self, shared: &Shared) -> SharingAbilities {
        SharingAbilities {
            apply: shared.can_use(self.viewer),
            ..self.abilities(shared)
        }
    }
}

pub fn shared_with(input: SharedWithInput) -> sharing::Audience {
    sharing::Audience {
        users: input.users,
        groups: input.groups,
    }
}

pub fn tag_listing(scope: Option<ListingScope>) -> trackflow_store::tags::Listing {
    use trackflow_store::tags::Listing;
    match scope.unwrap_or_default() {
        ListingScope::Mine => Listing::Mine,
        ListingScope::Shared => Listing::Shared,
        ListingScope::All => Listing::All,
    }
}

pub fn search_listing(scope: Option<ListingScope>) -> trackflow_store::saved_searches::Listing {
    use trackflow_store::saved_searches::Listing;
    match scope.unwrap_or_default() {
        ListingScope::Mine => Listing::Mine,
        ListingScope::Shared => Listing::Shared,
        ListingScope::All => Listing::All,
    }
}
