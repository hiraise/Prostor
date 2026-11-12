//! Issues over HTTP (step 1.3): creating, readable and former IDs, fields, texts with their
//! version, moves with a preview, tags, stars and votes, history, and the field screens' impact
//! counts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use trackflow_server::throttle::Throttle;

mod common;

use common::*;

fn field<'a>(issue: &'a Value, name: &str) -> &'a Value {
    issue["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"]["name"] == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn iss_001_iss_003_iss_019_issues_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let call = |method: Method, path: String, body: Option<Value>| {
        let node = &node;
        let admin = admin.clone();
        async move { node.call(method, &path, Some(&admin), body).await }
    };
    let web = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
    )
    .await;
    assert_eq!(web.status, StatusCode::CREATED, "{}", web.body);
    let web_id = web.body["id"].as_i64().unwrap();
    let app = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "App", "key": "APP" })),
    )
    .await;
    let app_id = app.body["id"].as_i64().unwrap();

    // ISS-001: create; defaults fill fields.
    let created = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id, "summary": "Login fails", "description": "Steps" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let issue = &created.body["issue"];
    let id = issue["id"].as_i64().unwrap();
    assert_eq!(issue["readable_id"], "WEB-1");
    assert_eq!(issue["draft"], false);
    assert_eq!(field(issue, "Priority")["values"][0]["name"], "Normal");
    assert_eq!(issue["can"]["update"], true);
    // Times are RFC 3339 strings, as the schema says (they once went out as number arrays).
    for key in ["created_at", "updated_at"] {
        let text = issue[key].as_str().unwrap_or_default();
        assert!(
            time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
                .is_ok(),
            "{key}: {}",
            issue[key]
        );
    }

    // Missing summary: one field error.
    let bad = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id })),
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.body["fields"][0]["path"], "summary");

    // ISS-019: set a field; FLD-007 durations from text.
    let estimation = field(issue, "Estimation")["field"]["id"].as_i64().unwrap();
    let changed = call(
        Method::PUT,
        format!("/api/v1/issues/{id}/fields/{estimation}"),
        Some(json!({ "value": "2h" })),
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(field(&changed.body["issue"], "Estimation")["minutes"], 120);

    // ISS-051: the text version.
    let text_version = changed.body["issue"]["text_version"].as_i64().unwrap();
    let edited = call(
        Method::PATCH,
        format!("/api/v1/issues/{id}"),
        Some(json!({ "summary": "Sign-in fails", "text_version": text_version })),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    let stale = call(
        Method::PATCH,
        format!("/api/v1/issues/{id}"),
        Some(json!({ "summary": "Other", "text_version": text_version })),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);

    // ISS-025: preview, then move; ISS-003: the old ID still finds it.
    let preview = call(
        Method::GET,
        format!("/api/v1/issues/{id}/move-preview?project={app_id}"),
        None,
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.body);
    assert_eq!(preview.body["loses_access"], false);
    let moved = call(
        Method::POST,
        format!("/api/v1/issues/{id}/move"),
        Some(json!({ "project": app_id })),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(moved.body["issue"]["readable_id"], "APP-1");
    let by_old = call(Method::GET, "/api/v1/issues/web-1".into(), None).await;
    assert_eq!(by_old.status, StatusCode::OK);
    assert_eq!(by_old.body["readable_id"], "APP-1");
    let missing = call(Method::GET, "/api/v1/issues/WEB-77".into(), None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // Tags, star; votes are refused on one's own issue.
    let tag = call(
        Method::POST,
        "/api/v1/tags".into(),
        Some(json!({ "name": "later", "color": 2 })),
    )
    .await;
    assert_eq!(tag.status, StatusCode::CREATED, "{}", tag.body);
    let tag_id = tag.body["id"].as_i64().unwrap();
    let tagged = call(
        Method::PUT,
        format!("/api/v1/issues/{id}/tags/{tag_id}"),
        None,
    )
    .await;
    assert_eq!(tagged.status, StatusCode::OK);
    assert_eq!(tagged.body["tags"][0]["name"], "later");
    let star = call(Method::PUT, format!("/api/v1/issues/{id}/star"), None).await;
    assert_eq!(star.status, StatusCode::NO_CONTENT);
    let vote = call(Method::PUT, format!("/api/v1/issues/{id}/vote"), None).await;
    assert_eq!(vote.status, StatusCode::CONFLICT);
    let tags = call(Method::GET, "/api/v1/tags?q=lat".into(), None).await;
    assert_eq!(tags.body["items"][0]["id"], tag_id);
    let one = call(Method::GET, format!("/api/v1/tags/{tag_id}"), None).await;
    assert_eq!(one.body["name"], "later", "{}", one.body);
    let none = call(Method::GET, "/api/v1/tags/999999".into(), None).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);

    // HIST-001: the activity stream newest first, typed.
    let history = call(
        Method::GET,
        format!("/api/v1/issues/{id}/activity?limit=10"),
        None,
    )
    .await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    let kinds: Vec<&str> = history.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["tags", "project", "summary", "field", "created"]);
    let items = history.body["items"].as_array().unwrap();
    assert_eq!(items[0]["added"]["tags"][0]["name"], "later");
    assert_eq!(items[1]["removed"]["place"]["readable_id"], "WEB-1");
    assert_eq!(items[2]["added"]["texts"][0], "Sign-in fails");
    assert_eq!(items[3]["added"]["numbers"][0], 120.0);
    assert_eq!(items[3]["field"]["name"], "Estimation");
    assert_eq!(items[4]["origin"], "web");
    assert!(items.iter().all(|i| i["type"] == "change"));

    // List and count; drafts are apart.
    let draft = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": app_id, "draft": true })),
    )
    .await;
    assert_eq!(draft.status, StatusCode::CREATED, "{}", draft.body);
    assert_eq!(draft.body["readable_id"], Value::Null);
    let list = call(
        Method::GET,
        format!("/api/v1/issues?project={app_id}"),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert_eq!(list.body["items"].as_array().unwrap().len(), 1);
    let count = call(
        Method::GET,
        format!("/api/v1/issues/count?project={app_id}"),
        None,
    )
    .await;
    assert_eq!(count.body, json!({ "all": 1, "unresolved": 1 }));
    let drafts = call(Method::GET, "/api/v1/issues/drafts".into(), None).await;
    assert_eq!(drafts.body["items"].as_array().unwrap().len(), 1);

    // FLD-054: the impact before detaching a field.
    let impact = call(
        Method::GET,
        format!("/api/v1/projects/{app_id}/fields/{estimation}/detach-impact"),
        None,
    )
    .await;
    assert_eq!(impact.status, StatusCode::OK, "{}", impact.body);
    assert_eq!(impact.body["issues"], 1);
    let deleted = call(Method::DELETE, format!("/api/v1/issues/{id}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let gone = call(Method::GET, format!("/api/v1/issues/{id}"), None).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn iss_001_iss_011_fld_001_batch_options_and_drafts_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let call = |method: Method, path: String, body: Option<Value>| {
        let node = &node;
        let admin = admin.clone();
        async move { node.call(method, &path, Some(&admin), body).await }
    };
    let web = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
    )
    .await;
    let web_id = web.body["id"].as_i64().unwrap();
    let mut ids = Vec::new();
    for summary in ["One", "Two"] {
        let created = call(
            Method::POST,
            "/api/v1/issues".into(),
            Some(json!({ "project": web_id, "summary": summary })),
        )
        .await;
        ids.push(created.body["issue"]["id"].as_i64().unwrap());
    }
    // An empty draft may be created (ISS-010).
    let draft = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id, "summary": "", "draft": true })),
    )
    .await;
    assert_eq!(draft.status, StatusCode::CREATED, "{}", draft.body);
    let draft_id = draft.body["issue"]["id"].as_i64().unwrap();

    // ISS-001: several issues at once, in the order asked.
    let batch = call(
        Method::POST,
        "/api/v1/issues/batch".into(),
        Some(json!({ "ids": [ids[1], 999_999, draft_id, ids[0]] })),
    )
    .await;
    assert_eq!(batch.status, StatusCode::OK, "{}", batch.body);
    let got: Vec<i64> = batch.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect();
    assert_eq!(got, [ids[1], draft_id, ids[0]]);
    let none = call(
        Method::POST,
        "/api/v1/issues/batch".into(),
        Some(json!({ "ids": [] })),
    )
    .await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);
    assert_eq!(none.body["fields"][0]["path"], "ids");

    // FLD-001: options of a field.
    let priority = field(&batch.body["items"][0], "Priority")["field"]["id"]
        .as_i64()
        .unwrap();
    let options = call(
        Method::GET,
        format!(
            "/api/v1/issues/{}/fields/{priority}/options?q=nor&limit=5",
            ids[0]
        ),
        None,
    )
    .await;
    assert_eq!(options.status, StatusCode::OK, "{}", options.body);
    assert_eq!(options.body["values"][0]["name"], "Normal");
    assert_eq!(options.body["can_add_value"], true);
    assert_eq!(options.body["more"], false);
    // FLD-047: the release of versions; the set takes a value added from the picker.
    assert_eq!(options.body["values"][0]["released"], false);
    assert!(options.body["values"][0]["release_date"].is_null());
    assert!(options.body["set"].is_i64());
    let too_many = call(
        Method::GET,
        format!(
            "/api/v1/issues/{}/fields/{priority}/options?limit=51",
            ids[0]
        ),
        None,
    )
    .await;
    assert_eq!(too_many.status, StatusCode::BAD_REQUEST);
    let missing = call(
        Method::GET,
        format!("/api/v1/issues/{}/fields/999999/options", ids[0]),
        None,
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // ISS-011: delete all drafts.
    let deleted = call(Method::DELETE, "/api/v1/issues/drafts".into(), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let drafts = call(Method::GET, "/api/v1/issues/drafts".into(), None).await;
    assert_eq!(drafts.body["items"], json!([]));
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn iss_001_page_matches_the_four_answers() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let call = |method: Method, path: String, body: Option<Value>| {
        let node = &node;
        let admin = admin.clone();
        async move { node.call(method, &path, Some(&admin), body).await }
    };
    let types = call(Method::GET, "/api/v1/link-types".into(), None).await;
    let subtask = types.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["builtin"] == "subtask")
        .unwrap()["id"]
        .clone();
    let web = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
    )
    .await;
    let epic = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web.body["id"], "summary": "Epic" })),
    )
    .await;
    let issue = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({
            "project": web.body["id"],
            "summary": "Login fails",
            "description": "See WEB-1",
            "links": [{ "link_type": subtask, "side": "target", "issue": epic.body["issue"]["id"] }],
        })),
    )
    .await;
    assert_eq!(issue.status, StatusCode::CREATED, "{}", issue.body);
    let id = issue.body["issue"]["id"].as_i64().unwrap();
    let posted = call(
        Method::POST,
        format!("/api/v1/issues/{id}/comments"),
        Some(json!({ "text": "Thanks, @admin" })),
    )
    .await;
    assert_eq!(posted.status, StatusCode::CREATED, "{}", posted.body);
    let (status, _, body) = node
        .raw(
            Method::POST,
            "/api/v1/uploads?name=notes.txt",
            Some(&admin),
            &[(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
            b"notes".to_vec(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let upload: Value = serde_json::from_slice(&body).unwrap();
    let attached = call(
        Method::POST,
        format!("/api/v1/issues/{id}/attachments"),
        Some(json!({ "uploads": [upload["id"]] })),
    )
    .await;
    assert_eq!(attached.status, StatusCode::CREATED, "{}", attached.body);

    let page = call(
        Method::GET,
        format!("/api/v1/issues/{id}/page?limit=10&changes=false"),
        None,
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let one = call(Method::GET, format!("/api/v1/issues/{id}"), None).await;
    let activity = call(
        Method::GET,
        format!("/api/v1/issues/{id}/activity?limit=10&changes=false"),
        None,
    )
    .await;
    let links = call(Method::GET, format!("/api/v1/issues/{id}/links"), None).await;
    let files = call(
        Method::GET,
        format!("/api/v1/issues/{id}/attachments"),
        None,
    )
    .await;
    assert_eq!(page.body["issue"], one.body);
    assert_eq!(page.body["activity"], activity.body);
    assert_eq!(page.body["links"], links.body);
    assert_eq!(page.body["attachments"], files.body);
    assert_eq!(page.body["issue"]["parent"]["readable_id"], "WEB-1");
    assert_eq!(page.body["activity"]["items"][0]["type"], "comment");
    assert_eq!(page.body["links"]["groups"][0]["total"], 1);
    assert_eq!(
        page.body["attachments"]["attachments"][0]["name"],
        "notes.txt"
    );

    // The activity's parameters are checked as on their own endpoint; other issues are not found.
    let neither = call(
        Method::GET,
        format!("/api/v1/issues/{id}/page?changes=false&comments=false"),
        None,
    )
    .await;
    assert_eq!(neither.status, StatusCode::BAD_REQUEST);
    let by_key = call(Method::GET, "/api/v1/issues/WEB-2/page".into(), None).await;
    assert_eq!(by_key.body["issue"]["id"], id);
    let missing = call(Method::GET, "/api/v1/issues/999999/page".into(), None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
