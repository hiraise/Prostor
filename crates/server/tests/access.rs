//! Access over HTTP (step 1.1): the access map, a required password change, refusals that name
//! missing permissions, token scopes, groups, roles and assignments, and the emailed links.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{Method, StatusCode};
use prostor_server::throttle::Throttle;
use serde_json::{Value, json};

mod common;

use common::*;

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn acc_062_auth_010_the_access_map_and_a_required_password_change() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;

    let map = node
        .call(Method::GET, "/api/v1/users/me/access", Some(&admin), None)
        .await;
    assert_eq!(map.status, StatusCode::OK, "{}", map.body);
    assert!(keys(&map.body["global"]).contains(&"system.write"));
    assert_eq!(map.body["capabilities"], json!(["share_views"]));

    // ACC-003: an administrator creates an account that must change its password.
    let created = node
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({
                "full_name": "Carol Clark",
                "username": "carol",
                "email": "carol@example.com",
                "password": "first password",
                "must_change_password": true,
            })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["tier"], "full");
    assert_eq!(created.body["password"]["must_change"], true);
    assert_eq!(created.body["email_verified"], false);
    let carol_id = created.body["id"].as_i64().unwrap();

    let signed_in = node
        .call(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "carol", "password": "first password" })),
        )
        .await;
    assert_eq!(signed_in.status, StatusCode::CREATED);
    assert_eq!(signed_in.body["password_change_required"], true);
    let carol = signed_in.session_cookie().unwrap();
    // AUTH-010: nothing but the session and the password change until then.
    let blocked = node
        .call(Method::GET, "/api/v1/users/me", Some(&carol), None)
        .await;
    assert_eq!(blocked.status, StatusCode::FORBIDDEN);
    assert_eq!(blocked.code(), "password_change_required");
    let session = node
        .call(Method::GET, "/api/v1/session", Some(&carol), None)
        .await;
    assert_eq!(session.status, StatusCode::OK);
    assert_eq!(session.body["password_change_required"], true);
    let changed = node
        .call(
            Method::POST,
            "/api/v1/users/me/password",
            Some(&carol),
            Some(json!({ "current_password": "first password", "new_password": "carol's own" })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{}", changed.body);
    let me = node
        .call(Method::GET, "/api/v1/users/me", Some(&carol), None)
        .await;
    assert_eq!(me.status, StatusCode::OK);

    // ACC-002, ACC-015: Carol sees the directory with emails (All members are directory viewers).
    let directory = node
        .call(Method::GET, "/api/v1/users?q=anna", Some(&carol), None)
        .await;
    assert_eq!(directory.status, StatusCode::OK, "{}", directory.body);
    assert_eq!(directory.body["items"][0]["email"], "anna@example.com");
    assert_eq!(directory.body["items"][0]["full_profile"], false);
    // ACC-050: refusals name what is missing.
    let refused = node
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&carol),
            Some(json!({
                "full_name": "Dave", "username": "dave", "email": "dave@example.com",
                "password": "some password",
            })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.body["missing_permissions"], json!(["user.create"]));
    let roles = node
        .call(
            Method::GET,
            &format!("/api/v1/users/{carol_id}/roles"),
            Some(&carol),
            None,
        )
        .await;
    assert_eq!(roles.status, StatusCode::OK);
    assert_eq!(
        roles.body["items"][0]["role"]["builtin"],
        "directory_viewer"
    );
    assert_eq!(roles.body["items"][0]["sources"][0]["kind"], "group");

    // A read-only tracker token cannot reach administration.
    let token = node
        .call(
            Method::POST,
            "/api/v1/users/me/tokens",
            Some(&admin),
            Some(json!({ "name": "reader", "scopes": ["tracker.read"] })),
        )
        .await;
    let secret = token.body["secret"].as_str().unwrap().to_owned();
    let groups = node
        .call(Method::GET, "/api/v1/groups", Some(&secret), None)
        .await;
    assert_eq!(groups.status, StatusCode::FORBIDDEN);
    let lookup = node
        .call(Method::GET, "/api/v1/users?q=car", Some(&secret), None)
        .await;
    assert_eq!(lookup.status, StatusCode::OK);
    assert_eq!(lookup.body["items"][0]["username"], "carol");
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn acc_022_acc_053_acc_060_groups_roles_and_assignments_over_http() {
    let node = Node::start(Throttle::default()).await;
    let admin = node.set_up().await;
    let catalog = node
        .call(Method::GET, "/api/v1/permissions", Some(&admin), None)
        .await;
    assert_eq!(catalog.body["permissions"].as_array().unwrap().len(), 34);
    assert_eq!(catalog.body["permissions"][1]["key"], "system.write");
    assert_eq!(
        catalog.body["permissions"][1]["implies"],
        json!(["system.read"])
    );

    let roles = node
        .call(Method::GET, "/api/v1/roles", Some(&admin), None)
        .await;
    assert_eq!(roles.status, StatusCode::OK);
    let predefined: Vec<&str> = roles.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|role| role["builtin"].as_str().unwrap())
        .collect();
    assert_eq!(
        predefined,
        [
            "system_admin",
            "project_admin",
            "project_member",
            "directory_viewer",
            "project_creator",
            "account_creator"
        ]
    );
    let account_creator = roles.body["items"][5]["id"].as_i64().unwrap();

    let custom = node
        .call(
            Method::POST,
            "/api/v1/roles",
            Some(&admin),
            Some(json!({ "name": "Triage", "permissions": ["comment.read"] })),
        )
        .await;
    assert_eq!(custom.status, StatusCode::CREATED, "{}", custom.body);
    let custom_id = custom.body["id"].as_i64().unwrap();
    let toggled = node
        .call(
            Method::PUT,
            &format!("/api/v1/roles/{custom_id}/permissions/issue.read"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(
        toggled.body["permissions"],
        json!(["project.basic", "issue.read", "comment.read"])
    );

    let group = node
        .call(
            Method::POST,
            "/api/v1/groups",
            Some(&admin),
            Some(json!({ "name": "Recruiters" })),
        )
        .await;
    assert_eq!(group.status, StatusCode::CREATED, "{}", group.body);
    let group_id = group.body["id"].as_i64().unwrap();
    let assigned = node
        .call(
            Method::POST,
            "/api/v1/role-assignments",
            Some(&admin),
            Some(json!({ "role_id": account_creator, "group_id": group_id, "global": true })),
        )
        .await;
    assert_eq!(assigned.status, StatusCode::CREATED, "{}", assigned.body);
    assert_eq!(assigned.body["items"][0]["holder"]["kind"], "group");

    let created = node
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({
                "full_name": "Bob", "username": "bob", "email": "bob@example.com",
                "password": "bob's password",
            })),
        )
        .await;
    let bob = created.body["id"].as_i64().unwrap();
    let added = node
        .call(
            Method::POST,
            &format!("/api/v1/groups/{group_id}/members"),
            Some(&admin),
            Some(json!({ "users": [bob] })),
        )
        .await;
    assert_eq!(added.body["users"], json!([bob]));
    let shown = node
        .call(
            Method::GET,
            &format!("/api/v1/groups/{group_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(shown.body["members"], 1);
    assert_eq!(shown.body["roles"][0]["role"]["builtin"], "account_creator");
    let members = node
        .call(
            Method::GET,
            &format!("/api/v1/groups/{group_id}/members"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(members.body["items"][0]["kind"], "person");
    assert_eq!(members.body["items"][0]["username"], "bob");

    // Bob may create accounts now: the cache was dropped by the change.
    let bob_session = node
        .call(
            Method::POST,
            "/api/v1/session",
            None,
            Some(json!({ "login": "bob", "password": "bob's password" })),
        )
        .await
        .session_cookie()
        .unwrap();
    let bob_map = node
        .call(
            Method::GET,
            "/api/v1/users/me/access",
            Some(&bob_session),
            None,
        )
        .await;
    assert!(keys(&bob_map.body["global"]).contains(&"user.create"));
    let removed = node
        .call(
            Method::POST,
            &format!("/api/v1/groups/{group_id}/members/remove"),
            Some(&admin),
            Some(json!({ "users": [bob] })),
        )
        .await;
    assert_eq!(removed.body["users"], json!([bob]));
    let bob_map = node
        .call(
            Method::GET,
            "/api/v1/users/me/access",
            Some(&bob_session),
            None,
        )
        .await;
    assert!(!keys(&bob_map.body["global"]).contains(&"user.create"));

    // ACC-006, ACC-013: a deleted account's ID answers with the replacement.
    let deleted = node
        .call(
            Method::POST,
            "/api/v1/users/delete",
            Some(&admin),
            Some(json!({ "users": [bob], "replacement": 1 })),
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
    let resolved = node
        .call(
            Method::GET,
            &format!("/api/v1/users/{bob}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(resolved.body["id"], 1);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_008_acc_004_links_reset_passwords_and_accept_invitations() {
    let node = Node::start_mailing().await;
    let admin = node.set_up().await;
    let bootstrap = node
        .call(Method::GET, "/api/v1/bootstrap", None, None)
        .await;
    assert_eq!(bootstrap.body["email_enabled"], true);

    // AUTH-008: the administrator's address was verified by the setup.
    let asked = node
        .call(
            Method::POST,
            "/api/v1/password-reset",
            None,
            Some(json!({ "email": "anna@example.com" })),
        )
        .await;
    assert_eq!(asked.status, StatusCode::ACCEPTED);
    let secret = node.last_link().await;
    let info = node
        .call(
            Method::POST,
            "/api/v1/account-links/inspect",
            None,
            Some(json!({ "token": secret })),
        )
        .await;
    assert_eq!(info.body["purpose"], "password_reset");
    assert_eq!(info.body["username"], "admin");
    let reset = node
        .call(
            Method::POST,
            "/api/v1/password-reset/confirm",
            None,
            Some(json!({ "token": secret, "new_password": "brand new password" })),
        )
        .await;
    assert_eq!(reset.status, StatusCode::CREATED, "{}", reset.body);
    assert!(reset.session_cookie().is_some());
    // The setup's session ended with the reset.
    let old = node
        .call(Method::GET, "/api/v1/session", Some(&admin), None)
        .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let admin = reset.session_cookie().unwrap();

    // ACC-004: an invitation, accepted with a password of one's own.
    let invited = node
        .call(
            Method::POST,
            "/api/v1/users/invitations",
            Some(&admin),
            Some(json!({ "emails": "new.person@example.com" })),
        )
        .await;
    assert_eq!(invited.status, StatusCode::CREATED, "{}", invited.body);
    assert_eq!(invited.body["items"][0]["invited"], true);
    let secret = node.last_link().await;
    let joined = node
        .call(
            Method::POST,
            "/api/v1/invitations/accept",
            None,
            Some(json!({ "token": secret, "password": "my first password" })),
        )
        .await;
    assert_eq!(joined.status, StatusCode::CREATED, "{}", joined.body);
    assert_eq!(joined.body["user"]["username"], "new.person");
    assert_eq!(joined.body["user"]["email_verified"], true);
    let used = node
        .call(
            Method::POST,
            "/api/v1/account-links/inspect",
            None,
            Some(json!({ "token": secret })),
        )
        .await;
    assert_eq!(used.status, StatusCode::NOT_FOUND);
    node.stop().await;
}
