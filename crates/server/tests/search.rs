//! Searching issues over HTTP: `GET /issues`, the counts, the query check and `invalid_query`.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use trackflow_server::throttle::Throttle;

use common::*;

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn qry_001_qry_062_qry_092_search_over_http() {
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
    let mut ids = Vec::new();
    for summary in ["Login fails", "Signup is slow", "Footer typo"] {
        let created = call(
            Method::POST,
            "/api/v1/issues".into(),
            Some(json!({ "project": web_id, "summary": summary })),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        ids.push(created.body["issue"]["id"].as_i64().unwrap());
    }
    // QRY-078: the fields an issue list may show.
    let fields = call(
        Method::GET,
        format!("/api/v1/issues/fields?project={web_id}"),
        None,
    )
    .await;
    assert_eq!(fields.status, StatusCode::OK, "{}", fields.body);
    assert!(
        fields.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "Priority")
    );
    let none = call(
        Method::GET,
        "/api/v1/issues/fields?project=999999".into(),
        None,
    )
    .await;
    assert_eq!(none.status, StatusCode::NOT_FOUND, "{}", none.body);
    let search = |q: &str, extra: &str| format!("/api/v1/issues?q={}{extra}", encode(q));

    // QRY-001: a query, its canonical text and the order used.
    let page = call(
        Method::GET,
        search("project:WEB  priority: normal sort: id", "&limit=2"),
        None,
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let got: Vec<i64> = page.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect();
    assert_eq!(got, [ids[2], ids[1]]);
    assert_eq!(page.body["query"], "project: WEB priority: normal sort: id");
    assert_eq!(
        page.body["sort"],
        json!([{ "key": "id", "field": null, "direction": "desc" }])
    );
    assert_eq!(page.body["single_id"], Value::Null);
    // The cursor continues the same query; another query refuses it.
    let cursor = page.body["next_cursor"].as_str().unwrap().to_owned();
    let next = call(
        Method::GET,
        search(
            "project:WEB  priority: normal sort: id",
            &format!("&limit=2&cursor={cursor}"),
        ),
        None,
    )
    .await;
    assert_eq!(next.body["items"][0]["id"], ids[0]);
    assert_eq!(next.body["next_cursor"], Value::Null);
    let other = call(
        Method::GET,
        search("project: WEB", &format!("&cursor={cursor}")),
        None,
    )
    .await;
    assert_eq!(other.status, StatusCode::BAD_REQUEST);
    assert_eq!(other.body["fields"][0]["path"], "cursor");

    // Without a query: every issue, most recently updated first; `project` alone still works.
    let all = call(
        Method::GET,
        format!("/api/v1/issues?project={web_id}"),
        None,
    )
    .await;
    assert_eq!(all.body["items"].as_array().unwrap().len(), 3);
    assert_eq!(all.body["sort"][0]["key"], "updated");
    // QRY-016: one readable ID.
    let one = call(Method::GET, search("web-2", ""), None).await;
    assert_eq!(one.body["single_id"], ids[1], "{}", one.body);

    // QRY-062: the same in Russian.
    let ru = call(Method::GET, search("автор: я это: открыта", ""), None).await;
    assert_eq!(ru.status, StatusCode::OK, "{}", ru.body);
    assert_eq!(ru.body["items"].as_array().unwrap().len(), 3);

    // QRY-092: a count and a batch; an invalid query gets its problems in its place.
    let count = call(
        Method::GET,
        format!(
            "/api/v1/issues/count?q={}",
            encode("summary: x or priority: nosuch")
        ),
        None,
    )
    .await;
    assert_eq!(count.status, StatusCode::BAD_REQUEST, "{}", count.body);
    let count = call(
        Method::GET,
        format!("/api/v1/issues/count?q={}", encode("is: unresolved")),
        None,
    )
    .await;
    assert_eq!(count.body, json!({ "all": 3, "unresolved": 3 }));
    let batch = call(
        Method::POST,
        "/api/v1/issues/counts".into(),
        Some(json!({ "queries": [
            { "q": "project: WEB" },
            { "q": "priority: nosuch" },
            { "q": "", "project": web_id },
        ] })),
    )
    .await;
    assert_eq!(batch.status, StatusCode::OK, "{}", batch.body);
    let items = batch.body["items"].as_array().unwrap();
    assert_eq!(
        (items[0]["all"].clone(), items[0]["unresolved"].clone()),
        (json!(3), json!(3))
    );
    assert_eq!(items[1]["all"], Value::Null);
    assert_eq!(items[1]["problems"][0]["code"], "unknown_value");
    assert_eq!(items[2]["all"], 3);
    let too_many = call(
        Method::POST,
        "/api/v1/issues/counts".into(),
        Some(json!({ "queries": vec![json!({ "q": "" }); 51] })),
    )
    .await;
    assert_eq!(too_many.status, StatusCode::BAD_REQUEST);

    // `invalid_query`: problems at UTF-16 offsets, repeated as field errors on `q`.
    let bad = call(Method::GET, search("статус: 🙂 or (", ""), None).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.body);
    assert_eq!(bad.body["code"], "invalid_query");
    assert_eq!(
        bad.body["problems"],
        json!([{ "code": "unclosed_group", "from": 14, "to": 15, "severity": "error" }]),
    );
    assert_eq!(bad.body["fields"][0]["path"], "q");
    assert_eq!(bad.body["fields"][0]["code"], "unclosed_group");
    let unknown = call(Method::GET, search("статус: 🙂", ""), None).await;
    assert_eq!(
        unknown.body["problems"],
        json!([{ "code": "unknown_attribute", "from": 0, "to": 6, "severity": "error" }]),
    );
    // No such project.
    let missing = call(Method::GET, "/api/v1/issues?project=999999".into(), None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // The check: classes with offsets, unknown names marked.
    let check = call(
        Method::GET,
        format!("/api/v1/queries/check?q={}", encode("автор: я цвет: red")),
        None,
    )
    .await;
    assert_eq!(check.status, StatusCode::OK, "{}", check.body);
    let tokens = check.body["tokens"].as_array().unwrap();
    assert_eq!(
        tokens[0],
        json!({ "from": 0, "to": 5, "class": "attribute" })
    );
    assert!(
        tokens
            .iter()
            .any(|t| t["from"] == 9 && t["class"] == "error"),
        "{}",
        check.body
    );
    assert_eq!(check.body["problems"][0]["code"], "unknown_attribute");
    assert_eq!(check.body["problems"][0]["from"], 9);

    node.stop().await;
}
