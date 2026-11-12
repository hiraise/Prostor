//! Capabilities (ACC-064, ADM-023): features switched on for chosen groups rather than granted by
//! roles. A person has a capability when they belong to any of its groups, the built-in ones
//! included; no groups means off. The list grows with the features that need one.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Changing who may view, use and edit tags, saved searches, boards, dashboards, Gantt charts
    /// and whiteboards (ACC-066). Everyone by default.
    ShareViews,
}

impl Capability {
    pub const ALL: [Capability; 1] = [Capability::ShareViews];

    /// Stable key, stored with the capability's groups and used in the API.
    pub fn key(self) -> &'static str {
        match self {
            Capability::ShareViews => "share_views",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.key() == key)
    }

    /// Whether the group list may be empty (the capability switched off for everybody).
    pub fn can_be_switched_off(self) -> bool {
        match self {
            Capability::ShareViews => true,
        }
    }

    fn bit(self) -> u32 {
        1 << (self as u8)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// The capabilities one person has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    pub fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    pub fn insert(&mut self, capability: Capability) {
        self.0 |= capability.bit();
    }

    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |capability| self.contains(*capability))
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        let mut set = Self::default();
        for capability in iter {
            set.insert(capability);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_064_capability_keys() {
        for capability in Capability::ALL {
            assert_eq!(Capability::parse(capability.key()), Some(capability));
        }
        let set: CapabilitySet = [Capability::ShareViews].into_iter().collect();
        assert!(set.contains(Capability::ShareViews));
        assert!(!CapabilitySet::default().contains(Capability::ShareViews));
        assert_eq!(set.iter().count(), 1);
    }
}
