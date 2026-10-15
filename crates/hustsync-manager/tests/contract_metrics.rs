//! `GET /metrics`: sync counters, mirror gauges and live worker probes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod contract;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
    routing::get,
};
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

async fn report(app: &Router, worker: &str, mirror: &str, status: &str) {
    let zero = "1970-01-01T00:00:00Z";
    let s = json!({
        "name": mirror, "worker": worker, "upstream": "", "size": "", "error_msg": "",
        "last_update": zero, "last_started": zero, "last_ended": zero, "next_schedule": zero,
        "status": status, "is_master": true
    });
    let uri = format!("/workers/{worker}/jobs/{mirror}");
    assert_eq!(post(app, &uri, s).await.status(), StatusCode::OK);
}

async fn scrape(app: &Router) -> String {
    let req = Request::get("/metrics").body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ty = resp.headers()[header::CONTENT_TYPE].to_str().unwrap();
    assert!(ty.starts_with("text/plain"), "{ty}");
    String::from_utf8(contract::body_bytes(resp).await).unwrap()
}

#[tokio::test]
async fn metrics_cover_syncs_mirrors_and_workers() {
    let (app, _dir) = contract::spawn_manager();
    let live = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", live.local_addr().unwrap());
    tokio::spawn(axum::serve(live, Router::new().route("/", get(|| async { "ok" }))).into_future());

    register(&app, "w1", &url).await;
    register(&app, "w2", "http://127.0.0.1:9/").await;
    report(&app, "w1", "m1", "pre-syncing").await;
    report(&app, "w1", "m1", "success").await;
    report(&app, "w1", "m2", "failed").await;
    report(&app, "w1", "m2", "failed").await;

    let text = scrape(&app).await;
    for line in [
        r#"hustsync_syncs_total{mirror="m1",result="success",worker="w1"} 1"#,
        r#"hustsync_syncs_total{mirror="m2",result="failed",worker="w1"} 2"#,
        r#"hustsync_mirrors{status="success"} 1"#,
        r#"hustsync_mirrors{status="failed"} 1"#,
        r#"hustsync_mirrors{status="syncing"} 0"#,
        "hustsync_workers 2",
        "hustsync_workers_online 1",
        r#"hustsync_worker_up{worker="w1"} 1"#,
        r#"hustsync_worker_up{worker="w2"} 0"#,
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "missing `{line}` in
{text}"
        );
    }
    for prefix in [
        r#"hustsync_mirror_sync_duration_seconds{mirror="m1",worker="w1"} "#,
        r#"hustsync_mirror_last_success_timestamp_seconds{mirror="m1",worker="w1"} "#,
    ] {
        assert!(
            text.lines().any(|l| l.starts_with(prefix)),
            "missing `{prefix}` in
{text}"
        );
    }
    assert!(!text.contains(r#"hustsync_mirror_last_success_timestamp_seconds{mirror="m2""#));
}
