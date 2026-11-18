//! Link types, links, subtasks and their history over HTTP (step 1.4c).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use prostor_server::throttle::Throttle;
use serde_json::{Value, json};

mod common;

use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn lnk_003_lnk_010_lnk_013_lnk_019_links_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let call = |method: Method, path: String, body: Option<Value>| {
        let node = &node;
        let admin = admin.clone();
        async move { node.call(method, &path, Some(&admin), body).await }
    };
    let types = call(Method::GET, "/api/v1/link-types".into(), None).await;
    assert_eq!(types.status, StatusCode::OK, "{}", types.body);
    let items = types.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    let subtask = items.iter().find(|t| t["builtin"] == "subtask").unwrap();
    assert_eq!(subtask["direction"], "hierarchy");
    assert_eq!(subtask["locked"], true);
    assert_eq!(subtask["usage"], 0);
    let subtask_id = subtask["id"].as_i64().unwrap();

    let web = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
    )
    .await;
    let web_id = web.body["id"].as_i64().unwrap();
    let parent = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id, "summary": "Epic" })),
    )
    .await;
    let parent_id = parent.body["issue"]["id"].as_i64().unwrap();
    assert_eq!(
        parent.body["issue"]["subtasks"],
        json!({ "total": 0, "unresolved": 0 })
    );
    assert_eq!(parent.body["parent"], Value::Null);
    // A new subtask, linked as it is created (ISS-009).
    let child = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({
            "project": web_id,
            "summary": "Part",
            "links": [{ "link_type": subtask_id, "side": "target", "issue": parent_id }],
        })),
    )
    .await;
    assert_eq!(child.status, StatusCode::CREATED, "{}", child.body);
    let child_id = child.body["issue"]["id"].as_i64().unwrap();
    assert_eq!(child.body["issue"]["parent"]["readable_id"], "WEB-1");

    let other = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id, "summary": "Other" })),
    )
    .await;
    let other_id = other.body["issue"]["id"].as_i64().unwrap();
    let added = call(
        Method::POST,
        format!("/api/v1/issues/{parent_id}/links"),
        Some(json!({ "link_type": subtask_id, "side": "source", "issues": [other_id, child_id] })),
    )
    .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    assert_eq!(added.body["added"], json!([other_id]));
    assert_eq!(added.body["existing"], json!([child_id]));
    let group = &added.body["links"]["groups"][0];
    assert_eq!(group["name"], "parent of");
    assert_eq!(group["side"], "source");
    assert_eq!(group["total"], 2);
    assert_eq!(group["items"][0]["issue"]["readable_id"], "WEB-3");
    assert_eq!(group["next_cursor"], Value::Null);

    // A loop is refused with its own code.
    let cycle = call(
        Method::POST,
        format!("/api/v1/issues/{other_id}/links"),
        Some(json!({ "link_type": subtask_id, "side": "source", "issues": [parent_id] })),
    )
    .await;
    assert_eq!(cycle.status, StatusCode::CONFLICT);
    assert_eq!(cycle.body["code"], "link_cycle");

    // The other side, a page of its group, the counts and the history.
    let seen = call(
        Method::GET,
        format!("/api/v1/issues/{other_id}/links"),
        None,
    )
    .await;
    assert_eq!(seen.body["groups"][0]["name"], "child of");
    let page = call(
        Method::GET,
        format!("/api/v1/issues/{parent_id}/links/{subtask_id}/source?limit=1"),
        None,
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert_eq!(page.body["items"].as_array().unwrap().len(), 1);
    let cursor = page.body["next_cursor"].as_str().unwrap().to_owned();
    let rest = call(
        Method::GET,
        format!("/api/v1/issues/{parent_id}/links/{subtask_id}/source?limit=1&cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(rest.body["items"][0]["issue"]["id"], child_id);
    let full = call(Method::GET, format!("/api/v1/issues/{parent_id}"), None).await;
    assert_eq!(
        full.body["subtasks"],
        json!({ "total": 2, "unresolved": 2 })
    );
    let activity = call(
        Method::GET,
        format!("/api/v1/issues/{parent_id}/activity?comments=false"),
        None,
    )
    .await;
    let item = &activity.body["items"][0];
    assert_eq!(item["kind"], "links");
    assert_eq!(item["added"]["links"][0]["name"], "parent of");
    assert_eq!(item["added"]["links"][0]["issue"]["readable_id"], "WEB-3");

    // Removing from either side.
    let link = seen.body["groups"][0]["items"][0]["link"].as_i64().unwrap();
    let removed = call(
        Method::DELETE,
        format!("/api/v1/issues/{other_id}/links/{link}"),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::OK);

    // Types: validation, locked, delete with impact, restore.
    let taken = call(
        Method::POST,
        "/api/v1/link-types".into(),
        Some(json!({ "name": "Ordering", "source_name": "Blocks", "target_name": "after", "direction": "directed" })),
    )
    .await;
    assert_eq!(taken.status, StatusCode::BAD_REQUEST);
    assert_eq!(taken.body["fields"][0]["path"], "source_name");
    assert_eq!(taken.body["fields"][0]["code"], "taken");
    let created = call(
        Method::POST,
        "/api/v1/link-types".into(),
        Some(json!({ "name": "Ordering", "source_name": "precedes", "target_name": "follows", "direction": "directed" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let ordering = created.body["id"].as_i64().unwrap();
    let locked = call(
        Method::PATCH,
        format!("/api/v1/link-types/{subtask_id}"),
        Some(json!({ "name": "Children", "version": 1 })),
    )
    .await;
    assert_eq!(locked.status, StatusCode::CONFLICT);
    assert_eq!(locked.body["code"], "link_type_locked");
    let renamed = call(
        Method::PATCH,
        format!("/api/v1/link-types/{ordering}"),
        Some(json!({ "direction": "undirected", "version": created.body["version"] })),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.body["target_name"], Value::Null);
    let impact = call(
        Method::GET,
        format!("/api/v1/link-types/{ordering}/impact"),
        None,
    )
    .await;
    assert_eq!(impact.body, json!({ "links": 0 }));
    let deleted = call(
        Method::DELETE,
        format!("/api/v1/link-types/{ordering}"),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let restored = call(
        Method::POST,
        format!("/api/v1/link-types/{ordering}/restore"),
        None,
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.body);
    assert_eq!(restored.body["deleted_at"], Value::Null);
}
