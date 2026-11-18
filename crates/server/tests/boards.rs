//! Boards over HTTP (BRD-003…BRD-016, BRD-042…BRD-046, BRD-057, BRD-068; plan step 1.8 A2):
//! routes, errors and version conflicts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;
use taskmaster_server::throttle::Throttle;

use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn brd_boards_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let created = node
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({
                "full_name": "Carol Clark",
                "username": "carol",
                "email": "carol@example.com",
                "password": "carol password",
            })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let carol = node
        .call(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "carol", "password": "carol password" })),
        )
        .await
        .session_cookie()
        .unwrap();
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "kanban", "name": "Flow", "key": "FLOW" })),
        )
        .await;
    assert_eq!(project.status, StatusCode::CREATED, "{}", project.body);
    let project_id = project.body["id"].as_i64().unwrap();
    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": project_id, "summary": "Card one" })),
        )
        .await;
    assert_eq!(issue.status, StatusCode::CREATED, "{}", issue.body);

    // Validation and refusals.
    let bad = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "kanban", "name": "a/b", "projects": [] })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    assert_eq!(bad.code(), "invalid_request");
    let fields: Vec<&str> = bad.body["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["name", "projects"]);
    let not_read = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&carol),
            Some(json!({ "preset": "kanban", "name": "Mine", "projects": [project_id] })),
        )
        .await;
    assert_eq!(
        not_read.status,
        StatusCode::BAD_REQUEST,
        "{}",
        not_read.body
    );

    // Create from a preset (BRD-003).
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "kanban", "name": "Flow board", "projects": [project_id] })),
        )
        .await;
    assert_eq!(board.status, StatusCode::CREATED, "{}", board.body);
    let id = board.body["id"].as_i64().unwrap();
    assert_eq!(board.body["preset"], "kanban");
    assert_eq!(board.body["sprints_on"], false);
    assert_eq!(board.body["mode"], "manual");
    assert_eq!(board.body["owner"]["username"], "admin");
    assert_eq!(board.body["rights"]["delete"], true);
    assert_eq!(board.body["projects"][0]["project"]["key"], "FLOW");
    let columns = board.body["columns"].as_array().unwrap().clone();
    assert!(columns.len() >= 3, "{}", board.body);
    assert_eq!(columns[1]["wip_max"], 5);
    let version = board.body["version"].as_i64().unwrap();

    // Others do not see a private board.
    let hidden = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);

    // PATCH with the version; a stale one conflicts.
    let renamed = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({ "name": "Renamed", "version": version })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.body["name"], "Renamed");
    let stale = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({ "name": "Again", "version": version })),
        )
        .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.code(), "conflict");

    // The list, the profile and favorites first.
    let profile = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}/profile"),
            Some(&admin),
            Some(json!({ "favorite": true, "card_size": 3 })),
        )
        .await;
    assert_eq!(profile.status, StatusCode::OK, "{}", profile.body);
    assert_eq!(profile.body["card_size"], 3);
    let too_big = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}/profile"),
            Some(&admin),
            Some(json!({ "card_size": 9 })),
        )
        .await;
    assert_eq!(too_big.status, StatusCode::BAD_REQUEST);
    let list = node
        .call(Method::GET, "/api/v1/boards?limit=1", Some(&admin), None)
        .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert_eq!(list.body["items"][0]["id"], id);
    assert_eq!(list.body["items"][0]["favorite"], true);
    let none = node
        .call(Method::GET, "/api/v1/boards", Some(&carol), None)
        .await;
    assert_eq!(none.body["items"].as_array().unwrap().len(), 0);

    // The view: the issue is not a card until it is put on the board (1.8a-3).
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.status, StatusCode::OK, "{}", view.body);
    assert_eq!(view.body["sprint"]["internal"], true);
    assert_eq!(view.body["sprints"].as_array().unwrap().len(), 0);
    assert_eq!(view.body["totals"]["count"], 0);
    assert_eq!(view.body["columns"][0]["wip"], "within");
    assert_eq!(view.body["profile"]["favorite"], true);
    assert!(view.body["live_topic"].is_string());
    let list = node
        .call(Method::GET, "/api/v1/boards", Some(&admin), None)
        .await;
    assert_eq!(list.body["last_visited"], id);
    let no_sprint = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view?sprint=999999"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(no_sprint.status, StatusCode::NOT_FOUND);
    let first_column = columns[0]["id"].as_i64().unwrap();
    let cell = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/cell?column={first_column}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(cell.status, StatusCode::OK, "{}", cell.body);
    assert_eq!(cell.body["next_cursor"], serde_json::Value::Null);
    let bad_cursor = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/cell?column={first_column}&cursor=junk"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(bad_cursor.status, StatusCode::BAD_REQUEST);

    // Columns: move, limits, remove, add back; both value and new_value is invalid.
    let last = columns.last().unwrap()["id"].as_i64().unwrap();
    let value = columns.last().unwrap()["value"]["id"].as_i64().unwrap();
    let moved = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}/columns/{last}"),
            Some(&admin),
            Some(json!({ "after": null, "wip_min": 1, "wip_max": null })),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(moved.body["columns"][0]["id"], last);
    assert_eq!(moved.body["columns"][0]["wip_min"], 1);
    let wrong = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}/columns/{last}"),
            Some(&admin),
            Some(json!({ "wip_min": 1000 })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let removed = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}/columns/{last}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.body);
    assert_eq!(
        removed.body["columns"].as_array().unwrap().len(),
        columns.len() - 1
    );
    let gone = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}/columns/{last}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    let both = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/columns"),
            Some(&admin),
            Some(json!({ "value": value, "new_value": "X" })),
        )
        .await;
    assert_eq!(both.status, StatusCode::BAD_REQUEST);
    let back = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/columns"),
            Some(&admin),
            Some(json!({ "value": value })),
        )
        .await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.body);
    assert_eq!(
        back.body["columns"].as_array().unwrap().len(),
        columns.len()
    );
    let twice = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/columns"),
            Some(&admin),
            Some(json!({ "value": value })),
        )
        .await;
    assert_eq!(twice.status, StatusCode::BAD_REQUEST);
    assert_eq!(twice.body["fields"][0]["code"], "taken");

    // Sharing with Carol: she views but does not edit; deleting is refused.
    let current = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            None,
        )
        .await;
    let carol_id = created.body["id"].as_i64().unwrap();
    let shared = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({
                "sharing": { "view": { "users": [carol_id] } },
                "version": current.body["version"],
            })),
        )
        .await;
    assert_eq!(shared.status, StatusCode::OK, "{}", shared.body);
    assert_eq!(
        shared.body["sharing"]["view"]["users"][0]["username"],
        "carol"
    );
    let seen = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(seen.status, StatusCode::OK, "{}", seen.body);
    assert_eq!(seen.body["sharing"], serde_json::Value::Null);
    assert_eq!(seen.body["rights"]["edit"], false);
    let refused = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&carol),
            Some(json!({ "name": "Hers", "version": seen.body["version"] })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let not_owner = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(not_owner.status, StatusCode::FORBIDDEN);

    // Impact, then delete.
    let impact = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/impact"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(impact.status, StatusCode::OK, "{}", impact.body);
    assert_eq!(
        impact.body,
        json!({ "sprints": 0, "cards": 0, "searches": 0 })
    );
    let deleted = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let after = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(after.status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_068_cards_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "kanban", "name": "Flow", "key": "FLOW" })),
        )
        .await;
    let project_id = project.body["id"].as_i64().unwrap();
    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": project_id, "summary": "Card one" })),
        )
        .await;
    let issue_id = issue.body["issue"]["id"].as_i64().unwrap();
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "kanban", "name": "Flow board", "projects": [project_id] })),
        )
        .await;
    assert_eq!(board.status, StatusCode::CREATED, "{}", board.body);
    let id = board.body["id"].as_i64().unwrap();
    let columns: Vec<i64> = board.body["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.body["live_topic"], format!("board:{id}"));
    let sprint = view.body["sprint"]["id"].as_i64().unwrap();
    let cards = format!("/api/v1/boards/{id}/sprints/{sprint}/cards");

    let added = node
        .call(
            Method::POST,
            &cards,
            Some(&admin),
            Some(json!({ "issue": issue_id, "column": columns[1] })),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    assert_eq!(added.body["card"]["column"], columns[1]);
    assert_eq!(added.body["issue"]["id"], issue_id);
    let version = added.body["issue"]["version"].as_i64().unwrap();

    let moved = node
        .call(
            Method::POST,
            &format!("{cards}/{issue_id}/move"),
            Some(&admin),
            Some(json!({ "column": columns[2], "version": version })),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(moved.body["card"]["column"], columns[2]);
    let stale = node
        .call(
            Method::POST,
            &format!("{cards}/{issue_id}/move"),
            Some(&admin),
            Some(json!({ "column": columns[0], "version": version })),
        )
        .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.body);
    let bad = node
        .call(
            Method::POST,
            &format!("{cards}/{issue_id}/move"),
            Some(&admin),
            Some(json!({ "column": 999_999, "version": moved.body["issue"]["version"] })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    assert_eq!(bad.code(), "invalid_request");

    let batch = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/cards/batch"),
            Some(&admin),
            Some(json!({ "issues": [issue_id, 999_999] })),
        )
        .await;
    assert_eq!(batch.status, StatusCode::OK, "{}", batch.body);
    assert_eq!(batch.body["cards"][0]["column"], columns[2]);
    assert_eq!(batch.body["cards"][1], json!(null));
    assert_eq!(batch.body["issues"].as_array().unwrap().len(), 1);
    // Cards carry the compact form: values by ID, what they name listed once (D-044).
    let card = &batch.body["issues"][0];
    assert!(card.get("description_html").is_none(), "{card}");
    let state = card["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"] == board.body["column_field"]["id"])
        .unwrap();
    let value = state["ids"][0].clone();
    assert!(
        batch.body["refs"]["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == value),
        "{}",
        batch.body
    );

    let removed = node
        .call(
            Method::DELETE,
            &format!("{cards}/{issue_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.body);
    let again = node
        .call(
            Method::DELETE,
            &format!("{cards}/{issue_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.body);

    // The issue's stream shows the board records (HIST-002).
    let activity = node
        .call(
            Method::GET,
            &format!("/api/v1/issues/{issue_id}/activity?comments=false"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(activity.status, StatusCode::OK, "{}", activity.body);
    let records: Vec<&serde_json::Value> = activity.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "boards")
        .collect();
    assert_eq!(records.len(), 2, "{}", activity.body);
    let named = |side: &serde_json::Value| side["boards"][0]["board"]["name"].clone();
    assert!(records.iter().any(|r| named(&r["added"]) == "Flow board"));
    assert!(records.iter().any(|r| named(&r["removed"]) == "Flow board"));
}

#[tokio::test]
async fn brd_098_sprints_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "scrum", "name": "Team", "key": "TEAM" })),
        )
        .await;
    let project_id = project.body["id"].as_i64().unwrap();
    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": project_id, "summary": "Carry me" })),
        )
        .await;
    let issue_id = issue.body["issue"]["id"].as_i64().unwrap();
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "scrum", "name": "Team board", "projects": [project_id] })),
        )
        .await;
    assert_eq!(board.status, StatusCode::CREATED, "{}", board.body);
    let id = board.body["id"].as_i64().unwrap();
    let list = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/sprints"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    let first = &list.body["items"][0];
    assert_eq!(first["name"], "Sprint 1");
    assert_eq!(first["current"], true);
    assert_eq!(first["finished"], false);
    let s1 = first["id"].as_i64().unwrap();
    let added = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{s1}/cards"),
            Some(&admin),
            Some(json!({ "issue": issue_id })),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);

    let bad = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints"),
            Some(&admin),
            Some(json!({ "name": "Bad", "starts_on": "2026-10-20", "ends_on": "2026-10-10" })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    let created = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints"),
            Some(&admin),
            Some(json!({ "name": "Sprint 2", "goal": "Ship", "auto_target": true })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let s2 = created.body["id"].as_i64().unwrap();
    assert_eq!(created.body["goal"], "Ship");
    let got = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(got.body["auto_sprint"], s2);
    assert_eq!(got.body["mode"], "auto");

    let edited = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}/sprints/{s2}"),
            Some(&admin),
            Some(json!({ "ends_on": null, "name": "Second", "version": created.body["version"] })),
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(edited.body["name"], "Second");
    let stale = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}/sprints/{s2}"),
            Some(&admin),
            Some(json!({ "name": "Third", "version": created.body["version"] })),
        )
        .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.body);

    let started = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{s2}/start"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.body);
    assert!(started.body["starts_on"].is_string(), "{}", started.body);
    assert_eq!(started.body["current"], true);
    let finished = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{s1}/finish"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(finished.status, StatusCode::OK, "{}", finished.body);
    assert_eq!(finished.body["finished"], true);
    let carried = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{s1}/carry"),
            Some(&admin),
            Some(json!({ "to": s2 })),
        )
        .await;
    assert_eq!(carried.status, StatusCode::OK, "{}", carried.body);
    assert_eq!(carried.body["id"], s2);
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view?sprint={s2}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.body["cards"][0]["issue"], issue_id, "{}", view.body);

    let archived = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{s1}/archive"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(archived.body["archived"], true, "{}", archived.body);
    let restored = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{s1}/restore"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(restored.body["archived"], false, "{}", restored.body);
    let deleted = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}/sprints/{s1}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
    let last = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}/sprints/{s2}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(last.status, StatusCode::FORBIDDEN, "{}", last.body);

    // Sprints off: the board keeps its cards in the hidden sprint (BRD-024).
    let got = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            None,
        )
        .await;
    let off = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({ "sprints_on": false, "version": got.body["version"] })),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    assert_eq!(off.body["sprints_on"], false);
    assert_eq!(off.body["auto_sprint"], json!(null));
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.body["cards"][0]["issue"], issue_id, "{}", view.body);
    assert_eq!(view.body["sprints"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_081_header_filter_and_query_board_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "kanban", "name": "Flow", "key": "FLOW" })),
        )
        .await;
    let project_id = project.body["id"].as_i64().unwrap();
    let mut issues = Vec::new();
    for summary in ["Alpha", "Beta"] {
        let issue = node
            .call(
                Method::POST,
                "/api/v1/issues",
                Some(&admin),
                Some(json!({ "project": project_id, "summary": summary })),
            )
            .await;
        issues.push(issue.body["issue"]["id"].as_i64().unwrap());
    }
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "kanban", "name": "Flow board", "projects": [project_id] })),
        )
        .await;
    let id = board.body["id"].as_i64().unwrap();
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert!(view.body["sort"].is_null());
    let sprint = view.body["sprint"]["id"].as_i64().unwrap();
    let cards = format!("/api/v1/boards/{id}/sprints/{sprint}/cards");
    for issue in &issues {
        let added = node
            .call(
                Method::POST,
                &cards,
                Some(&admin),
                Some(json!({ "issue": issue })),
            )
            .await;
        assert_eq!(added.status, StatusCode::OK, "{}", added.body);
        assert_eq!(added.body["changes"], json!([]));
    }

    // A sort clause in the header filter orders the cells (BRD-070).
    let sorted = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view?q=sort%3A%20summary%20desc"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(sorted.status, StatusCode::OK, "{}", sorted.body);
    assert_eq!(sorted.body["sort"][0]["key"], "summary");
    let order: Vec<i64> = sorted.body["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["issue"].as_i64().unwrap())
        .collect();
    assert_eq!(order, vec![issues[1], issues[0]]);
    let bad = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view?q=state%3A%20nowhere"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);

    // A query board shows what matches; an issue no change can make match is refused.
    let patched = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({
                "mode": "query",
                "query": "summary: Alpha",
                "version": board.body["version"],
            })),
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);
    assert_eq!(patched.body["mode"], "query");
    assert_eq!(patched.body["query"], "summary: Alpha");
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.body["cards"].as_array().unwrap().len(), 1);
    let preview = node
        .call(
            Method::POST,
            &cards,
            Some(&admin),
            Some(json!({ "issue": issues[1], "preview": true })),
        )
        .await;
    assert_eq!(preview.status, StatusCode::FORBIDDEN, "{}", preview.body);
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_089_backlog_and_issue_boards_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let made = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "blank", "name": "App", "key": "APP" })),
        )
        .await;
    let project = made.body["id"].as_i64().unwrap();
    let mut issue_ids = Vec::new();
    for _ in 0..2 {
        let made = node
            .call(
                Method::POST,
                "/api/v1/issues",
                Some(&admin),
                Some(json!({ "project": project, "summary": "Backlog" })),
            )
            .await;
        issue_ids.push(made.body["issue"]["id"].as_i64().unwrap());
    }
    let (a, b) = (issue_ids[0], issue_ids[1]);
    let created = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "scrum", "name": "Team", "projects": [project] })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id = created.body["id"].as_i64().unwrap();
    assert!(created.body["backlog"].is_i64(), "{}", created.body);

    let page = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/backlog?limit=1"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert_eq!(page.body["search"]["name"], "Team · backlog");
    assert_eq!(page.body["search"]["may_order"], true);
    assert_eq!(page.body["missing"], serde_json::Value::Null);
    let first = page.body["page"]["items"][0]["id"].as_i64().unwrap();
    let cursor = page.body["page"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let last = if first == a { b } else { a };
    let ordered = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/backlog/order"),
            Some(&admin),
            Some(json!({ "issue": last })),
        )
        .await;
    assert_eq!(ordered.status, StatusCode::NO_CONTENT, "{}", ordered.body);
    let page = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/backlog"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(page.body["page"]["items"][0]["id"], last);
    // A cursor of the order before is still read: the order is not in it.
    let next = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/backlog?limit=1&cursor={cursor}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.body);
    let discarded = node
        .call(
            Method::DELETE,
            &format!("/api/v1/boards/{id}/backlog/order"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(
        discarded.status,
        StatusCode::NO_CONTENT,
        "{}",
        discarded.body
    );

    // The issue side.
    let added = node
        .call(
            Method::POST,
            &format!("/api/v1/issues/{a}/boards"),
            Some(&admin),
            Some(json!({ "board": id })),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    assert_eq!(added.body["items"][0]["board"]["name"], "Team");
    assert_eq!(added.body["items"][0]["sprint"]["name"], "Sprint 1");
    let page = node
        .call(
            Method::GET,
            &format!("/api/v1/issues/{a}/page"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert_eq!(page.body["boards"].as_array().unwrap().len(), 1);
    let removed = node
        .call(
            Method::DELETE,
            &format!("/api/v1/issues/{a}/boards/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.body);
    assert_eq!(removed.body["items"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_049_lanes_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "kanban", "name": "Flow", "key": "FLOW" })),
        )
        .await;
    let project_id = project.body["id"].as_i64().unwrap();
    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": project_id, "summary": "Card one" })),
        )
        .await;
    let issue_id = issue.body["issue"]["id"].as_i64().unwrap();
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "kanban", "name": "Flow board", "projects": [project_id] })),
        )
        .await;
    let id = board.body["id"].as_i64().unwrap();
    assert_eq!(board.body["lanes"]["mode"], "none");
    // Value lanes need a field or an attribute.
    let refused = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({ "lanes": { "mode": "values" }, "version": board.body["version"] })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.body);
    assert_eq!(refused.body["fields"][0]["path"], "lanes.field");
    let changed = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({ "lanes": { "mode": "parents", "other": "top" }, "version": board.body["version"] })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["lanes"]["mode"], "parents");
    assert_eq!(changed.body["lanes"]["other"], "top");
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    let sprint = view.body["sprint"]["id"].as_i64().unwrap();
    let lanes = format!("/api/v1/boards/{id}/sprints/{sprint}/lanes");
    let made = node
        .call(
            Method::POST,
            &lanes,
            Some(&admin),
            Some(json!({ "title": "Login" })),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let lane_issue = made.body["lane"]["issue"].as_i64().unwrap();
    let key = format!("i:{lane_issue}");
    assert_eq!(made.body["lane"]["key"], key);
    assert_eq!(made.body["issue"]["summary"], "Login");
    // A card dropped into the lane becomes a subtask of its issue.
    let added = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/sprints/{sprint}/cards"),
            Some(&admin),
            Some(json!({ "issue": issue_id, "lane": key })),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    assert_eq!(added.body["card"]["lane"], key);
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.body["lanes"][0]["key"], "other");
    assert_eq!(view.body["lanes"][1]["key"], key);
    assert_eq!(view.body["lanes"][1]["count"], 1);
    assert_eq!(view.body["lanes_total"], 2);
    assert!(view.body["lanes_next"].is_null());
    let bad = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view?lanes=x"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    let moved = node
        .call(
            Method::POST,
            &format!("{lanes}/{lane_issue}/move"),
            Some(&admin),
            Some(json!({})),
        )
        .await;
    assert_eq!(moved.status, StatusCode::NO_CONTENT, "{}", moved.body);
    let gone = node
        .call(
            Method::DELETE,
            &format!("{lanes}/{lane_issue}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::OK, "{}", gone.body);
    assert_eq!(gone.body["issues"].as_array().unwrap().len(), 2);
    // BRD-053: lanes without cards have titles from `labels`.
    let tag = node
        .call(
            Method::POST,
            "/api/v1/tags",
            Some(&admin),
            Some(json!({ "name": "triage" })),
        )
        .await;
    let tag_key = format!("t:{}", tag.body["id"].as_i64().unwrap());
    let tagged = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({
                "lanes": { "mode": "values", "attribute": "tags", "values": [tag_key] },
                "version": changed.body["version"],
            })),
        )
        .await;
    assert_eq!(tagged.status, StatusCode::OK, "{}", tagged.body);
    assert_eq!(tagged.body["lanes"]["labels"][0]["key"], tag_key);
    assert_eq!(tagged.body["lanes"]["labels"][0]["name"], "triage");
}

