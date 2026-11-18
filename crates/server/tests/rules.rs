//! The rules channel over HTTP (step 1.9): the answers of the use cases that can fire behaviors
//! carry their messages (§3.11), and a component value's owner becomes the assignee of an issue
//! that takes the value (AUT-090, FLD-049).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::{Method, StatusCode};
use common::Node;
use prostor_server::throttle::Throttle;
use serde_json::json;

/// A project's rules say nothing yet, and every mutating answer names the field (§3.11).
#[tokio::test]
async fn aut_023_messages_in_answer() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
        )
        .await;
    assert_eq!(project.status, StatusCode::CREATED, "{}", project.body);
    let project_id = project.body["id"].as_i64().unwrap();

    // Creating an issue answers { issue, messages }.
    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": project_id, "summary": "Crash on save" })),
        )
        .await;
    assert_eq!(issue.status, StatusCode::CREATED, "{}", issue.body);
    assert!(
        issue.body["issue"]["readable_id"].is_string(),
        "{}",
        issue.body
    );
    assert_eq!(issue.body["messages"], json!([]));
    let issue_id = issue.body["issue"]["id"].as_i64().unwrap();

    // Changing a field answers the same shape, with the field options to pick a value from.
    let fields = &issue.body["issue"]["fields"];
    let state = fields
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"]["name"] == "State")
        .unwrap();
    let field_id = state["field"]["id"].as_i64().unwrap();
    let options = node
        .call(
            Method::GET,
            &format!("/api/v1/issues/{issue_id}/fields/{field_id}/options"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(options.status, StatusCode::OK, "{}", options.body);
    let resolved = options.body["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["resolved"] == json!(true))
        .unwrap();
    let changed = node
        .call(
            Method::PUT,
            &format!("/api/v1/issues/{issue_id}/fields/{field_id}"),
            Some(&admin),
            Some(json!({ "value": resolved["id"] })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["messages"], json!([]));
    assert!(
        changed.body["issue"]["resolved_at"].is_string(),
        "{}",
        changed.body
    );

    // A comment answers { comment, messages }.
    let comment = node
        .call(
            Method::POST,
            &format!("/api/v1/issues/{issue_id}/comments"),
            Some(&admin),
            Some(json!({ "text": "Looks fixed" })),
        )
        .await;
    assert_eq!(comment.status, StatusCode::CREATED, "{}", comment.body);
    assert!(comment.body["comment"]["id"].is_i64(), "{}", comment.body);
    assert_eq!(comment.body["messages"], json!([]));

    node.stop().await;
}

/// FLD-049 over HTTP: a component value's owner becomes the assignee of an issue that takes the
/// value and has nobody assigned, the answer says so in `messages`, and the change is the
/// rule's in history terms — a value without an owner changes nothing (AUT-090).
#[tokio::test]
async fn fld_049_owner_assigns() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let session = node
        .call(Method::GET, "/api/v1/session", Some(&admin), None)
        .await;
    assert_eq!(session.status, StatusCode::OK, "{}", session.body);
    let admin_id = session.body["user"]["id"].as_i64().unwrap();

    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
        )
        .await;
    assert_eq!(project.status, StatusCode::CREATED, "{}", project.body);
    let project_id = project.body["id"].as_i64().unwrap();

    // The project's own set of its component field.
    let fields = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/fields"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(fields.status, StatusCode::OK, "{}", fields.body);
    let component = fields.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"]["builtin"] == json!("component"))
        .unwrap();
    let set_id = component["set"]["id"].as_i64().unwrap();

    // A value with an owner (FLD-049): the admin, who is in the project's team.
    let server = node
        .call(
            Method::POST,
            &format!("/api/v1/value-sets/{set_id}/values"),
            Some(&admin),
            Some(json!({ "name": "Server", "owner_id": admin_id })),
        )
        .await;
    assert_eq!(server.status, StatusCode::OK, "{}", server.body);
    let server_id = server.body["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == json!("Server"))
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    // One without an owner.
    let bare = node
        .call(
            Method::POST,
            &format!("/api/v1/value-sets/{set_id}/values"),
            Some(&admin),
            Some(json!({ "name": "Barebone" })),
        )
        .await;
    assert_eq!(bare.status, StatusCode::OK, "{}", bare.body);
    let bare_id = bare.body["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == json!("Barebone"))
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    // An issue that takes the component value gets the owner as its assignee.
    let first = new_issue(&node, &admin, project_id, "Crash on save").await;
    let first_id = first["issue"]["id"].as_i64().unwrap();
    let component_id = field_of(&first, "component")["field"]["id"]
        .as_i64()
        .unwrap();
    let changed = node
        .call(
            Method::PUT,
            &format!("/api/v1/issues/{first_id}/fields/{component_id}"),
            Some(&admin),
            Some(json!({ "value": server_id })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(
        changed.body["messages"],
        json!([{
            "rule": "component_owner",
            "code": "assigned_to_owner",
            "params": { "assignee": "Anna Admin", "value": "Server" },
        }]),
        "{}",
        changed.body
    );
    let assignee = field_of(&changed.body, "assignee");
    assert_eq!(
        assignee["users"][0]["id"],
        json!(admin_id),
        "{}",
        changed.body
    );

    // A value without an owner assigns nobody, and no message rides.
    let second = new_issue(&node, &admin, project_id, "Drift in docs").await;
    let second_id = second["issue"]["id"].as_i64().unwrap();
    let changed = node
        .call(
            Method::PUT,
            &format!("/api/v1/issues/{second_id}/fields/{component_id}"),
            Some(&admin),
            Some(json!({ "value": bare_id })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["messages"], json!([]), "{}", changed.body);
    let assignee = field_of(&changed.body, "assignee");
    assert!(assignee["users"].as_array().unwrap().is_empty());

    node.stop().await;
}

/// Creates an issue and answers the whole response body.
async fn new_issue(node: &Node, admin: &str, project: i64, summary: &str) -> serde_json::Value {
    let answer = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(admin),
            Some(json!({ "project": project, "summary": summary })),
        )
        .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    answer.body
}

/// The issue field of a built-in kind, from an answer that carries the issue.
fn field_of(body: &serde_json::Value, builtin: &str) -> serde_json::Value {
    body["issue"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"]["builtin"] == json!(builtin))
        .unwrap()
        .clone()
}

/// Creates a project of a template and answers its rules view with the project's ID.
async fn rules_of(node: &Node, admin: &str, template: &str, key: &str) -> (i64, serde_json::Value) {
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(admin),
            Some(json!({ "template": template, "name": key, "key": key })),
        )
        .await;
    assert_eq!(project.status, StatusCode::CREATED, "{}", project.body);
    let id = project.body["id"].as_i64().unwrap();
    let rules = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{id}/rules"),
            Some(admin),
            None,
        )
        .await;
    assert_eq!(rules.status, StatusCode::OK, "{}", rules.body);
    (id, rules.body)
}

/// The keys of the rules a project turned on, in the registry's order.
fn enabled(rules: &serde_json::Value) -> Vec<&str> {
    rules["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["enabled"] == json!(true))
        .map(|item| item["rule"].as_str().unwrap())
        .collect()
}

/// One rule's view, by key.
fn item<'a>(rules: &'a serde_json::Value, rule: &str) -> &'a serde_json::Value {
    rules["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["rule"] == json!(rule))
        .unwrap()
}

/// AUT-022: a project of each template starts with its rule set on — blank and kanban the eight
/// of PRJ-022, scrum its five.
#[tokio::test]
async fn aut_022_blank_project_gets_set() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let (_, blank) = rules_of(&node, &admin, "blank", "WEB").await;
    let on = enabled(&blank);
    assert_eq!(
        on,
        [
            "assignee_visibility",
            "plus_one_vote",
            "component_owner",
            "subtasks_follow",
            "inherit_fix_versions",
            "fixed_in_build",
            "duplicates",
            "clone_issue",
        ],
        "{}",
        blank
    );
    // Kanban takes the blank set (PRJ-024); its stage rule arrives in P2, disabled.
    let (_, kanban) = rules_of(&node, &admin, "kanban", "BRD").await;
    assert_eq!(enabled(&kanban), on, "{}", kanban);
    // Scrum carries its own five (PRJ-023).
    let (_, scrum) = rules_of(&node, &admin, "scrum", "SCR").await;
    assert_eq!(
        enabled(&scrum),
        [
            "assignee_visibility",
            "plus_one_vote",
            "duplicates",
            "scrum",
            "clone_issue",
        ],
        "{}",
        scrum
    );
    node.stop().await;
}

/// AUT-023: the tab names what a rule lacks — a kanban project has no build field and its stage
/// set holds no «Дубликат», so those rules say so; a rule that can run says nothing.
#[tokio::test]
async fn aut_023_needs_setup_answered() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let (_, rules) = rules_of(&node, &admin, "kanban", "BRD").await;
    assert_eq!(
        item(&rules, "fixed_in_build")["needs_setup"],
        json!([{ "code": "field", "params": { "kind": "build", "name": "Fixed in build" } }]),
        "{}",
        rules
    );
    assert_eq!(
        item(&rules, "duplicates")["needs_setup"],
        json!([{ "code": "value", "params": { "field": "Stage", "value": "Duplicate" } }]),
        "{}",
        rules
    );
    assert_eq!(item(&rules, "plus_one_vote")["needs_setup"], json!([]));
    node.stop().await;
}

/// AUT-023: the toggle takes the project's version — a change moves it, a stale one answers
/// `conflict`, the same value again changes nothing, and an unknown rule key names the field.
#[tokio::test]
async fn aut_023_patch_conflict() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let (id, rules) = rules_of(&node, &admin, "blank", "WEB").await;
    let version = rules["version"].as_i64().unwrap();

    let off = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}/rules"),
            Some(&admin),
            Some(json!({ "rule": "subtasks_follow", "enabled": false, "version": version })),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    assert_eq!(off.body["version"], json!(version + 1));
    assert_eq!(item(&off.body, "subtasks_follow")["enabled"], json!(false));

    // The old version no longer passes.
    let stale = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}/rules"),
            Some(&admin),
            Some(json!({ "rule": "subtasks_follow", "enabled": true, "version": version })),
        )
        .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.body);

    // The same value again changes nothing: the version stays.
    let fresh = off.body["version"].as_i64().unwrap();
    let same = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}/rules"),
            Some(&admin),
            Some(json!({ "rule": "subtasks_follow", "enabled": false, "version": fresh })),
        )
        .await;
    assert_eq!(same.status, StatusCode::OK, "{}", same.body);
    assert_eq!(same.body["version"], json!(fresh));

    // An unknown rule key names the field.
    let unknown = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}/rules"),
            Some(&admin),
            Some(json!({ "rule": "no_such_rule", "enabled": true, "version": fresh })),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST, "{}", unknown.body);
    assert_eq!(unknown.body["fields"][0]["path"], json!("rule"));

    node.stop().await;
}

