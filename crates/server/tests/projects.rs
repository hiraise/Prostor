//! Projects and fields over HTTP (step 1.2): creating from a template, keys and the lookup by a
//! former key, favorites, the people page, the fields of a project, value sets and the patch rules
//! of `null`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use trackflow_server::throttle::Throttle;

mod common;

use common::*;

fn names(list: &Value, key: &str) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item[key].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn prj_017_prj_005_prj_012_projects_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;

    // PRJ-018: the wizard checks before creating.
    let check = node
        .call(
            Method::GET,
            "/api/v1/projects/check?name=Website%20redesign&key=1x",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(check.status, StatusCode::OK, "{}", check.body);
    assert_eq!(check.body["suggested_key"], "WR");
    assert_eq!(check.body["fields"][0]["path"], "key");
    assert_eq!(check.body["fields"][0]["code"], "invalid");

    // PRJ-017: create from a template.
    let created = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "scrum", "name": "Website redesign" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["key"], "WR");
    assert_eq!(created.body["template"], "scrum");
    assert_eq!(created.body["team_size"], 1);
    assert_eq!(created.body["team"]["team_of"], created.body["id"]);
    let id = created.body["id"].as_i64().unwrap();
    let version = created.body["version"].as_i64().unwrap();

    // PRJ-005: a new key; the old one still finds the project.
    let changed = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}"),
            Some(&admin),
            Some(json!({ "key": "web", "version": version })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["former_keys"], json!(["WR"]));
    let old = node
        .call(
            Method::GET,
            "/api/v1/projects/by-key/wr",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(old.status, StatusCode::OK);
    assert_eq!(old.body["key"], "WEB");

    // PRJ-039: `null` clears the default audience, absence keeps it.
    let team = changed.body["team"]["id"].as_i64().unwrap();
    let version = changed.body["version"].as_i64().unwrap();
    let with_audience = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}"),
            Some(&admin),
            Some(json!({ "default_audience_id": team, "version": version })),
        )
        .await;
    assert_eq!(with_audience.body["default_audience"]["id"], team);
    let version = with_audience.body["version"].as_i64().unwrap();
    let kept = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}"),
            Some(&admin),
            Some(json!({ "description": "Our site", "version": version })),
        )
        .await;
    assert_eq!(kept.body["default_audience"]["id"], team);
    let version = kept.body["version"].as_i64().unwrap();
    let cleared = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}"),
            Some(&admin),
            Some(json!({ "default_audience_id": null, "version": version })),
        )
        .await;
    assert_eq!(cleared.body["default_audience"], Value::Null);

    // PRJ-012: stars.
    let starred = node
        .call(
            Method::PUT,
            &format!("/api/v1/projects/{id}/favorite"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(starred.status, StatusCode::NO_CONTENT);
    let list = node
        .call(Method::GET, "/api/v1/projects", Some(&admin), None)
        .await;
    assert_eq!(list.body["items"][0]["favorite"], true);

    // PRJ-046: the people page.
    let people = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{id}/people"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(people.status, StatusCode::OK, "{}", people.body);
    assert_eq!(people.body["owner"]["username"], "admin");
    assert_eq!(people.body["assignments"].as_array().unwrap().len(), 2);
    let users = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{id}/team/users"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(names(&users.body["items"], "username"), ["admin"]);

    // ACC-022: teams appear in the group list on request.
    let teams = node
        .call(Method::GET, "/api/v1/groups?kind=teams", Some(&admin), None)
        .await;
    assert_eq!(names(&teams.body["items"], "name"), ["Website redesign"]);
    let groups = node
        .call(Method::GET, "/api/v1/groups", Some(&admin), None)
        .await;
    assert!(!names(&groups.body["items"], "name").contains(&"Website redesign".to_owned()));

    // PRJ-064: deletion is accepted and runs in the background.
    let deleted = node
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::ACCEPTED);
    let gone = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    let deleting = node
        .call(
            Method::GET,
            "/api/v1/projects?section=deleting",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(deleting.body["items"][0]["deletion"]["state"], "queued");
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn fld_051_fld_032_fields_and_values_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let created = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "blank", "name": "Shop", "key": "SHOP" })),
        )
        .await;
    let id = created.body["id"].as_i64().unwrap();
    let fields = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{id}/fields"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(fields.status, StatusCode::OK, "{}", fields.body);
    let items = fields.body["items"].as_array().unwrap();
    assert_eq!(items[0]["field"]["builtin"], "priority");
    assert_eq!(items[0]["default_values"][0]["name"], "Normal");
    assert_eq!(items[3]["field"]["kind"], "user");

    // FLD-052: a new field from the project.
    let attached = node
        .call(
            Method::POST,
            &format!("/api/v1/projects/{id}/fields"),
            Some(&admin),
            Some(json!({ "source": "new", "name": "Customer", "kind": "enum" })),
        )
        .await;
    assert_eq!(attached.status, StatusCode::CREATED, "{}", attached.body);
    let set = attached.body["set"]["id"].as_i64().unwrap();
    let field = attached.body["field"]["id"].as_i64().unwrap();
    // FLD-032: values; FLD-047 dates travel as calendar dates.
    let added = node
        .call(
            Method::POST,
            &format!("/api/v1/value-sets/{set}/values"),
            Some(&admin),
            Some(json!({ "name": "Acme", "color": 3 })),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    let value = &added.body["values"][0];
    assert_eq!(value["color"], 3);
    let value_id = value["id"].as_i64().unwrap();
    let value_version = value["version"].as_i64().unwrap();
    // `null` clears the color.
    let cleared = node
        .call(
            Method::PATCH,
            &format!("/api/v1/value-sets/{set}/values/{value_id}"),
            Some(&admin),
            Some(json!({ "color": null, "version": value_version })),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.body);
    assert_eq!(cleared.body["values"][0]["color"], Value::Null);
    // FLD-055: the value becomes the default.
    let version = attached.body["version"].as_i64().unwrap();
    let defaulted = node
        .call(
            Method::PATCH,
            &format!("/api/v1/projects/{id}/fields/{field}"),
            Some(&admin),
            Some(
                json!({ "default_values": [value_id], "can_be_empty": false, "version": version }),
            ),
        )
        .await;
    assert_eq!(defaulted.status, StatusCode::OK, "{}", defaulted.body);
    assert_eq!(defaulted.body["default_values"][0]["name"], "Acme");
    assert_eq!(defaulted.body["required"], false);
    // FLD-058: order.
    let moved = node
        .call(
            Method::POST,
            &format!("/api/v1/projects/{id}/fields/{field}/move"),
            Some(&admin),
            Some(json!({ "after": null })),
        )
        .await;
    assert_eq!(moved.body["items"][0]["field"]["name"], "Customer");
    // FLD-025: the instance lists of fields and sets.
    let all = node
        .call(Method::GET, "/api/v1/fields?q=cust", Some(&admin), None)
        .await;
    assert_eq!(names(&all.body["items"], "name"), ["Customer"]);
    assert_eq!(all.body["items"][0]["project_count"], 1);
    let sets = node
        .call(
            Method::GET,
            "/api/v1/value-sets?kind=enum&q=shop",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(names(&sets.body["items"], "name"), ["Shop: Customer"]);
    // FLD-054: detach.
    let detached = node
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{id}/fields/{field}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(detached.status, StatusCode::NO_CONTENT);
    // TIME-005: the client learns the work schedule.
    let bootstrap = node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(bootstrap.body["work_schedule"]["day_minutes"], 480);
    node.stop().await;
}
