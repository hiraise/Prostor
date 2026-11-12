//! Files over HTTP (step 1.4b): raw uploads with their limit and origin check, attachments, signed
//! links with the serving headers and ranges, avatars and the bootstrap.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};
use trackflow_server::throttle::Throttle;

mod common;

use common::*;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn header(headers: &axum::http::HeaderMap, name: header::HeaderName) -> &str {
    headers
        .get(name)
        .map(|v| v.to_str().unwrap())
        .unwrap_or_default()
}

async fn upload(node: &Node, cookie: &str, name: &str, bytes: &[u8]) -> Value {
    let (status, _, body) = node
        .raw(
            Method::POST,
            &format!("/api/v1/uploads?name={name}"),
            Some(cookie),
            &[(header::CONTENT_TYPE, "application/octet-stream")],
            bytes.to_vec(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn cnt_017_cnt_019_cnt_026_files_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let call = |method: Method, path: String, body: Option<Value>| {
        let node = &node;
        let admin = admin.clone();
        async move { node.call(method, &path, Some(&admin), body).await }
    };
    // The bootstrap gives the limit in force: the setting within the deployment's ceiling.
    let bootstrap = call(Method::GET, "/api/v1/bootstrap".into(), None).await;
    assert_eq!(bootstrap.body["limits"]["upload_max_bytes"], UPLOAD_CEILING);
    assert_eq!(bootstrap.body["gravatar"], false);

    let web = call(
        Method::POST,
        "/api/v1/projects".into(),
        Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
    )
    .await;
    let web_id = web.body["id"].as_i64().unwrap();
    let issue = call(
        Method::POST,
        "/api/v1/issues".into(),
        Some(json!({ "project": web_id, "summary": "Login fails" })),
    )
    .await;
    let issue_id = issue.body["issue"]["id"].as_i64().unwrap();

    // Too large: refused by the declared length and while streaming.
    let big = vec![0u8; usize::try_from(UPLOAD_CEILING).unwrap() + 1];
    for headers in [vec![(header::CONTENT_LENGTH, "65537")], vec![]] {
        let (status, _, body) = node
            .raw(
                Method::POST,
                "/api/v1/uploads?name=big.bin",
                Some(&admin),
                &headers,
                big.clone(),
            )
            .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "payload_too_large");
    }
    // Pages of other sites cannot upload with the session.
    let (status, _, _) = node
        .raw(
            Method::POST,
            "/api/v1/uploads?name=x.txt",
            Some(&admin),
            &[(header::ORIGIN, "https://evil.example.com")],
            b"x".to_vec(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let shot = upload(&node, &admin, "shot.png", PNG).await;
    assert_eq!(shot["kind"], "image");
    assert_eq!(shot["content_type"], "image/png");
    let notes = upload(
        &node,
        &admin,
        "my%20notes.html",
        b"<script>alert(1)</script>",
    )
    .await;
    assert_eq!(notes["name"], "my notes.html");
    assert_eq!(notes["content_type"], "application/octet-stream");
    let attached = call(
        Method::POST,
        format!("/api/v1/issues/{issue_id}/attachments"),
        Some(json!({ "uploads": [shot["id"], notes["id"]] })),
    )
    .await;
    assert_eq!(attached.status, StatusCode::CREATED, "{}", attached.body);
    let files = attached.body["attachments"].as_array().unwrap().clone();
    let picture_url = files[0]["url"].as_str().unwrap().to_owned();
    let notes_url = files[1]["url"].as_str().unwrap().to_owned();
    assert!(picture_url.starts_with(&format!("/api/v1/files/{}/shot.png?", files[0]["id"])));

    // A picture opens inline without a session, sandboxed, cached privately until the expiry.
    let (status, headers, body) = node.raw(Method::GET, &picture_url, None, &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, PNG);
    assert_eq!(header(&headers, header::CONTENT_TYPE), "image/png");
    assert_eq!(
        header(&headers, header::CONTENT_DISPOSITION),
        "inline; filename=\"shot.png\"; filename*=UTF-8''shot.png"
    );
    assert_eq!(header(&headers, header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert!(header(&headers, header::CONTENT_SECURITY_POLICY).ends_with("; sandbox"));
    assert!(header(&headers, header::CACHE_CONTROL).starts_with("private, max-age="));
    // Ranges for players.
    let (status, headers, body) = node
        .raw(
            Method::GET,
            &picture_url,
            None,
            &[(header::RANGE, "bytes=0-7")],
            vec![],
        )
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, &PNG[..8]);
    assert!(header(&headers, header::CONTENT_RANGE).starts_with("bytes 0-7/"));
    // Markup downloads instead of running.
    let (status, headers, _) = node.raw(Method::GET, &notes_url, None, &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, header::CONTENT_TYPE),
        "application/octet-stream"
    );
    assert_eq!(
        header(&headers, header::CONTENT_DISPOSITION),
        "attachment; filename=\"my notes.html\"; filename*=UTF-8''my%20notes.html"
    );
    // A changed signature or another person: 404.
    let tampered = picture_url.replace("&s=", "&s=AA");
    let (status, _, _) = node.raw(Method::GET, &tampered, None, &[], vec![]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Files posted with a comment come with it.
    let log = upload(&node, &admin, "log.txt", b"line").await;
    let posted = call(
        Method::POST,
        format!("/api/v1/issues/{issue_id}/comments"),
        Some(json!({ "text": "See ![](shot.png)", "uploads": [log["id"]] })),
    )
    .await;
    assert_eq!(posted.status, StatusCode::CREATED, "{}", posted.body);
    assert_eq!(posted.body["comment"]["files"][0]["name"], "log.txt");
    assert_eq!(
        posted.body["comment"]["refs"]["files"][0]["name"],
        "shot.png"
    );
    assert_eq!(
        posted.body["comment"]["refs"]["files"][0]["url"],
        picture_url.as_str()
    );

    // Deleted: the link stops working.
    let deleted = call(
        Method::DELETE,
        format!("/api/v1/attachments/{}", files[0]["id"]),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let (status, _, _) = node.raw(Method::GET, &picture_url, None, &[], vec![]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let listed = call(
        Method::GET,
        format!("/api/v1/issues/{issue_id}/attachments"),
        None,
    )
    .await;
    let names: Vec<&str> = listed.body["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["my notes.html", "log.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn acc_017_prj_007_pictures_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let me = node
        .call(Method::GET, "/api/v1/users/me", Some(&admin), None)
        .await;
    let my_id = me.body["id"].as_i64().unwrap();
    assert_eq!(me.body["avatar_url"], Value::Null);
    let photo = upload(&node, &admin, "me.png", PNG).await;
    let set = node
        .call(
            Method::PUT,
            &format!("/api/v1/users/{my_id}/avatar"),
            Some(&admin),
            Some(json!({ "upload": photo["id"] })),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.body);
    let url = set.body["url"].as_str().unwrap().to_owned();
    let me = node
        .call(Method::GET, "/api/v1/users/me", Some(&admin), None)
        .await;
    assert_eq!(me.body["avatar_url"], url.as_str());
    let (status, headers, _) = node.raw(Method::GET, &url, Some(&admin), &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, header::CACHE_CONTROL),
        "private, max-age=31536000, immutable"
    );
    // Pictures need a session.
    let (status, _, _) = node.raw(Method::GET, &url, None, &[], vec![]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // The import is off on this node.
    let gravatar = node
        .call(
            Method::POST,
            &format!("/api/v1/users/{my_id}/avatar/gravatar"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(gravatar.status, StatusCode::NOT_FOUND);
    let reset = node
        .call(
            Method::DELETE,
            &format!("/api/v1/users/{my_id}/avatar"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(reset.status, StatusCode::NO_CONTENT);
    let (status, _, _) = node.raw(Method::GET, &url, Some(&admin), &[], vec![]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let web = node
        .call(
            Method::POST,
            "/api/v1/projects",
            Some(&admin),
            Some(json!({ "template": "blank", "name": "Web", "key": "WEB" })),
        )
        .await;
    let web_id = web.body["id"].as_i64().unwrap();
    let logo = upload(&node, &admin, "logo.png", PNG).await;
    let set = node
        .call(
            Method::PUT,
            &format!("/api/v1/projects/{web_id}/logo"),
            Some(&admin),
            Some(json!({ "upload": logo["id"] })),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.body);
    let project = node
        .call(
            Method::GET,
            &format!("/api/v1/projects/{web_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(project.body["logo_url"], set.body["url"]);
    let (status, headers, _) = node
        .raw(
            Method::GET,
            set.body["url"].as_str().unwrap(),
            Some(&admin),
            &[],
            vec![],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, header::CONTENT_TYPE), "image/png");
}
