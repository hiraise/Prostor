//! Wire types of the built-in behaviors (AUT-023, step 1.9): what a rule said rides the answers
//! of the use cases that fired it (§3.11), and the project's rules tab lists and changes its
//! toggles (`GET`/`PATCH /projects/{id}/rules`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a rule told the actor (§3.11); the web shows each as a toast (§3.13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Message {
    /// The rule's key, the one `GET /projects/{id}/rules` names.
    pub rule: String,
    /// What happened; the texts live in the web.
    pub code: String,
    /// The values the text names; the keys belong to the code.
    pub params: BTreeMap<String, String>,
}

/// One rule of the project's rules tab (§3.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuleView {
    /// The rule's key, the one a `PATCH` takes back.
    pub rule: String,
    /// The rule's name, in the instance language.
    pub name: String,
    /// Whether the rule runs in the project.
    pub enabled: bool,
    /// What the project lacks for the rule; empty when it can run.
    pub needs_setup: Vec<NeedView>,
}

/// One thing a rule lacks (§3.11): a code the web renders and the values its text names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NeedView {
    /// `field` (a field to attach) or `value` (a value the set lacks).
    pub code: String,
    /// The values the text names: `kind` and `name` for a field, `field` and `value` for a
    /// value; `value` is absent when any value does.
    pub params: BTreeMap<String, String>,
}

/// The answer of both endpoints of the rules tab (§3.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RulesView {
    pub items: Vec<RuleView>,
    /// The project's optimistic version, as every settings tab answers it.
    pub version: i64,
}

/// Turns one rule on or off (§3.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RulesPatch {
    /// The rule's key, as `GET` answered it.
    pub rule: String,
    pub enabled: bool,
    /// The project's version, as `GET` answered it.
    pub version: i64,
}
