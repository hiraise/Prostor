//! The benchmark on a small dataset: generation fills every table and leaves the schema as the
//! migrations made it, the access the scenarios assume is the one the server compiles, and every
//! scenario and every query variant runs, the HTTP ones through a node. The benchmark's queries
//! are checked only here (lib.rs).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use sqlx::{AssertSqlSafe, Connection, PgConnection};
use taskmaster_bench::catalog::Catalog;
use taskmaster_bench::dataset::{self, Plan};
use taskmaster_bench::http::Node;
use taskmaster_bench::report::{self, Environment, Timing};
use taskmaster_bench::runner::{self, RunOptions};
use taskmaster_bench::scenarios::live::{self, LiveOptions};
use taskmaster_bench::scenarios::{Mode, Scenario};
use taskmaster_domain::access::Projects;
use taskmaster_domain::access::permission::Permission;
use taskmaster_testkit::TestDb;

#[tokio::test(flavor = "multi_thread")]
async fn a_small_dataset_runs_every_scenario() {
    let db = TestDb::new().await;
    let mut conn = PgConnection::connect_with(db.options()).await.unwrap();
    let schema_before = schema(&mut conn).await;
    let plan = Plan::new(0.003, 7);
    let summary = dataset::generate(db.options(), plan, &|_| {})
        .await
        .unwrap();
    let rows = |table: &str| {
        summary
            .tables
            .iter()
            .find(|t| t.0 == table)
            .unwrap_or_else(|| panic!("no table {table}"))
            .1
    };
    assert_eq!(rows("issues"), 3000);
    assert_eq!(rows("users"), 50);
    assert_eq!(rows("fields"), 60);
    assert_eq!(rows("projects"), 5);
    assert_eq!(rows("project_keys"), 5);
    for table in [
        "api_tokens",
        "groups",
        "group_members",
        "roles",
        "role_assignments",
        "value_sets",
        "field_values",
        "value_set_groups",
        "project_fields",
        "tags",
        "issue_ref_values",
        "issue_int_values",
        "issue_float_values",
        "issue_date_values",
        "issue_datetime_values",
        "issue_string_values",
        "issue_text_values",
        "issue_history",
        "comments",
        "issue_watchers",
        "issue_votes",
        "issue_tags",
        "boards",
        "board_columns",
        "sprints",
        "sprint_issues",
    ] {
        assert!(rows(table) > 0, "{table} is empty");
    }
    // The bulk load dropped keys and indexes and built them again as the migrations define them.
    assert_eq!(schema(&mut conn).await, schema_before);
    // Numbers continue after the loaded issues; a few issues are limited to an audience.
    let (next, restricted): (i64, i64) = sqlx::query_as(
        "select (select count(*) from projects
                 where next_number <> 1 + (select max(number) from issues where project_id = projects.id)),
                (select count(*) from issues
                 where cardinality(audience_users) > 0 or cardinality(audience_groups) > 0)",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert_eq!(next, 0);
    assert!((30..300).contains(&restricted), "{restricted}");

    assert_eq!(
        dataset::fingerprint(&mut conn).await.unwrap(),
        Some(plan.fingerprint())
    );
    // Rows loaded with their IDs: the server's next inserts get new ones.
    for table in [
        "groups",
        "projects",
        "value_sets",
        "field_values",
        "fields",
        "tags",
        "issues",
        "issue_history",
        "comments",
    ] {
        let (last, max): (Option<i64>, i64) = sqlx::query_as(AssertSqlSafe(format!(
            "select pg_sequence_last_value(pg_get_serial_sequence('{table}', 'id')::regclass),
                    max(id)
             from {table}"
        )))
        .fetch_one(&mut conn)
        .await
        .unwrap();
        assert_eq!(last, Some(max), "the sequence of {table}");
    }
    let catalog = Arc::new(Catalog::load(&mut conn).await.unwrap());
    assert_eq!(catalog.priorities.len(), 5);
    // Positions follow the ranks of the values: the first is the highest priority.
    assert_eq!(catalog.priority_order.len(), 5);
    assert_eq!(catalog.priority_order[0], catalog.urgent[..1]);
    assert!(!catalog.bugs.is_empty() && !catalog.fixed.is_empty());
    assert!(!catalog.sprints.is_empty() && !catalog.assignees.is_empty());

    viewers_match_the_server(db.pool(), &catalog).await;

    runner::prewarm(db.options()).await.unwrap();
    let node = Arc::new(Node::start(db.options(), 2).await.unwrap());
    let run = RunOptions {
        iterations: 12,
        warmup: 1,
        clients: 2,
        seed: 7,
        breakdown: false,
    };
    let mut measurements = Vec::new();
    for scenario in Scenario::ALL {
        let m = runner::measure(db.options(), &catalog, &node, scenario, &run)
            .await
            .unwrap();
        assert_eq!(m.overall.count, 12, "{scenario:?}");
        for (variant, stats) in &m.variants {
            assert!(stats.count > 0, "{scenario:?} {variant} never ran");
        }
        measurements.push(m);
        let plans = runner::explain(db.options(), &catalog, scenario, 7)
            .await
            .unwrap();
        for (variant, plan) in plans {
            assert!(plan.contains("Execution Time"), "{variant}: {plan}");
        }
    }
    // Lists, boards and searches find issues, the issue page has activity; a change commits.
    for m in &measurements {
        let found = m.overall.rows;
        assert!(found > 0.0, "{:?} found nothing", m.scenario);
    }
    let changed: i64 = sqlx::query_scalar(
        "select count(*) from issue_history where kind = 'field' and at > now() - interval '1 hour'",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert!(changed > 0, "no change was committed over HTTP");
    assert_eq!(Scenario::Open.mode(0), Mode::Http);
    assert_eq!(Scenario::Board.mode(0), Mode::Http);

    // The breakdown lists the statements of a request over HTTP.
    let one = RunOptions {
        clients: 1,
        iterations: 4,
        breakdown: true,
        ..run.clone()
    };
    let open = runner::measure(db.options(), &catalog, &node, Scenario::Open, &one)
        .await
        .unwrap();
    assert!(
        open.statements
            .iter()
            .any(|s| s.text.contains("from issues")),
        "{:?}",
        open.statements
    );
    assert!(open.statements.iter().all(|s| s.calls > 0.0));
    measurements.push(open);

    // Live delivery: every subscriber of a changed issue gets its event.
    let options = LiveOptions {
        connections: 50,
        changes: 8,
        seed: 7,
        clients: env!("CARGO_BIN_EXE_taskmaster-bench").into(),
    };
    let delivered = live::measure(&catalog, &node, &options).await.unwrap();
    assert_eq!(delivered.opened, 50);
    assert!(delivered.capacity.is_none(), "{delivered:?}");
    assert_eq!(delivered.team.missed, 0);
    assert_eq!(delivered.team.change.count, 8);
    assert!(delivered.team.delivery.count >= 8, "{delivered:?}");

    let env = Environment::read(&mut conn).await.unwrap();
    let mut out = Vec::new();
    let timing = Timing {
        phases: vec![("measuring".to_owned(), std::time::Duration::from_secs(1))],
        generated: false,
        ..Timing::default()
    };
    report::write(
        &mut out,
        &env,
        &run,
        &measurements,
        Some(&delivered),
        &timing,
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("3 000 issues"), "{text}");
    assert!(
        text.contains("Database round trip (select 1, median)"),
        "{text}"
    );
    assert!(
        text.contains("Board with 500 cards (D-022) [http]"),
        "{text}"
    );
    assert!(text.contains("Open issue [http]"), "{text}");
    assert!(
        text.contains("Issue list page of 50 (D-022) [http]"),
        "{text}"
    );
    assert!(text.contains("SQL of «Open issue»"), "{text}");
    assert!(
        text.contains("Live delivery, team: 50 sockets, 5/s [http+ws]"),
        "{text}"
    );
    assert!(text.contains("50 of 50 sockets opened"), "{text}");
    assert!(
        text.contains("Bench time: 1.0 s (budget 3 min 0 s)"),
        "{text}"
    );
    if let Some(node) = Arc::into_inner(node) {
        node.stop().await;
    }
    conn.close().await.unwrap();
}

/// The access the scenarios assume (`Catalog::viewer`) is what the server compiles from the
/// dataset's role assignments, for a sample of people.
async fn viewers_match_the_server(pool: &sqlx::PgPool, catalog: &Catalog) {
    let only = |projects: Projects| match projects {
        Projects::All => panic!("nobody in the dataset holds a global grant"),
        Projects::Only(mut ids) => {
            ids.sort_unstable();
            ids
        }
    };
    let mut create_only_seen = false;
    for user in catalog
        .users
        .iter()
        .step_by(catalog.users.len() / 20)
        .copied()
    {
        let access = taskmaster_service::access::load(pool, user).await.unwrap();
        let viewer = catalog.viewer(user);
        let read = only(access.projects_with(Permission::IssueRead));
        let create_only: Vec<i64> = only(access.projects_with(Permission::IssueCreate))
            .into_iter()
            .filter(|p| !read.contains(p))
            .collect();
        let mut groups = access.groups().to_vec();
        groups.sort_unstable();
        let mut expected_groups = viewer.groups.clone();
        expected_groups.sort_unstable();
        expected_groups.dedup();
        assert_eq!(read, viewer.read, "read projects of user {user}");
        assert_eq!(
            create_only, viewer.create_only,
            "create-only projects of user {user}"
        );
        assert_eq!(groups, expected_groups, "groups of user {user}");
        assert_eq!(
            only(access.projects_with(Permission::VisibilityBypass)),
            viewer.bypass,
            "every audience of user {user}"
        );
        create_only_seen |= !create_only.is_empty();
    }
    assert!(
        create_only_seen,
        "no sampled person reports outside their teams"
    );
}

/// Definitions of every constraint and index in the public schema.
async fn schema(conn: &mut PgConnection) -> Vec<String> {
    sqlx::query_scalar(
        "select c.conrelid::regclass::text || ' ' || c.conname || ' ' || pg_get_constraintdef(c.oid)
                || ' ' || c.convalidated
         from pg_constraint c join pg_namespace n on n.oid = c.connamespace
         where n.nspname = 'public'
         union all
         select indexdef from pg_indexes where schemaname = 'public'
         order by 1",
    )
    .fetch_all(conn)
    .await
    .unwrap()
}