/// PRJ-023: a scrum project starts with its five rules on and nothing to set up — the template
/// attaches the state set with Done, the type set with Epic and Task, story points and ideal
/// days, so `scrum` runs from the first issue (AUT-022's set, seen from the fields it needs).
#[tokio::test]
async fn prj_023_scrum_set_attached() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let (_, rules) = rules_of(&node, &admin, "scrum", "SCR").await;
    let on = enabled(&rules);
    assert_eq!(
        on,
        [
            "assignee_visibility",
            "plus_one_vote",
            "duplicates",
            "scrum",
            "clone_issue",
        ],
        "{}",
        rules
    );
    for rule in ["assignee_visibility", "duplicates", "scrum"] {
        assert_eq!(
            item(&rules, rule)["needs_setup"],
            json!([]),
            "{rule}: {}",
            rules
        );
    }
    node.stop().await;
}

/// Cloning over HTTP (AUT-066): the endpoints answer the issue and the drafts with their
/// messages, the copy's stream names the original, and a turned-off toggle refuses with
/// `forbidden`.
#[tokio::test]
async fn aut_066_clone_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let (project, rules) = rules_of(&node, &admin, "blank", "WEB").await;

    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": project, "summary": "Crash on save" })),
        )
        .await;
    assert_eq!(issue.status, StatusCode::CREATED, "{}", issue.body);
    let issue_id = issue.body["issue"]["id"].as_i64().unwrap();

    // A published copy at once, with the message the toast offers to open.
    let cloned = node
        .call(
            Method::POST,
            &format!("/api/v1/issues/{issue_id}/clone"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(cloned.status, StatusCode::CREATED, "{}", cloned.body);
    assert_eq!(cloned.body["issue"]["readable_id"], json!("WEB-2"));
    assert_eq!(
        cloned.body["issue"]["summary"],
        json!("Crash on save (copy)")
    );
    assert_eq!(
        cloned.body["messages"],
        json!([{ "rule": "clone_issue", "code": "clone_created", "params": { "issue": "WEB-2" } }]),
        "{}",
        cloned.body
    );
    let clone_id = cloned.body["issue"]["id"].as_i64().unwrap();

    // The copy's stream names the original as the source of the creation.
    let activity = node
        .call(
            Method::GET,
            &format!("/api/v1/issues/{clone_id}/activity"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(activity.status, StatusCode::OK, "{}", activity.body);
    let created = activity.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == json!("change") && item["kind"] == json!("created"))
        .unwrap()
        .clone();
    assert_eq!(created["clone_of"]["readable"], json!(true), "{}", created);
    assert_eq!(
        created["clone_of"]["issue"]["readable_id"],
        json!("WEB-1"),
        "{}",
        created
    );

    // The same copy as a draft, without a number.
    let drafts = node
        .call(
            Method::POST,
            "/api/v1/issues/clone-drafts",
            Some(&admin),
            Some(json!({ "issues": [issue_id] })),
        )
        .await;
    assert_eq!(drafts.status, StatusCode::OK, "{}", drafts.body);
    assert_eq!(drafts.body["drafts"].as_array().unwrap().len(), 1);
    assert_eq!(
        drafts.body["drafts"][0]["readable_id"],
        serde_json::Value::Null
    );
    assert_eq!(
        drafts.body["messages"],
        json!([{ "rule": "clone_issue", "code": "draft_saved", "params": { "issue": "WEB-1" } }]),
        "{}",
        drafts.body
    );

    // The toggle off refuses both endpoints.
    let off = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{project}/rules"),
            Some(&admin),
            Some(json!({
                "rule": "clone_issue",
                "enabled": false,
                "version": rules["version"].as_i64().unwrap()
            })),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    let refused = node
        .call(
            Method::POST,
            &format!("/api/v1/issues/{issue_id}/clone"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    let refused = node
        .call(
            Method::POST,
            "/api/v1/issues/clone-drafts",
            Some(&admin),
            Some(json!({ "issues": [issue_id] })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    node.stop().await;
}