#[tokio::test(flavor = "multi_thread")]
async fn brd_059_card_settings_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "scrum", "name": "Scrum", "key": "SCR" })),
        )
        .await;
    let project_id = project.body["id"].as_i64().unwrap();
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "scrum", "name": "Sprints", "projects": [project_id] })),
        )
        .await;
    assert_eq!(board.status, StatusCode::CREATED, "{}", board.body);
    let id = board.body["id"].as_i64().unwrap();
    assert_eq!(board.body["cards"]["fields"].as_array().unwrap().len(), 3);
    assert_eq!(board.body["cards"]["color"], "field");
    assert!(board.body["health"].as_array().unwrap().is_empty());
    let estimation = board.body["cards"]["estimation"]["id"].as_i64().unwrap();
    let changed = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({
                "cards": {
                    "fields": [], "color": "project", "estimation": estimation,
                    "project_colors": [{ "project": project_id, "color": 4 }],
                },
                "version": board.body["version"],
            })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["cards"]["color"], "project");
    assert_eq!(changed.body["projects"][0]["color"], 4);
    let bad = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({
                "cards": { "fields": [], "color": "field" },
                "version": changed.body["version"],
            })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(view.status, StatusCode::OK, "{}", view.body);
    assert_eq!(view.body["totals"]["estimate"], 0.0);
    assert_eq!(view.body["columns"][0]["estimate"], 0.0);
    // Cell drafts (BRD-077): the same cell answers the kept draft.
    let cell = json!({
        "sprint": view.body["sprint"]["id"],
        "column": view.body["board"]["columns"][0]["id"],
    });
    let made = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/cell-draft"),
            Some(&admin),
            Some(cell.clone()),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let kept = node
        .call(
            Method::POST,
            &format!("/api/v1/boards/{id}/cell-draft"),
            Some(&admin),
            Some(cell),
        )
        .await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.body);
    assert_eq!(kept.body["issue"], made.body["issue"]);
    // Field usage (BRD-022).
    let usage = node
        .call(
            Method::GET,
            &format!("/api/v1/fields/{estimation}/boards"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(usage.status, StatusCode::OK, "{}", usage.body);
    let ours = usage.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["board"]["id"] == id)
        .unwrap();
    assert_eq!(ours["estimation"], true);
    let impact = node
        .call(
            Method::GET,
            &format!("/api/v1/fields/{estimation}/deletion-impact"),
            Some(&admin),
            None,
        )
        .await;
    assert!(
        impact.body["boards"].as_i64().unwrap() >= 1,
        "{}",
        impact.body
    );
}

