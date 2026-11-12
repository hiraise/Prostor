//! Saved searches and the sharing of tags over HTTP (QRY-063…QRY-068, ISS-037, ACC-066).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use trackflow_server::throttle::Throttle;

use common::*;

fn names(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_063_qry_065_acc_066_saved_searches_and_shared_tags_over_http() {
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
    let signed_in = node
        .call(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "carol", "password": "carol password" })),
        )
        .await;
    let carol = signed_in.session_cookie().unwrap();
    let web = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
        )
        .await;
    let web_id = web.body["id"].as_i64().unwrap();
    let issue = node
        .call(
            Method::POST,
            "/api/v1/issues",
            Some(&admin),
            Some(json!({ "project": web_id, "summary": "Login fails" })),
        )
        .await;
    assert_eq!(issue.status, StatusCode::CREATED, "{}", issue.body);

    // QRY-063: a query under a name; one with errors is refused with its problems.
    let bad = node
        .call(
            Method::POST,
            "/api/v1/saved-searches",
            Some(&admin),
            Some(json!({ "name": "Broken", "query": "project: (WEB" })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    assert_eq!(bad.code(), "invalid_query");
    let saved = node
        .call(
            Method::POST,
            "/api/v1/saved-searches",
            Some(&admin),
            Some(json!({ "name": "Web work", "query": "project:WEB  sort: id" })),
        )
        .await;
    assert_eq!(saved.status, StatusCode::CREATED, "{}", saved.body);
    let id = saved.body["id"].as_i64().unwrap();
    assert_eq!(saved.body["query"], "project: WEB sort: id");
    assert_eq!(saved.body["owner"]["username"], "admin");
    assert_eq!(saved.body["builtin"], Value::Null);
    assert_eq!(
        saved.body["abilities"],
        json!({ "apply": false, "edit": true, "delete": true, "share": true, "transfer": true })
    );
    assert_eq!(
        saved.body["sharing"],
        json!({ "view": { "users": [], "groups": [] }, "edit": { "users": [], "groups": [] } })
    );

    // QRY-068: the predefined searches come with the first listing; the project's "unassigned"
    // search (PRJ-019) is there too.
    let mine = node
        .call(Method::GET, "/api/v1/saved-searches", Some(&admin), None)
        .await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.body);
    assert!(names(&mine.body).contains(&"Web work"));
    let builtins = mine.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| !s["builtin"].is_null())
        .count();
    assert_eq!(builtins, 4);

    // Carol knows nothing of it until it is shared.
    let hidden = node
        .call(
            Method::GET,
            &format!("/api/v1/saved-searches/{id}"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);
    let shared = node
        .call(
            Method::PATCH,
            &format!("/api/v1/saved-searches/{id}"),
            Some(&admin),
            Some(json!({
                "sharing": { "view": { "users": [carol_id] } },
                "version": saved.body["version"],
            })),
        )
        .await;
    assert_eq!(shared.status, StatusCode::OK, "{}", shared.body);
    assert_eq!(shared.body["sharing"]["view"]["users"][0]["id"], carol_id);

    // QRY-065: Carol sees it among the shared ones and runs it, but sees no audiences and may not
    // change them.
    let theirs = node
        .call(
            Method::GET,
            "/api/v1/saved-searches?scope=shared",
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(names(&theirs.body), ["Web work"]);
    assert_eq!(theirs.body["items"][0]["sharing"], Value::Null);
    assert_eq!(
        theirs.body["items"][0]["abilities"],
        json!({ "apply": false, "edit": false, "delete": false, "share": false, "transfer": false })
    );
    let run = node
        .call(
            Method::GET,
            &format!("/api/v1/issues?saved_search={id}"),
            Some(&carol),
            None,
        )
        .await;
    // As Carol sees it (QRY-069): she is not in the WEB team, so the name means nothing to her.
    assert_eq!(run.status, StatusCode::BAD_REQUEST, "{}", run.body);
    assert_eq!(run.body["problems"][0]["code"], "unknown_value");
    let own = node
        .call(
            Method::GET,
            &format!("/api/v1/issues?saved_search={id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(own.status, StatusCode::OK, "{}", own.body);
    assert_eq!(own.body["items"].as_array().unwrap().len(), 1);
    let refused = node
        .call(
            Method::PATCH,
            &format!("/api/v1/saved-searches/{id}"),
            Some(&carol),
            Some(json!({ "sharing": {}, "version": shared.body["version"] })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    assert_eq!(refused.code(), "forbidden");
    let not_deleted = node
        .call(
            Method::DELETE,
            &format!("/api/v1/saved-searches/{id}"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(not_deleted.status, StatusCode::FORBIDDEN);

    // ISS-037: a tag shared for use; `GET /tags?scope=` lists it for Carol.
    let tag = node
        .call(
            Method::POST,
            "/api/v1/tags",
            Some(&admin),
            Some(json!({ "name": "triage" })),
        )
        .await;
    assert_eq!(tag.status, StatusCode::CREATED, "{}", tag.body);
    assert_eq!(tag.body["owner"]["username"], "admin");
    let tag_id = tag.body["id"].as_i64().unwrap();
    let tag_shared = node
        .call(
            Method::PATCH,
            &format!("/api/v1/tags/{tag_id}"),
            Some(&admin),
            Some(json!({
                "sharing": { "view": { "users": [carol_id] }, "use": { "users": [carol_id] } },
                "version": tag.body["version"],
            })),
        )
        .await;
    assert_eq!(tag_shared.status, StatusCode::OK, "{}", tag_shared.body);
    assert_eq!(
        tag_shared.body["sharing"]["use"]["users"][0]["id"],
        carol_id
    );
    let carols_own = node
        .call(Method::GET, "/api/v1/tags", Some(&carol), None)
        .await;
    assert_eq!(names(&carols_own.body), Vec::<&str>::new());
    let carols_all = node
        .call(Method::GET, "/api/v1/tags?scope=all", Some(&carol), None)
        .await;
    assert_eq!(names(&carols_all.body), ["triage"]);
    assert_eq!(carols_all.body["items"][0]["sharing"], Value::Null);
    // ISS-035: the tag picker offers only tags the caller may put on issues.
    assert_eq!(carols_all.body["items"][0]["abilities"]["apply"], true);
    assert_eq!(tag.body["abilities"]["apply"], true);

    // QRY-064: the owner deletes it.
    let deleted = node
        .call(
            Method::DELETE,
            &format!("/api/v1/saved-searches/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    node.stop().await;
}
