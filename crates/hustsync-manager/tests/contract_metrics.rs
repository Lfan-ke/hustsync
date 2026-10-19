//! `GET /metrics` and `/metrics/<group>`: sync counters, mirror gauges, live worker probes, caching and pushes.

#![cfg(feature = "p8s")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod contract;

use std::collections::BTreeSet;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
    routing::get,
};
use hustsync_config_parser::MetricsConfig;
use hustsync_manager::Manager;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn post(app: &Router, uri: &str, body: Value) -> Response {
    let req = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}

async fn register(app: &Router, id: &str, url: &str) {
    let w = json!({
        "id": id, "url": url, "token": "",
        "last_online": "1970-01-01T00:00:00Z", "last_register": "1970-01-01T00:00:00Z"
    });
    assert_eq!(post(app, "/workers", w).await.status(), StatusCode::OK);
}

async fn report(app: &Router, worker: &str, mirror: &str, status: &str, size: &str) {
    let zero = "1970-01-01T00:00:00Z";
    let s = json!({
        "name": mirror, "worker": worker, "upstream": "rsync://up/m/", "size": size, "error_msg": "",
        "last_update": zero, "last_started": zero, "last_ended": zero,
        "next_schedule": "2030-01-01T00:00:00Z", "status": status, "is_master": true
    });
    let uri = format!("/workers/{worker}/jobs/{mirror}");
    assert_eq!(post(app, &uri, s).await.status(), StatusCode::OK);
}

async fn get_text(app: &Router, uri: &str) -> (StatusCode, String) {
    let req = Request::get(uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    if status == StatusCode::OK {
        let ty = resp.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ty.starts_with("text/plain"), "{ty}");
    }
    (
        status,
        String::from_utf8(contract::body_bytes(resp).await).unwrap(),
    )
}

async fn scrape(app: &Router, uri: &str) -> String {
    let (status, text) = get_text(app, uri).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {text}");
    text
}

fn families(text: &str) -> BTreeSet<&str> {
    text.lines()
        .filter_map(|l| l.strip_prefix("# TYPE "))
        .filter_map(|l| l.split(' ').next())
        .collect()
}

fn has(text: &str, line: &str) {
    assert!(
        text.lines().any(|l| l == line),
        "missing `{line}` in\n{text}"
    );
}

fn has_prefix(text: &str, prefix: &str) {
    assert!(
        text.lines().any(|l| l.starts_with(prefix)),
        "missing `{prefix}` in\n{text}"
    );
}

async fn seeded() -> (Router, tempfile::TempDir) {
    let (app, dir) = contract::spawn_manager();
    let live = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", live.local_addr().unwrap());
    tokio::spawn(axum::serve(live, Router::new().route("/", get(|| async { "ok" }))).into_future());

    register(&app, "w1", &url).await;
    register(&app, "w2", "http://127.0.0.1:9/").await;
    report(&app, "w1", "m1", "pre-syncing", "").await;
    report(&app, "w1", "m1", "success", "1.5G").await;
    report(&app, "w1", "m2", "failed", "unknown").await;
    report(&app, "w1", "m2", "failed", "unknown").await;
    (app, dir)
}