/// FLD-062: a viewer who may not read the column or lane field gets neither its name nor its
/// values' names or IDs — not through the board answer, not through the view (1.12b #1).
#[tokio::test(flavor = "multi_thread")]
async fn fld_062_private_column_and_lane_fields_hidden() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let created = node
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({
                "full_name": "Carol Clark",
                "username": "carol",
                "email": "carol@example.com",
                "password": "carol password",
            })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let carol_id = created.body["id"].as_i64().unwrap();
    let carol = node
        .call(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "carol", "password": "carol password" })),
        )
        .await
        .session_cookie()
        .unwrap();
    let project = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "kanban", "name": "Flow", "key": "FLOW" })),
        )
        .await;
    assert_eq!(project.status, StatusCode::CREATED, "{}", project.body);
    let project_id = project.body["id"].as_i64().unwrap();
    // Carol reads the project's issues but not its private fields.
    let role = node
        .call(
            Method::POST,
            "/api/v1/roles",
            Some(&admin),
            Some(json!({ "name": "Readers", "permissions": ["project.basic", "issue.read"] })),
        )
        .await;
    assert_eq!(role.status, StatusCode::CREATED, "{}", role.body);
    let assigned = node
        .call(
            Method::POST,
            "/api/v1/role-assignments",
            Some(&admin),
            Some(json!({
                "role_id": role.body["id"],
                "user_id": carol_id,
                "projects": [project_id],
            })),
        )
        .await;
    assert_eq!(assigned.status, StatusCode::CREATED, "{}", assigned.body);

    // A lane field with two values, and a board with lanes by it.
    let attached = node
        .call(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/fields"),
            Some(&admin),
            Some(json!({ "source": "new", "name": "Customer", "kind": "enum" })),
        )
        .await;
    assert_eq!(attached.status, StatusCode::CREATED, "{}", attached.body);
    let set = attached.body["set"]["id"].as_i64().unwrap();
    let customer = attached.body["field"]["id"].as_i64().unwrap();
    let mut lane_values: Vec<i64> = Vec::new();
    for name in ["Acme", "Beta"] {
        let added = node
            .call(
                Method::POST,
                &format!("/api/v1/value-sets/{set}/values"),
                Some(&admin),
                Some(json!({ "name": name })),
            )
            .await;
        assert_eq!(added.status, StatusCode::OK, "{}", added.body);
        lane_values.push(
            added.body["values"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["name"] == name)
                .unwrap()["id"]
                .as_i64()
                .unwrap(),
        );
    }
    let board = node
        .call(
            Method::POST,
            "/api/v1/boards",
            Some(&admin),
            Some(json!({ "preset": "kanban", "name": "Flow board", "projects": [project_id] })),
        )
        .await;
    assert_eq!(board.status, StatusCode::CREATED, "{}", board.body);
    let id = board.body["id"].as_i64().unwrap();
    let state = board.body["column_field"]["id"].as_i64().unwrap();
    let lanes = node
        .call(
            Method::PATCH,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            Some(json!({
                "lanes": {
                    "mode": "values",
                    "field": customer,
                    "values": lane_values.iter().map(|v| format!("v:{v}")).collect::<Vec<_>>(),
                },
                "sharing": { "view": { "users": [carol_id] } },
                "version": board.body["version"],
            })),
        )
        .await;
    assert_eq!(lanes.status, StatusCode::OK, "{}", lanes.body);

    // Both fields go private.
    let fields = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/fields"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(fields.status, StatusCode::OK, "{}", fields.body);
    for field in [state, customer] {
        let row = fields.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["field"]["id"].as_i64() == Some(field))
            .unwrap();
        let private = node
            .call(
                Method::PATCH,
                &format!("/api/v1/projects/{project_id}/fields/{field}"),
                Some(&admin),
                Some(json!({ "private": true, "version": row["version"] })),
            )
            .await;
        assert_eq!(private.status, StatusCode::OK, "{}", private.body);
    }

    // The viewer still sees the board — but none of the private fields' names, values or IDs.
    let seen = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(seen.status, StatusCode::OK, "{}", seen.body);
    assert_eq!(seen.body["column_field"], json!(null));
    assert_eq!(seen.body["columns"], json!([]));
    assert_eq!(seen.body["lanes"]["mode"], "none");
    assert_eq!(seen.body["lanes"]["field"], json!(null));
    assert_eq!(seen.body["lanes"]["values"], json!([]));
    assert_eq!(seen.body["lanes"]["labels"], json!([]));
    assert_eq!(seen.body["lanes"]["new_card_value"], json!(null));
    let seen_text = seen.body.to_string();
    assert!(!seen_text.contains("Acme") && !seen_text.contains("Beta"));

    // The view carries no lane keys of the private lane field either.
    let view = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}/view"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(view.status, StatusCode::OK, "{}", view.body);
    assert_eq!(view.body["board"]["columns"], json!([]));
    assert_eq!(view.body["lanes"], json!([]));
    let view_text = view.body.to_string();
    assert!(!view_text.contains("v:") && !view_text.contains("Acme"));

    // The admin still sees everything.
    let full = node
        .call(
            Method::GET,
            &format!("/api/v1/boards/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(full.status, StatusCode::OK, "{}", full.body);
    assert_eq!(full.body["column_field"]["id"], state);
    assert!(full.body["columns"].as_array().unwrap().len() >= 3);
    assert_eq!(full.body["lanes"]["mode"], "values");
    assert_eq!(
        full.body["lanes"]["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        lane_values
            .iter()
            .map(|v| format!("v:{v}"))
            .collect::<Vec<_>>()
    );
    assert!(
        full.body["lanes"]["labels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["name"] == "Acme")
    );
}
