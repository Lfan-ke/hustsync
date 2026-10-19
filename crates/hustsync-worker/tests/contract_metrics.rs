//! `GET /metrics` and `/metrics/<group>` on the worker control server.

#![cfg(feature = "p8s")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt as _;
use hustsync_config_parser::MetricsConfig;
use hustsync_worker::{
    metrics::Metrics,
    schedule::ScheduleQueue,
    server::{AppState, make_http_server},
};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tower::ServiceExt;

fn state(semaphore: Arc<Semaphore>, metrics: Metrics) -> Arc<AppState> {
    Arc::new(AppState {
        jobs: Arc::new(RwLock::new(HashMap::new())),
        schedule_queue: Arc::new(Mutex::new(ScheduleQueue::new())),
        semaphore,
        concurrent: 3,
        reload_tx: None,
        metrics: Arc::new(metrics),
    })
}

async fn get(s: &Arc<AppState>, uri: &str) -> (StatusCode, String) {
    let req = Request::get(uri).body(Body::empty()).unwrap();
    let resp = make_http_server(Arc::clone(s)).oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

fn has(text: &str, line: &str) {
    assert!(
        text.lines().any(|l| l == line),
        "missing `{line}` in\n{text}"
    );
}

#[tokio::test]
async fn metrics_report_jobs_queue_and_concurrency() {
    let semaphore = Arc::new(Semaphore::new(3));
    let _held = Arc::clone(&semaphore).acquire_owned().await.unwrap();
    let s = state(semaphore, Metrics::default());
    for uri in ["/metrics", "/metrics/all", "/metrics/jobs"] {
        let (status, text) = get(&s, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        for line in [
            "hustsync_worker_jobs 0",
            "hustsync_worker_jobs_running 1",
            "hustsync_worker_jobs_queued 0",
            "hustsync_worker_concurrency 3",
        ] {
            has(&text, line);
        }
    }
    let (_, process) = get(&s, "/metrics/process").await;
    assert!(
        process.contains("hustsync_build_info{version="),
        "{process}"
    );
    assert!(!process.contains("hustsync_worker_jobs"), "{process}");
}

#[tokio::test]
async fn unknown_groups_are_404() {
    let s = state(Arc::new(Semaphore::new(3)), Metrics::default());
    let (status, body) = get(&s, "/metrics/mirrors").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        "unknown metric group `mirrors`; groups are jobs, process"
    );
}

#[tokio::test]
async fn cached_jobs_keep_their_values() {
    let cfg = MetricsConfig {
        cache: [("jobs".into(), 3600)].into(),
        push: Vec::new(),
    };
    let semaphore = Arc::new(Semaphore::new(3));
    let s = state(Arc::clone(&semaphore), Metrics::new(Some(&cfg)).unwrap());
    let (_, before) = get(&s, "/metrics/jobs").await;
    let _held = semaphore.acquire_owned().await.unwrap();
    assert_eq!(get(&s, "/metrics/jobs").await.1, before);
    assert!(
        Metrics::new(Some(&MetricsConfig {
            cache: [("job".into(), 1)].into(),
            push: Vec::new(),
        }))
        .is_err()
    );
}