#[tokio::test]
async fn metrics_cover_syncs_mirrors_and_workers() {
    let (app, _dir) = seeded().await;
    let text = scrape(&app, "/metrics").await;
    for line in [
        r#"hustsync_syncs_total{mirror="m1",result="success",worker="w1"} 1"#,
        r#"hustsync_syncs_total{mirror="m2",result="failed",worker="w1"} 2"#,
        r#"hustsync_sync_duration_seconds_count{result="success",worker="w1"} 1"#,
        r#"hustsync_mirrors{status="success"} 1"#,
        r#"hustsync_mirrors{status="failed"} 1"#,
        r#"hustsync_mirrors{status="syncing"} 0"#,
        r#"hustsync_mirror_state{mirror="m1",worker="w1"} 2"#,
        r#"hustsync_mirror_state{mirror="m2",worker="w1"} 1"#,
        r#"hustsync_mirror_info{mirror="m1",role="master",upstream="rsync://up/m/",worker="w1"} 1"#,
        r#"hustsync_mirror_size_bytes{mirror="m1",worker="w1"} 1500000000"#,
        r#"hustsync_mirror_next_sync_timestamp_seconds{mirror="m1",worker="w1"} 1893456000"#,
        "hustsync_workers 2",
        "hustsync_workers_online 1",
        r#"hustsync_worker_up{worker="w1"} 1"#,
        r#"hustsync_worker_up{worker="w2"} 0"#,
    ] {
        has(&text, line);
    }
    for prefix in [
        r#"hustsync_mirror_sync_duration_seconds{mirror="m1",worker="w1"} "#,
        r#"hustsync_mirror_last_success_timestamp_seconds{mirror="m1",worker="w1"} "#,
        r#"hustsync_worker_last_online_timestamp_seconds{worker="w1"} "#,
        r#"hustsync_build_info{version=""#,
    ] {
        has_prefix(&text, prefix);
    }
    assert!(!text.contains(r#"hustsync_mirror_last_success_timestamp_seconds{mirror="m2""#));
    assert!(!text.contains(r#"hustsync_mirror_size_bytes{mirror="m2""#));
}

#[tokio::test]
async fn groups_split_the_whole_without_overlap() {
    let (app, _dir) = seeded().await;
    let all = scrape(&app, "/metrics").await;
    assert_eq!(
        families(&all),
        families(&scrape(&app, "/metrics/all").await)
    );
    let mut union = BTreeSet::new();
    for g in ["mirrors", "workers", "syncs", "process"] {
        let text = scrape(&app, &format!("/metrics/{g}")).await;
        let fams = families(&text)
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        assert!(!fams.is_empty(), "{g} is empty");
        assert!(union.is_disjoint(&fams), "{g} overlaps: {fams:?}");
        union.extend(fams);
    }
    let all: BTreeSet<_> = families(&all).into_iter().map(str::to_owned).collect();
    assert_eq!(union, all);
}

#[tokio::test]
async fn unknown_groups_are_404_with_the_list() {
    let (app, _dir) = contract::spawn_manager();
    let (status, body) = get_text(&app, "/metrics/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        "unknown metric group `nope`; groups are mirrors, workers, syncs, process"
    );
}

#[tokio::test]
async fn cached_groups_stay_until_they_expire() {
    let cfg = MetricsConfig {
        cache: [("mirrors".into(), 3600)].into(),
        push: Vec::new(),
    };
    let (mgr, _dir) = contract::manager_with(Some(cfg));
    let app = mgr.make_router();
    report(&app, "w1", "m1", "success", "").await;
    let before = scrape(&app, "/metrics/mirrors").await;
    report(&app, "w1", "m2", "failed", "").await;
    assert_eq!(scrape(&app, "/metrics/mirrors").await, before);
    has(
        &scrape(&app, "/metrics/syncs").await,
        r#"hustsync_syncs_total{mirror="m2",result="failed",worker="w1"} 1"#,
    );
}

#[test]
fn unknown_cache_groups_stop_the_manager() {
    let cfg = MetricsConfig {
        cache: [("mirror".into(), 10)].into(),
        push: Vec::new(),
    };
    let dir = tempfile::TempDir::new().unwrap();
    let err = Manager::new(contract::config(&dir.path().join("t.db"), Some(cfg)))
        .err()
        .unwrap();
    assert!(
        err.to_string().contains("unknown metric group `mirror`"),
        "{err}"
    );
}

#[cfg(feature = "influx")]
#[tokio::test]
async fn pushes_reach_the_target_as_line_protocol() {
    use std::{sync::Arc, time::Duration};

    use axum::routing::post as on_post;
    use hustsync_config_parser::PushConfig;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<(String, String)>(4);
    let sink = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/telegraf", sink.local_addr().unwrap());
    let recv = Router::new().route(
        "/telegraf",
        on_post(move |headers: axum::http::HeaderMap, body: String| {
            let tx = tx.clone();
            async move {
                let auth = headers[header::AUTHORIZATION].to_str().unwrap().to_owned();
                let _ = tx.send((auth, body)).await;
                StatusCode::NO_CONTENT
            }
        }),
    );
    tokio::spawn(axum::serve(sink, recv).into_future());

    let cfg = MetricsConfig {
        cache: Default::default(),
        push: vec![PushConfig {
            url,
            format: Default::default(),
            interval: 1,
            groups: Some(vec!["mirrors".into(), "syncs".into()]),
            headers: [("Authorization".into(), "Token t".into())].into(),
        }],
    };
    let (mgr, _dir) = contract::manager_with(Some(cfg));
    let app = Arc::clone(&mgr).make_router();
    report(&app, "w1", "m1", "success", "").await;
    let run = tokio::spawn(Arc::clone(&mgr).run(std::future::pending()));

    let (auth, body) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    run.abort();
    assert_eq!(auth, "Token t");
    let lines: Vec<_> = body
        .lines()
        .map(|l| l.rsplit_once(' ').unwrap().0)
        .collect();
    for want in [
        "prometheus,status=success hustsync_mirrors=1",
        "prometheus,mirror=m1,worker=w1 hustsync_mirror_state=2",
        "prometheus,mirror=m1,result=success,worker=w1 hustsync_syncs_total=1",
    ] {
        assert!(lines.contains(&want), "missing `{want}` in\n{body}");
    }
    assert!(
        !body.contains("hustsync_workers"),
        "workers were not asked for:\n{body}"
    );
}
